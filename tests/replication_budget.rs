use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
};
use vesper3d::viewer::{net::*, server::DedicatedServer, simulation::HeadlessWorld};

#[derive(Default)]
struct Wire {
    sent: RefCell<Vec<Vec<u8>>>,
    reject: bool,
    limit: Cell<usize>,
    capacity: Cell<usize>,
    incoming: Vec<Datagram>,
    last_peer: Cell<Option<SocketAddr>>,
}
impl DatagramTransport for Wire {
    fn send(&self, peer: SocketAddr, bytes: &[u8]) -> vesper3d::Result<usize> {
        if self.reject
            || (self.capacity.get() > 0 && self.sent.borrow().len() >= self.capacity.get())
        {
            return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock).into());
        }
        self.last_peer.set(Some(peer));
        self.sent.borrow_mut().push(bytes.to_vec());
        Ok(bytes.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(std::mem::take(&mut self.incoming))
    }
    fn payload_limit(&self, _: SocketAddr) -> usize {
        if self.limit.get() == 0 {
            MAX_PACKET_BYTES
        } else {
            self.limit.get()
        }
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:4000".parse()?)
    }
}
fn server() -> DedicatedServer<Wire> {
    let mut server =
        DedicatedServer::with_transport(Wire::default(), HeadlessWorld::new().unwrap()).unwrap();
    for id in 1..=8 {
        server.handle_hello(
            format!("127.0.0.1:{}", 5000 + id).parse().unwrap(),
            PROTOCOL_VERSION,
            id,
            server.world.content_hash,
        );
    }
    server.world.tick = 3;
    server.transport.sent.borrow_mut().clear();
    server
}
#[test]
fn oversized_world_makes_wire_progress() {
    let mut server = server();
    assert!(Packet::Snapshot(server.world.snapshot_for_player(1, 0))
        .encode()
        .is_err());
    server.broadcast_snapshots();
    assert!(
        !server.transport.sent.borrow().is_empty(),
        "NET-BUDGET-002: oversized world produced no updates; run python tools/be2.py context NET-BUDGET-002"
    );
}
#[test]
fn rejected_update_is_not_acknowledgement_history() {
    let mut server = server();
    server.transport.reject = true;
    server.broadcast_snapshots();
    assert!(
        server
            .sessions
            .values()
            .all(|s| s.snapshot_history.is_empty()),
        "NET-BUDGET-002: rejected update entered ack history; run python tools/be2.py context NET-BUDGET-002"
    );
}

fn world(count: usize) -> WorldSnapshot {
    use vesper3d::{
        math::V,
        viewer::{controller::Controller, lifecycle::Generation},
    };
    WorldSnapshot {
        session: None,
        tick: 3,
        ack_client_tick: 10,
        players: vec![PlayerNetState::from_controller(
            1,
            3,
            &Controller::default(),
            None,
        )],
        props: (0..count)
            .map(|i| PropNetState {
                id: format!("prop-{i:04}"),
                position: V(i as f32, 0., 0.),
                rotation: [0., 0., 0., 1.],
                linear_velocity: V::ZERO,
                angular_velocity: V::ZERO,
                sleeping: true,
                held_by: None,
                generation: Generation(1),
            })
            .collect(),
    }
}
fn normalized(mut world: WorldSnapshot) -> WorldSnapshot {
    world.tick = 0;
    world.session = None;
    world.players.sort_by_key(|p| p.id);
    world.props.sort_by(|a, b| a.id.cmp(&b.id));
    world
}
fn deliver(sender: &mut ReplicationSender, base: &mut Option<WorldSnapshot>, bytes: &[u8]) {
    match receive_update(base, Packet::decode(bytes).unwrap()) {
        Ok(_) => {
            sender.acknowledge(base.as_ref().unwrap().tick);
        }
        Err(_) => sender.resynchronize(),
    }
}
#[test]
fn loss_duplication_reordering_saturation_and_recovery_converge() {
    let mut sender = ReplicationSender::for_session([u64::MAX; 2]);
    let mut receiver = None;
    let mut desired = world(96);
    let mut wire = Wire::default();
    wire.limit.set(700); // smaller active transport than the global codec ceiling
    let peer = "127.0.0.1:5".parse().unwrap();
    let mut delayed = vec![];
    let mut total = 0;
    for step in 0..400 {
        desired.tick += 3;
        if step < 100 {
            desired.players[0].tick = desired.tick;
            desired.players[0].position.0 += 0.1;
            desired.ack_client_tick += 1;
            desired.props[0].position.2 += 1.;
        }
        if step == 35 {
            desired.props.drain(10..30);
        } // relevance exit / removals
        if step == 65 {
            desired.props.extend(world(30).props.into_iter().skip(10));
        } // relevance reentry
        if step == 90 {
            desired.props[1].generation.0 += 1;
        } // same-ID recreation
        wire.reject = (110..145).contains(&step) || step % 17 == 0;
        sender.send(&wire, peer, &desired, 1).unwrap();
        assert!(sender.pending_bytes() <= 700);
        assert!(!sender.acknowledge(u64::MAX)); // future/unsent ack never commits
        for bytes in wire.sent.borrow_mut().drain(..) {
            assert!(bytes.len() <= 700);
            total += bytes.len();
            if step < 180 && step % 5 == 0 {
                continue;
            } // loss
            if step < 180 && step % 7 == 0 {
                delayed.push(bytes);
                continue;
            }
            deliver(&mut sender, &mut receiver, &bytes);
            deliver(&mut sender, &mut receiver, &bytes); // duplication including lost ack recovery
        }
        if step % 11 == 0 {
            for bytes in delayed.drain(..).rev() {
                deliver(&mut sender, &mut receiver, &bytes);
            }
        }
        if step == 150 {
            receiver = None;
        } // missing baseline requests a new keyframe
    }
    assert_eq!(normalized(receiver.unwrap()), normalized(desired));
    assert!(sender.counters.backpressured > 0 && sender.counters.retries > 0);
    assert!(sender.counters.resyncs > 0);
    println!(
        "impaired 96-prop run: accepted_bytes={total} retained_serialized_bytes={} counters={:?}",
        sender.retained_payload_bytes(),
        sender.counters
    );
}
#[test]
fn continuously_moving_owner_cannot_starve_cold_props() {
    let mut sender = ReplicationSender::default();
    let mut base = None;
    let mut desired = world(128);
    let wire = Wire::default();
    let peer = "127.0.0.1:5".parse().unwrap();
    let mut owner_updates = 0;
    for _ in 0..140 {
        desired.tick += 3;
        desired.players[0].tick = desired.tick;
        desired.players[0].position.0 += 1.;
        sender.send(&wire, peer, &desired, 1).unwrap();
        let bytes = wire.sent.borrow_mut().pop().unwrap();
        deliver(&mut sender, &mut base, &bytes);
        if base.as_ref().unwrap().players[0].tick == desired.tick {
            owner_updates += 1;
        }
    }
    assert_eq!(base.unwrap().props.len(), 128);
    assert_eq!(
        owner_updates, 140,
        "owner should fit alongside the rotating fair slot"
    );
}
#[test]
fn only_accepted_and_exactly_acknowledged_state_advances_baseline() {
    let mut sender = ReplicationSender::default();
    let desired = world(30);
    let mut wire = Wire {
        reject: true,
        ..Default::default()
    };
    let peer = "127.0.0.1:5".parse().unwrap();
    sender.send(&wire, peer, &desired, 1).unwrap();
    assert!(!sender.acknowledge(3));
    assert!(sender.baseline().is_none());
    wire.reject = false;
    sender.send(&wire, peer, &desired, 1).unwrap();
    assert!(sender.baseline().is_none(), "queued is not received");
    let bytes = wire.sent.borrow_mut().pop().unwrap();
    let mut base = None;
    deliver(&mut sender, &mut base, &bytes);
    assert_eq!(sender.baseline(), base.as_ref());
    assert!(sender.baseline().unwrap().props.len() < desired.props.len());
}
#[test]
fn active_limit_reduction_rebudgets_pending_without_acknowledging_it() {
    let mut sender = ReplicationSender::default();
    let desired = world(20);
    let wire = Wire::default();
    let peer = "127.0.0.1:5".parse().unwrap();
    sender.send(&wire, peer, &desired, 1).unwrap();
    let old = wire.sent.borrow_mut().pop().unwrap();
    wire.limit.set(550);
    sender.send(&wire, peer, &desired, 1).unwrap();
    assert!(!sender.acknowledge(3));
    let bytes = wire.sent.borrow_mut().pop().unwrap();
    assert!(bytes.len() <= 550 && old.len() > 550);
    let mut base = None;
    deliver(&mut sender, &mut base, &bytes);
    let tick = base.as_ref().unwrap().tick;
    assert!(receive_update(&mut base, Packet::decode(&old).unwrap())
        .unwrap()
        .is_none());
    assert_eq!(base.unwrap().tick, tick);
}
#[test]
fn unsupported_record_count_id_and_payload_are_actionable() {
    let mut w = world(1);
    w.props[0].id = "x".repeat(MAX_ENTITY_ID_BYTES + 1);
    assert!(validate_world(&w, 1100)
        .unwrap_err()
        .to_string()
        .contains("shorten"));
    assert!(validate_world(&world(MAX_REPLICATED_ENTITIES), 1100)
        .unwrap_err()
        .to_string()
        .contains("entity count"));
    assert!(validate_world(&world(1), 100)
        .unwrap_err()
        .to_string()
        .contains("active transport allows 100"));
}
#[test]
fn maximum_world_and_wholesale_relevance_replacement_remain_bounded() {
    let mut sender = ReplicationSender::for_session([u64::MAX; 2]);
    let mut base = None;
    let mut desired = world(MAX_REPLICATED_ENTITIES - 1);
    let wire = Wire::default();
    let peer = "127.0.0.1:5".parse().unwrap();
    let mut bytes = 0;
    for cycle in 0..2 {
        if cycle == 1 {
            for p in &mut desired.props {
                p.id = format!("new-{}", p.id);
            }
        }
        for _ in 0..1100 {
            desired.tick += 3;
            sender.send(&wire, peer, &desired, 1).unwrap();
            let packet = wire.sent.borrow_mut().pop().unwrap();
            bytes += packet.len();
            deliver(&mut sender, &mut base, &packet);
            assert!(
                base.as_ref().unwrap().players.len() + base.as_ref().unwrap().props.len()
                    <= MAX_REPLICATED_ENTITIES
            );
            if normalized(base.as_ref().unwrap().clone()) == normalized(desired.clone()) {
                break;
            }
        }
        assert_eq!(
            normalized(base.as_ref().unwrap().clone()),
            normalized(desired.clone())
        );
    }
    println!("1024-entity initial sync + replacement: accepted_bytes={bytes}, packets={}, retained_serialized_bytes={} old_60_world_history_bytes={}", sender.counters.accepted_packets, sender.retained_payload_bytes(), serde_json::to_vec(&desired).unwrap().len()*60);
}
#[test]
fn dedicated_server_filters_forged_ack_and_rotates_peers_under_saturation() {
    let mut server = server();
    server.transport.capacity.set(1);
    let mut receivers = std::collections::HashMap::new();
    for cycle in 1..=160 {
        server.world.tick = cycle * 3;
        server.broadcast_snapshots();
        let sent: Vec<_> = server.transport.sent.borrow_mut().drain(..).collect();
        assert!(sent.len() <= 1);
        if let Some(bytes) = sent.first() {
            let peer = server.transport.last_peer.get().unwrap();
            let id = server.clients[&peer];
            let baseline = receivers.entry(id).or_insert(None);
            receive_update(baseline, Packet::decode(bytes).unwrap()).unwrap();
            server.transport.incoming.push(Datagram {
                peer,
                data: Packet::Input(InputFrame {
                    client_tick: cycle,
                    ack_server_tick: baseline.as_ref().unwrap().tick,
                    session_token: Some(server.sessions[&id].session_token),
                    ..Default::default()
                })
                .encode()
                .unwrap(),
            });
        }
        server.poll_network().unwrap();
    }
    assert!(server
        .sessions
        .values()
        .all(|s| s.replication.counters.acknowledged > 0));
    assert!(receivers
        .values()
        .all(|b| b.as_ref().unwrap().players.len() == 8));
    // Acknowledgements fabricated for an unissued future tick are ignored by the actual input path.
    let before = server.sessions[&1].last_acked_tick;
    server.transport.incoming.push(Datagram {
        peer: "127.0.0.1:5001".parse().unwrap(),
        data: Packet::Input(InputFrame {
            client_tick: 999,
            ack_server_tick: u64::MAX,
            session_token: Some(server.sessions[&1].session_token),
            ..Default::default()
        })
        .encode()
        .unwrap(),
    });
    server.poll_network().unwrap();
    assert_eq!(server.sessions[&1].last_acked_tick, before);
}

#[test]
fn reconnect_discards_old_baseline_ack_and_resync_requests() {
    let mut server = server();
    let peer = "127.0.0.1:5001".parse().unwrap();
    let old = server.sessions[&1].session_token;
    server.broadcast_snapshots();
    server.handle_hello(peer, PROTOCOL_VERSION, 1, server.world.content_hash);
    assert_eq!(server.sessions[&1].session_token, old); // duplicate Hello is idempotent
    assert!(server.sessions[&1].replication.pending_bytes() > 0);
    server.transport.incoming.push(Datagram {
        peer,
        data: Packet::Disconnect {
            player_id: 1,
            session_token: Some(old),
        }
        .encode()
        .unwrap(),
    });
    server.poll_network().unwrap();
    server.world.tick = 30;
    server.handle_hello(peer, PROTOCOL_VERSION, 1, server.world.content_hash);
    let new = server.sessions[&1].session_token;
    assert_ne!(old, new);
    assert!(server.sessions[&1].replication.baseline().is_none());
    server.broadcast_snapshots();
    for packet in [
        Packet::Input(InputFrame {
            client_tick: 1,
            ack_server_tick: 30,
            session_token: Some(old),
            ..Default::default()
        }),
        Packet::Resynchronize {
            session: old,
            after_tick: 0,
        },
    ] {
        server.transport.incoming.push(Datagram {
            peer,
            data: packet.encode().unwrap(),
        });
    }
    server.poll_network().unwrap();
    assert_eq!(server.sessions[&1].last_acked_tick, 0);
    assert!(!server.sessions[&1].keyframe_requested);
    server.transport.incoming.push(Datagram {
        peer,
        data: Packet::Input(InputFrame {
            client_tick: 2,
            ack_server_tick: 30,
            session_token: Some(new),
            ..Default::default()
        })
        .encode()
        .unwrap(),
    });
    server.poll_network().unwrap();
    assert_eq!(server.sessions[&1].last_acked_tick, 30);
}

#[test]
fn small_payload_alternates_owner_priority_and_fair_progress() {
    let mut sender = ReplicationSender::for_session([u64::MAX; 2]);
    let wire = Wire::default();
    wire.limit.set(550);
    let mut desired = world(24);
    let mut base = None;
    let mut owners = 0;
    for _ in 0..60 {
        desired.tick += 3;
        desired.players[0].tick = desired.tick;
        sender
            .send(&wire, "127.0.0.1:5".parse().unwrap(), &desired, 1)
            .unwrap();
        deliver(
            &mut sender,
            &mut base,
            &wire.sent.borrow_mut().pop().unwrap(),
        );
        owners += usize::from(
            base.as_ref()
                .unwrap()
                .players
                .iter()
                .any(|p| p.tick == desired.tick),
        );
    }
    assert!(owners >= 30);
    assert_eq!(base.unwrap().props.len(), 24);
}
#[test]
fn invalid_numbers_and_unsupported_removals_fail_explicitly() {
    let mut bad = world(1);
    bad.props[0].position.0 = f32::NAN;
    assert!(validate_world(&bad, 1100)
        .unwrap_err()
        .to_string()
        .contains("non-finite"));
    let mut sender = ReplicationSender::default();
    let wire = Wire::default();
    let peer = "127.0.0.1:5".parse().unwrap();
    let mut desired = world(1);
    desired.props[0].id = "\u{0001}".repeat(80);
    let mut base = None;
    for _ in 0..3 {
        desired.tick += 3;
        sender.send(&wire, peer, &desired, 1).unwrap();
        deliver(
            &mut sender,
            &mut base,
            &wire.sent.borrow_mut().pop().unwrap(),
        );
    }
    desired.props.clear();
    wire.limit.set(500);
    assert!(sender
        .send(&wire, peer, &desired, 1)
        .unwrap_err()
        .to_string()
        .contains("Removal record"));
}

#[test]
fn resync_replaces_stale_initial_keyframe_but_coalesces_duplicates() {
    let mut sender = ReplicationSender::default();
    let wire = Wire::default();
    let peer = "127.0.0.1:5".parse().unwrap();
    let mut desired = world(10);
    sender.send(&wire, peer, &desired, 1).unwrap();
    let stale = wire.sent.borrow_mut().pop().unwrap();
    sender.resynchronize_after(20); // new round supersedes even the initial unacked world
    desired.tick = 21;
    sender.send(&wire, peer, &desired, 1).unwrap();
    let fresh = wire.sent.borrow_mut().pop().unwrap();
    sender.resynchronize_after(20); // duplicate request must not restart the transfer
    desired.tick = 24;
    sender.send(&wire, peer, &desired, 1).unwrap();
    assert_eq!(fresh, wire.sent.borrow_mut().pop().unwrap());
    assert_ne!(stale, fresh);
    assert!(!sender.acknowledge(3));
    assert!(sender.acknowledge(21));
    sender.resynchronize_after(20); // delayed duplicate after successful recovery
    assert_eq!(sender.baseline().unwrap().tick, 21);
}
#[test]
fn world_and_game_lanes_both_progress_with_one_queue_slot() {
    use vesper3d::viewer::{game::LoadedGame, game_example};
    let (document, map) = game_example::documents().unwrap();
    let world = LoadedGame { document, map }.world().unwrap();
    let mut server = DedicatedServer::with_transport(Wire::default(), world).unwrap();
    for id in 1..=2 {
        server.handle_hello(
            format!("127.0.0.1:{}", 5000 + id).parse().unwrap(),
            PROTOCOL_VERSION,
            id,
            server.world.content_hash,
        );
    }
    server.transport.sent.borrow_mut().clear();
    server.transport.capacity.set(1);
    for tick in 1..=40 {
        server.world.tick = tick * 3;
        server.try_broadcast_snapshots().unwrap();
        server.transport.sent.borrow_mut().clear();
    }
    for session in server.sessions.values() {
        assert!(session.replication.counters.accepted_packets > 0);
        assert!(session.game_replication.accepted_packets > 0);
        assert!(session.game_replication.backpressured > 0);
    }
    server.transport.limit.set(100);
    assert!(server
        .run_ticks(3)
        .unwrap_err()
        .to_string()
        .contains("transport"));
    assert!(server.replication_error.is_some());
}

#[test]
fn lost_ack_followed_by_mtu_reduction_recovers_via_explicit_resync() {
    let mut sender = ReplicationSender::default();
    let wire = Wire::default();
    let peer = "127.0.0.1:5".parse().unwrap();
    let mut desired = world(12);
    let mut receiver = None;
    sender.send(&wire, peer, &desired, 1).unwrap();
    let packet = Packet::decode(&wire.sent.borrow_mut().pop().unwrap()).unwrap();
    receive_update(&mut receiver, packet).unwrap(); // ACK deliberately lost
    wire.limit.set(550);
    desired.tick = 6;
    sender.send(&wire, peer, &desired, 1).unwrap();
    // The initial packet is independently applicable, even after rebudgeting.
    deliver(
        &mut sender,
        &mut receiver,
        &wire.sent.borrow_mut().pop().unwrap(),
    );
    desired.tick = 9;
    wire.limit.set(1100);
    sender.send(&wire, peer, &desired, 1).unwrap();
    receive_update(
        &mut receiver,
        Packet::decode(&wire.sent.borrow_mut().pop().unwrap()).unwrap(),
    )
    .unwrap(); // second lost ACK, this time a delta
    desired.tick = 12;
    wire.limit.set(550);
    sender.send(&wire, peer, &desired, 1).unwrap();
    assert!(receive_update(
        &mut receiver,
        Packet::decode(&wire.sent.borrow_mut().pop().unwrap()).unwrap()
    )
    .is_err());
    sender.resynchronize_after(receiver.as_ref().unwrap().tick);
    for _ in 0..20 {
        desired.tick += 3;
        sender.send(&wire, peer, &desired, 1).unwrap();
        deliver(
            &mut sender,
            &mut receiver,
            &wire.sent.borrow_mut().pop().unwrap(),
        );
    }
    assert_eq!(normalized(receiver.unwrap()), normalized(desired));
}

//! Actual workshop rules on the existing NetServer/NetClient path, not the engine toy fixture.
#![cfg(feature = "online")]
use automation_workshop::net::{
    check_snapshot_budget, snapshot_datagram_bytes, NetInput, WorkshopNet, WorkshopSnapshot,
};
use automation_workshop::{Command, Envelope, Workshop};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use vesper3d::runtime::Simulation;
use vesper3d::viewer::net::codec::{Reader, Writer};
use vesper3d::viewer::net::loopback::{LoopEnd, LoopNet};
use vesper3d::viewer::net::{DatagramTransport, UdpTransport};
use vesper3d::viewer::netplay::{
    ClientConfig, ClientState, ConnectFailure, NetClient, NetGame, NetServer, Seat, ServerConfig, Stage,
};

fn address(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}
type Client = NetClient<WorkshopNet, LoopEnd>;
type Server = NetServer<WorkshopNet, LoopEnd>;

struct World {
    network: LoopNet,
    server: Server,
    clients: Vec<Client>,
    held: Vec<NetInput>,
    tick: u64,
    wall: Instant,
    hashes: BTreeMap<u64, u64>,
}

impl World {
    fn new(loss: f32) -> Self {
        let network = LoopNet::new(2, 1, loss, 73);
        let server = NetServer::new(
            network.endpoint(address(4100)),
            ServerConfig {
                participants: 2,
                auto_start_seconds: 0,
                countdown_seconds: 1,
                seed: Some(7),
                session_timeout: Duration::from_secs(6),
                ..Default::default()
            },
        )
        .unwrap();
        let clients = (0..2).map(|seat| Self::client(&network, 4101 + seat)).collect();
        Self {
            network,
            server,
            clients,
            held: vec![NetInput::default(); 2],
            tick: 0,
            wall: Instant::now(),
            hashes: BTreeMap::new(),
        }
    }

    fn client(network: &LoopNet, port: u16) -> Client {
        NetClient::new(
            network.endpoint(address(port)),
            address(4100),
            ClientConfig { name: format!("Workshop {port}"), key: String::new(), choice: 0 },
        )
        .unwrap()
    }

    fn step_with_client_poll(&mut self, poll_clients: bool) {
        self.tick += 1;
        self.network.advance();
        let wall = self.wall + Duration::from_secs_f64(self.tick as f64 / 60.);
        self.server.poll(wall);
        self.server.step(wall);
        if let Some(game) = self.server.current() {
            self.hashes.insert(game.workshop.state.tick, game.workshop.state_hash());
        }
        if poll_clients {
            let now = self.tick as f64 / 60.;
            for (index, client) in self.clients.iter_mut().enumerate() {
                client.poll(now);
                if *client.state() == ClientState::Playing {
                    client.tick(self.held[index]);
                }
                client.frame(now, 1. / 60.);
                if let Some(snapshot) = &client.view().snapshot {
                    assert_eq!(
                        Some(&snapshot.state_hash),
                        self.hashes.get(&snapshot.tick),
                        "client state came from the real workshop authority at tick {}",
                        snapshot.tick
                    );
                }
            }
        }
    }

    fn advance(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.step_with_client_poll(true);
        }
    }

    fn start(&mut self) {
        for _ in 0..300 {
            self.advance(1);
            if self.clients.iter().all(|client| *client.state() == ClientState::Lobby) {
                break;
            }
        }
        assert!(self.clients.iter().all(|client| *client.state() == ClientState::Lobby));
        for client in &mut self.clients {
            client.ready(true);
        }
        for _ in 0..300 {
            self.advance(1);
            if self.clients.iter().all(|client| *client.state() == ClientState::Playing) {
                break;
            }
        }
        assert_eq!(self.server.stage(), Stage::Match);
        assert!(self.clients.iter().all(|client| *client.state() == ClientState::Playing));
    }

    fn workshop(&self) -> &Workshop {
        &self.server.current().unwrap().workshop
    }

    fn command(&mut self, client: usize, serial: u64, command: Command) {
        self.held[client] = NetInput::from_command(serial, &command).unwrap();
        self.advance(45);
        self.held[client] = NetInput::default();
        self.advance(12);
        let seat = self.clients[client].seat();
        assert_eq!(self.workshop().state.last_command.get(&seat), Some(&serial));
    }
}

#[test]
fn online_adapter_uses_exactly_the_offline_rules_and_command_deduplication() {
    let seats = [
        Seat { id: 0, choice: 0, name: "First operator".into() },
        Seat { id: 1, choice: 0, name: "Second operator".into() },
    ];
    let (mut online, _) = WorkshopNet::start(7, &seats, 2);
    let mut offline = Workshop::standard();
    let item = "workshop:ore".to_owned();
    let queue = Command::Queue { machine: "workshop:machine-a".into(), recipe: "workshop:smelt".into() };
    WorkshopNet::step(&mut online, &[Some(NetInput::from_command(1, &queue).unwrap()), None]);
    offline.step(&vec![Envelope { seat: 0, serial: 1, command: queue }]);
    for _ in 0..6 {
        WorkshopNet::step(&mut online, &[None, None]);
        offline.step(&Vec::new());
    }
    assert!(offline.state.machines.values().all(|machine| machine.queue.is_empty()));
    let before = offline.state.inventory.quantity(&item);
    let command = Command::Gather { item: item.clone(), amount: 1 };
    let intention = NetInput::from_command(2, &command).unwrap();
    for tick in 0..90 {
        let inputs = if tick < 30 { vec![Some(intention), None] } else { vec![None, None] };
        WorkshopNet::step(&mut online, &inputs);
        let commands =
            if tick < 30 { vec![Envelope { seat: 0, serial: 2, command: command.clone() }] } else { Vec::new() };
        offline.step(&commands);
        assert_eq!(online.workshop.state_hash(), offline.state_hash(), "same authority after tick {tick}");
    }
    assert_eq!(online.workshop.state.last_command.get(&0), Some(&2));
    assert_eq!(
        online.workshop.state.inventory.quantity(&item),
        before + 1,
        "one commit for thirty repeated intentions"
    );
}

#[test]
fn workshop_production_and_resource_commands_recover_after_packet_interruption() {
    let mut world = World::new(8.);
    world.start();
    let recipe = world
        .workshop()
        .definitions
        .recipes
        .values()
        .find(|recipe| {
            recipe.required_upgrade.is_none()
                && recipe.inputs.iter().all(|(id, amount)| {
                    world.workshop().state.supply.get(id).copied().unwrap_or(0)
                        + world.workshop().state.inventory.counts().get(id).copied().unwrap_or(0)
                        >= *amount
                })
        })
        .unwrap()
        .clone();
    let mut serial = 1;
    for (item, amount) in &recipe.inputs {
        let existing = world.workshop().state.inventory.counts().get(item).copied().unwrap_or(0);
        if existing < *amount {
            world.command(0, serial, Command::Gather { item: item.clone(), amount: amount - existing });
            serial += 1;
        }
    }
    let machine = world.workshop().state.machines.keys().next().unwrap().clone();
    let before_outputs: Vec<_> = recipe
        .outputs
        .iter()
        .map(|(id, _)| world.workshop().state.inventory.counts().get(id).copied().unwrap_or(0))
        .collect();
    world.command(0, serial, Command::Queue { machine, recipe: recipe.id.clone() });
    world.advance(recipe.duration_ticks as u64 + 30);
    for ((id, produced), before) in recipe.outputs.iter().zip(before_outputs) {
        assert_eq!(world.workshop().state.inventory.counts().get(id).copied().unwrap_or(0), before + produced);
    }
    assert!(world.workshop().state.machines.values().all(|machine| machine.queue.is_empty()));
    let before_interruption = world.clients[0].view().snapshot.as_ref().unwrap().tick;
    // Keep the actual server progressing while neither client receives/polls. This is shorter than
    // session timeout, and the recovered full state must come from the same running authority.
    for _ in 0..60 {
        world.step_with_client_poll(false);
    }
    world.advance(90);
    for client in &world.clients {
        assert_eq!(*client.state(), ClientState::Playing);
        assert!(client.view().snapshot.as_ref().unwrap().tick > before_interruption + 60);
        let snapshot = client.view().snapshot.as_ref().unwrap();
        assert_eq!(snapshot.inventory, WorkshopSnapshot::capture(world.workshop()).inventory);
        assert!(snapshot.machines.iter().all(|machine| machine.queue.is_empty()));
    }
    assert!(world.network.counts().1 > 0, "loss actually occurred");
    let send = world.server.send_stats();
    assert!(send.accepted_bytes > 0);
    assert_eq!(send.oversized, 0);
    let simulated_seconds = world.workshop().state.tick as f64 / 60.;
    println!("workshop_netplay: datagram={} accepted_bytes={} accepted_snapshots={} packets_sent={} packets_dropped={} simulated_seconds={simulated_seconds:.3} local_accepted_bytes_per_simulated_second={:.1}; local acceptance is not delivery acknowledgement",
        snapshot_datagram_bytes(world.workshop()), send.accepted_bytes, send.snapshots_accepted,
        world.network.counts().0, world.network.counts().1, send.accepted_bytes as f64 / simulated_seconds);
}

#[test]
fn timeout_reconnect_to_a_running_workshop_is_explicitly_unsupported() {
    let mut world = World::new(0.);
    world.start();
    world.clients.pop();
    world.held.pop();
    world.advance(390);
    assert_eq!(world.server.players(), 1, "the disconnected seat timed out");
    let authority_tick = world.workshop().state.tick;
    // LoopNet retains an inbox after its endpoint is dropped. A newly bound native socket would not
    // inherit the previous socket's buffered packets, so clear that artificial retained inbox first.
    world.network.endpoint(address(4102)).receive().unwrap();
    let mut reconnect = World::client(&world.network, 4102);
    let mut late_join = World::client(&world.network, 4199);
    for _ in 0..90 {
        world.advance(1);
        reconnect.poll(world.tick as f64 / 60.);
        late_join.poll(world.tick as f64 / 60.);
    }
    assert_eq!(reconnect.failure(), Some(ConnectFailure::MatchInProgress));
    assert!(matches!(reconnect.state(), ClientState::Rejected(_)));
    assert_eq!(late_join.failure(), Some(ConnectFailure::MatchInProgress));
    assert_eq!(world.server.stage(), Stage::Match);
    assert!(world.workshop().state.tick > authority_tick, "machines keep their logical clock after a seat leaves");
}

#[test]
fn full_snapshot_codec_preserves_queued_work_and_refuses_malformed_intentions() {
    let mut workshop = Workshop::standard();
    let recipe = workshop
        .definitions
        .recipes
        .values()
        .find(|recipe| {
            recipe.required_upgrade.is_none()
                && recipe.duration_ticks > 1
                && recipe.inputs.iter().all(|(item, amount)| {
                    workshop.state.supply.get(item).copied().unwrap_or(0) + workshop.state.inventory.quantity(item)
                        >= *amount
                })
        })
        .unwrap()
        .clone();
    let machine = workshop.state.machines.keys().next().unwrap().clone();
    let mut serial = 1;
    for (item, amount) in &recipe.inputs {
        let needed = amount.saturating_sub(workshop.state.inventory.quantity(item));
        if needed == 0 {
            continue;
        }
        workshop.step(&vec![Envelope {
            seat: 0,
            serial,
            command: Command::Gather { item: item.clone(), amount: needed },
        }]);
        serial += 1;
    }
    workshop.step(&vec![Envelope { seat: 0, serial, command: Command::Queue { machine, recipe: recipe.id } }]);
    assert!(
        workshop.state.machines.values().any(|machine| !machine.queue.is_empty()),
        "codec fixture really has in-progress work"
    );
    let snapshot = WorkshopSnapshot::capture(&workshop);
    let mut writer = Writer::new();
    WorkshopNet::write_snapshot(&snapshot, &mut writer);
    let bytes = writer.finish();
    let mut reader = Reader::new(&bytes);
    assert_eq!(WorkshopNet::read_snapshot(&mut reader).unwrap(), snapshot);
    reader.done().unwrap();
    for length in 0..bytes.len() {
        assert!(WorkshopNet::read_snapshot(&mut Reader::new(&bytes[..length])).is_err(), "truncated snapshot {length}");
    }
    let mut impossible = snapshot.clone();
    impossible.max_units = 0;
    let mut impossible_writer = Writer::new();
    WorkshopNet::write_snapshot(&impossible, &mut impossible_writer);
    assert!(WorkshopNet::read_snapshot(&mut Reader::new(impossible_writer.as_slice())).is_err());
    let mut invalid_job = snapshot.clone();
    invalid_job.machines.iter_mut().find(|machine| !machine.queue.is_empty()).unwrap().queue[0].speed_divisor = 0;
    let mut invalid_job_writer = Writer::new();
    WorkshopNet::write_snapshot(&invalid_job, &mut invalid_job_writer);
    assert!(WorkshopNet::read_snapshot(&mut Reader::new(invalid_job_writer.as_slice())).is_err());
    let mut invalid = Writer::new();
    invalid.varint(1);
    invalid.u8(255);
    invalid.u16(0);
    invalid.u16(0);
    invalid.varint(0);
    assert!(WorkshopNet::read_input(&mut Reader::new(invalid.as_slice())).is_err());
    assert!(NetInput::from_command(1, &Command::Queue { machine: "missing".into(), recipe: "missing".into() }).is_err());
    assert!(NetInput::from_command(
        0,
        &Command::Gather { item: workshop.definitions.items.keys().next().unwrap().clone(), amount: 1 }
    )
    .is_err());
    assert!(check_snapshot_budget(&workshop, 1_100).is_ok());
    assert!(check_snapshot_budget(&workshop, 32).is_err());
}

#[test]
fn larger_offline_machine_ladder_does_not_claim_online_capacity() {
    for count in [2, 10, 100, 1_000] {
        let workshop = Workshop::with_machine_count(count).unwrap();
        let bytes = snapshot_datagram_bytes(&workshop);
        println!("workshop_snapshot_probe: machines={count} encoded_datagram_bytes={bytes} limit=1100");
        if count == 2 {
            assert!(check_snapshot_budget(&workshop, 1_100).is_ok());
        }
        if bytes > 1_100 {
            assert!(check_snapshot_budget(&workshop, 1_100).unwrap_err().contains("bounded replication"));
            break;
        }
    }
}

#[test]
fn full_queues_blocked_outputs_and_pending_cancellation_fit_without_state_trimming() {
    let mut workshop = Workshop::standard();
    for (serial, machine) in
        [(1, "workshop:machine-a"), (2, "workshop:machine-b"), (3, "workshop:machine-a"), (4, "workshop:machine-b")]
    {
        workshop.step(&vec![Envelope {
            seat: 0,
            serial,
            command: Command::Queue { machine: machine.into(), recipe: "workshop:smelt".into() },
        }]);
    }
    assert_eq!(workshop.state.inventory.total_units(), 0);
    workshop.step(&vec![Envelope {
        seat: 0,
        serial: 5,
        command: Command::Gather { item: "workshop:ore".into(), amount: 12 },
    }]);
    for _ in 0..6 {
        workshop.step(&Vec::new());
    }
    assert!(workshop.state.machines.values().all(|machine| machine.queue.len() == 2 && machine.blocked.is_some()));
    workshop.step(&vec![Envelope {
        seat: 0,
        serial: 6,
        command: Command::Cancel { machine: "workshop:machine-a".into() },
    }]);
    assert!(workshop.state.machines["workshop:machine-a"].queue[0].cancel_pending);
    let snapshot = WorkshopSnapshot::capture(&workshop);
    let mut writer = Writer::new();
    WorkshopNet::write_snapshot(&snapshot, &mut writer);
    assert_eq!(WorkshopNet::read_snapshot(&mut Reader::new(writer.as_slice())).unwrap(), snapshot);
    let bytes = check_snapshot_budget(&workshop, 1_100).unwrap();
    println!("workshop_snapshot_probe: machines=2 queued_jobs=4 blocked_machines=2 cancel_pending=1 encoded_datagram_bytes={bytes} limit=1100; required state retained");
}

#[test]
fn workshop_runs_over_real_native_loopback_udp() {
    let server_transport = UdpTransport::bind("127.0.0.1:0").unwrap();
    let server_address = server_transport.local_addr().unwrap();
    let mut server = NetServer::<WorkshopNet, _>::new(
        server_transport,
        ServerConfig {
            participants: 1,
            countdown_seconds: 1,
            auto_start_seconds: 0,
            seed: Some(7),
            ..Default::default()
        },
    )
    .unwrap();
    let mut client = NetClient::<WorkshopNet, _>::new(
        UdpTransport::bind("127.0.0.1:0").unwrap(),
        server_address,
        ClientConfig { name: "UDP workshop".into(), key: String::new(), choice: 0 },
    )
    .unwrap();
    let standard = Workshop::standard();
    let item = "workshop:ore".to_owned();
    let before = standard.state.inventory.quantity(&item);
    let before_supply = standard.state.supply[&item];
    let queue = NetInput::from_command(
        1,
        &Command::Queue { machine: "workshop:machine-a".into(), recipe: "workshop:smelt".into() },
    )
    .unwrap();
    let gather = NetInput::from_command(2, &Command::Gather { item: item.clone(), amount: 1 }).unwrap();
    let mut ready = false;
    for tick in 0..300 {
        server.poll(Instant::now());
        server.step(Instant::now());
        client.poll(tick as f64 / 60.);
        if *client.state() == ClientState::Lobby && !ready {
            client.ready(true);
            ready = true;
        }
        if *client.state() == ClientState::Playing {
            let completed = client.view().snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.last_command.iter().any(|(seat, serial)| *seat == client.seat() && *serial >= 1)
                    && snapshot.machines.iter().all(|machine| machine.queue.is_empty())
            });
            client.tick(if completed { gather } else { queue });
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(*client.state(), ClientState::Playing);
    let authoritative = &server.current().unwrap().workshop;
    assert_eq!(authoritative.state.inventory.quantity(&item), before - 1);
    assert_eq!(authoritative.state.supply[&item], before_supply - 1);
    assert_eq!(authoritative.state.inventory.quantity("workshop:plate"), 2);
    let replica = client.view().snapshot.as_ref().unwrap();
    assert_eq!(replica.inventory.iter().find(|(id, _)| id == &item).unwrap().1, before - 1);
    assert_eq!(replica.inventory.iter().find(|(id, _)| id == "workshop:plate").unwrap().1, 2);
    assert!(server.send_stats().snapshots_accepted > 0);
}

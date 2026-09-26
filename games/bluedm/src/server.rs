use crate::{content_path, protocol::*};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use vesper3d::{
    math::{Ray, V},
    viewer::{
        authoring::MapDocument,
        fps::{
            armory, AimState, OperativeModel, TeamDeathmatch, TeamDeathmatchConfig, WeaponState,
        },
        net::{session::random_token, transport::DatagramTransport, UdpTransport},
        simulation::{HeadlessWorld, TICK_SECONDS},
    },
};

struct ConnectedPlayer {
    peer: SocketAddr,
    token: [u8; 16],
    last_seen: Instant,
    input: ClientInput,
    last_fire_counter: u32,
    last_reload_counter: u32,
    aim: AimState,
    weapons: Vec<WeaponState>,
}

pub struct Server {
    transport: UdpTransport,
    key: String,
    world: HeadlessWorld,
    match_state: TeamDeathmatch,
    players: BTreeMap<u64, ConnectedPlayer>,
    next_player_id: u64,
}

impl Server {
    pub fn bind(address: &str, key: String) -> vesper3d::Result<Self> {
        let map = MapDocument::load(&content_path())?;
        let world = HeadlessWorld::try_with_room(map.build()?)?;
        let match_state = TeamDeathmatch::new(
            TeamDeathmatchConfig::default(),
            vec![(V(-20.0, 0.0, -3.6), 1.57), (V(-20.0, 0.0, 3.6), 1.57)],
            vec![(V(22.0, 0.0, -3.6), -1.57), (V(22.0, 0.0, 3.6), -1.57)],
        )?;
        Ok(Self {
            transport: UdpTransport::bind(address)?,
            key,
            world,
            match_state,
            players: BTreeMap::new(),
            next_player_id: 1,
        })
    }

    pub fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        self.transport.local_addr()
    }

    pub fn run(&mut self, stop: Arc<AtomicBool>, max_ticks: Option<u64>) -> vesper3d::Result<()> {
        println!(
            "BlueDM server listening on {}",
            self.transport.local_addr()?
        );
        let tick_duration = Duration::from_secs_f64(TICK_SECONDS as f64);
        let mut next_tick = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            next_tick += tick_duration;
            self.receive()?;
            self.step();
            if self.world.tick.is_multiple_of(3) {
                self.broadcast();
            }
            if max_ticks.is_some_and(|limit| self.world.tick >= limit) {
                break;
            }
            let now = Instant::now();
            if next_tick > now {
                std::thread::sleep(next_tick - now);
            } else if now.duration_since(next_tick) > tick_duration * 8 {
                next_tick = now;
            }
        }
        Ok(())
    }

    fn receive(&mut self) -> vesper3d::Result<()> {
        let now = Instant::now();
        for datagram in self.transport.receive()? {
            let Some(message) = WireMessage::decode(&datagram.data) else {
                continue;
            };
            match message {
                WireMessage::Join {
                    version,
                    key,
                    name: _,
                } => {
                    if version != PROTOCOL_VERSION || key != self.key {
                        let message = WireMessage::Reject {
                            reason: "Protocol mismatch or incorrect join key".into(),
                        };
                        let _ = self.transport.send(datagram.peer, &message.encode()?);
                        continue;
                    }
                    if let Some((&id, player)) = self
                        .players
                        .iter()
                        .find(|(_, player)| player.peer == datagram.peer)
                    {
                        let welcome = WireMessage::Welcome {
                            player_id: id,
                            token: player.token,
                        };
                        let _ = self.transport.send(datagram.peer, &welcome.encode()?);
                        continue;
                    }
                    if self.players.len() >= 8 {
                        let message = WireMessage::Reject {
                            reason: "Server is full".into(),
                        };
                        let _ = self.transport.send(datagram.peer, &message.encode()?);
                        continue;
                    }
                    let id = self.next_player_id;
                    self.next_player_id += 1;
                    let random = random_token()?;
                    let mut token = [0u8; 16];
                    token[..8].copy_from_slice(&random[0].to_le_bytes());
                    token[8..].copy_from_slice(&random[1].to_le_bytes());
                    let model = match id % 4 {
                        0 => OperativeModel::Vanguard,
                        1 => OperativeModel::Recon,
                        2 => OperativeModel::Breacher,
                        _ => OperativeModel::FieldTech,
                    };
                    let spawn = self
                        .match_state
                        .add_player(id, model)
                        .expect("new player spawn");
                    self.world.join_at(id, spawn.position);
                    if let Some(controller) = self.world.player_mut(id) {
                        controller.set_physics_state(spawn.position + V(0.0, 1.68, 0.0), 0.0, true);
                        controller.yaw = spawn.yaw;
                    }
                    let catalog = armory();
                    self.players.insert(
                        id,
                        ConnectedPlayer {
                            peer: datagram.peer,
                            token,
                            last_seen: now,
                            input: ClientInput {
                                sequence: 0,
                                movement: Default::default(),
                                yaw: spawn.yaw,
                                pitch: 0.0,
                                weapon_slot: 4,
                                fire_counter: 0,
                                fire_held: false,
                                reload_counter: 0,
                                ads: false,
                            },
                            last_fire_counter: 0,
                            last_reload_counter: 0,
                            aim: AimState::default(),
                            weapons: catalog.weapons.iter().map(WeaponState::new).collect(),
                        },
                    );
                    let welcome = WireMessage::Welcome {
                        player_id: id,
                        token,
                    };
                    self.transport.send(datagram.peer, &welcome.encode()?)?;
                    println!("player {id} joined from {}", datagram.peer);
                }
                WireMessage::Input { token, input } if input.valid() => {
                    if let Some((&id, player)) = self
                        .players
                        .iter_mut()
                        .find(|(_, player)| player.token == token && player.peer == datagram.peer)
                    {
                        if input.sequence > player.input.sequence {
                            player.last_seen = now;
                            player.input = input;
                            let _ = self.world.input(
                                id,
                                player.input.movement,
                                player.input.yaw,
                                player.input.pitch,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        let expired: Vec<_> = self
            .players
            .iter()
            .filter_map(|(&id, player)| {
                (now.duration_since(player.last_seen) > Duration::from_secs(10)).then_some(id)
            })
            .collect();
        for id in expired {
            self.players.remove(&id);
            self.world.leave(id);
            self.match_state.remove_player(id);
            println!("player {id} timed out");
        }
        Ok(())
    }

    fn step(&mut self) {
        self.world.step();
        let catalog = armory();
        let mut attacks = Vec::new();
        for (&id, player) in &mut self.players {
            let Some(combatant) = self.match_state.players.get(&id) else {
                continue;
            };
            if !combatant.alive() {
                self.world.neutralize_input(id);
                continue;
            }
            let slot = usize::from(player.input.weapon_slot).min(catalog.weapons.len() - 1);
            let definition = &catalog.weapons[slot];
            let fire_pressed = player.input.fire_counter != player.last_fire_counter;
            let reload_pressed = player.input.reload_counter != player.last_reload_counter;
            player.last_fire_counter = player.input.fire_counter;
            player.last_reload_counter = player.input.reload_counter;
            player
                .aim
                .update(player.input.ads, TICK_SECONDS, definition);
            let shots = player.weapons[slot].tick(
                definition,
                fire_pressed,
                player.input.fire_held,
                reload_pressed,
                player.aim.amount(),
            );
            if !shots.is_empty() {
                if let Some(controller) = self.world.player(id) {
                    attacks.push((
                        id,
                        controller.position,
                        player.input.yaw,
                        player.input.pitch,
                        slot,
                        shots,
                    ));
                }
            }
        }
        for (attacker, origin, yaw, pitch, slot, shots) in attacks {
            let definition = &catalog.weapons[slot];
            for shot in shots {
                let ray = Ray {
                    o: origin,
                    d: shot.direction(yaw, pitch),
                };
                let wall_distance = self
                    .world
                    .room
                    .world
                    .hit(ray, definition.range, false)
                    .map_or(definition.range, |hit| hit.t);
                if let Some((victim, headshot, _distance)) =
                    self.closest_target(attacker, ray, wall_distance)
                {
                    let damage = definition.damage
                        * if headshot {
                            definition.headshot_multiplier
                        } else {
                            1.0
                        };
                    if self
                        .match_state
                        .apply_damage(attacker, victim, damage)
                        .killed
                    {
                        if let Some(controller) = self.world.player_mut(victim) {
                            controller.set_physics_state(V(0.0, -20.0, 0.0), 0.0, true);
                        }
                    }
                }
            }
        }
        for event in self.match_state.step() {
            if let Some(controller) = self.world.player_mut(event.player_id) {
                controller.set_physics_state(event.position + V(0.0, 1.68, 0.0), 0.0, true);
                controller.yaw = event.yaw;
                controller.pitch = 0.0;
            }
        }
    }

    fn closest_target(
        &self,
        attacker: u64,
        ray: Ray,
        max_distance: f32,
    ) -> Option<(u64, bool, f32)> {
        let mut best = None;
        for (&id, combatant) in &self.match_state.players {
            if id == attacker || !combatant.alive() {
                continue;
            }
            let Some(controller) = self.world.player(id) else {
                continue;
            };
            let head = controller.position;
            let chest = head - V(0.0, 0.58, 0.0);
            for (point, radius, headshot) in [(head, 0.22, true), (chest, 0.48, false)] {
                let along = (point - ray.o).dot(ray.d);
                if along <= 0.0 || along >= max_distance || best.is_some_and(|(_, _, d)| along >= d)
                {
                    continue;
                }
                let offset = point - (ray.o + ray.d * along);
                if offset.dot(offset) <= radius * radius {
                    best = Some((id, headshot, along));
                }
            }
        }
        best
    }

    fn broadcast(&mut self) {
        let catalog = armory();
        let winner = self.match_state.winner();
        let players = self
            .match_state
            .players
            .iter()
            .filter_map(|(&id, combatant)| {
                let controller = self.world.player(id)?;
                let connected = self.players.get(&id)?;
                let slot = usize::from(connected.input.weapon_slot).min(catalog.weapons.len() - 1);
                let weapon = &connected.weapons[slot];
                Some(PlayerSnapshot {
                    id,
                    position: controller.position,
                    yaw: controller.yaw,
                    pitch: controller.pitch,
                    team: combatant.team,
                    model: combatant.model,
                    health: combatant.health,
                    kills: combatant.kills,
                    deaths: combatant.deaths,
                    weapon_slot: slot as u8,
                    magazine: weapon.magazine,
                    reserve: weapon.reserve,
                    reloading: weapon.is_reloading(),
                })
            })
            .collect();
        let state = MatchSnapshot {
            tick: self.world.tick,
            team_scores: self.match_state.team_scores,
            score_limit: self.match_state.config.score_limit,
            winner,
            players,
        };
        for player in self.players.values() {
            if let Ok(bytes) = (WireMessage::Snapshot {
                token: player.token,
                state: state.clone(),
            })
            .encode()
            {
                let _ = self.transport.send(player.peer, &bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_constructs_with_valid_foundry_and_armory() {
        let server = Server::bind("127.0.0.1:0", "test".into()).unwrap();
        assert_eq!(server.match_state.team_scores, [0, 0]);
        assert_eq!(armory().weapons.len(), 10);
    }
}

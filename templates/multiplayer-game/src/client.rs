//! Client-side network loop, input collection, prediction, and reconciliation.
use super::{
    game::{GameAction, GameInput, MatchPhase, MatchSnapshot},
    lobby::{WireMessage, PROTOCOL_VERSION},
    menu::{AppScreen, MenuUi},
};
use macroquad::prelude::*;
use std::{
    collections::VecDeque,
    net::SocketAddr,
    time::{Duration, Instant},
};
use vesper3d::viewer::{
    controller::{CharacterKind, Controller, Movement},
    input::{axes, capture, foreground, Keys},
    net::{
        action_counters::ActionCounters,
        session::{random_token, SessionToken},
        transport::DatagramTransport,
        UdpTransport,
    },
};

pub struct GameClient {
    pub ui: MenuUi,
    keys: Keys,
    fullscreen: bool,
    controller: Controller,
    transport: Option<UdpTransport>,
    server_addr: Option<SocketAddr>,
    nonce: SessionToken,
    session_token: Option<SessionToken>,
    slot: Option<usize>,
    client_tick: u64,
    last_server_tick: u64,
    snapshot: Option<MatchSnapshot>,
    prediction_history: VecDeque<(u64, GameInput, Controller)>,
    counters: ActionCounters,
    pending_commands: VecDeque<(u64, GameAction)>,
    command_sequence: u64,
    last_send: Instant,
    last_receive: Instant,
}

impl GameClient {
    pub fn new() -> vesper3d::Result<Self> {
        let nonce = random_token()?;
        Ok(Self {
            ui: MenuUi::new(),
            keys: Keys::new(),
            fullscreen: false,
            controller: Controller::for_character(CharacterKind::Scientist),
            transport: None,
            server_addr: None,
            nonce,
            session_token: None,
            slot: None,
            client_tick: 0,
            last_server_tick: 0,
            snapshot: None,
            prediction_history: VecDeque::with_capacity(128),
            counters: ActionCounters::new(),
            pending_commands: VecDeque::new(),
            command_sequence: 0,
            last_send: Instant::now(),
            last_receive: Instant::now(),
        })
    }

    pub fn connect(&mut self, addr: SocketAddr, key: String) -> vesper3d::Result<()> {
        let transport = UdpTransport::bind("0.0.0.0:0")?;
        self.server_addr = Some(addr);
        self.transport = Some(transport);
        self.ui.join_key = key.clone();
        self.session_token = None;
        self.slot = None;
        self.client_tick = 0;
        self.pending_commands.clear();
        self.prediction_history.clear();

        // Send initial Hello
        let hello = WireMessage::Hello {
            version: PROTOCOL_VERSION,
            nonce: self.nonce,
            key,
        };
        let bytes = hello.encode()?;
        if let (Some(ref t), Some(dest)) = (&self.transport, self.server_addr) {
            t.send(dest, &bytes)?;
        }
        self.last_send = Instant::now();
        self.last_receive = Instant::now();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn send_command(&mut self, action: GameAction) {
        if self.pending_commands.len() < 8 {
            self.command_sequence += 1;
            self.pending_commands.push_back((self.command_sequence, action));
        }
    }

    pub fn update(&mut self) -> vesper3d::Result<bool> {
        let focused = foreground();
        self.keys.poll(focused);
        if focused && (self.keys.pressed(KeyCode::F) || self.keys.pressed(KeyCode::F11)) {
            self.fullscreen = !self.fullscreen;
            set_fullscreen(self.fullscreen);
        }

        // Receive network packets
        let dgrams = if let Some(ref mut t) = self.transport {
            t.receive()?
        } else {
            Vec::new()
        };

        if let Some(expected_server) = self.server_addr {
            for d in dgrams {
                if d.peer != expected_server {
                    continue;
                }
                if let Some(msg) = WireMessage::decode(&d.data) {
                    self.last_receive = Instant::now();
                    match msg {
                        WireMessage::Welcome { nonce, token, slot } if nonce == self.nonce => {
                            self.session_token = Some(token);
                            self.slot = Some(slot);
                            self.ui.screen = AppScreen::InLobby;
                            self.ui.status_message = None;
                        }
                        WireMessage::Reject { nonce, reason } if nonce == self.nonce => {
                            self.ui.status_message = Some(reason);
                            self.ui.screen = AppScreen::ConnectServer;
                        }
                        WireMessage::Snapshot {
                            token,
                            command_ack,
                            state,
                        } if Some(token) == self.session_token => {
                            // Acknowledge reliable commands
                            self.pending_commands.retain(|(seq, _)| *seq > command_ack);

                            // Server reconciliation for local player
                            if let Some(my_slot) = self.slot {
                                if let Some(my_snap) = state.players.iter().find(|p| p.slot == my_slot) {
                                    self.reconcile(my_snap.ack_sequence, &my_snap.pose);
                                }
                            }

                            if state.phase == MatchPhase::Playing && self.ui.screen != AppScreen::Playing && self.ui.screen != AppScreen::Paused {
                                self.ui.screen = AppScreen::Playing;
                                capture(true);
                            } else if state.phase == MatchPhase::Finished && self.ui.screen != AppScreen::Results {
                                self.ui.screen = AppScreen::Results;
                                capture(false);
                            }

                            self.last_server_tick = state.tick;
                            self.snapshot = Some(state);
                        }
                        _ => {}
                    }
                }
            }

            // Periodic resend of unacknowledged command or hello
            if self.last_send.elapsed() >= Duration::from_millis(250) {
                self.last_send = Instant::now();
                if let Some(ref t) = self.transport {
                    if let Some(token) = self.session_token {
                        if let Some((seq, ref cmd)) = self.pending_commands.front() {
                            let pkt = WireMessage::Command {
                                token,
                                sequence: *seq,
                                action: cmd.clone(),
                            };
                            let bytes = pkt.encode()?;
                            t.send(expected_server, &bytes)?;
                        }
                    } else if self.ui.screen == AppScreen::ConnectServer || self.ui.screen == AppScreen::InLobby {
                        let hello = WireMessage::Hello {
                            version: PROTOCOL_VERSION,
                            nonce: self.nonce,
                            key: self.ui.join_key.clone(),
                        };
                        let bytes = hello.encode()?;
                        t.send(expected_server, &bytes)?;
                    }
                }
            }
        }

        // Handle in-game movement & input
        if self.ui.screen == AppScreen::Playing && focused {
            let (fwd, right) = axes(&self.keys.down);
            let jump = self.keys.pressed(KeyCode::Space);
            let primary = is_mouse_button_pressed(MouseButton::Left);
            let secondary = is_mouse_button_pressed(MouseButton::Right);
            let interact = self.keys.pressed(KeyCode::E);

            if jump { self.counters.jump += 1; }
            if primary { self.counters.primary += 1; }
            if secondary { self.counters.secondary += 1; }
            if interact { self.counters.interact += 1; }

            let m_delta = mouse_delta_position();
            let mdx = m_delta.x;
            let mdy = m_delta.y;
            self.controller.yaw -= mdx * 2.5;
            self.controller.pitch = (self.controller.pitch - mdy * 2.5).clamp(-1.5, 1.5);

            let movement = Movement {
                forward: fwd,
                right,
                jump,
                crouch: self.keys.down.contains(&KeyCode::LeftControl),
                sprint: self.keys.down.contains(&KeyCode::LeftShift),
            };

            self.client_tick += 1;
            let input = GameInput {
                sequence: self.client_tick,
                round: self.snapshot.as_ref().map_or(0, |s| s.round),
                movement,
                yaw: self.controller.yaw,
                pitch: self.controller.pitch,
                aim_tick: self.last_server_tick,
                counters: self.counters,
            };

            // Client prediction step
            let colliders = vec![];
            self.controller.update(movement, 1.0 / 60.0, &colliders);
            self.prediction_history.push_back((self.client_tick, input.clone(), self.controller.clone()));
            while self.prediction_history.len() > 120 {
                self.prediction_history.pop_front();
            }

            // Transmit input
            if let (Some(ref t), Some(dest), Some(token)) = (&self.transport, self.server_addr, self.session_token) {
                let pkt = WireMessage::Input { token, input };
                if let Ok(bytes) = pkt.encode() {
                    let _ = t.send(dest, &bytes);
                }
            }

            if self.keys.pressed(KeyCode::Escape) {
                self.ui.screen = AppScreen::Paused;
                capture(false);
            }
        }

        // Draw UI
        let quit = self.ui.draw();
        Ok(quit)
    }

    /// Server reconciliation: restore authoritative server pose and replay unacknowledged prediction history.
    fn reconcile(&mut self, ack_seq: u64, server_pose: &vesper3d::viewer::controller::ControllerState) {
        // Discard acknowledged inputs
        self.prediction_history.retain(|(seq, _, _)| *seq > ack_seq);

        // Apply authoritative server state
        self.controller.restore_network_state(server_pose);

        // Replay remaining predicted inputs
        let colliders = vec![];
        for (_, input, ref mut snap_controller) in &mut self.prediction_history {
            self.controller.yaw = input.yaw;
            self.controller.pitch = input.pitch;
            self.controller.update(input.movement, 1.0 / 60.0, &colliders);
            *snap_controller = self.controller.clone();
        }
    }
}

//! The compact binary form of world updates: exact, small, and safe against hostile bytes.
use vesper3d::{
    math::V,
    viewer::{
        controller::CharacterKind,
        lifecycle::Generation,
        net::{
            worldwire::{self, MARK},
            DeltaSnapshot, PlayerNetState, PropNetState, WorldSnapshot,
        },
        spatial::RoomId,
    },
};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    /// Mostly ordinary values, sometimes the awkward ones.
    fn float(&mut self) -> f32 {
        match self.next() % 12 {
            0 => 0.0,
            1 => -0.0,
            2 => f32::MIN_POSITIVE / 4.0, // subnormal
            3 => f32::MAX,
            4 => -f32::MAX,
            5 => f32::EPSILON,
            _ => (self.next() % 2_000_000) as f32 / 1000.0 - 1000.0,
        }
    }
    fn tick(&mut self) -> u64 {
        match self.next() % 6 {
            0 => 0,
            1 => u64::MAX,
            2 => u64::MAX - self.next() % 3,
            _ => self.next() % 1_000_000,
        }
    }
    fn vec(&mut self) -> V {
        V(self.float(), self.float(), self.float())
    }
    fn player(&mut self, near: u64) -> PlayerNetState {
        PlayerNetState {
            id: if self.chance(20) {
                self.next()
            } else {
                self.next() % 300
            },
            tick: if self.chance(80) {
                near.wrapping_sub(self.next() % 5)
            } else {
                self.tick()
            },
            position: self.vec(),
            yaw: self.float(),
            pitch: if self.chance(50) { 0.0 } else { self.float() },
            vertical_vel: if self.chance(60) { 0.0 } else { self.float() },
            grounded: self.chance(50),
            crouched: self.chance(20),
            character_kind: if self.chance(30) {
                CharacterKind::Feta
            } else {
                CharacterKind::Scientist
            },
            room_id: self.chance(60).then(|| RoomId((self.next() % 40) as u32)),
        }
    }
    fn prop(&mut self) -> PropNetState {
        let id: String = match self.next() % 4 {
            0 => String::new(),
            1 => "snowman-\u{2603}-\u{1F600}".into(),
            2 => "x".repeat(60),
            _ => format!("prop-{}", self.next() % 5000),
        };
        PropNetState {
            id,
            position: self.vec(),
            rotation: [self.float(), self.float(), self.float(), self.float()],
            linear_velocity: if self.chance(50) {
                V(0., 0., 0.)
            } else {
                self.vec()
            },
            angular_velocity: if self.chance(50) {
                V(0., 0., 0.)
            } else {
                self.vec()
            },
            sleeping: self.chance(50),
            held_by: self.chance(20).then(|| self.next()),
            generation: Generation(if self.chance(70) { 0 } else { self.next() }),
        }
    }
    fn session(&mut self) -> Option<[u64; 2]> {
        self.chance(70).then(|| [self.next(), self.next()])
    }
    fn snapshot(&mut self) -> WorldSnapshot {
        let tick = self.tick();
        WorldSnapshot {
            session: self.session(),
            tick,
            ack_client_tick: self.tick(),
            players: (0..self.next() % 9).map(|_| self.player(tick)).collect(),
            props: (0..self.next() % 2).map(|_| self.prop()).collect(),
        }
    }
    fn delta(&mut self) -> DeltaSnapshot {
        let target = self.tick();
        DeltaSnapshot {
            session: self.session(),
            base_tick: self.tick(),
            target_tick: target,
            ack_client_tick: self.tick(),
            changed_players: (0..self.next() % 9).map(|_| self.player(target)).collect(),
            changed_props: (0..self.next() % 2).map(|_| self.prop()).collect(),
            removed_players: (0..self.next() % 3).map(|_| self.next()).collect(),
            removed_props: (0..self.next() % 2).map(|_| self.prop().id).collect(),
        }
    }
}

/// Floats compare by bit pattern, so `-0.0` and subnormals are held to exactness too.
fn bits(s: &WorldSnapshot) -> String {
    format!("{:?}", s).replace("NaN", "?")
}

#[test]
fn snapshots_round_trip_bit_for_bit() {
    let mut rng = Rng(0x5EED_1234_ABCD);
    for _ in 0..3000 {
        let s = rng.snapshot();
        let bytes = worldwire::encode_snapshot(&s);
        assert_eq!(bytes[0], MARK);
        match worldwire::decode(&bytes).unwrap() {
            vesper3d::viewer::net::Packet::Snapshot(back) => {
                assert_eq!(back, s);
                // `==` treats -0.0 and 0.0 as equal; the Debug text does not.
                assert_eq!(bits(&back), bits(&s));
            }
            other => panic!("decoded a {other:?}"),
        }
    }
}

#[test]
fn deltas_round_trip_bit_for_bit() {
    let mut rng = Rng(0x000D_E17A_5EED);
    for _ in 0..3000 {
        let d = rng.delta();
        let bytes = worldwire::encode_delta(&d);
        match worldwire::decode(&bytes).unwrap() {
            vesper3d::viewer::net::Packet::Delta(back) => {
                assert_eq!(back, d);
                assert_eq!(format!("{back:?}"), format!("{d:?}"));
            }
            other => panic!("decoded a {other:?}"),
        }
    }
}

#[test]
fn sizes_reported_without_encoding_match_the_bytes_written() {
    let mut rng = Rng(0x51_2E5);
    for _ in 0..500 {
        let (s, d) = (rng.snapshot(), rng.delta());
        assert_eq!(
            worldwire::snapshot_len(&s),
            worldwire::encode_snapshot(&s).len()
        );
        assert_eq!(worldwire::delta_len(&d), worldwire::encode_delta(&d).len());
        for p in &d.changed_players {
            let mut alone = d.clone();
            alone.changed_players = vec![p.clone()];
            alone.changed_props.clear();
            alone.removed_players.clear();
            alone.removed_props.clear();
            let empty = DeltaSnapshot {
                changed_players: vec![],
                ..alone.clone()
            };
            assert_eq!(
                worldwire::delta_len(&alone) - worldwire::delta_len(&empty),
                worldwire::player_len(p, d.target_tick),
                "one player adds exactly its own length"
            );
        }
        for p in &d.changed_props {
            let mut alone = DeltaSnapshot {
                changed_players: vec![],
                changed_props: vec![p.clone()],
                removed_players: vec![],
                removed_props: vec![],
                ..d.clone()
            };
            let with = worldwire::delta_len(&alone);
            alone.changed_props.clear();
            assert_eq!(with - worldwire::delta_len(&alone), worldwire::prop_len(p));
        }
    }
}

fn ordinary_player(id: u64) -> PlayerNetState {
    PlayerNetState {
        id,
        tick: 1_000_001,
        position: V(12.345, 1.68, -7.891),
        yaw: 2.75,
        pitch: 0.0,
        vertical_vel: 0.0,
        grounded: true,
        crouched: false,
        character_kind: CharacterKind::Scientist,
        room_id: Some(RoomId(1)),
    }
}

#[test]
fn a_typical_player_is_under_an_eighth_of_its_json_size_and_a_packet_holds_dozens() {
    let p = ordinary_player(3);
    let binary = worldwire::player_len(&p, 1_000_003);
    let json = serde_json::to_vec(&p).unwrap().len();
    assert!(binary <= 26, "a grounded walking player is {binary} bytes");
    assert!(
        json >= 4 * binary,
        "{json} bytes of JSON against {binary} binary"
    );
    // A whole packet's worth: how many fit in the 1,100-byte budget, against the JSON count.
    let envelope = worldwire::delta_len(&DeltaSnapshot {
        session: Some([u64::MAX; 2]),
        base_tick: u64::MAX,
        target_tick: u64::MAX,
        ack_client_tick: u64::MAX,
        changed_players: vec![],
        changed_props: vec![],
        removed_players: vec![],
        removed_props: vec![],
    });
    let binary_fit = (1100 - envelope) / binary;
    let json_fit = 1100 / (json + 1);
    assert!(
        binary_fit >= 4 * json_fit,
        "{binary_fit} records per packet against {json_fit} as JSON"
    );
}

#[test]
fn every_truncation_of_a_valid_update_is_refused() {
    let mut rng = Rng(0x7A_C1);
    for _ in 0..40 {
        for bytes in [
            worldwire::encode_snapshot(&rng.snapshot()),
            worldwire::encode_delta(&rng.delta()),
        ] {
            assert!(worldwire::decode(&bytes).is_ok());
            for cut in 0..bytes.len() {
                assert!(
                    worldwire::decode(&bytes[..cut]).is_err(),
                    "a {cut}-byte prefix of {} bytes decoded",
                    bytes.len()
                );
            }
        }
    }
}

#[test]
fn trailing_bytes_and_unknown_kinds_are_refused() {
    let mut rng = Rng(0x7AA1);
    let mut bytes = worldwire::encode_delta(&rng.delta());
    bytes.push(0);
    assert!(worldwire::decode(&bytes).is_err(), "trailing byte");
    let mut bytes = worldwire::encode_snapshot(&rng.snapshot());
    bytes[1] = 9;
    assert!(worldwire::decode(&bytes).is_err(), "unknown kind");
    assert!(worldwire::decode(&[]).is_err() && worldwire::decode(&[MARK]).is_err());
    assert!(
        worldwire::decode(b"{\"Ping\":{}}").is_err(),
        "JSON is not a binary update"
    );
}

/// A one-player snapshot whose record tick is 2 behind the packet, so the record starts at a known byte:
/// MARK kind session tick(3 bytes) ack count | id offset flags | x ...
fn one_player_snapshot() -> (Vec<u8>, usize) {
    let mut p = ordinary_player(1);
    p.tick = 1_000_001;
    let snapshot = WorldSnapshot {
        tick: 1_000_003,
        players: vec![p],
        ..WorldSnapshot::default()
    };
    let bytes = worldwire::encode_snapshot(&snapshot);
    assert!(
        worldwire::decode(&bytes).is_ok(),
        "the unmodified packet must decode"
    );
    let header = 2 + 1 + 3 + 1 + 1; // MARK kind session tick ack count
    assert_eq!(bytes[header], 1, "player id");
    assert_eq!(bytes[header + 1], 4, "tick offset 2, zigzag 4");
    (bytes, header + 2) // index of the flags byte
}

#[test]
fn reserved_bits_and_bad_flags_are_refused() {
    let (bytes, flags) = one_player_snapshot();
    let mut bad_session = bytes.clone();
    bad_session[2] = 2;
    assert!(
        worldwire::decode(&bad_session).is_err(),
        "session flag must be 0 or 1"
    );
    for reserved in [0x40u8, 0x80] {
        let mut b = bytes.clone();
        b[flags] |= reserved;
        assert!(
            worldwire::decode(&b).is_err(),
            "reserved player flag {reserved:#x}"
        );
    }
}

#[test]
fn counts_larger_than_the_datagram_or_the_entity_cap_are_refused_without_allocating() {
    // MARK, snapshot, no session, tick 0, ack 0, then a player count of 1,000,000 and nothing after it.
    let mut w = vesper3d::viewer::net::codec::Writer::new();
    w.u8(MARK);
    w.u8(1);
    w.u8(0);
    w.varint(0);
    w.varint(0);
    w.varint(1_000_000);
    assert!(worldwire::decode(w.as_slice()).is_err());
    // A count the entity cap allows but the remaining bytes cannot hold.
    let mut w = vesper3d::viewer::net::codec::Writer::new();
    w.u8(MARK);
    w.u8(1);
    w.u8(0);
    w.varint(0);
    w.varint(0);
    w.varint(1000);
    assert!(worldwire::decode(w.as_slice()).is_err());
}

#[test]
fn non_finite_numbers_on_the_wire_are_refused() {
    let (bytes, flags) = one_player_snapshot();
    let x = flags + 1;
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut b = bytes.clone();
        b[x..x + 4].copy_from_slice(&bad.to_le_bytes());
        assert!(worldwire::decode(&b).is_err(), "{bad} position");
    }
}

#[test]
fn a_long_prop_id_survives_and_one_past_the_limit_does_not() {
    let mut rng = Rng(9);
    let mut prop = rng.prop();
    prop.id = "y".repeat(255);
    let snapshot = WorldSnapshot {
        props: vec![prop.clone()],
        ..WorldSnapshot::default()
    };
    let bytes = worldwire::encode_snapshot(&snapshot);
    assert!(bytes.len() <= 1100);
    assert!(worldwire::decode(&bytes).is_ok());
    // The writer never emits a longer id, but a hostile sender can claim one.
    let mut claimed = bytes.clone();
    let id_at = 2 + 1 + 1 + 1 + 1 + 1; // MARK kind session tick ack players(0) | props count
    claimed[id_at + 1] = 0xFF;
    claimed.insert(id_at + 2, 0x7F); // length 0x3FFF, far past the cap
    assert!(worldwire::decode(&claimed).is_err());
}

#[test]
fn hostile_bytes_never_panic() {
    let mut rng = Rng(0xBAD_F00D);
    // Pure noise behind the marker.
    for _ in 0..20_000 {
        let len = (rng.next() % 64) as usize;
        let mut bytes = vec![MARK, (rng.next() % 4) as u8];
        bytes.extend((0..len).map(|_| rng.next() as u8));
        let _ = worldwire::decode(&bytes);
    }
    // Valid updates with bytes flipped, inserted and removed.
    for _ in 0..4_000 {
        let mut bytes = if rng.chance(50) {
            worldwire::encode_snapshot(&rng.snapshot())
        } else {
            worldwire::encode_delta(&rng.delta())
        };
        for _ in 0..1 + rng.next() % 3 {
            let at = (rng.next() as usize) % bytes.len();
            match rng.next() % 3 {
                0 => bytes[at] ^= 1 << (rng.next() % 8),
                1 => bytes.insert(at, rng.next() as u8),
                _ => {
                    bytes.remove(at);
                }
            }
            if bytes.is_empty() {
                break;
            }
        }
        let _ = worldwire::decode(&bytes);
    }
}

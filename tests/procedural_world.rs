use vesper3d::viewer::devkit::procedural::{field, ChunkCache, ChunkId, DayCycle, WorldPoint};

#[test]
fn seams_negative_coordinates_and_distant_origins_keep_local_precision() {
    let origin = ChunkId {
        x: 1_000_000_000_000,
        z: -1_000_000_000_000,
    };
    let point = WorldPoint::new(origin, [-0.25, 32.125], 32.).unwrap();
    assert_eq!(point.chunk, origin.offset(-1, 1).unwrap());
    assert_eq!(point.local, [31.75, 0.125]);
    assert_eq!(point.relative(origin, 32.).unwrap(), [-0.25, 32.125]);
    assert!(point.relative(ChunkId::default(), 32.).is_err());
    for size in [0., f32::NAN, f32::INFINITY] {
        assert!(WorldPoint::new(origin, [0.; 2], size).is_err());
    }
    assert!(WorldPoint::new(origin, [f32::NAN, 0.], 32.).is_err());
    assert!(WorldPoint::new(ChunkId { x: i64::MAX, z: 0 }, [32., 0.], 32.).is_err());
}

#[test]
fn streaming_travel_revisit_order_and_failures_preserve_bounded_identity() {
    let generate = |id: ChunkId| Ok(id.rng(42, 7).next_u64());
    let mut cache = ChunkCache::new(2).unwrap();
    let first = cache.update(ChunkId::default(), generate).unwrap();
    assert_eq!(first.added.len(), 25);
    let saved = cache.chunks()[&ChunkId::default()];
    assert!(cache
        .update(ChunkId::default(), |_| panic!(
            "stationary chunks regenerated"
        ))
        .unwrap()
        .added
        .is_empty());
    for n in 1..1000 {
        cache
            .update(
                ChunkId {
                    x: n * 1_000_000,
                    z: -n,
                },
                generate,
            )
            .unwrap();
        assert_eq!(cache.chunks().len(), 25);
    }
    let before = cache.chunks().clone();
    assert!(cache
        .update(ChunkId::default(), |_| Err("generation failed".into()))
        .is_err());
    assert_eq!(cache.chunks(), &before);
    assert!(cache
        .update(ChunkId { x: i64::MAX, z: 0 }, generate)
        .is_err());
    assert_eq!(cache.chunks(), &before);
    cache.update(ChunkId::default(), generate).unwrap();
    assert_eq!(cache.chunks()[&ChunkId::default()], saved);
    let mut other = ChunkCache::new(2).unwrap();
    other.update(ChunkId { x: -99, z: 44 }, generate).unwrap();
    other.update(ChunkId::default(), generate).unwrap();
    assert_eq!(cache.chunks(), other.chunks());
    assert!(ChunkCache::<()>::new(9).is_err());
}

#[test]
fn noise_meets_at_chunk_seams_and_varies_with_world_seed() {
    for x in [-1_000_000_000_000, -4, -1, 0, 4, 1_000_000_000_000] {
        let id = ChunkId { x, z: -1 };
        let left = field(9, WorldPoint::new(id, [31.9999, 7.], 32.).unwrap(), 32., 4).unwrap();
        let right = field(9, WorldPoint::new(id, [32., 7.], 32.).unwrap(), 32., 4).unwrap();
        assert!((left - right).abs() < 0.00001);
        assert!((0. ..=1.).contains(&right));
    }
    let at = WorldPoint::default();
    assert_ne!(field(1, at, 32., 3).unwrap(), field(2, at, 32., 3).unwrap());
    assert!(field(1, at, 32., 0).is_err());
}

#[test]
fn day_cycles_repeat_without_float_clock_accumulation() {
    let cycle = DayCycle::new(240).unwrap();
    assert_eq!(cycle.at(60).phase, 0.25);
    assert_eq!(cycle.at(180).phase, 0.75);
    assert_eq!(cycle.at(240).days, 1);
    assert_eq!(cycle.at(240).phase, 0.);
    let far = 1_000_000_000_000u64 * 240 + 120;
    assert_eq!(cycle.at(far).days, 1_000_000_000_000);
    assert_eq!(cycle.at(far).phase, 0.5);
    assert!(DayCycle::new(0).is_err());
    assert!(
        DayCycle::new(u32::MAX)
            .unwrap()
            .at(u64::from(u32::MAX) - 1)
            .phase
            < 1.
    );
}

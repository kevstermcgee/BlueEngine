use rift_delver::*;
use vesper3d::viewer::devkit::{snapshot, Simulation};
#[test]
fn damaged_save_preserves_running_expedition() {
    let mut s = Sim::new(7);
    s.start_run();
    for _ in 0..150 {
        s.step(&Input { fire: true, ..Default::default() });
    }
    let bytes = snapshot::save(&s, "expedition").unwrap();
    let mut damaged = bytes.clone();
    let mid = damaged.len() / 2;
    damaged[mid] ^= 1;
    let hash = s.state_hash();
    assert!(snapshot::restore(&mut s, &damaged).is_err());
    assert_eq!(hash, s.state_hash());
    let mut other = Sim::new(9);
    snapshot::restore(&mut other, &bytes).unwrap();
    assert_eq!(other.state_hash(), hash);
}

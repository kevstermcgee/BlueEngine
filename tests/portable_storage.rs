//! Native progress belongs to the player, survives reinstall, and migrates the previous layout.
use vesper3d::runtime::{
    snapshot,
    storage::{self, PlatformStorage},
    Simulation, Snapshot,
};
#[derive(Clone)]
struct Counter(u32);
impl Simulation for Counter {
    type Input = ();
    fn step(&mut self, _: &()) {
        self.0 += 1;
    }
    fn state_hash(&self) -> u64 {
        u64::from(self.0)
    }
}
impl Snapshot for Counter {
    const KIND: &'static str = "portable-storage-test";
    type State = u32;
    fn capture(&self) -> u32 {
        self.0
    }
    fn restore(&mut self, v: u32) -> Result<(), String> {
        self.0 = v;
        Ok(())
    }
}
#[test]
fn native_storage_child() {
    let Ok(mode) = std::env::var("BE2_STORAGE_TEST_MODE") else {
        return;
    };
    let store = PlatformStorage::new("storage-check").unwrap();
    if mode == "write" {
        storage::save(&store, &Counter(11)).unwrap();
        storage::save_slot(&store, "progress", &Counter(23)).unwrap();
        storage::write_settings(&store, &vec![true]).unwrap();
        let legacy = std::env::current_exe()
            .unwrap()
            .with_file_name("blueengine_legacy-check_v1_quick");
        std::fs::write(legacy, snapshot::save(&Counter(37), "Legacy").unwrap()).unwrap();
    }
    let mut game = Counter(0);
    assert!(storage::load(&store, &mut game).unwrap());
    assert_eq!(game.0, 11);
    assert!(storage::load_slot(&store, "progress", &mut game).unwrap());
    assert_eq!(game.0, 23);
    assert_eq!(
        storage::read_settings::<Vec<bool>>(&store).unwrap(),
        vec![true]
    );
    let legacy = PlatformStorage::new("legacy-check").unwrap();
    assert!(storage::load(&legacy, &mut game).unwrap());
    assert_eq!(game.0, 37);
}
#[test]
fn native_storage_survives_an_install_location_change() {
    use std::{fs, process::Command};
    let root = std::env::temp_dir().join(format!(
        "be2-storage-{}-{}",
        std::process::id(),
        storage::timestamp_ms()
    ));
    fs::create_dir_all(&root).unwrap();
    for (folder, mode) in [("install-one", "write"), ("install-two", "read")] {
        let install = root.join(folder);
        fs::create_dir_all(&install).unwrap();
        let exe = install.join("storage-test.exe");
        fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let result = Command::new(&exe)
            .args(["--exact", "native_storage_child", "--nocapture"])
            .env("BE2_STORAGE_TEST_MODE", mode)
            .env("BLUEENGINE_DATA_DIR", root.join("player-data"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::remove_dir_all(root).unwrap();
}

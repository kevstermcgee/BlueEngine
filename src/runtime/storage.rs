//! Fallible persistence, namespaced per game, with identical native/browser snapshot bytes.
use super::{snapshot, Snapshot};
use serde::{de::DeserializeOwned, Serialize};

/// The smallest platform boundary: read an optional value, or atomically replace it.
pub trait Storage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String>;
}
pub fn save<S: Snapshot>(store: &impl Storage, sim: &S) -> Result<(), String> {
    save_slot(store, "quick", sim)
}
pub fn save_slot<S: Snapshot>(store: &impl Storage, slot: &str, sim: &S) -> Result<(), String> {
    store.write(
        slot,
        &snapshot::save(sim, "Quick save").map_err(|e| e.to_string())?,
    )
}
pub fn load<S: Snapshot>(store: &impl Storage, sim: &mut S) -> Result<bool, String> {
    load_slot(store, "quick", sim)
}
pub fn load_slot<S: Snapshot>(
    store: &impl Storage,
    slot: &str,
    sim: &mut S,
) -> Result<bool, String> {
    match store.read(slot)? {
        None => Ok(false),
        Some(bytes) => {
            snapshot::restore(sim, &bytes).map_err(|e| e.to_string())?;
            Ok(true)
        }
    }
}
pub fn read_settings<T: DeserializeOwned + Default>(store: &impl Storage) -> Result<T, String> {
    match store.read("settings")? {
        None => Ok(T::default()),
        Some(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            format!("Settings invalid: {e}; reset browser/site storage or settings file")
        }),
    }
}
pub fn write_settings<T: Serialize>(store: &impl Storage, settings: &T) -> Result<(), String> {
    store.write(
        "settings",
        &serde_json::to_vec(settings).map_err(|e| e.to_string())?,
    )
}

pub struct PlatformStorage {
    namespace: String,
}
impl PlatformStorage {
    pub fn new(game: &str) -> Result<Self, String> {
        if !crate::viewer::savestate::valid_slot(game) {
            return Err(
                "Storage game ID must be portable lower-case letters/digits/hyphens".into(),
            );
        }
        Ok(Self {
            namespace: format!("blueengine:{game}:v1:"),
        })
    }
    fn key(&self, key: &str) -> Result<String, String> {
        if !crate::viewer::savestate::valid_slot(key) {
            return Err("Invalid storage slot".into());
        }
        Ok(format!("{}{key}", self.namespace))
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl Storage for PlatformStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let name = self.key(key)?.replace(':', "_");
        let path = native_directory()?.join(&name);
        match std::fs::read(&path) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Read-only migration from older executable-adjacent storage.
                let legacy = std::env::current_exe()
                    .map_err(|e| e.to_string())?
                    .with_file_name(name);
                match std::fs::read(legacy) {
                    Ok(bytes) => {
                        self.write(key, &bytes)?;
                        Ok(Some(bytes))
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(format!("Legacy storage unavailable: {e}")),
                }
            }
            Err(e) => Err(format!("Storage unavailable: {e}")),
        }
    }
    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let name = self.key(key)?.replace(':', "_");
        let directory = native_directory()?;
        std::fs::create_dir_all(&directory).map_err(|e| format!("Storage unavailable: {e}"))?;
        let path = directory.join(name);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)
            .and_then(|()| std::fs::rename(&tmp, &path))
            .map_err(|e| format!("Storage unavailable: {e}"))
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn native_directory() -> Result<std::path::PathBuf, String> {
    use std::{env, path::PathBuf};
    if let Some(path) = env::var_os("BLUEENGINE_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    let root = env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")));
    root.map(|p| p.join("BlueEngine")).ok_or_else(|| {
        "User storage location unavailable; set BLUEENGINE_DATA_DIR to a writable directory".into()
    })
}
#[cfg(target_arch = "wasm32")]
mod browser {
    #![allow(unsafe_code)] // only audited JS imports; gameplay never touches raw memory
    use super::*;
    unsafe extern "C" {
        fn be2_storage_read(key: *const u8, len: usize, out: *mut u8, cap: usize) -> i32;
        fn be2_storage_write(key: *const u8, len: usize, bytes: *const u8, count: usize) -> i32;
        fn be2_timestamp() -> f64;
    }
    pub fn timestamp() -> u64 {
        unsafe { be2_timestamp() as u64 }
    }
    impl Storage for PlatformStorage {
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
            let key = self.key(key)?;
            let len = unsafe { be2_storage_read(key.as_ptr(), key.len(), std::ptr::null_mut(), 0) };
            if len == -1 {
                return Ok(None);
            }
            if !(0..=4 * 1024 * 1024).contains(&len) {
                return Err("Browser storage unavailable/invalid; allow site storage, leave private mode, or reset this site's data".into());
            }
            let mut bytes = vec![0; len as usize];
            let count = unsafe {
                be2_storage_read(key.as_ptr(), key.len(), bytes.as_mut_ptr(), bytes.len())
            };
            if count != len {
                return Err("Browser storage changed during read; retry load".into());
            }
            Ok(Some(bytes))
        }
        fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
            let key = self.key(key)?;
            if unsafe { be2_storage_write(key.as_ptr(), key.len(), bytes.as_ptr(), bytes.len()) }
                != 0
            {
                return Err("Browser storage write failed; quota or privacy policy blocked it. Previous save retained.".into());
            }
            Ok(())
        }
    }
}
pub fn timestamp_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        browser::timestamp()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |v| v.as_millis() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Simulation;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Memory {
        values: RefCell<BTreeMap<String, Vec<u8>>>,
        blocked: Cell<bool>,
    }
    impl Storage for Memory {
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
            if self.blocked.get() {
                return Err("Storage unavailable".into());
            }
            Ok(self.values.borrow().get(key).cloned())
        }
        fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
            if self.blocked.get() {
                return Err("Storage write failed".into());
            }
            self.values.borrow_mut().insert(key.into(), bytes.to_vec());
            Ok(())
        }
    }
    #[derive(Clone)]
    struct Counter(u64);
    impl Simulation for Counter {
        type Input = ();
        fn step(&mut self, _: &()) {
            self.0 += 1;
        }
        fn state_hash(&self) -> u64 {
            self.0
        }
    }
    impl Snapshot for Counter {
        const KIND: &'static str = "storage-test";
        type State = u64;
        fn capture(&self) -> u64 {
            self.0
        }
        fn restore(&mut self, state: u64) -> Result<(), String> {
            self.0 = state;
            Ok(())
        }
    }
    #[test]
    fn automatic_and_manual_slots_are_independent_and_share_snapshot_format() {
        let store = Memory::default();
        save(&store, &Counter(10)).unwrap();
        save_slot(&store, "progress", &Counter(12)).unwrap();
        let mut restored = Counter(0);
        load(&store, &mut restored).unwrap();
        assert_eq!(restored.0, 10);
        load_slot(&store, "progress", &mut restored).unwrap();
        assert_eq!(restored.0, 12);
        store.blocked.set(true);
        assert!(save_slot(&store, "progress", &Counter(14)).is_err());
        store.blocked.set(false);
        load_slot(&store, "progress", &mut restored).unwrap();
        assert_eq!(restored.0, 12);
    }
    #[test]
    fn storage_failures_preserve_previous_save_and_corruption_preserves_simulation() {
        let store = Memory::default();
        let mut game = Counter(7);
        assert!(!load(&store, &mut game).unwrap());
        save(&store, &game).unwrap();
        game.step(&());
        store.blocked.set(true);
        assert!(save(&store, &game).is_err());
        assert!(load(&store, &mut game).is_err());
        assert_eq!(game.0, 8);
        store.blocked.set(false);
        assert!(load(&store, &mut game).unwrap());
        assert_eq!(game.0, 7);
        store.values.borrow_mut().get_mut("quick").unwrap()[0] ^= 1;
        game.0 = 19;
        assert!(load(&store, &mut game).is_err());
        assert_eq!(game.0, 19);
        store
            .values
            .borrow_mut()
            .insert("settings".into(), b"broken".to_vec());
        assert!(read_settings::<Vec<String>>(&store)
            .unwrap_err()
            .contains("Settings invalid"));
    }
}

use serde::Deserialize;
use std::path::{Path, PathBuf};
use vesper3d::{prelude::*, viewer::maps::MapId};

pub const CATALOG: &str = "assets/games/blueengine-sandbox/catalog.json";

#[derive(Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub assets: Vec<Asset>,
    pub maps: Vec<Destination>,
}
#[derive(Deserialize)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub group: String,
    pub source: String,
    pub half: [f32; 3],
    pub tags: String,
    pub map: String,
}
#[derive(Deserialize)]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub description: String,
    pub group: String,
    pub path: String,
    pub signs: Vec<Sign>,
}
#[derive(Deserialize)]
pub struct Sign {
    pub text: String,
    pub position: [f32; 3],
    pub height: f32,
}
impl Catalog {
    pub fn load(root: &Path) -> Result<Self> {
        let catalog: Self = serde_json::from_slice(&std::fs::read(root.join(CATALOG))?)?;
        if catalog.version != 1 || catalog.assets.is_empty() || catalog.maps.is_empty() {
            return Err("Unsupported or empty sandbox catalog".into());
        }
        Ok(catalog)
    }
    pub fn matching_assets(&self, query: &str) -> Vec<usize> {
        let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        self.assets
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                let text = format!("{} {} {} {}", a.name, a.id, a.group, a.tags).to_lowercase();
                words.iter().all(|w| text.contains(w)).then_some(i)
            })
            .collect()
    }
}
pub fn load_map(root: &Path, path: &str) -> Result<MapDocument> {
    if path == "builtin:test-lab" {
        MapDocument::from_map(MapId::TestLab)
    } else {
        MapDocument::load(&root.join(path))
    }
}
pub fn find_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        if root.join(CATALOG).is_file() {
            return Ok(root);
        }
        return Err("--root does not contain the sandbox catalog".into());
    }
    let exe = std::env::current_exe()?;
    for path in [
        std::env::current_dir()?,
        exe.parent().unwrap().to_path_buf(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    ] {
        if path.join(CATALOG).is_file() {
            return Ok(path);
        }
    }
    Err(
        "Sandbox content not found. Keep assets beside the executable or pass --root DIRECTORY."
            .into(),
    )
}

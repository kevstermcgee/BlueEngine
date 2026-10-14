//! Small game-owned definitions compiled once. IDs and JSON pointers survive presentation changes.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use vesper3d::runtime::resources::Inventory;
use vesper3d::runtime::StateHasher;
pub const MAX_CONTENT_BYTES: usize = 64 * 1024;
pub const V1_CONTENT: &str = include_str!("../assets/content-v1.json");
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub label: String,
    pub capacity: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: String,
    pub label: String,
    pub duration_ticks: u32,
    pub inputs: Vec<(String, u64)>,
    pub outputs: Vec<(String, u64)>,
    pub required_upgrade: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upgrade {
    pub id: String,
    pub label: String,
    pub cost: Vec<(String, u64)>,
    pub duration_divisor: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub game_id: String,
    pub content_version: u32,
    pub inventory_capacity: u64,
    pub items: Vec<Item>,
    pub recipes: Vec<Recipe>,
    pub upgrades: Vec<Upgrade>,
    pub initial: BTreeMap<String, u64>,
    pub sources: BTreeMap<String, u64>,
    pub deliverable: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct DefinitionLocation {
    pub source: String,
    pub pointer: String,
    pub id: String,
    pub content_version: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub at: String,
    pub ids: Vec<String>,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.at, self.message)
    }
}
impl std::error::Error for Diagnostic {}
fn err(code: &'static str, source: &str, pointer: &str, message: String) -> Diagnostic {
    Diagnostic { code, severity: "error", message, at: format!("{source}#{pointer}"), ids: Vec::new() }
}
pub fn valid_id(id: &str) -> bool {
    let Some((ns, name)) = id.split_once(':') else { return false };
    !ns.is_empty()
        && !name.is_empty()
        && id.len() <= 128
        && [ns, name]
            .iter()
            .all(|s| s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b)))
}
#[derive(Clone, Debug)]
pub struct Definitions {
    pub content_version: u32,
    pub inventory_capacity: u64,
    pub items: BTreeMap<String, Item>,
    pub recipes: BTreeMap<String, Recipe>,
    pub upgrades: BTreeMap<String, Upgrade>,
    pub initial: BTreeMap<String, u64>,
    pub sources: BTreeMap<String, u64>,
    pub deliverable: BTreeSet<String>,
    hash: u64,
    locations: BTreeMap<String, DefinitionLocation>,
    legacy_recipes: BTreeMap<String, Recipe>,
    legacy_upgrades: BTreeMap<String, Upgrade>,
}
impl Definitions {
    pub fn standard() -> Self {
        let root = vesper3d::viewer::devkit::runtime_assets("assets", Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("workshop assets");
        Self::load(&root.join("workshop.json")).expect("bundled content must validate")
    }
    pub fn load(path: &Path) -> Result<Self, Diagnostic> {
        let source = path.display().to_string();
        let meta = std::fs::metadata(path).map_err(|e| err("WORKSHOP_CONTENT_IO", &source, "", e.to_string()))?;
        if meta.len() > MAX_CONTENT_BYTES as u64 {
            return Err(err("WORKSHOP_CONTENT_LIMIT", &source, "", format!("maximum {MAX_CONTENT_BYTES} bytes")));
        }
        let text = std::fs::read_to_string(path).map_err(|e| err("WORKSHOP_CONTENT_IO", &source, "", e.to_string()))?;
        Self::from_json(&text, &source)
    }
    pub fn from_json(text: &str, source: &str) -> Result<Self, Diagnostic> {
        if text.len() > MAX_CONTENT_BYTES {
            return Err(err("WORKSHOP_CONTENT_LIMIT", source, "", format!("maximum {MAX_CONTENT_BYTES} bytes")));
        }
        let mut doc: Document = serde_json::from_str(text).map_err(|e| {
            err("WORKSHOP_CONTENT_JSON", source, "", format!("{} (line {}, column {})", e, e.line(), e.column()))
        })?;
        if doc.game_id != crate::GAME_ID || !(1..=2).contains(&doc.content_version) {
            return Err(err(
                "WORKSHOP_CONTENT_VERSION",
                source,
                "/content_version",
                "expected this game and content version 1 or 2".into(),
            ));
        }
        if doc.items.len() > 64
            || doc.recipes.len() > 64
            || doc.upgrades.len() > 32
            || doc.items.is_empty()
            || doc.inventory_capacity == 0
        {
            return Err(err(
                "WORKSHOP_CONTENT_LIMIT",
                source,
                "",
                "maximum 64 items, 64 recipes, 32 upgrades; positive inventory capacity".into(),
            ));
        }
        let mut locations = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for (group, names) in [
            ("items", doc.items.iter().map(|x| (&x.id, &x.label)).collect::<Vec<_>>()),
            ("recipes", doc.recipes.iter().map(|x| (&x.id, &x.label)).collect()),
            ("upgrades", doc.upgrades.iter().map(|x| (&x.id, &x.label)).collect()),
        ] {
            for (i, (id, label)) in names.iter().enumerate() {
                let pointer = format!("/{group}/{i}");
                if !valid_id(id) || label.is_empty() || label.len() > 256 {
                    return Err(err(
                        "WORKSHOP_DEFINITION_ID",
                        source,
                        &pointer,
                        "namespaced ASCII ID and 1..256-byte label required".into(),
                    ));
                }
                if !ids.insert((*id).clone()) {
                    return Err(err(
                        "WORKSHOP_DUPLICATE_ID",
                        source,
                        &pointer,
                        format!("duplicate persistent definition ID {id}"),
                    ));
                }
                locations.insert(
                    (*id).clone(),
                    DefinitionLocation {
                        source: source.into(),
                        pointer,
                        id: (*id).clone(),
                        content_version: doc.content_version,
                    },
                );
            }
        }
        let items: BTreeMap<_, _> = doc.items.iter().map(|i| (i.id.clone(), i.clone())).collect();
        let upgrades: BTreeMap<_, _> = doc.upgrades.iter().map(|u| (u.id.clone(), u.clone())).collect();
        let terms = |values: &[(String, u64)], pointer: &str| -> Result<(), Diagnostic> {
            if values.is_empty() || values.len() > 8 {
                return Err(err(
                    "WORKSHOP_TERMS_LIMIT",
                    source,
                    pointer,
                    "1..=8 unique positive item terms required".into(),
                ));
            }
            let mut seen = BTreeSet::new();
            for (i, (id, n)) in values.iter().enumerate() {
                if !items.contains_key(id) || *n == 0 || !seen.insert(id) {
                    return Err(err(
                        "WORKSHOP_ITEM_REFERENCE",
                        source,
                        &format!("{pointer}/{i}"),
                        format!("unknown/duplicate item or zero quantity: {id}"),
                    ));
                }
            }
            Ok(())
        };
        for (i, recipe) in doc.recipes.iter().enumerate() {
            terms(&recipe.inputs, &format!("/recipes/{i}/inputs"))?;
            terms(&recipe.outputs, &format!("/recipes/{i}/outputs"))?;
            if recipe.duration_ticks == 0 || recipe.duration_ticks > 36000 {
                return Err(err(
                    "WORKSHOP_DURATION",
                    source,
                    &format!("/recipes/{i}/duration_ticks"),
                    "duration must be 1..=36000 fixed 60Hz ticks".into(),
                ));
            }
            if recipe.required_upgrade.as_ref().is_some_and(|id| !upgrades.contains_key(id)) {
                return Err(err(
                    "WORKSHOP_UPGRADE_REFERENCE",
                    source,
                    &format!("/recipes/{i}/required_upgrade"),
                    "unresolved upgrade".into(),
                ));
            }
        }
        for (i, u) in doc.upgrades.iter().enumerate() {
            terms(&u.cost, &format!("/upgrades/{i}/cost"))?;
            if !(1..=4).contains(&u.duration_divisor) {
                return Err(err(
                    "WORKSHOP_UPGRADE_VALUE",
                    source,
                    &format!("/upgrades/{i}/duration_divisor"),
                    "duration divisor must be 1..=4".into(),
                ));
            }
        }
        for (field, map) in [("initial", &doc.initial), ("sources", &doc.sources)] {
            for (id, n) in map {
                if !items.contains_key(id) || *n > 1_000_000_000 {
                    return Err(err(
                        "WORKSHOP_ITEM_REFERENCE",
                        source,
                        &format!("/{field}/{id}"),
                        "unknown item or source exceeds 1,000,000,000 units".into(),
                    ));
                }
            }
        }
        let deliverable: BTreeSet<_> = doc.deliverable.iter().cloned().collect();
        if deliverable.len() != doc.deliverable.len() || deliverable.iter().any(|id| !items.contains_key(id)) {
            return Err(err(
                "WORKSHOP_SINK_REFERENCE",
                source,
                "/deliverable",
                "duplicate or unresolved sink item".into(),
            ));
        }
        Inventory::from_counts(
            doc.inventory_capacity,
            items.iter().map(|(id, item)| (id.clone(), item.capacity)).collect(),
            doc.initial.clone(),
        )
        .map_err(|e| err("WORKSHOP_INVENTORY", source, "/initial", e.to_string()))?;
        // Order of independent authored definitions does not change content identity.
        doc.items.sort_by(|a, b| a.id.cmp(&b.id));
        doc.recipes.sort_by(|a, b| a.id.cmp(&b.id));
        doc.upgrades.sort_by(|a, b| a.id.cmp(&b.id));
        doc.deliverable.sort();
        let canonical =
            serde_json::to_vec(&doc).map_err(|e| err("WORKSHOP_CONTENT_JSON", source, "", e.to_string()))?;
        let mut h = StateHasher::new();
        h.bytes(&canonical);
        let hash = h.finish();
        let legacy =
            if doc.content_version == 2 { Some(Self::from_json(V1_CONTENT, "frozen:content-v1.json")?) } else { None };
        Ok(Self {
            content_version: doc.content_version,
            inventory_capacity: doc.inventory_capacity,
            items,
            recipes: doc.recipes.into_iter().map(|r| (r.id.clone(), r)).collect(),
            upgrades,
            initial: doc.initial,
            sources: doc.sources,
            deliverable,
            hash,
            locations,
            legacy_recipes: legacy.as_ref().map(|d| d.recipes.clone()).unwrap_or_default(),
            legacy_upgrades: legacy.map(|d| d.upgrades).unwrap_or_default(),
        })
    }
    pub fn fingerprint(&self) -> String {
        format!("{:016x}", self.hash)
    }
    pub fn content_hash(&self) -> u64 {
        self.hash
    }
    pub fn rules_for_version(&self, version: u32) -> Result<&BTreeMap<String, Recipe>, String> {
        if version == self.content_version {
            Ok(&self.recipes)
        } else if version == 1 && self.content_version == 2 {
            Ok(&self.legacy_recipes)
        } else {
            Err(format!("WORKSHOP_JOB_CONTENT: unsupported source version {version}"))
        }
    }
    pub fn upgrades_for_version(&self, version: u32) -> Result<&BTreeMap<String, Upgrade>, String> {
        if version == self.content_version {
            Ok(&self.upgrades)
        } else if version == 1 && self.content_version == 2 {
            Ok(&self.legacy_upgrades)
        } else {
            Err("WORKSHOP_JOB_CONTENT: unsupported upgrade version".into())
        }
    }
    pub fn location(&self, id: &str, version: u32) -> Option<DefinitionLocation> {
        if version == self.content_version {
            self.locations.get(id).cloned()
        } else if version == 1 && self.content_version == 2 {
            Self::from_json(V1_CONTENT, "frozen:content-v1.json").ok()?.locations.get(id).cloned()
        } else {
            None
        }
    }
}
/// Semantic content addition/tuning validates a copy before writing. No gameplay/debug state mutation.
pub fn edit(path: &Path, addition: &serde_json::Value) -> Result<serde_json::Value, String> {
    let original = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let before = Definitions::from_json(&original, &path.display().to_string()).map_err(|e| e.to_string())?;
    let mut doc: serde_json::Value = serde_json::from_str(&original).map_err(|e| e.to_string())?;
    let obj = addition.as_object().ok_or("WORKSHOP_EDIT: additions object required")?;
    for (group, values) in obj {
        if !["recipes", "upgrades"].contains(&group.as_str()) {
            return Err("WORKSHOP_EDIT: only recipes/upgrades additions supported".into());
        }
        let added = values.as_array().ok_or("WORKSHOP_EDIT: arrays required")?;
        let target = doc[group].as_array_mut().ok_or("WORKSHOP_EDIT: missing group")?;
        target.extend(added.iter().cloned());
    }
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    let after = Definitions::from_json(&text, &path.display().to_string()).map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("edit-{}.tmp", std::process::id()));
    std::fs::write(&temporary, text).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, path)
        .map_err(|e| format!("WORKSHOP_EDIT: original preserved; replacement failed: {e}"))?;
    Ok(
        serde_json::json!({"before":before.fingerprint(),"after":after.fingerprint(),"added_recipes":after.recipes.keys().filter(|id|!before.recipes.contains_key(*id)).collect::<Vec<_>>(),"added_upgrades":after.upgrades.keys().filter(|id|!before.upgrades.contains_key(*id)).collect::<Vec<_>>()}),
    )
}

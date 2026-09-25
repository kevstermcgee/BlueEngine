//! Native discovery. Command arities are also used by the CLI, avoiding a second inventory.
use crate::Result;
use serde_json::{json, Value};

/// Command name and complete argument signature. The CLI validates these arities.
pub const COMMANDS: &[(&str, &str)] = &[
    ("help", ""),
    ("describe", ""),
    ("search", "TEXT"),
    ("game-describe", ""),
    ("game-schema", ""),
    ("game-validate", "GAME.json"),
    ("game-example", "NEW_DIRECTORY"),
    ("export-house", "OUT.json"),
    ("export-lab", "OUT.json"),
    ("inspect", "MAP.json"),
    ("audit", "MAP.json"),
    ("inspect-performance", ""),
    ("validate-budget", ""),
    ("net-test", ""),
    ("bench", ""),
    ("replay-test", ""),
    ("apply", "MAP.json PATCH.json OUT.json"),
    ("diff", "BEFORE.json AFTER.json"),
    ("export-scene", "MAP.json OUT.json"),
    ("floorplan", "MAP.json OUT.svg"),
    ("ray", "MAP.json ORIGIN_X,Y,Z TARGET_X,Y,Z"),
    ("route", "MAP.json ROUTE.json"),
    ("select", "MAP.json OBJECT_ID"),
    ("near", "MAP.json X,Y,Z RADIUS"),
    ("catalog", ""),
    ("lint", "MAP.json"),
    ("reach", "MAP.json"),
    ("walk-auto", "MAP.json FROM_X,Z TO_X,Z"),
    ("walk-explain", "MAP.json FROM_X,Z TO_X,Z OUT.svg"),
    ("build", "SPEC.json OUT.json"),
    ("blueprint-example", "OUT.json"),
    ("scatter", "MAP.json KIND COUNT X1,Z1,X2,Z2 SEED OUT.json"),
    ("line", "MAP.json KIND COUNT X1,Z1,X2,Z2 OUT.json"),
    ("verify", "MAP.json [CHECKS.json]"),
    ("sim", "SCENARIO.json [TRACE.json]"),
    ("replay-trace", "TRACE.json"),
    ("src", "ACTION [QUERY]"),
    ("new-game", "NAME DIRECTORY"),
    ("ui-check", ""),
    ("mcp", ""),
    ("net-proxy", "LISTEN UPSTREAM [PRESET]"),
    ("doc-check", "[ROOT]"),
];

/// Feature-to-source/check metadata embedded at compile time; no source reads at runtime.
pub fn features() -> Result<Value> {
    Ok(serde_json::from_str(include_str!(
        "../../tools/FEATURES.json"
    ))?)
}

/// Bounded orientation response. Detailed feature records are returned by [`search`].
pub fn describe() -> Result<Value> {
    let data = features()?;
    let names: Vec<_> = data["features"]
        .as_object()
        .ok_or("Invalid feature index")?
        .keys()
        .collect();
    Ok(json!({
        "ok": true, "engine": "BlueEngine", "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": super::net::PROTOCOL_VERSION, "map_schema_version": 1, "game_schema_version": 1,
        "units": "metres; +Y up; yaw/pitch radians; yaw 0 faces -Z; boxes use half extents; prop origins are bottoms",
        "default_map": "Blue Test Lab",
        "commands": COMMANDS.iter().map(|(name, args)| json!({"name":name,"arguments":args})).collect::<Vec<_>>(),
        "features": names,
        "limits": {"packet_bytes": super::net::MAX_PACKET_BYTES, "players": 8, "map_bytes": 8_000_000, "patch_operations": 1000, "search_results": 10},
        "start": ["be2-tools export-lab NEW.json", "be2-tools catalog", "be2-tools search multiplayer", "docs/AI_QUICKSTART.md"],
        "unsupported": ["arbitrary gameplay scripts", "runtime mesh import"],
        "metadata": "Curated feature index; executable commands/arities come from the native CLI registry."
    }))
}

/// Case-insensitive feature/file/check search, capped at ten hits and 100 query bytes.
/// Empty, oversized and control-character queries are errors; zero matches is valid.
pub fn search(query: &str) -> Result<Value> {
    if query.trim().is_empty() || query.len() > 100 || query.chars().any(char::is_control) {
        return Err("Search requires 1..100 bytes without control characters".into());
    }
    let data = features()?;
    let query = query.to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    let mut matches = Vec::new();
    let mut total = 0;
    for (name, feature) in data["features"]
        .as_object()
        .ok_or("Invalid feature index")?
    {
        let haystack = format!("{name} {feature}").to_lowercase();
        if words.iter().all(|word| haystack.contains(word)) {
            total += 1;
            if matches.len() < 10 {
                matches.push(json!({"feature": name, "details": feature}));
            }
        }
    }
    Ok(json!({"ok":true,"matches":matches,"total":total,"limit":10,"engine_source_read":false}))
}

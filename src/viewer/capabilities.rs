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
    ("schema-generate", "KIND"),
    ("game-validate", "GAME.json"),
    ("game-example", "NEW_DIRECTORY"),
    ("export-house", "OUT.json"),
    ("export-lab", "OUT.json"),
    ("inspect", "MAP.json"),
    ("audit", "MAP.json"),
    ("inspect-performance", ""),
    ("validate-budget", ""),
    ("net-test", "[--ticks=N] [--latency-ms=N] [--loss-rate=R]"),
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
    ("lint", "MAP.json [--game=GAME.json] [--scenario=SCENARIO.json]"),
    ("reach", "MAP.json"),
    ("walk-auto", "MAP.json FROM_X,Z TO_X,Z"),
    ("walk-explain", "MAP.json FROM_X,Z TO_X,Z OUT.svg"),
    ("build", "SPEC.json OUT.json"),
    ("blueprint-example", "OUT.json"),
    ("scatter", "MAP.json KIND COUNT X1,Z1,X2,Z2 SEED OUT.json"),
    ("line", "MAP.json KIND COUNT X1,Z1,X2,Z2 OUT.json"),
    ("verify", "MAP.json [CHECKS.json]"),
    ("sim", "SCENARIO.json [TRACE.json]"),
    ("replay-trace", "TRACE.json [GAME.json]"),
    ("src", "ACTION [QUERY]"),
    ("new-game", "NAME DIRECTORY [ENGINE_PATH] [TEMPLATE]"),
    ("icon", "TITLE DIRECTORY [VARIANT] [--replace]"),
    ("audio", "ACTION [PROJECT_OR_BUNDLE] [NEW_DIRECTORY]"),
    ("narrative", "ACTION [CONTENT.poe] [BINDINGS.json]"),
    ("ambient-music", "OUT.wav [MINUTES] [SEED] [--major] [--title=TEXT] [--tagline=TEXT]"),
    ("game-explore", "GAME.json [--max-states=N] [--scenario=OUT.json]"),
    ("add-interactable", "GAME.json ID --at=X,Y,Z [--size=HX,HY,HZ] [--color=R,G,B] [--label=TEXT] [--disabled] [--hidden] [--write]"),
    ("save-info", "FILE [GAME_JSON]"),
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

/// Which runtime path gets which networking, saving and shutdown behaviour, with every number read from the constant that
/// enforces it (nothing here is typed in by hand), so it cannot drift from the code.
pub fn runtime_support() -> Value {
    use super::{game, net, netplay, savestate, simulation};
    json!({
        "stock_server": {
            "available_in_this_build": cfg!(all(feature = "physics", feature = "networking")),
            "what": "be2-headless --server",
            "players": {
                "default": simulation::DEFAULT_MAX_PLAYERS,
                "configurable_up_to": simulation::MAX_PLAYERS_LIMIT,
                "how": "--max-players N",
                "capacity_evidence": "configured limit, not measured capacity",
            },
            "datagram_bytes": net::MAX_PACKET_BYTES,
            "world_updates": format!("protocol {}: binary world, JSON elsewhere", net::PROTOCOL_VERSION),
            "replication": {
                "partial_world_updates": true,
                "records_per_peer": net::replication::MAX_REPLICATED_ENTITIES,
                "note": "acknowledged; nearest records first",
            },
            "interest_management": "snapshot_for_player: room relevance",
            "prediction": "PredictionBuffer",
            "graceful_shutdown": "INT/TERM/HUP: finish tick, save, exit 0; second forces exit. Unix tested; Windows compile-only.",
            "saving": {
                "autosave": "--autosave SECONDS; --load auto; shutdown save",
                "players_in_a_save": savestate::world::MAX_PLAYERS,
                "payload_bytes": savestate::MAX_PAYLOAD_BYTES,
            },
            "game_document": {"file_bytes": game::MAX_GAME_BYTES, "counters": game::MAX_GAME_COUNTERS, "flags": game::MAX_GAME_FLAGS},
        },
        "custom_sim_netplay": {
            "production_transport_available_in_this_build": cfg!(feature = "networking"),
            "what": "NetGame + ClientView; loop: cli::serve",
            "datagram_bytes": netplay::wire::MAX_DATAGRAM,
            "datagram_budget": "min(payload_limit, datagram_bytes), including framing",
            "players": "MAX_SEATS/lobby_capacity/minimum_players: configured, unmeasured",
            "replication": "snapshot_with_budget trims optional state; no fragmentation/stock deltas/interest scheduler",
            "prediction": "your ClientView",
            "graceful_shutdown": "cli::serve shutdown handlers; bots: NetGame::release",
            "transport": "UDP; QUIC/TLS needs networking; hub rooms: UDP",
            "delivery": "local queue acceptance, not remote acknowledgement",
            "saving": format!("devkit::Snapshot: {}B; no stock --autosave", savestate::MAX_PAYLOAD_BYTES),
        },
    })
}

/// Bounded orientation response. Detailed feature records are returned by [`search`].
pub fn describe() -> Result<Value> {
    let data = features()?;
    let names: Vec<_> = data["features"]
        .as_object()
        .ok_or("Invalid feature index")?
        .keys()
        .collect();
    let starters = super::newgame::starter_catalog();
    Ok(json!({
        "ok": true, "engine": "BlueEngine", "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": super::net::PROTOCOL_VERSION, "map_schema_version": 1, "game_schema_version": 1,
        "units": "m; +Y up; radians; yaw 0=-Z; box half extents; prop bottom origins",
        "default_map": "Blue Test Lab",
        "commands": COMMANDS.iter().map(|(name, args)| json!({"name":name,"arguments":args})).collect::<Vec<_>>(),
        "features": names,
        "starters": {
            "catalog": "templates/starters.json",
            "cli_default": starters["cli_default"], "library_default": starters["library_default"]
        },
        "limits": {
            "packet_bytes": super::net::MAX_PACKET_BYTES,
            "players": super::simulation::DEFAULT_MAX_PLAYERS,
        "players_note": "Default admission; see runtime_support",
            "map_bytes": super::game::MAX_MAP_BYTES, "patch_operations": 1000, "search_results": 10
        },
        "runtime_support": runtime_support(),
        "start": ["python3 tools/be2.py start TASK --compact", "be2-tools search TOPIC", "docs/GAME_AUTHORING_ENTRY.md"],
        "unsupported": ["arbitrary gameplay scripts", "runtime mesh import"],
        "metadata": "Curated index."
    }))
}

/// Curated public API and case-insensitive feature search, capped at ten hits and 100 query bytes.
/// Empty, oversized and control-character queries are errors; zero matches is valid.
pub fn search(query: &str) -> Result<Value> {
    if query.trim().is_empty() || query.len() > 100 || query.chars().any(char::is_control) {
        return Err("Search requires 1..100 bytes without control characters".into());
    }
    let data = features()?;
    let query = query.trim().trim_matches('`').to_lowercase();
    let words: Vec<_> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|word| !word.is_empty())
        .collect();
    let browser = words
        .iter()
        .any(|word| matches!(*word, "browser" | "web" | "wasm"))
        && !words
            .iter()
            .any(|word| matches!(*word, "room" | "rooms" | "lobby"));
    let mut ranked = Vec::new();
    for (name, feature) in data["features"]
        .as_object()
        .ok_or("Invalid feature index")?
    {
        let exact_api = feature["public_api"].as_array().is_some_and(|apis| {
            apis.iter()
                .any(|api| api.as_str().is_some_and(|api| api.to_lowercase() == query))
        });
        let mut score = if *name == query || exact_api { 2000 } else { 0 };
        if let Some(answers) = feature["answers"].as_array() {
            for answer in answers {
                if answer["identifiers"].as_array().is_some_and(|apis| {
                    apis.iter()
                        .any(|api| api.as_str().is_some_and(|api| api.to_lowercase() == query))
                }) {
                    score = 2000;
                } else if let Some(groups) = answer["when"].as_array() {
                    if !groups.is_empty()
                        && groups.iter().all(|group| {
                            group.as_array().is_some_and(|aliases| {
                                aliases.iter().any(|alias| {
                                    alias.as_str().is_some_and(|alias| words.contains(&alias))
                                })
                            })
                        })
                    {
                        score = score.max(100 + groups.len());
                    }
                }
            }
        }
        if feature["status"] == "retired" && score < 2000 {
            if !browser {
                continue;
            }
            score = score.max(100);
        }
        if score == 0 {
            let haystack = format!("{name} {feature}").to_lowercase();
            if !words.is_empty() && words.iter().all(|word| haystack.contains(word)) {
                score = 1;
            }
        }
        if score > 0 {
            ranked.push((score, name, feature));
        }
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    let best = ranked.first().map_or(0, |item| item.0);
    let selected: Vec<_> = ranked
        .iter()
        .filter(|item| {
            if best >= 2000 {
                item.0 >= 2000
            } else if best >= 100 {
                item.0 >= 100
            } else {
                true
            }
        })
        .collect();
    let matches: Vec<_> = selected
        .iter()
        .take(10)
        .map(|(_, name, feature)| json!({"feature":name,"details":feature}))
        .collect();
    Ok(
        json!({"ok":true,"matches":matches,"total":selected.len(),"limit":10,"engine_source_read":false,
        "confidence": if best >= 2000 { "exact indexed identifier" } else if best >= 100 { "curated public contract; confirm runtime" } else { "low: uncurated text match" },
        "next": "Use be2.py context TASK --level 1 --compact for a small answer; refine by runtime or exact API identifier."}),
    )
}

//! Native Model Context Protocol (MCP) server over standard I/O for BlueEngine.
//!
//! Exposes BlueEngine's full native toolkit directly to AI assistants (Claude Code,
//! Antigravity, Cursor, Zed, VS Code) over JSON-RPC 2.0 stdio.
//!
//! Eliminates subprocess spawning overhead and exposes structured tool calls for:
//! - Map authoring and compilation (`build_blueprint`, `scatter`, `line`)
//! - Design linting and reachability (`lint`, `reach`)
//! - Physical pathfinding and walk execution (`walk_auto`)
//! - Unified map verification (`verify`)
//! - Simulation scenarios and deterministic replay debugging (`sim`, `replay_trace`)
//! - Rust source navigation (`src_lookup`)
//! - Headless UI auditing (`ui_check`)
//! - Performance budgeting and netcode validation (`perf_inspect`, `validate_budget`, `net_test`)

use super::{
    authoring::MapDocument,
    blueprint::{compile_blueprint, BlueprintSpec},
    capabilities,
    gen::scatter,
    lint::lint_map,
    pathing::execute_walk,
    reach::analyze_reach,
    scenario::{run_scenario, verify_replay_trace, Scenario, SimulationTrace},
    symbols::SourceIndex,
    ui_check::audit_all_screens,
    verify::{verify_map, ChecksBlock},
};
use crate::math::V;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;

pub fn run_mcp_server() -> crate::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = stdin.lock();

    let mut line_buf = String::new();
    while reader.read_line(&mut line_buf)? > 0 {
        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            line_buf.clear();
            continue;
        }

        if let Ok(request) = serde_json::from_str::<Value>(trimmed) {
            let id = request.get("id").cloned();
            let method = request.get("method").and_then(Value::as_str).unwrap_or("");

            let response = match method {
                "initialize" => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "serverInfo": {
                            "name": "blue-engine-mcp",
                            "version": env!("CARGO_PKG_VERSION")
                        },
                        "capabilities": {
                            "tools": {}
                        }
                    }
                })),
                "notifications/initialized" => None,
                "ping" => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {}
                })),
                "tools/list" => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": list_mcp_tools()
                    }
                })),
                "tools/call" => {
                    let params = request.get("params").cloned().unwrap_or(Value::Null);
                    let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
                    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                    let result = call_tool(tool_name, &arguments);
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
                                }
                            ],
                            "isError": !result.get("ok").and_then(Value::as_bool).unwrap_or(true)
                        }
                    }))
                }
                _ => {
                    if id.is_some() {
                        Some(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": format!("Method not found: {method}")
                            }
                        }))
                    } else {
                        None
                    }
                }
            };

            if let Some(resp) = response {
                let resp_str = serde_json::to_string(&resp)?;
                writeln!(stdout, "{resp_str}")?;
                stdout.flush()?;
            }
        }

        line_buf.clear();
    }

    Ok(())
}

fn list_mcp_tools() -> Vec<Value> {
    let schemas = input_schemas();
    let tools = [
        ("describe", "Get BlueEngine capabilities, features, command registry, and specifications."),
        ("search", "Search features, source files, and tests across BlueEngine."),
        ("catalog", "List all available props in the asset catalog with their dimensions."),
        ("lint", "Perform design-level static analysis on a map document for floating props, leaks, overlaps, drop hazards, etc."),
        ("reach", "Analyze walkable reachability from spawn: detects drop hazards, perimeter leaks, and unreachable entities."),
        ("walk_auto", "Plan an A* route with player clearance and execute it using real 60 Hz physics. Pinpoints blockers if obstructed."),
        ("build_blueprint", "Compile a declarative blueprint specification into a complete, validated, lint-clean MapDocument."),
        ("scatter", "Procedurally scatter catalog props into a map within a rectangle, checking ground height and collision clearance."),
        ("verify", "Run self-verification checks on a map or game (linting, reach, walk, objects, simulation scenarios)."),
        ("sim", "Run a multi-agent headless gameplay simulation scenario and validate assertions."),
        ("replay_trace", "Replay a simulation trace and pinpoint the exact tick and state diff where divergence occurs."),
        ("src_lookup", "Navigate and inspect BlueEngine Rust symbols, signatures, and modules without dumping whole files."),
        ("ui_check", "Audit BlueEngine UI screens (menu, lobby, HUD, pause) across 9 window resolutions."),
    ];
    tools.into_iter().map(|(name, description)| json!({
        "name":name,"description":description,
        "inputSchema":schemas.get(name).expect("generated input schema covers every MCP tool")
    })).collect()
}

#[cfg(not(feature = "schema-generation"))]
fn input_schemas() -> serde_json::Map<String, Value> {
    serde_json::from_str(include_str!("../../tools/mcp.schemas.json"))
        .expect("committed MCP schemas are validated by the generation gate")
}

#[cfg(feature = "schema-generation")]
pub fn input_schemas() -> serde_json::Map<String, Value> {
    use crate::authoring_schemas::generated;
    use arguments::*;
    let mut schemas = serde_json::Map::new();
    macro_rules! insert {
        ($name:literal, $ty:ty) => {
            schemas.insert($name.into(), generated::<$ty>());
        };
    }
    insert!("describe", Empty);
    insert!("search", Search);
    insert!("catalog", Empty);
    insert!("lint", Lint);
    insert!("reach", Reach);
    insert!("walk_auto", Walk);
    insert!("build_blueprint", Blueprint);
    insert!("scatter", Scatter);
    insert!("verify", MapPath);
    insert!("sim", ScenarioPath);
    insert!("replay_trace", Trace);
    insert!("src_lookup", Lookup);
    insert!("ui_check", Empty);
    schemas
}

#[cfg(feature = "schema-generation")]
mod arguments {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value};
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Empty {}
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Search {
        /// Search text (up to 100 bytes).
        pub query: String,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct MapPath {
        /// Path to map JSON.
        pub map_path: String,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Lint {
        /// Path to map JSON.
        pub map_path: String,
        /// Fail on warnings.
        #[serde(default)]
        pub strict: bool,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Reach {
        /// Path to map JSON.
        pub map_path: String,
        /// Optional [x, y, z] start position.
        pub start: Option<Vec<f32>>,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Walk {
        /// Path to map JSON.
        pub map_path: String,
        /// [x, z] origin.
        pub from: Vec<f32>,
        /// [x, z] destination.
        pub to: Vec<f32>,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Blueprint {
        /// Blueprint JSON specification.
        pub spec: Map<String, Value>,
        /// Output map path.
        pub out_path: String,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Scatter {
        /// Input map path.
        pub map_path: String,
        /// Prop kind from catalog.
        pub kind: String,
        /// Number of props to place.
        pub count: u64,
        /// [min_x, min_z, max_x, max_z].
        pub rect: Vec<f32>,
        /// RNG seed.
        pub seed: Option<u64>,
        /// Output map path.
        pub out_path: String,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct ScenarioPath {
        /// Path to scenario JSON.
        pub scenario_path: String,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Trace {
        /// Path to trace JSON.
        pub trace_path: String,
        /// Optional game path.
        pub game_path: Option<String>,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(transform = crate::authoring_schemas::mcp_arguments)]
    pub struct Lookup {
        pub action: LookupAction,
        /// Symbol name, keyword, or file path.
        pub query: Option<String>,
    }
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum LookupAction {
        Map,
        Find,
        Outline,
        Show,
        Refs,
        Coverage,
    }
}

fn call_tool(name: &str, args: &Value) -> Value {
    match name {
        "describe" => capabilities::describe()
            .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()})),
        "search" => {
            let q = args.get("query").and_then(Value::as_str).unwrap_or("");
            capabilities::search(q).unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}))
        }
        "catalog" => json!({
            "ok": true,
            "props": [
                "apple", "chair", "table", "book-stack", "flower-vase", "tall-vase",
                "table-lamp", "candle-trio", "potted-cactus", "mantel-clock", "woven-basket",
                "framed-art", "framed-botanical", "sculpture", "vase-plant", "bowl", "cereal"
            ]
        }),
        "lint" => {
            let path_str = args.get("map_path").and_then(Value::as_str).unwrap_or("");
            let strict = args.get("strict").and_then(Value::as_bool).unwrap_or(false);
            match MapDocument::load(Path::new(path_str)) {
                Ok(doc) => {
                    let rep = lint_map(&doc, strict, &[]);
                    serde_json::to_value(&rep).unwrap()
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            }
        }
        "reach" => {
            let path_str = args.get("map_path").and_then(Value::as_str).unwrap_or("");
            match MapDocument::load(Path::new(path_str)) {
                Ok(doc) => {
                    let start = args.get("start").and_then(Value::as_array).and_then(|arr| {
                        let v: Vec<f32> = arr
                            .iter()
                            .filter_map(Value::as_f64)
                            .map(|f| f as f32)
                            .collect();
                        if v.len() >= 3 {
                            Some(V(v[0], v[1], v[2]))
                        } else {
                            None
                        }
                    });
                    match analyze_reach(&doc, start) {
                        Ok(rep) => serde_json::to_value(&rep).unwrap(),
                        Err(e) => json!({"ok": false, "error": e.to_string()}),
                    }
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            }
        }
        "walk_auto" => {
            let path_str = args.get("map_path").and_then(Value::as_str).unwrap_or("");
            let from_arr = args.get("from").and_then(Value::as_array);
            let to_arr = args.get("to").and_then(Value::as_array);

            if let (Some(f), Some(t)) = (from_arr, to_arr) {
                let from_v = V(
                    f[0].as_f64().unwrap_or(0.0) as f32,
                    0.0,
                    f[1].as_f64().unwrap_or(0.0) as f32,
                );
                let to_v = V(
                    t[0].as_f64().unwrap_or(0.0) as f32,
                    0.0,
                    t[1].as_f64().unwrap_or(0.0) as f32,
                );

                match MapDocument::load(Path::new(path_str)) {
                    Ok(doc) => {
                        let res = execute_walk(&doc, from_v, to_v, None);
                        serde_json::to_value(&res).unwrap()
                    }
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                }
            } else {
                json!({"ok": false, "error": "Invalid 'from' or 'to' coordinates"})
            }
        }
        "build_blueprint" => {
            if let (Some(spec_val), Some(out_p)) = (
                args.get("spec"),
                args.get("out_path").and_then(Value::as_str),
            ) {
                match serde_json::from_value::<BlueprintSpec>(spec_val.clone()) {
                    Ok(spec) => match compile_blueprint(&spec) {
                        Ok(doc) => {
                            match std::fs::write(out_p, serde_json::to_string_pretty(&doc).unwrap())
                            {
                                Ok(()) => {
                                    json!({"ok": true, "output": out_p, "rooms": spec.rooms.len()})
                                }
                                Err(e) => {
                                    json!({"ok": false, "error": format!("Write error: {e}")})
                                }
                            }
                        }
                        Err(e) => json!({"ok": false, "error": format!("Compile error: {e}")}),
                    },
                    Err(e) => json!({"ok": false, "error": format!("Parse error: {e}")}),
                }
            } else {
                json!({"ok": false, "error": "Missing spec or out_path"})
            }
        }
        "scatter" => {
            let map_p = args.get("map_path").and_then(Value::as_str).unwrap_or("");
            let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
            let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
            let seed = args.get("seed").and_then(Value::as_u64).unwrap_or(42);
            let out_p = args
                .get("out_path")
                .and_then(Value::as_str)
                .unwrap_or(map_p);
            let rect_arr = args.get("rect").and_then(Value::as_array);

            if let Some(r) = rect_arr {
                let rect = [
                    r[0].as_f64().unwrap_or(-5.0) as f32,
                    r[1].as_f64().unwrap_or(-5.0) as f32,
                    r[2].as_f64().unwrap_or(5.0) as f32,
                    r[3].as_f64().unwrap_or(5.0) as f32,
                ];
                match MapDocument::load(Path::new(map_p)) {
                    Ok(doc) => match scatter(doc, kind, count, rect, seed, None) {
                        Ok((updated, placed)) => match std::fs::write(
                            out_p,
                            serde_json::to_string_pretty(&updated).unwrap(),
                        ) {
                            Ok(()) => json!({"ok": true, "placed": placed, "output": out_p}),
                            Err(e) => json!({"ok": false, "error": format!("Write error: {e}")}),
                        },
                        Err(e) => json!({"ok": false, "error": format!("Scatter error: {e}")}),
                    },
                    Err(e) => json!({"ok": false, "error": format!("Load error: {e}")}),
                }
            } else {
                json!({"ok": false, "error": "Missing rect"})
            }
        }
        "verify" => {
            let map_p = args.get("map_path").and_then(Value::as_str).unwrap_or("");
            match MapDocument::load(Path::new(map_p)) {
                Ok(doc) => {
                    let checks = ChecksBlock::default();
                    let rep = verify_map(&doc, &checks, map_p);
                    serde_json::to_value(&rep).unwrap()
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            }
        }
        "sim" => {
            let scen_p = args
                .get("scenario_path")
                .and_then(Value::as_str)
                .unwrap_or("");
            match std::fs::read_to_string(scen_p) {
                Ok(s) => match serde_json::from_str::<Scenario>(&s) {
                    Ok(scen) => match run_scenario(&scen) {
                        Ok((trace, _)) => json!({
                            "ok": true,
                            "scenario": scen.name,
                            "ticks": trace.total_ticks,
                            "checksum": format!("0x{:016x}", trace.checkpoints.last().map(|c| c.checksum).unwrap_or(0))
                        }),
                        Err(e) => json!({"ok": false, "error": e.to_string()}),
                    },
                    Err(e) => json!({"ok": false, "error": format!("Parse error: {e}")}),
                },
                Err(e) => json!({"ok": false, "error": format!("Read error: {e}")}),
            }
        }
        "replay_trace" => {
            let trace_p = args.get("trace_path").and_then(Value::as_str).unwrap_or("");
            let game_p = args.get("game_path").and_then(Value::as_str);
            match std::fs::read_to_string(trace_p) {
                Ok(s) => match serde_json::from_str::<SimulationTrace>(&s) {
                    Ok(trace) => match verify_replay_trace(&trace, game_p) {
                        Ok(rep) => serde_json::to_value(&rep).unwrap(),
                        Err(e) => json!({"ok": false, "error": e.to_string()}),
                    },
                    Err(e) => json!({"ok": false, "error": format!("Parse error: {e}")}),
                },
                Err(e) => json!({"ok": false, "error": format!("Read error: {e}")}),
            }
        }
        "src_lookup" => {
            let action = args.get("action").and_then(Value::as_str).unwrap_or("map");
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            match SourceIndex::scan(root) {
                Ok(idx) => match action {
                    "map" => json!({"ok": true, "modules": idx.map()}),
                    "find" => json!({"ok": true, "symbols": idx.find(query)}),
                    "outline" => json!({"ok": true, "symbols": idx.outline(query)}),
                    "show" => match idx.show(query) {
                        Ok(code) => json!({"ok": true, "code": code}),
                        Err(e) => json!({"ok": false, "error": e.to_string()}),
                    },
                    "refs" => json!({"ok": true, "refs": idx.refs(query)}),
                    "coverage" => json!({"ok": true, "undocumented": idx.coverage()}),
                    _ => json!({"ok": false, "error": format!("Unknown action: {action}")}),
                },
                Err(e) => json!({"ok": false, "error": format!("Scan error: {e}")}),
            }
        }
        "ui_check" => {
            let rep = audit_all_screens();
            serde_json::to_value(&rep).unwrap()
        }
        _ => json!({"ok": false, "error": format!("Unknown tool: {name}")}),
    }
}

//! Generated outputs, maintained fixtures and independently invalid authoring cases.
#![cfg(feature = "schema-validation")]
use serde_json::{json, Value};
use std::{fs, path::Path};
use vesper3d::authoring_schemas::generate;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
fn read(path: &str) -> Value {
    serde_json::from_slice(&fs::read(root().join(path)).unwrap()).unwrap()
}
fn validator(kind: &str) -> jsonschema::Validator {
    jsonschema::options()
        .should_validate_formats(true)
        .build(&generate(kind).unwrap())
        .unwrap()
}
fn assert_valid(validator: &jsonschema::Validator, value: &Value) {
    let errors: Vec<_> = validator
        .iter_errors(value)
        .map(|error| error.to_string())
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn committed_schema_files_equal_rust_generation() {
    for (kind, path) in [
        ("game", "tools/game.schema.json"),
        ("patch", "tools/patch.schema.json"),
        ("asset-pack", "assets/asset-pack.schema.json"),
        ("mcp", "tools/mcp.schemas.json"),
    ] {
        let generated = generate(kind).unwrap();
        assert!(
            read(path) == generated,
            "{path}: schema differs from Rust; run python3 tools/be2.py schemas --write"
        );
        assert!(
            fs::read_to_string(root().join(path)).unwrap()
                == serde_json::to_string_pretty(&generated).unwrap() + "\n",
            "{path}: canonical bytes differ; run python3 tools/be2.py schemas --write"
        );
    }
}

#[test]
fn maintained_games_and_packs_still_validate() {
    let game = validator("game");
    for path in [
        "assets/games/three-switches/game.json",
        "assets/games/observatory/content/game.json",
        "assets/games/observatory/content/game-audio.json",
        "assets/games/observatory/content/closed-gate.json",
        "assets/games/timed-relay/content/game.json",
    ] {
        assert_valid(&game, &read(path));
    }
    let pack = validator("asset-pack");
    for path in [
        "assets/models/cc0/pack.json",
        "assets/games/blueengine-sandbox/assets.json",
    ] {
        assert_valid(&pack, &read(path));
    }
}

#[test]
fn published_definition_anchors_keep_their_v1_contracts() {
    let asset = read("assets/models/cc0/pack.json")["assets"][0].clone();
    for (kind, anchor, valid, invalid) in [
        (
            "game",
            "condition",
            json!({"counter":"switches","equals":1}),
            json!({"all":[]}),
        ),
        ("patch", "vector", json!([0, 0, 0]), json!([0, 0])),
        ("patch", "id", json!("box"), json!("bad id!")),
        ("patch", "ids", json!(["a", "b"]), json!(["bad id!"])),
        ("asset-pack", "id", json!("props/chair"), json!("Bad ID!")),
        ("asset-pack", "vec3", json!([1, 2]), json!([1, 2, 3, 4])),
        ("asset-pack", "asset", asset, json!({"id":"incomplete"})),
    ] {
        let schema = generate(kind).unwrap();
        assert!(
            schema["$defs"].get(anchor).is_some(),
            "published {kind} #{anchor} anchor lost"
        );
        let fragment = json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
            "$defs":schema["$defs"],"$ref":format!("#/$defs/{anchor}")});
        let validator = jsonschema::validator_for(&fragment).unwrap();
        assert_valid(&validator, &valid);
        assert!(
            !validator.is_valid(&invalid),
            "{kind} #{anchor} legacy constraint lost"
        );
    }
}

#[test]
fn rule_conditions_and_actions_retain_their_constraints() {
    let validator = validator("game");
    let original = read("assets/games/three-switches/game.json");
    for bad in [
        json!({}),
        json!({"counter":"switches"}),
        json!({"counter":"switches","equals":1000001}),
        json!({"counter":"switches","equals":0,"all":[{"counter":"switches","equals":0}]}),
        json!({"all":[]}),
        json!({"any":[]}),
        json!({"counter":null,"equals":0}),
        json!({"counter":"switches","modulo":0,"equals":0}),
        json!({"not":null}),
    ] {
        let mut value = original.clone();
        value["rules"][0]["condition"] = bad.clone();
        assert!(
            !validator.is_valid(&value),
            "invalid condition accepted: {bad}"
        );
    }
    for field in ["on_interact", "on_enter", "on_exit", "on_timer"] {
        let mut value = original.clone();
        value["rules"][0][field] = json!("bad id!");
        assert!(!validator.is_valid(&value), "{field} identity bound lost");
    }
    for bad in [
        json!([]),
        json!([{"action":"increment","counter":"switches","amount":1000001}]),
        json!([{"action":"complete","unknown":true}]),
        json!([{"action":"new_gameplay_language"}]),
    ] {
        let mut value = original.clone();
        value["rules"][0]["actions"] = bad.clone();
        assert!(
            !validator.is_valid(&value),
            "invalid action accepted: {bad}"
        );
    }
}

#[test]
fn game_profile_vectors_and_collection_limits_are_not_lost() {
    let validator = validator("game");
    let original = read("assets/games/three-switches/game.json");
    for (pointer, bad) in [
        ("/schema_version", json!(2)),
        ("/name", json!("")),
        ("/map", json!("")),
        ("/player_profile/height", json!(3.01)),
        ("/player_profile/radius", json!(0.01)),
        ("/player_profile/jump_height", json!(-0.01)),
        ("/player_profile/walk_speed", json!(21)),
        ("/spawn_points/0/feet", json!([0, 0])),
        ("/spawn_points/0/feet", json!([0, 1001, 0])),
        ("/rules", json!([])),
        ("/spawn_points", json!([])),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        assert!(!validator.is_valid(&value), "constraint lost: {pointer}");
    }
    let mut value = original.clone();
    value["spawn_points"] = json!(vec![original["spawn_points"][0].clone(); 9]);
    assert!(!validator.is_valid(&value));
    for collection in [
        "interactables",
        "trigger_zones",
        "movers",
        "timers",
        "rules",
    ] {
        let mut value = original.clone();
        let item = match collection {
            "interactables" => json!({"entity":"switch","enabled":true}),
            "trigger_zones" => {
                json!({"id":"zone","bounds":{"min":[0,0,0],"max":[1,1,1]},"enabled":true})
            }
            "movers" => json!({"id":"door","entity":"door","translation":[1,0,0]}),
            "timers" => json!({"id":"clock","duration_ticks":60}),
            "rules" => original["rules"][0].clone(),
            _ => unreachable!(),
        };
        value[collection] = json!(vec![item.clone(); 64]);
        assert_valid(&validator, &value);
        value[collection] = json!(vec![item; 65]);
        assert!(
            !validator.is_valid(&value),
            "collection cap lost: {collection}"
        );
    }
    let mut value = original;
    value["counters"] = serde_json::to_value(
        (0..33)
            .map(|i| (format!("c{i}"), 0))
            .collect::<std::collections::BTreeMap<_, _>>(),
    )
    .unwrap();
    assert!(!validator.is_valid(&value));
}

#[test]
fn nullable_presentation_and_optional_rule_fields_remain_compatible() {
    let validator = validator("game");
    let mut value = read("assets/games/three-switches/game.json");
    value["presentation"] = Value::Null;
    assert_valid(&validator, &value);
    value["presentation"] = json!({"objective":null,"success":null,"failure":null,
        "counters":{"switches":{"label":null}},"hud":{"width":null},"audio":null});
    for field in [
        "condition",
        "on_interact",
        "on_enter",
        "on_exit",
        "on_timer",
    ] {
        value["rules"][0][field] = Value::Null;
    }
    assert_valid(&validator, &value);
    // Optional presentation does not relax malformed non-null values.
    value["presentation"]["hud"]["width"] = json!(901);
    assert!(!validator.is_valid(&value));
}

#[test]
fn valid_patch_operations_and_invalid_transactions() {
    let validator = validator("patch");
    let operations = json!([
        {"op":"add_box","id":"box","label":"Box","center":[0,0,0],"half_extents":[1,1,1],"color":[0,0.5,1]},
        {"op":"add_prop","id":"chair","label":"Chair","kind":"chair","origin":[0,0,0]},
        {"op":"translate","nodes":["box"],"colliders":["box"],"entities":["box"],"delta":[1,0,0]},
        {"op":"remove","nodes":["box"],"colliders":["box"],"entities":["box"]}
    ]);
    assert_valid(&validator, &operations);
    for (pointer, bad) in [
        ("/0/half_extents", json!([0, 1, 1])),
        ("/0/color", json!([1.01, 0, 0])),
        ("/0/center", json!([1001, 0, 0])),
        ("/1/kind", json!("unsupported")),
        ("/2/nodes", json!(["bad id!"])),
    ] {
        let mut value = operations.clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        assert!(
            !validator.is_valid(&value),
            "patch constraint lost: {pointer}"
        );
    }
    assert!(!validator.is_valid(&json!(vec![operations[3].clone(); 1001])));
}

#[test]
fn asset_identity_provenance_and_shapes_reject_bad_metadata() {
    let validator = validator("asset-pack");
    let original = read("assets/models/cc0/pack.json");
    for (pointer, bad) in [
        ("/schema_version", json!(2)),
        ("/pack/version", json!("latest")),
        ("/pack/license", json!("")),
        ("/pack/scope", json!("unknown")),
        ("/assets/0/id", json!("Bad ID!")),
        ("/assets/0/source/sha256", json!("abc")),
        ("/assets/0/geometry/triangle_count", json!(0)),
        ("/assets/0/geometry/half_extents", json!([1, 2])),
        ("/assets/0/source/source_url", json!("not a URI")),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        assert!(
            !validator.is_valid(&value),
            "asset constraint lost: {pointer}"
        );
    }
    let mut value = original;
    value["assets"][0]["unknown"] = json!(true);
    assert!(!validator.is_valid(&value));
    value["assets"][0]
        .as_object_mut()
        .unwrap()
        .remove("unknown");
    value["assets"][0]["geometry"]["forward"] = Value::Null;
    assert!(
        !validator.is_valid(&value),
        "optional enum must reject explicit null as v1 did"
    );
}

#[test]
fn mcp_typed_inputs_keep_names_required_fields_and_valid_inputs() {
    let schemas = generate("mcp").unwrap();
    assert_eq!(schemas.as_object().unwrap().len(), 13);
    for (name, valid, invalid) in [
        ("search", json!({"query":"controller"}), json!({})),
        (
            "lint",
            json!({"map_path":"map.json","strict":true}),
            json!({"map_path":"map.json","strict":2}),
        ),
        (
            "walk_auto",
            json!({"map_path":"map.json","from":[0,0],"to":[1,1]}),
            json!({"map_path":"map.json","from":"bad","to":[1,1]}),
        ),
        (
            "src_lookup",
            json!({"action":"find","query":"controller"}),
            json!({"action":"unknown"}),
        ),
        (
            "scatter",
            json!({"map_path":"map.json","kind":"chair","count":1,"rect":[0,0,2,2],"seed":u64::MAX,"out_path":"new.json"}),
            json!({"kind":"chair"}),
        ),
    ] {
        let validator = jsonschema::validator_for(&schemas[name]).unwrap();
        assert_valid(&validator, &valid);
        assert!(
            !validator.is_valid(&invalid),
            "invalid MCP {name} input accepted"
        );
    }
}

#[test]
fn schema_validity_does_not_replace_authoritative_semantic_validation() {
    use vesper3d::viewer::game::GameDocument;
    let mut value = read("assets/games/three-switches/game.json");
    value["rules"][0]["actions"] =
        json!([{"action":"increment","counter":"not_declared","amount":1}]);
    assert_valid(&validator("game"), &value);
    let document: GameDocument = serde_json::from_value(value.clone()).unwrap();
    let loaded = GameDocument::load(&root().join("assets/games/three-switches/game.json")).unwrap();
    let error = document.validate(&loaded.map).unwrap_err().to_string();
    assert!(
        error.contains(&document.rules[0].id),
        "diagnostic must locate the rule: {error}"
    );
    value["rules"][0]["actions"] = json!([{"action":"complete"}]);
    value["rules"][0]["condition"] = json!({"counter":"not_declared","equals":0});
    assert_valid(&validator("game"), &value);
    let document: GameDocument = serde_json::from_value(value).unwrap();
    let error = document.validate(&loaded.map).unwrap_err().to_string();
    assert!(
        error.contains("not_declared") && error.contains(&document.rules[0].id),
        "semantic diagnostic must identify both rule and missing counter: {error}"
    );
}

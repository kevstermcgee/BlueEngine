//! Opt-in authoring schemas. Serde types own shape; Rust hooks retain v1 constraints.
use crate::viewer::{authoring::Edit, game::GameDocument};
use schemars::{generate::SchemaSettings, JsonSchema, Schema};
use serde_json::{json, Map, Value};

mod assets;

fn property<'a>(schema: &'a mut Schema, name: &str) -> &'a mut Value {
    schema
        .as_object_mut()
        .and_then(|o| o.get_mut("properties"))
        .and_then(Value::as_object_mut)
        .and_then(|o| o.get_mut(name))
        .unwrap_or_else(|| panic!("schema constraint refers to missing typed field {name}"))
}

fn set(value: &mut Value, constraints: Value) {
    let object = value
        .as_object_mut()
        .expect("typed field schema is an object");
    object.extend(
        constraints
            .as_object()
            .expect("constraints are an object")
            .clone(),
    );
}

fn non_null(value: &mut Value) {
    if let Some(variants) = value.get_mut("enum").and_then(Value::as_array_mut) {
        variants.retain(|variant| !variant.is_null());
    }
    if let Some(variants) = value.get_mut("anyOf").and_then(Value::as_array_mut) {
        variants.retain(|v| v.get("type") != Some(&json!("null")));
        if variants.len() == 1 {
            let single = variants.remove(0);
            value.as_object_mut().unwrap().remove("anyOf");
            set(value, single);
        }
    }
    if let Some(types) = value.get_mut("type").and_then(Value::as_array_mut) {
        types.retain(|t| t != "null");
        if types.len() == 1 {
            let single = types.remove(0);
            value["type"] = single;
        }
    }
    if value.get("default") == Some(&Value::Null) {
        value.as_object_mut().unwrap().remove("default");
    }
}

fn id(value: &mut Value, max: usize) {
    set(
        value,
        json!({"minLength":1,"maxLength":max,"pattern":"^[A-Za-z0-9_/-]+$"}),
    );
}

fn vector(value: &mut Value, count: usize, minimum: f64, maximum: f64) {
    non_null(value);
    value.as_object_mut().unwrap().remove("prefixItems");
    set(
        value,
        json!({"type":"array","minItems":count,"maxItems":count,
        "items":{"type":"number","minimum":minimum,"maximum":maximum}}),
    );
}

fn require(schema: &mut Schema, names: &[&str]) {
    let object = schema.as_object_mut().unwrap();
    let required = object
        .entry("required")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap();
    for name in names {
        if !required.contains(&json!(name)) {
            required.push(json!(name));
        }
    }
}

pub fn profile(schema: &mut Schema) {
    for field in ["height", "crouched_height"] {
        set(property(schema, field), json!({"minimum":0.1,"maximum":3}));
    }
    set(
        property(schema, "radius"),
        json!({"minimum":0.05,"maximum":1}),
    );
    set(
        property(schema, "eye_height"),
        json!({"minimum":0.05,"maximum":3}),
    );
    for field in ["walk_speed", "sprint_speed", "crouch_speed"] {
        set(property(schema, field), json!({"minimum":0.1,"maximum":20}));
    }
    set(
        property(schema, "jump_height"),
        json!({"minimum":0,"maximum":3}),
    );
}

pub fn collider(schema: &mut Schema) {
    for field in ["min", "max"] {
        vector(property(schema, field), 3, -1000., 1000.);
    }
}

pub fn spawn_point(schema: &mut Schema) {
    id(property(schema, "id"), 64);
    vector(property(schema, "feet"), 3, -1000., 1000.);
}

pub fn interactable(schema: &mut Schema) {
    id(property(schema, "entity"), 64);
}

pub fn trigger_zone(schema: &mut Schema) {
    id(property(schema, "id"), 64);
    require(schema, &["enabled"]);
}

pub fn mover(schema: &mut Schema) {
    for field in ["id", "entity"] {
        id(property(schema, field), 64);
    }
    vector(property(schema, "translation"), 3, -1000., 1000.);
    set(
        property(schema, "duration_ticks"),
        json!({"minimum":1,"maximum":3600}),
    );
}

pub fn timer(schema: &mut Schema) {
    id(property(schema, "id"), 64);
    set(
        property(schema, "duration_ticks"),
        json!({"minimum":1,"maximum":36000}),
    );
}

pub fn condition(schema: &mut Schema) {
    use crate::viewer::game::{MAX_CONDITION_NODES, MAX_COUNTER};
    for field in [
        "counter",
        "modulo",
        "equals",
        "not_equals",
        "less_than",
        "greater_than",
        "at_most",
        "at_least",
        "all",
        "any",
        "not",
    ] {
        non_null(property(schema, field));
    }
    id(property(schema, "counter"), 64);
    for field in [
        "equals",
        "not_equals",
        "less_than",
        "greater_than",
        "at_most",
        "at_least",
    ] {
        set(
            property(schema, field),
            json!({"minimum":-MAX_COUNTER,"maximum":MAX_COUNTER}),
        );
    }
    set(
        property(schema, "modulo"),
        json!({"minimum":1,"maximum":MAX_COUNTER}),
    );
    for field in ["all", "any"] {
        set(
            property(schema, field),
            json!({"minItems":1,"maxItems":MAX_CONDITION_NODES}),
        );
    }
    schema.as_object_mut().unwrap().insert("oneOf".into(), json!([
        {"required":["counter"],"anyOf":[{"required":["equals"]},{"required":["not_equals"]},
        {"required":["less_than"]},{"required":["greater_than"]},{"required":["at_most"]},{"required":["at_least"]}]},
        {"required":["all"]},{"required":["any"]},{"required":["not"]}
    ]));
}

fn variants(schema: &mut Schema, mut hook: impl FnMut(&mut Map<String, Value>)) {
    for variant in schema
        .as_object_mut()
        .unwrap()
        .get_mut("oneOf")
        .expect("tagged enum variants")
        .as_array_mut()
        .unwrap()
    {
        hook(variant["properties"].as_object_mut().unwrap());
    }
}

pub fn game_action(schema: &mut Schema) {
    use crate::viewer::game::MAX_COUNTER;
    variants(schema, |fields| {
        for name in ["counter", "entity", "mover", "timer"] {
            if let Some(value) = fields.get_mut(name) {
                id(value, 64);
            }
        }
        for name in ["amount", "value"] {
            if let Some(value) = fields.get_mut(name) {
                set(value, json!({"minimum":-MAX_COUNTER,"maximum":MAX_COUNTER}));
            }
        }
    });
}

pub fn rule(schema: &mut Schema) {
    id(property(schema, "id"), 64);
    for field in ["on_interact", "on_enter", "on_exit", "on_timer"] {
        id(property(schema, field), 64);
    }
    set(
        property(schema, "actions"),
        json!({"minItems":1,"maxItems":4}),
    );
}

pub fn game_document(schema: &mut Schema) {
    use crate::viewer::game::{MAX_COUNTER, MAX_GAME_COUNTERS, MAX_GAME_FLAGS};
    set(property(schema, "schema_version"), json!({"const":1}));
    set(
        property(schema, "name"),
        json!({"minLength":1,"maxLength":100}),
    );
    set(property(schema, "map"), json!({"minLength":1}));
    set(
        property(schema, "spawn_points"),
        json!({"minItems":1,"maxItems":8}),
    );
    let counters = property(schema, "counters");
    set(
        counters,
        json!({"maxProperties":MAX_GAME_COUNTERS,"propertyNames":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9_/-]+$"}}),
    );
    set(
        &mut counters["additionalProperties"],
        json!({"minimum":-MAX_COUNTER,"maximum":MAX_COUNTER}),
    );
    for field in ["interactables", "trigger_zones", "movers", "timers"] {
        set(
            property(schema, field),
            json!({"minItems":0,"maxItems":MAX_GAME_FLAGS}),
        );
    }
    set(
        property(schema, "rules"),
        json!({"minItems":1,"maxItems":MAX_GAME_FLAGS}),
    );
}

pub fn stock_presentation(schema: &mut Schema) {
    for field in ["objective", "success", "failure"] {
        let value = property(schema, field);
        set(
            value,
            json!({"minLength":1,"maxLength":200,"pattern":"^[ -~]+$"}),
        );
    }
}

pub fn counter_display(schema: &mut Schema) {
    let label = property(schema, "label");
    set(
        label,
        json!({"minLength":1,"maxLength":48,"pattern":"^[ -~]+$"}),
    );
    set(
        property(schema, "units"),
        json!({"maxLength":12,"pattern":"^[ -~]*$"}),
    );
}

pub fn palette(schema: &mut Schema) {
    for field in [
        "background",
        "panel",
        "text",
        "accent",
        "success",
        "failure",
    ] {
        vector(property(schema, field), 4, 0., 1.);
    }
}

pub fn hud(schema: &mut Schema) {
    set(
        property(schema, "scale"),
        json!({"minimum":0.75,"maximum":1.5}),
    );
    set(
        property(schema, "margin"),
        json!({"minimum":8,"maximum":48}),
    );
    let width = property(schema, "width");
    set(width, json!({"minimum":240,"maximum":900}));
}

pub fn stock_audio(schema: &mut Schema) {
    set(
        property(schema, "bundle"),
        json!({"minLength":1,"maxLength":200,"pattern":r"^[^\\:]+$"}),
    );
    set(property(schema, "cues"), json!({"maxItems":64}));
    set(
        property(schema, "music"),
        json!({"maxProperties":16,"propertyNames":{"type":"string","minLength":1,"maxLength":48,"pattern":"^[A-Za-z0-9_-]+$"}}),
    );
}

pub fn cue(schema: &mut Schema) {
    set(
        property(schema, "cue"),
        json!({"minLength":1,"maxLength":48,"pattern":"^[A-Za-z0-9_-]+$"}),
    );
    set(property(schema, "volume"), json!({"minimum":0,"maximum":1}));
}

pub fn cue_event(schema: &mut Schema) {
    use crate::viewer::game::MAX_COUNTER;
    variants(schema, |fields| {
        if let Some(value) = fields.get_mut("value") {
            set(value, json!({"minimum":-MAX_COUNTER,"maximum":MAX_COUNTER}));
        }
    });
}

pub fn music_layer(schema: &mut Schema) {
    set(property(schema, "level"), json!({"minimum":0,"maximum":1}));
}

pub fn counter_mix(schema: &mut Schema) {
    use crate::viewer::game::MAX_COUNTER;
    for field in ["from", "to"] {
        set(
            property(schema, field),
            json!({"minimum":-MAX_COUNTER,"maximum":MAX_COUNTER}),
        );
    }
}

pub fn edit(schema: &mut Schema) {
    variants(schema, |fields| {
        if let Some(value) = fields.get_mut("id") {
            id(value, 100);
        }
        if let Some(value) = fields.get_mut("label") {
            set(value, json!({"maxLength":100}));
        }
        for field in ["center", "origin", "delta"] {
            if let Some(value) = fields.get_mut(field) {
                vector(value, 3, -1000., 1000.);
            }
        }
        if let Some(value) = fields.get_mut("half_extents") {
            vector(value, 3, 0., 1000.);
            value["items"].as_object_mut().unwrap().remove("minimum");
            value["items"]["exclusiveMinimum"] = json!(0);
        }
        if let Some(value) = fields.get_mut("color") {
            vector(value, 3, 0., 1.);
        }
        for field in ["nodes", "colliders", "entities"] {
            if let Some(value) = fields.get_mut(field) {
                id(&mut value["items"], 100);
            }
        }
        if let Some(value) = fields.get_mut("kind") {
            set(
                value,
                json!({"enum":generated::<crate::viewer::props::PropKind>()["enum"]}),
            );
        }
    });
}

pub fn generated<T: JsonSchema>() -> Value {
    let settings =
        SchemaSettings::draft2020_12().with(|settings| settings.inline_subschemas = true);
    serde_json::to_value(settings.into_generator().into_root_schema_for::<T>()).unwrap()
}

pub fn mcp_arguments(schema: &mut Schema) {
    let properties = schema
        .as_object_mut()
        .unwrap()
        .entry("properties")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap();
    for value in properties.values_mut() {
        non_null(value);
    }
}

pub fn generate(kind: &str) -> crate::Result<Value> {
    let schema = match kind {
        "game" => {
            let mut schema = generated::<GameDocument>();
            schema["title"] = json!("BE2 GameDocument v1");
            schema["$comment"] = json!("Generated from Rust. Also run game-validate: references, byte limits, condition depth/nodes, profiles, paths and geometry need semantic validation.");
            schema
        }
        "patch" => {
            let mut schema = generated::<Vec<Edit>>();
            schema["title"] = json!("BE2 edit transaction v1");
            schema["maxItems"] = json!(1000);
            let box_fields = &schema["items"]["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .find(|operation| operation["properties"]["op"]["const"] == "add_box")
                .expect("typed Edit::AddBox owns legacy patch definitions")["properties"];
            schema["$defs"] = json!({"vector":box_fields["center"],"id":box_fields["id"],
                "ids":{"type":"array","items":{"$ref":"#/$defs/id"}}});
            schema
        }
        "asset-pack" => {
            let mut schema = generated::<assets::AssetPack>();
            let asset = schema["properties"]["assets"]["items"].take();
            let mut vector = asset["properties"]["geometry"]["properties"]["half_extents"].clone();
            // The published vec3 fragment historically allowed shorter arrays; the pack's
            // typed vectors still enforce the three components runtime validation requires.
            vector.as_object_mut().unwrap().remove("minItems");
            schema["$defs"] = json!({"id":schema["properties"]["pack"]["properties"]["id"],
                "vec3":vector,"asset":asset});
            schema["properties"]["assets"]["items"] = json!({"$ref":"#/$defs/asset"});
            schema
        }
        "mcp" => Value::Object(crate::viewer::mcp::input_schemas()),
        _ => return Err(format!("unknown schema {kind}; use game, patch, asset-pack, mcp").into()),
    };
    Ok(schema)
}

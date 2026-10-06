//! Typed asset-pack v1 authoring contract; adapters/runtime assets keep their own validation.
use super::{non_null, property, set};
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = asset_pack)]
pub struct AssetPack {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub schema_version: u32,
    pub pack: Pack,
    pub assets: Vec<Asset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(transform = identifier)]
pub struct Id(String);

fn identifier(schema: &mut Schema) {
    schema.as_object_mut().unwrap().extend(
        json!({
            "pattern":"^[a-z0-9]+(?:[._/-][a-z0-9]+)*$", "maxLength":100
        })
        .as_object()
        .unwrap()
        .clone(),
    );
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = pack)]
pub struct Pack {
    pub id: Id,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub scope: Scope,
    pub license: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    GameLocal,
    Shared,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = asset)]
pub struct Asset {
    pub id: Id,
    pub label: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: AssetType,
    pub source: Source,
    pub taxonomy: Taxonomy,
    pub geometry: Geometry,
    pub physics: Physics,
    pub lifecycle: Lifecycle,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    Prefab,
    Mesh,
    Material,
    Texture,
    Audio,
    Animation,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = source)]
pub struct Source {
    pub path: String,
    pub format: String,
    pub method: Method,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_sha256: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Reused,
    Modified,
    Generated,
    Imported,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = taxonomy)]
pub struct Taxonomy {
    pub categories: Vec<Id>,
    pub tags: Vec<Id>,
    pub aliases: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = geometry)]
pub struct Geometry {
    pub units: Units,
    pub origin: Origin,
    pub half_extents: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forward: Option<Forward>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds_min: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds_max: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triangle_count: Option<u64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Meters,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    BottomCenter,
    Center,
    Custom,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub enum Forward {
    #[serde(rename = "+x")]
    PositiveX,
    #[serde(rename = "-x")]
    NegativeX,
    #[serde(rename = "+z")]
    PositiveZ,
    #[serde(rename = "-z")]
    NegativeZ,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = physics)]
pub struct Physics {
    pub mobility: Mobility,
    pub collision: Collision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Mobility {
    Static,
    Dynamic,
    RuntimePolicy,
    None,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Collision {
    Aabb,
    Compound,
    ConservativeBounds,
    Mesh,
    None,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = lifecycle)]
pub struct Lifecycle {
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_from: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Experimental,
    Candidate,
    Stable,
    Deprecated,
}

fn strict_options(schema: &mut Schema) {
    for value in schema
        .as_object_mut()
        .unwrap()
        .get_mut("properties")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        non_null(value);
    }
}

fn asset_pack(schema: &mut Schema) {
    strict_options(schema);
    set(property(schema, "schema_version"), json!({"const":1}));
    schema
        .as_object_mut()
        .unwrap()
        .insert("title".into(), json!("BlueEngine Asset Pack"));
    schema.as_object_mut().unwrap().insert(
        "$id".into(),
        json!("https://blueengine.dev/schemas/asset-pack-v1.json"),
    );
}

fn pack(schema: &mut Schema) {
    strict_options(schema);
    for name in ["name", "license"] {
        set(
            property(schema, name),
            json!({"minLength":1,"maxLength":100}),
        );
    }
    set(
        property(schema, "version"),
        json!({"pattern":r"^[0-9]+\.[0-9]+\.[0-9]+$"}),
    );
    set(property(schema, "description"), json!({"maxLength":500}));
}

fn asset(schema: &mut Schema) {
    set(
        property(schema, "label"),
        json!({"minLength":1,"maxLength":100}),
    );
    set(
        property(schema, "description"),
        json!({"minLength":1,"maxLength":500}),
    );
}

fn source(schema: &mut Schema) {
    strict_options(schema);
    for name in ["path", "format"] {
        set(property(schema, name), json!({"minLength":1}));
    }
    set(property(schema, "attribution"), json!({"maxLength":500}));
    set(property(schema, "source_url"), json!({"format":"uri"}));
    for name in ["sha256", "original_sha256"] {
        set(property(schema, name), json!({"pattern":"^[a-f0-9]{64}$"}));
    }
}

fn taxonomy(schema: &mut Schema) {
    set(property(schema, "categories"), json!({"minItems":1}));
    set(
        &mut property(schema, "aliases")["items"],
        json!({"minLength":1}),
    );
}

fn geometry(schema: &mut Schema) {
    strict_options(schema);
    set(property(schema, "triangle_count"), json!({"minimum":1}));
}

fn physics(schema: &mut Schema) {
    strict_options(schema);
    set(property(schema, "metadata"), json!({"minLength":1}));
}

fn lifecycle(schema: &mut Schema) {
    strict_options(schema);
    set(property(schema, "notes"), json!({"maxLength":500}));
}

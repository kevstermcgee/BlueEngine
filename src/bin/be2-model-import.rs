//! Low-level converter. Use tools/assets.py import-model for provenance/catalog/atomic pack writes.
use std::path::Path;

fn run() -> Result<serde_json::Value, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 && !(args.len() == 3 && args[2] == "--repair-degenerate") {
        return Err(
            "usage: be2-model-import SOURCE.glb SCALE (outputs JSON; no files written)".into(),
        );
    }
    let scale = args[1].parse().map_err(|_| "scale must be a number")?;
    let imported =
        vesper3d::model_import::import_with_options(Path::new(&args[0]), scale, args.len() == 3)?;
    Ok(
        serde_json::json!({"api_version":1,"ok":true,"model":imported.model,"collider":imported.collider,"warnings":imported.warnings}),
    )
}

fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"api_version":1,"ok":false,"error":error,"next":"Inspect source geometry/material; triangulate/bake, then retry tools/assets.py import-model. Previous packs remain valid."})
            );
            std::process::exit(1);
        }
    }
}

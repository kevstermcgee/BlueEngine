//! Internal generator for the existing schemas maintenance command.
//! Keeping this small avoids selecting physics/networking just to inspect Rust schemas.
fn main() -> vesper3d::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err("Usage: be2-schema KIND (game, patch, asset-pack, mcp); use python tools/be2.py schemas --write".into());
    }
    let value = vesper3d::authoring_schemas::generate(&args[0])?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

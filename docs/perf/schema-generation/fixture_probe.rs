use serde_json::{json, Value};
use std::{env, fs, time::Instant};
fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let schema: Value = serde_json::from_slice(&fs::read(&args[0]).unwrap()).unwrap();
    let start = Instant::now();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let results: Vec<_> = args[1..]
        .iter()
        .map(|path| {
            let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            let errors: Vec<_> = validator
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            json!({"file": path, "valid": errors.is_empty(), "errors": errors})
        })
        .collect();
    println!(
        "{}",
        json!({"schema": args[0], "seconds": start.elapsed().as_secs_f64(), "results": results})
    );
}

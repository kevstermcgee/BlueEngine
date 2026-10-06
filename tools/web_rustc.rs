//! Cargo hashes local checkout paths into crate identities. Normalize those identities for web releases.
use std::{env, ffi::OsString, path::PathBuf, process::Command};

fn normalized_package() -> Result<String, String> {
    let package =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("Missing Cargo package path")?);
    let package = package.canonicalize().unwrap_or(package);
    for (key, label) in [
        ("BE2_WASM_GAME_ROOT", "/game"),
        ("BE2_WASM_ENGINE_ROOT", "/blueengine"),
        ("BE2_WASM_CARGO_HOME", "/cargo"),
    ] {
        let prefix = PathBuf::from(env::var_os(key).ok_or_else(|| format!("Missing {key}"))?);
        let prefix = prefix.canonicalize().unwrap_or(prefix);
        if let Ok(relative) = package.strip_prefix(prefix) {
            return Ok(format!(
                "{label}/{}",
                relative.to_string_lossy().replace('\\', "/")
            ));
        }
    }
    Err("Compiler package is outside the recorded engine, game and Cargo source roots".into())
}

fn metadata(args: &[OsString]) -> Result<String, String> {
    let mut fields = vec![
        "blueengine-web-crate-identity-v1".to_owned(),
        normalized_package()?,
        env::var("CARGO_PKG_NAME").unwrap_or_default(),
        env::var("CARGO_PKG_VERSION").unwrap_or_default(),
    ];
    for pair in args.windows(2) {
        let key = pair[0].to_string_lossy();
        let value = pair[1].to_string_lossy();
        if matches!(
            key.as_ref(),
            "--crate-name" | "--crate-type" | "--target" | "--cfg"
        ) {
            fields.push(format!("{key}={value}"));
        } else if key == "-C"
            && !value.starts_with("metadata=")
            && !value.starts_with("extra-filename=")
        {
            fields.push(format!("codegen={value}"));
        }
    }
    if args.iter().any(|arg| arg == "--test") {
        fields.push("test-harness".into());
    }
    fields.sort();
    // This is a deterministic compiler namespace, not the release's cryptographic source/content hash.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in fields.join("\0").bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    Ok(format!("be2-web-v1-{hash:016x}"))
}

fn main() {
    let mut incoming = env::args_os().skip(1);
    let compiler = incoming
        .next()
        .expect("Cargo must supply rustc to its wrapper");
    let mut args: Vec<_> = incoming.collect();
    if args
        .windows(2)
        .any(|pair| pair[0] == "-C" && pair[1].to_string_lossy().starts_with("metadata="))
    {
        let identity = metadata(&args).unwrap_or_else(|error| {
            eprintln!("Reproducible browser compiler: {error}");
            std::process::exit(1);
        });
        for index in 1..args.len() {
            if args[index - 1] == "-C" && args[index].to_string_lossy().starts_with("metadata=") {
                args[index] = format!("metadata={identity}").into();
            }
        }
    }
    let status = Command::new(compiler)
        .args(args)
        .status()
        .expect("Unable to execute rustc");
    std::process::exit(status.code().unwrap_or(1));
}

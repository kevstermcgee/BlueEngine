use bluedm::{server::Server, DEFAULT_KEY};
use std::sync::{atomic::AtomicBool, Arc};

fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let address = value_after(&args, "--listen").unwrap_or_else(|| "0.0.0.0:4100".into());
    let key = value_after(&args, "--key").unwrap_or_else(|| DEFAULT_KEY.into());
    Server::bind(&address, key)?.run(Arc::new(AtomicBool::new(false)), None)
}

fn value_after(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

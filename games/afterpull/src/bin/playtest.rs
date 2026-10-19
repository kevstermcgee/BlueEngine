//! Developer-only rendering-free adapter for the shared bounded playtest runner.
use std::io::Read;

fn run() -> Result<afterpull::playtest::Report, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("playtest") => {
            let seed = args
                .get(1)
                .ok_or("missing seed")?
                .parse()
                .map_err(|_| "invalid seed")?;
            let ticks = args
                .get(2)
                .ok_or("missing ticks")?
                .parse()
                .map_err(|_| "invalid ticks")?;
            if !(22..=100000).contains(&ticks) {
                return Err("ticks must be 22..100000".into());
            }
            Ok(afterpull::playtest::run(
                seed,
                ticks,
                args.get(3).is_some_and(|v| v == "fault"),
            ))
        }
        Some("replay-playtest") => {
            let mut bytes = Vec::new();
            std::fs::File::open(args.get(1).ok_or("missing trace path")?)
                .map_err(|e| e.to_string())?
                .take(64 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 64 * 1024 * 1024 {
                return Err("trace exceeds 64 MiB".into());
            }
            Ok(afterpull::playtest::replay(
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            ))
        }
        _ => Err("playtest SEED TICKS [fault] | replay-playtest TRACE".into()),
    }
}

fn main() {
    match run() {
        Ok(report) => {
            println!("{}", serde_json::to_string(&report).unwrap());
            if !report.ok {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

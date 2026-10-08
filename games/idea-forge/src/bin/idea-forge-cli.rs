//! Headless batch generation; no window, service, network or custom save format.
use idea_forge::{markdown, Forge, GENRES};
use vesper3d::two_d::GameLogic;

fn main() {
    if let Err(error) = run() {
        eprintln!("Idea Forge: {error}");
        std::process::exit(2);
    }
}
fn run() -> Result<(), String> {
    let mut seed = 7_u64;
    let mut count = 5_usize;
    let mut genre = 0;
    let mut story = false;
    let mut format = "markdown".to_string();
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!("Idea Forge - distinct gameplay mechanic briefs\n\n--seed INTEGER       Reproducible order (default 7)\n--count INTEGER      1..36 ideas (default 5)\n--genre GENRE        all, puzzle, action, strategy\n--story              Include optional narrative\n--format FORMAT      markdown or json\n--output PATH        Create a new file; never overwrite an existing file\n\nWithout --output, writes to stdout. Genre catalogs contain 12 ideas each.");
            return Ok(());
        }
        if arg == "--story" {
            story = true;
            continue;
        }
        if !["--seed", "--count", "--genre", "--format", "--output"].contains(&arg.as_str()) {
            return Err(format!("Unknown argument {arg}. Use --help."));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {arg}"))?;
        match arg.as_str() {
            "--seed" => {
                seed = value
                    .parse()
                    .map_err(|_| "Seed must be a nonnegative integer")?
            }
            "--count" => {
                count = value
                    .parse()
                    .map_err(|_| "Count must be a positive integer")?
            }
            "--genre" => {
                genre = GENRES
                    .iter()
                    .position(|g| *g == value)
                    .ok_or("Genre must be all, puzzle, action or strategy")?
            }
            "--format" => format = value,
            "--output" => output = Some(value),
            _ => unreachable!(),
        }
    }
    let capacity = if genre == 0 { 36 } else { 12 };
    if count == 0 || count > capacity {
        return Err(format!(
            "Count must be 1..{capacity} for this genre; ideas never repeat."
        ));
    }
    if !["markdown", "json"].contains(&format.as_str()) {
        return Err("Format must be markdown or json".into());
    }
    let mut forge = Forge::new(seed);
    // The initial suggestion is unrelated to CLI filtering; generate a fresh filtered batch.
    forge.state.generated.clear();
    forge.state.genre = genre;
    for _ in 0..count {
        if !forge.generate() {
            return Err("Catalog exhausted".into());
        }
    }
    let ideas: Vec<_> = forge
        .state
        .generated
        .iter()
        .map(|id| forge.ideas[*id].clone())
        .collect();
    let text = if format == "json" {
        let mut exported = serde_json::to_value(&ideas).map_err(|e| e.to_string())?;
        if !story {
            for idea in exported.as_array_mut().unwrap() {
                idea.as_object_mut().unwrap().remove("story");
            }
        }
        serde_json::to_string_pretty(&serde_json::json!({"seed": seed, "genre":GENRES[genre],
            "originality":"Distinct within this catalog; external originality requires review", "ideas":exported})).map_err(|e| e.to_string())? + "\n"
    } else {
        markdown(&ideas, story)
    };
    if let Some(path) = output {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    } else {
        print!("{text}");
    }
    Ok(())
}

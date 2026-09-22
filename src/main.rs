#![forbid(unsafe_code)]
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
use vesper3d::{
    geometry::Compiled,
    output,
    render::{self, Options, Quality},
    scene::Scene,
    Result,
};

const HELP:&str="Vesper3D 0.1 — compact scenes, cinematic 3D\n\n  vesper3d validate SCENE.json\n  vesper3d frame SCENE.json OUTPUT.png [--time 2] [--quality high]\n  vesper3d contact SCENE.json OUTPUT.png [--quality draft]\n  vesper3d render SCENE.json OUTPUT.mp4 [--quality standard]\n  vesper3d bench SCENE.json [--time 2] [--quality draft]\n  vesper3d doctor\n  vesper3d reference\n\nOptions: --width EVEN --threads 1..64 --overwrite --ffmpeg PATH\nAll commands return JSON on stdout; render progress and errors use stderr.\nRead AI_REFERENCE.md for the complete scene contract.";
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", serde_json::json!({"ok":false,"error":e.to_string()}));
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        println!("{HELP}");
        return Ok(());
    };
    if matches!(command, "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if matches!(command, "--version" | "version") {
        println!("vesper3d 0.1.0");
        return Ok(());
    }
    if command == "reference" {
        print!("{}", include_str!("../AI_REFERENCE.md"));
        return Ok(());
    }
    let mut positional = vec![];
    let mut options = Options::default();
    let mut time = 0.;
    let mut width = None;
    let mut overwrite = false;
    let mut ffmpeg = "ffmpeg".to_string();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--overwrite" => overwrite = true,
            "--quality" | "--time" | "--width" | "--threads" | "--ffmpeg" => {
                let key = &args[i];
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| format!("missing value for {key}"))?;
                match key.as_str() {
                    "--quality" => options.quality = Quality::named(v)?,
                    "--time" => time = v.parse()?,
                    "--width" => width = Some(v.parse::<u32>()?),
                    "--threads" => options.threads = v.parse()?,
                    _ => ffmpeg = v.clone(),
                }
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}").into())
            }
            _ => positional.push(args[i].as_str()),
        }
        i += 1;
    }
    if !(1..=64).contains(&options.threads) {
        return Err("threads must be 1..64".into());
    }
    if command == "doctor" {
        let result = std::process::Command::new(&ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output();
        let (available, h264) = match result {
            Ok(r) => (
                r.status.success(),
                String::from_utf8_lossy(&r.stdout).contains("libx264"),
            ),
            Err(_) => (false, false),
        };
        println!(
            "{}",
            serde_json::json!({"engine":"vesper3d","version":"0.1.0","threads":options.threads,"ffmpeg":available,"h264":h264,"renderer":"native Rust CPU / BVH ray tracing"})
        );
        if !available || !h264 {
            return Err(
                "FFmpeg with libx264 is required for MP4; PNG rendering remains available".into(),
            );
        }
        return Ok(());
    }
    if !matches!(
        command,
        "validate" | "frame" | "contact" | "render" | "bench"
    ) {
        return Err(format!("unknown command {command}; use --help").into());
    }
    let expected = if matches!(command, "validate" | "bench") {
        1
    } else {
        2
    };
    if positional.len() != expected {
        return Err(format!("{command} expects {expected} path argument(s); use --help").into());
    }
    let path = Path::new(positional[0]);
    let base = path
        .parent()
        .filter(|x| !x.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut scene = Scene::load(path)?;
    if let Some(w) = width {
        if !(16..=3840).contains(&w) || w % 2 != 0 {
            return Err("width must be even and within 16..3840".into());
        }
        scene.size = [
            w,
            ((w as f64 * scene.size[1] as f64 / scene.size[0] as f64 / 2.).round() as u32 * 2)
                .max(16),
        ];
    }
    let compiled = Compiled::new(scene, base)?;
    if expected == 2 {
        let output_path = Path::new(positional[1]);
        let extension = if command == "render" { "mp4" } else { "png" };
        if !output_path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        {
            return Err(format!("{command} output must have .{extension} extension").into());
        }
        if let Ok(resolved) = output_path.canonicalize() {
            let mut inputs = vec![path.canonicalize()?];
            if let Some(a) = &compiled.scene.audio {
                inputs.push(vesper3d::scene::asset_path(base, a)?);
            }
            for n in &compiled.scene.nodes {
                if let Some(m) = &n.mesh {
                    inputs.push(vesper3d::scene::asset_path(base, m)?);
                }
            }
            if inputs.contains(&resolved) {
                return Err("output cannot overwrite a scene or input asset".into());
            }
        }
        if output_path.exists() && !overwrite {
            return Err("output already exists; use --overwrite".into());
        }
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let handler = cancel.clone();
    ctrlc::set_handler(move || handler.store(true, Ordering::Relaxed))?;
    match command {
        "validate" => {
            println!(
                "{}",
                serde_json::json!({"ok":true,"nodes":compiled.scene.nodes.len(),"primitives_at_zero":compiled.at(0.).instances.len(),"frames":compiled.scene.frames(),"size":compiled.scene.size})
            );
        }
        "render" => {
            if Path::new(positional[1]) == path {
                return Err("output cannot overwrite scene".into());
            }
            println!(
                "{}",
                output::video(
                    &compiled,
                    base,
                    Path::new(positional[1]),
                    &options,
                    overwrite,
                    &cancel,
                    &ffmpeg
                )?
            );
        }
        "frame" | "bench" => {
            let start = Instant::now();
            let data = render::frame(&compiled, time, &options, &cancel)?;
            let seconds = start.elapsed().as_secs_f64();
            if command == "frame" {
                output::png(
                    Path::new(positional[1]),
                    compiled.scene.size[0],
                    compiled.scene.size[1],
                    &data,
                    overwrite,
                )?;
            }
            println!(
                "{}",
                serde_json::json!({"ok":true,"seconds":seconds,"time":time,"size":compiled.scene.size,"samples":options.quality.samples,"threads":options.threads,"rgb_bytes":data.len()})
            );
        }
        "contact" => {
            let mut s = compiled.scene.clone();
            let aspect = s.size[1] as f32 / s.size[0] as f32;
            s.size = if aspect <= 1. {
                [384, ((384. * aspect / 2.).round() as u32 * 2).max(16)]
            } else {
                [((384. / aspect / 2.).round() as u32 * 2).max(16), 384]
            };
            let c = Compiled::new(s, base)?;
            let w = c.scene.size[0] as usize;
            let h = c.scene.size[1] as usize;
            let mut sheet = vec![0u8; w * h * 6 * 3];
            for k in 0..6 {
                let t = c.scene.duration * (k as f32 + 0.5) / 6.;
                let data = render::frame(&c, t, &options, &cancel)?;
                for y in 0..h {
                    let dst = ((k / 3 * h + y) * w * 3 + k % 3 * w) * 3;
                    sheet[dst..dst + w * 3].copy_from_slice(&data[y * w * 3..(y + 1) * w * 3]);
                }
            }
            output::png(
                Path::new(positional[1]),
                (w * 3) as u32,
                (h * 2) as u32,
                &sheet,
                overwrite,
            )?;
            println!(
                "{}",
                serde_json::json!({"ok":true,"output":positional[1],"frames":6})
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}

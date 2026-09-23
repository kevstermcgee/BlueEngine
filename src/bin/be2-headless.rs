//! A bounded local simulation runner; networking is supplied by PulseNet later.
use std::time::{Duration, Instant};
use vesper3d::viewer::{controller::Movement, simulation::HeadlessWorld};
fn main() -> vesper3d::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut ticks = 600_u64;
    let mut realtime = false;
    let mut map_file = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--ticks" => {
                index += 1;
                ticks = args.get(index).ok_or("--ticks needs a number")?.parse()?;
            }
            "--map" => {
                index += 1;
                map_file = Some(args.get(index).ok_or("--map needs a file")?);
            }
            "--realtime" => realtime = true,
            "--help" => {
                println!("be2-headless [--ticks N] [--realtime] [--map FILE]\nLocal two-player simulation benchmark; no network listener.");
                return Ok(());
            }
            other => return Err(format!("Unknown argument: {other}").into()),
        }
        index += 1;
    }
    if ticks == 0 || ticks > 10_000_000 {
        return Err("ticks must be 1..10000000".into());
    }
    let mut world = if let Some(path) = map_file {
        HeadlessWorld::with_room(
            vesper3d::viewer::authoring::MapDocument::load(std::path::Path::new(path))?.build()?,
        )
    } else {
        HeadlessWorld::new()?
    };
    world.join(1);
    world.join(2);
    let started = Instant::now();
    for i in 0..ticks {
        for id in [1, 2] {
            world.input(
                id,
                Movement {
                    forward: 1.,
                    jump: i % 120 == 0,
                    crouch: i % 240 > 180,
                    ..Default::default()
                },
                i as f32 * 0.013 + id as f32,
                0.,
            );
        }
        world.step();
        if realtime {
            let deadline = started + Duration::from_secs_f64((i + 1) as f64 / 60.);
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
    }
    println!(
        "BE2 headless: players=2 ticks={} elapsed_ms={:.3} mean_tick_us={:.3} realtime={}",
        world.tick,
        started.elapsed().as_secs_f64() * 1000.,
        started.elapsed().as_secs_f64() * 1_000_000. / ticks as f64,
        realtime
    );
    Ok(())
}

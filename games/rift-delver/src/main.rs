//! Rift Delver: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "fwd:0-200,look:0.01@0-100,jump@60"      drive the human input path from a cue script
//!   --seed N   --size WxH   --mute   --audible   --perf           reproducible run, window size, silence, frame times
//!   --shadows off|simple|full                          shadow tier for this run (Esc > Settings changes and remembers it)
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! F5 saves the run to the `quick` slot and F9 loads it. `--script` accepts `save@N` and `load@N` cues too.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use macroquad::prelude::*;
use rift_delver::{Event, Input, Kind, Phase, Sim, ARENA, BIOMES, PERKS, WEAPONS, WORKSHOP};
use std::sync::{Arc, OnceLock};
use vesper3d::viewer::{devkit::snapshot, gamepad::Button};
use vesper3d::viewer::{
    devkit::{
        beside_exe, downloads_dir, flag_value, has_flag, parse_size, sanitize_filename, synth, unique_path, Juice,
        Lifecycle, Notice, Settings, ShadowQuality,
    },
    game_client::{self, AudioMenu, GameShell},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, Shadows, SoundBank, Template, Tint, View},
};

/// Title, tagline and controls live in one file, shared with the build script and `scripts/ship.py`.
const IDENTITY: &str = include_str!("../assets/identity.json");

fn identity() -> Identity {
    Identity::parse(IDENTITY).expect("assets/identity.json is invalid; run: python scripts/ship.py verify")
}

fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let mut conf = game_client::window_config_with_icon(
        &identity().title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        conf.miniquad_conf.window_width = w.clamp(320, 7680) as i32;
        conf.miniquad_conf.window_height = h.clamp(240, 4320) as i32;
    }
    if has_flag(&args, "--novsync") {
        conf.miniquad_conf.platform.swap_interval = Some(0);
    }
    conf
}

/// Held device state; press edges travel separately as bits (`JUMP`).
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
    fire: bool,
}
const DASH: u32 = 1;
const JUMP: u32 = 2;
const PULSE: u32 = 4;
const INTERACT: u32 = 8;
const WEAPON: u32 = 16;
const CUES: [&str; 15] = [
    "fwd", "back", "left", "right", "fire", "dash", "jump", "pulse", "enter", "one", "two", "three", "four", "weapon",
    "look",
];
const SOUNDS: [synth::Preset; 13] = [
    synth::Preset::Shoot,
    synth::Preset::Hit,
    synth::Preset::Explosion,
    synth::Preset::Hurt,
    synth::Preset::Whoosh,
    synth::Preset::Zap,
    synth::Preset::Coin,
    synth::Preset::Success,
    synth::Preset::PowerUp,
    synth::Preset::GameOver,
    synth::Preset::Select,
    synth::Preset::Error,
    synth::Preset::Warning,
];
fn sound(p: synth::Preset) -> usize {
    SOUNDS.iter().position(|v| *v == p).unwrap_or(0)
}
/// Not every game needs music: generated ambient music can fight a gameplay mechanic that depends on
/// precise or diegetic audio (rhythm timing, sound-based detection, a soundtrack the game itself is
/// about), or just not suit the game's feel. Set this to `false` to ship with sound effects only; the
/// Settings screen adapts on its own (it drops the music toggle and "Save music" button, not just hides
/// them silently as broken). Default on because most games benefit from it, not because every game must
/// have it.
const HAS_MUSIC: bool = true;

/// Renders on the worker thread; `music_wav` is filled once so the Settings-screen "Save music" button
/// can hand the player the exact bytes the stem below plays, without regenerating or hitching.
/// `ambient_spec_for` keys the track to this game's own title and tagline (see assets/identity.json),
/// so a fresh game does not sound identical to every other game made from this template; replace the
/// spec with your own if your theme calls for something that heuristic cannot read from those words, or
/// turn [`HAS_MUSIC`] off if this game should not have music at all.
fn render_audio(music_wav: Arc<OnceLock<Vec<u8>>>, title: &str, tagline: &str) -> Rendered {
    let stems = if HAS_MUSIC {
        let spec = synth::ambient_spec_for(title, tagline);
        let ambient = synth::wav_bytes(&synth::ambient_loop(&spec), synth::RATE);
        let _ = music_wav.set(ambient.clone());
        vec![ambient]
    } else {
        Vec::new()
    };
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
            .collect(),
        stems,
    }
}

/// Write the currently playing ambient track to the player's Downloads folder as a `.wav`, named after
/// the game, never overwriting an earlier save of it.
fn save_music(music_wav: &OnceLock<Vec<u8>>, title: &str) -> Notice {
    let Some(bytes) = music_wav.get() else {
        return Notice {
            title: "NOT READY",
            detail: "the music is still rendering; try again in a moment".into(),
            color: [1., 0.7, 0.3],
            ok: false,
        };
    };
    let Some(dir) = downloads_dir() else {
        return Notice {
            title: "SAVE FAILED",
            detail: "could not find your Downloads folder".into(),
            color: [1., 0.5, 0.3],
            ok: false,
        };
    };
    let path = unique_path(&dir, &format!("{} - Ambient Music", sanitize_filename(title)), "wav");
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)) {
        Ok(()) => Notice { title: "MUSIC SAVED", detail: path.display().to_string(), color: [0.4, 0.9, 0.6], ok: true },
        Err(error) => Notice { title: "SAVE FAILED", detail: error.to_string(), color: [1., 0.5, 0.3], ok: false },
    }
}

fn v(p: vesper3d::math::V) -> Vec3 {
    vec3(p.0, p.1, p.2)
}
const ACCENT: [f32; 3] = [0.35, 0.95, 0.85];
fn palette(b: usize) -> ([f32; 3], [f32; 3], [f32; 3]) {
    match b {
        0 => ([0.12, 0.18, 0.22], [0.27, 0.37, 0.4], [0.35, 0.95, 0.85]),
        1 => ([0.19, 0.12, 0.15], [0.38, 0.22, 0.23], [1., 0.5, 0.19]),
        _ => ([0.12, 0.15, 0.24], [0.24, 0.31, 0.48], [0.6, 0.65, 1.]),
    }
}
struct Scene {
    sky: Vec<Mesh>,
    arena: Vec<Mesh>,
    enemies: Vec<Template>,
    drop: Template,
    bolt: Template,
    guns: Vec<Template>,
    rift: Template,
    biome: usize,
    depth: u32,
}
fn build_scene(sim: &Sim) -> Scene {
    let biome = sim.biome();
    let (dark, stone, light) = palette(biome);
    let mut sky = Template::new();
    sky.sky_dome(140., |e| [0.035 + 0.14 * (1. - e), 0.055 + 0.16 * (1. - e), 0.10 + 0.19 * (1. - e)], 28, 14);
    let mut arena = Template::new();
    arena.box_top(
        vec3(0., -0.5, 0.),
        vec3(ARENA, 0.5, ARENA),
        dark,
        [dark[0] + 0.04, dark[1] + 0.04, dark[2] + 0.04],
        0.,
    );
    // Inlaid separated tiles sit above the foundation; no coplanar markings.
    for x in -10i32..=10 {
        for z in -10i32..=10 {
            let tint = if (x + z).rem_euclid(3) == 0 { 1.10 } else { 1. };
            arena.box_(
                vec3(x as f32 * 2., 0.012, z as f32 * 2.),
                vec3(0.97, 0.01, 0.97),
                [dark[0] * tint, dark[1] * tint, dark[2] * tint],
                0.,
            );
        }
    }
    for w in &sim.walls[1..] {
        let c = (v(w.min) + v(w.max)) * 0.5;
        let h = (v(w.max) - v(w.min)) * 0.5;
        arena.box_top(c, h, dark, stone, 0.);
        if h.x < 8. && h.z < 8. {
            arena.box_(vec3(c.x, h.y * 2. + 0.05, c.z), vec3(h.x + 0.12, 0.08, h.z + 0.12), stone, 0.);
            for dx in [-1., 1.] {
                for dz in [-1., 1.] {
                    arena.box_(
                        vec3(c.x + dx * (h.x - 0.12), h.y, c.z + dz * (h.z - 0.12)),
                        vec3(0.07, h.y, 0.07),
                        light,
                        0.75,
                    );
                }
            }
        }
    }
    // Tall segmented pylons and distant silhouettes establish a large ruined world.
    for i in 0..12 {
        let a = i as f32 * std::f32::consts::TAU / 12.;
        let p = vec3(a.sin() * 25., 0., a.cos() * 25.);
        arena.cylinder(p, 1., 9., stone, 0., 6);
        arena.cylinder(p + vec3(0., 9.05, 0.), 1.2, 0.2, light, 0.8, 6);
        arena.cone(p + vec3(0., 9.3, 0.), 0.8, 0.2, 2.5, stone, 0., 6);
    }
    for i in 0..22 {
        let a = i as f32 * 2.399;
        let r = 45. + (i % 4) as f32 * 8.;
        let height = 10. + (i % 7) as f32 * 4.;
        arena.box_(vec3(a.sin() * r, height * 0.5, a.cos() * r), vec3(2.5, height * 0.5, 2.5), [0.08, 0.12, 0.17], 0.);
    }
    for i in 0..4 {
        let a = i as f32 * std::f32::consts::FRAC_PI_2;
        let transform = Mat4::from_rotation_y(a);
        let mut arch = Template::new();
        arch.box_(vec3(0., 7.4, -22.8), vec3(17., 0.48, 0.8), stone, 0.);
        for x in [-16., -12., 12., 16.] {
            arch.box_(vec3(x, 3.8, -23.), vec3(0.8, 3.8, 1.), stone, 0.);
            arch.box_(vec3(x, 4., -21.95), vec3(0.08, 2.5, 0.05), light, 0.8);
        }
        arena.append(&arch.transformed(transform));
    }
    for i in 0..30 {
        let a = i as f32 * 2.399;
        let radius = 23.8 + (i % 3) as f32 * 1.3;
        let p = vec3(a.sin() * radius, 0., a.cos() * radius);
        let mut crystal = Template::new();
        crystal.cone(p, 0.20 + (i % 4) as f32 * 0.07, 0., 0.7 + (i % 5) as f32 * 0.25, light, 0.5, 5);
        arena.append(&crystal);
    }
    let mut gate = Template::new();
    gate.ring(Vec3::ZERO, 1.8, 2.15, light, 1., 32);
    gate.ring(vec3(0., 0.03, 0.), 2.35, 2.5, stone, 0., 32);
    arena.append(&gate.transformed(Mat4::from_translation(vec3(0., 0.07, -16.))));
    let mut enemies = vec![];
    for kind in [Kind::Seeker, Kind::Drone, Kind::Charger, Kind::Brute, Kind::Sentinel, Kind::Warden] {
        let mut t = Template::new();
        let col = match kind {
            Kind::Drone => [0.5, 0.3, 0.9],
            Kind::Brute => [0.6, 0.28, 0.16],
            Kind::Sentinel => [0.7, 0.55, 0.15],
            Kind::Warden => [0.45, 0.15, 0.22],
            _ => [0.58, 0.2, 0.27],
        };
        let eye = [1., 0.32, 0.17];
        match kind {
            Kind::Drone => {
                t.ball(Vec3::ZERO, vec3(0.65, 0.38, 0.65), col, 0., 12, 7);
                t.ring(vec3(0., -0.08, 0.), 0.7, 0.95, [0.2, 0.17, 0.3], 0., 16);
                t.ball(vec3(0., 0., -0.36), vec3(0.27, 0.15, 0.25), [0.7, 0.4, 1.], 0.9, 10, 6);
                for x in [-1., 1.] {
                    t.box_(vec3(x * 0.85, 0.08, 0.), vec3(0.12, 0.04, 0.6), [0.7, 0.4, 1.], 0.8);
                }
            }
            Kind::Warden => {
                t.rounded_box(Vec3::ZERO, vec3(1.4, 1.1, 0.8), 0.2, col, 0.);
                for x in [-1., 1.] {
                    t.rounded_box(vec3(x * 1.45, -0.15, 0.), vec3(0.5, 0.8, 0.55), 0.12, [0.2, 0.16, 0.2], 0.);
                    t.box_(vec3(x * 1.45, -0.15, -0.6), vec3(0.25, 0.5, 0.12), eye, 0.8);
                    t.box_(vec3(x * 0.7, -1.4, 0.), vec3(0.4, 0.5, 0.4), col, 0.);
                }
                t.ball(vec3(0., 0.45, -0.7), vec3(0.65, 0.5, 0.25), eye, 0.9, 12, 7);
                t.cone(vec3(0., 1.1, 0.), 0.75, 0., 1., [0.8, 0.6, 0.2], 0., 6);
            }
            _ => {
                let k = if kind == Kind::Brute { 1.4 } else { 1. };
                t.rounded_box(vec3(0., 0.1, 0.), vec3(0.45 * k, 0.5 * k, 0.35 * k), 0.1, col, 0.);
                t.rounded_box(vec3(0., 0.65 * k, 0.), vec3(0.3 * k, 0.25 * k, 0.28 * k), 0.06, [0.18, 0.17, 0.22], 0.);
                t.box_(vec3(0., 0.66 * k, -0.295 * k), vec3(0.25 * k, 0.07, 0.035), eye, 1.);
                for x in [-1., 1.] {
                    t.rod(vec3(x * 0.57 * k, 0.2, 0.), vec3(x * 0.68 * k, -0.4, -0.12), 0.13 * k, 0.18 * k, col, 0., 6);
                    t.box_(
                        vec3(x * 0.23 * k, -0.65 * k, 0.),
                        vec3(0.15 * k, 0.27 * k, 0.22 * k),
                        [0.19, 0.18, 0.22],
                        0.,
                    );
                }
                if kind == Kind::Charger {
                    t.cone(vec3(-0.22, 0.8, 0.), 0.16, 0., 0.65, eye, 0.4, 6);
                    t.cone(vec3(0.22, 0.8, 0.), 0.16, 0., 0.65, eye, 0.4, 6);
                }
                if kind == Kind::Sentinel {
                    t.box_(vec3(0.6, 0.05, -0.45), vec3(0.16, 0.16, 0.5), [0.25, 0.22, 0.16], 0.);
                    t.ball(vec3(0.6, 0.05, -0.95), Vec3::splat(0.14), [1., 0.7, 0.15], 1., 8, 5);
                }
            }
        }
        enemies.push(t);
    }
    let mut drop = Template::new();
    drop.cone(vec3(0., -0.2, 0.), 0., 0.2, 0.25, [0.3, 1., 0.8], 0.85, 5);
    drop.cone(vec3(0., 0.05, 0.), 0.2, 0., 0.3, [0.7, 1., 0.85], 0.85, 5);
    let mut bolt = Template::new();
    bolt.ball(Vec3::ZERO, Vec3::splat(0.15), [1., 0.25, 0.15], 1., 8, 5);
    let mut gun = Template::new();
    gun.rounded_box(vec3(0., 0., 0.), vec3(0.095, 0.095, 0.26), 0.025, [0.14, 0.2, 0.23], 0.);
    gun.box_(vec3(0., 0.085, -0.09), vec3(0.055, 0.025, 0.18), [0.32, 0.48, 0.5], 0.);
    gun.rod(vec3(0., 0., -0.22), vec3(0., 0., -0.49), 0.068, 0.06, [0.13, 0.16, 0.18], 0., 10);
    gun.ring(Vec3::ZERO, 0.038, 0.060, ACCENT, 0.8, 12);
    gun.box_(vec3(0.10, 0., -0.06), vec3(0.016, 0.038, 0.15), ACCENT, 0.9);
    gun.box_(vec3(-0.10, 0., -0.06), vec3(0.016, 0.038, 0.15), ACCENT, 0.9);
    gun.rounded_box(vec3(0., -0.15, 0.15), vec3(0.07, 0.15, 0.07), 0.025, [0.15, 0.18, 0.2], 0.);
    gun.rounded_box(vec3(0.01, -0.19, 0.2), vec3(0.08, 0.065, 0.09), 0.025, [0.30, 0.35, 0.39], 0.);
    gun.rod(vec3(0.02, -0.20, 0.28), vec3(0.07, -0.33, 0.5), 0.07, 0.10, [0.20, 0.25, 0.3], 0., 8);
    let mut shard = Template::new();
    shard.rounded_box(Vec3::ZERO, vec3(0.14, 0.10, 0.22), 0.025, [0.26, 0.20, 0.14], 0.);
    for x in [-0.085, 0.085] {
        shard.rod(vec3(x, 0., -0.1), vec3(x, 0., -0.52), 0.055, 0.055, [0.35, 0.3, 0.24], 0., 10);
        shard.box_(vec3(x, 0.06, -0.3), vec3(0.022, 0.016, 0.22), [1., 0.65, 0.22], 0.9);
    }
    shard.rounded_box(vec3(0., -0.15, 0.15), vec3(0.07, 0.16, 0.08), 0.025, [0.24, 0.26, 0.27], 0.);
    let mut lance = Template::new();
    lance.rounded_box(Vec3::ZERO, vec3(0.1, 0.1, 0.3), 0.025, [0.23, 0.19, 0.33], 0.);
    for x in [-0.09, 0.09] {
        lance.box_(vec3(x, 0., -0.45), vec3(0.028, 0.045, 0.22), [0.45, 0.34, 0.6], 0.);
        lance.box_(vec3(x * 0.75, 0., -0.45), vec3(0.012, 0.018, 0.21), [0.85, 0.55, 1.], 1.);
    }
    lance.rounded_box(vec3(0., -0.15, 0.16), vec3(0.07, 0.14, 0.07), 0.025, [0.28, 0.25, 0.3], 0.);
    let guns = vec![gun, shard, lance];
    let mut rift = Template::new();
    rift.cone(vec3(0., 0., 0.), 1.15, 0., 3., light, 0.8, 6);
    rift.cone(vec3(0., -2., 0.), 0., 1.15, 2., light, 0.8, 6);
    Scene { sky: sky.to_meshes(), arena: arena.to_meshes(), enemies, drop, bolt, guns, rift, biome, depth: sim.s.depth }
}
fn index(k: Kind) -> usize {
    match k {
        Kind::Seeker => 0,
        Kind::Drone => 1,
        Kind::Charger => 2,
        Kind::Brute => 3,
        Kind::Sentinel => 4,
        Kind::Warden => 5,
    }
}
struct Trail {
    from: Vec3,
    to: Vec3,
    life: f32,
}
fn react(
    event: &Event,
    sounds: &mut SoundBank,
    fx: &mut Fx,
    juice: &mut Juice,
    trails: &mut Vec<Trail>,
    hit: &mut f32,
    recoil: &mut f32,
) {
    let play = |s: &mut SoundBank, p, vol| s.play(sound(p), vol);
    match event {
        Event::Fired { weapon } => {
            play(
                sounds,
                if *weapon == 2 { synth::Preset::Zap } else { synth::Preset::Shoot },
                if *weapon == 0 { 0.25 } else { 0.5 },
            );
            *recoil = if *weapon == 0 { 0.035 } else { 0.085 };
            juice.kick(if *weapon == 0 { 0.25 } else { 0.9 });
        }
        Event::Shot { from, to } => {
            trails.push(Trail { from: v(*from), to: v(*to), life: 0.09 });
        }
        Event::Hit { at, damage, crit } => {
            *hit = 0.13;
            play(sounds, synth::Preset::Hit, 0.18);
            fx.sparks(v(*at), 5, 3., if *crit { [1., 0.85, 0.3] } else { ACCENT });
            fx.popup(
                v(*at) + vec3(0., 0.6, 0.),
                format!("{:.0}{}", damage, if *crit { "!" } else { "" }),
                if *crit { [1., 0.85, 0.3] } else { [0.9, 1., 1.] },
                if *crit { 25. } else { 18. },
            );
        }
        Event::Kill { at } => {
            play(sounds, synth::Preset::Explosion, 0.35);
            fx.sparks(v(*at), 20, 5., [1., 0.4, 0.2]);
            fx.dust(v(*at), 7, 2., [0.3, 0.3, 0.4]);
            juice.shake(0.08);
        }
        Event::Hurt => {
            play(sounds, synth::Preset::Hurt, 0.6);
            juice.flash([1., 0.12, 0.08], 0.7);
            juice.shake(0.3);
        }
        Event::Dash => {
            play(sounds, synth::Preset::Whoosh, 0.7);
            juice.kick(1.5);
        }
        Event::Pulse { at } => {
            play(sounds, synth::Preset::Zap, 0.5);
            fx.ring(v(*at), Vec3::Y, 0.1, 9., 0.6, ACCENT);
            fx.sparks(v(*at), 40, 10., ACCENT);
            juice.shake(0.16);
        }
        Event::Pickup { at, value } => {
            play(sounds, synth::Preset::Coin, 0.2);
            fx.popup(v(*at), format!("+{value}"), ACCENT, 17.);
        }
        Event::Wave => {
            play(sounds, synth::Preset::Select, 0.5);
        }
        Event::Choice => {
            play(sounds, synth::Preset::PowerUp, 0.6);
            fx.banner("ENCOUNTER CLEAR", "Choose a circuit for your build", ACCENT);
        }
        Event::Banked { amount } => {
            play(sounds, synth::Preset::Success, 0.7);
            fx.banner("SALVAGE SECURED", format!("+{amount} to your workshop"), ACCENT);
        }
        Event::Lost { kept } => {
            play(sounds, synth::Preset::GameOver, 0.6);
            fx.banner("SIGNAL LOST", format!("Insurance recovered {kept} salvage"), [1., 0.4, 0.3]);
        }
        Event::Denied => play(sounds, synth::Preset::Error, 0.4),
        Event::Bought => play(sounds, synth::Preset::PowerUp, 0.5),
        Event::Boss => {
            fx.banner("WARDEN ONLINE", "Clear its escort. Dash through the pulse ring.", [1., 0.5, 0.2]);
            play(sounds, synth::Preset::Warning, 0.5);
        }
    }
}
fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    hud::text_outlined(s, x, y, size, c)
}
fn button(rect: Rect, title: &str, sub: &str, selected: bool, enabled: bool, ui: f32) -> bool {
    let hover = rect.contains(Vec2::from(mouse_position()));
    let active = selected || hover;
    let bg = if active { Color::new(0.11, 0.25, 0.29, 0.94) } else { Color::new(0.025, 0.055, 0.075, 0.93) };
    hud::panel(rect.x, rect.y, rect.w, rect.h, 7. * ui, bg);
    draw_rectangle(
        rect.x,
        rect.y,
        3. * ui,
        rect.h,
        if active { hud::col(ACCENT, 1.) } else { Color::new(0.2, 0.34, 0.38, 1.) },
    );
    text(title, rect.x + 18. * ui, rect.y + 28. * ui, 21. * ui, if enabled { WHITE } else { GRAY });
    text(
        sub,
        rect.x + 18. * ui,
        rect.y + 53. * ui,
        15. * ui,
        if enabled { hud::col([0.63, 0.77, 0.8], 1.) } else { GRAY },
    );
    hover && is_mouse_button_pressed(MouseButton::Left)
}
fn menus(sim: &Sim, focus: usize) -> (u8, bool) {
    let ui = hud::ui_scale();
    let w = screen_width();
    let h = screen_height();
    let mut choose = 0;
    let mut enter = false;
    if sim.s.phase == Phase::Combat {
        return (0, false);
    }
    draw_rectangle(0., 0., w, h, Color::new(0.015, 0.03, 0.06, 0.6));
    match sim.s.phase {
        Phase::Camp => {
            let x = (w - 1040. * ui) * 0.5;
            let y = (h - 540. * ui) * 0.5;
            text("RIFT / EXPEDITION SYSTEMS", x, y + 28. * ui, 17. * ui, hud::col(ACCENT, 1.));
            text("RIFT DELVER", x, y + 88. * ui, 54. * ui, WHITE);
            text("One more depth.", x, y + 124. * ui, 25. * ui, hud::col([0.65, 0.8, 0.86], 1.));
            for (i, line) in [
                "Fight through three encounters per depth.",
                "Choose circuits. Create an absurd build.",
                "Extract salvage or risk it all by descending.",
                "Permanent upgrades survive every expedition.",
            ]
            .iter()
            .enumerate()
            {
                text(line, x, y + (183. + i as f32 * 27.) * ui, 18. * ui, hud::col([0.73, 0.8, 0.85], 1.));
            }
            let r = Rect::new(x, y + 325. * ui, 440. * ui, 70. * ui);
            if button(r, "[E / ENTER]  BEGIN EXPEDITION", WEAPONS[sim.s.weapon], focus == 0, true, ui) {
                enter = true;
            }
            text("Q changes unlocked weapon", x, y + 420. * ui, 16. * ui, hud::col(ACCENT, 1.));
            text(
                &format!("BEST DEPTH {}    RUNS {}    WARDENS {}", sim.s.meta.best, sim.s.meta.runs, sim.s.meta.bosses),
                x,
                y + 462. * ui,
                16. * ui,
                WHITE,
            );
            text(
                &format!("CONTRACT: {} / 100 kills   +50 salvage", sim.s.meta.kills % 100),
                x,
                y + 490. * ui,
                16. * ui,
                hud::col([0.7, 0.76, 0.8], 1.),
            );
            let rx = x + 525. * ui;
            text("PERMANENT WORKSHOP", rx, y + 29. * ui, 20. * ui, hud::col(ACCENT, 1.));
            text(&format!("{} SALVAGE", sim.s.meta.bank), rx, y + 79. * ui, 38. * ui, WHITE);
            for (i, workshop) in WORKSHOP.iter().enumerate() {
                let r = Rect::new(rx, y + (108. + i as f32 * 91.) * ui, 510. * ui, 76. * ui);
                let level = sim.s.meta.levels[i];
                let label = format!("[{}]  {}    {}/10", i + 1, workshop.0, level);
                let sub = if level == 10 {
                    "MAXIMUM LEVEL".to_string()
                } else {
                    format!("{}  /  {} salvage", workshop.1, sim.cost(i))
                };
                if button(r, &label, &sub, focus == i + 1, sim.s.meta.bank >= sim.cost(i) && level < 10, ui) {
                    choose = i as u8 + 1;
                }
            }
            text(
                "Shard Caster: depth 3  /  Arc Lance: depth 6",
                rx,
                y + 500. * ui,
                16. * ui,
                hud::col([0.8, 0.75, 0.55], 1.),
            );
        }
        Phase::Draft => {
            let x = (w - 1020. * ui) * 0.5;
            let y = h * 0.45;
            text("INSTALL A CIRCUIT", x, y - 67. * ui, 38. * ui, WHITE);
            text(
                "Your build lasts for this expedition.  Choose with 1 / 2 / 3.",
                x,
                y - 30. * ui,
                18. * ui,
                hud::col(ACCENT, 1.),
            );
            for i in 0..3 {
                let p = sim.s.offers[i];
                let rect = Rect::new(x + i as f32 * 345. * ui, y, 330. * ui, 118. * ui);
                if button(rect, &format!("[{}] {}", i + 1, PERKS[p].0), PERKS[p].1, focus == i, true, ui) {
                    choose = i as u8 + 1;
                }
                text(
                    &format!("STACK {}", sim.s.perks[p] + 1),
                    rect.x + 18. * ui,
                    rect.y + 92. * ui,
                    17. * ui,
                    hud::col(ACCENT, 1.),
                );
            }
        }
        Phase::Gate => {
            let x = (w - 850. * ui) * 0.5;
            let y = h * 0.42;
            text(&format!("DEPTH {} SECURED", sim.s.depth), x, y - 65. * ui, 38. * ui, WHITE);
            text(
                &format!("{} salvage aboard.  The next depth is more dangerous.", sim.s.haul),
                x,
                y - 25. * ui,
                20. * ui,
                hud::col(ACCENT, 1.),
            );
            if button(
                Rect::new(x, y, 410. * ui, 90. * ui),
                "[1] EXTRACT",
                &format!("Bank {} salvage and upgrade", sim.s.haul + 15 * sim.s.depth),
                focus == 0,
                true,
                ui,
            ) {
                choose = 1;
            }
            if button(
                Rect::new(x + 435. * ui, y, 410. * ui, 90. * ui),
                "[2 / E] DESCEND",
                "Keep your circuits. Risk the haul.",
                focus == 1,
                true,
                ui,
            ) {
                choose = 2;
            }
        }
        Phase::Lost => {
            let x = (w - 660. * ui) * 0.5;
            let y = h * 0.43;
            text("EXPEDITION LOST", x, y - 70. * ui, 43. * ui, WHITE);
            text(
                &format!("Reached depth {} / defeated {} enemies", sim.s.depth, sim.s.run_kills),
                x,
                y - 25. * ui,
                22. * ui,
                hud::col([0.9, 0.7, 0.65], 1.),
            );
            if button(
                Rect::new(x, y, 660. * ui, 85. * ui),
                "[E / ENTER] RETURN TO WORKSHOP",
                "Your bank, unlocks and permanent upgrades are safe.",
                true,
                true,
                ui,
            ) {
                enter = true;
            }
        }
        _ => {}
    }
    text(
        "WASD move  /  Mouse aim  /  LMB fire  /  Shift dash  /  RMB shockwave  /  Space jump",
        20. * ui,
        h - 39. * ui,
        16. * ui,
        hud::col([0.68, 0.78, 0.82], 1.),
    );
    text(
        "F5 save  /  F9 load  /  Esc settings  /  Automatic expedition save",
        20. * ui,
        h - 15. * ui,
        14. * ui,
        hud::col([0.5, 0.65, 0.7], 1.),
    );
    (choose, enter)
}
fn draw_hud(sim: &Sim, view: &View, hit: f32) {
    let ui = hud::ui_scale();
    let w = screen_width();
    let h = screen_height();
    hud::panel(20. * ui, 18. * ui, 325. * ui, 73. * ui, 8. * ui, Color::new(0.015, 0.035, 0.055, 0.88));
    text(&format!("DEPTH {:02}  /  ENCOUNTER {}", sim.s.depth, sim.s.wave), 35. * ui, 44. * ui, 20. * ui, WHITE);
    text(BIOMES[sim.biome()], 35. * ui, 73. * ui, 16. * ui, hud::col(ACCENT, 1.));
    text(&format!("{} HOSTILES", sim.remaining()), w - 195. * ui, 42. * ui, 21. * ui, hud::col([1., 0.7, 0.45], 1.));
    text(&format!("{} SALVAGE", sim.s.haul), w - 195. * ui, 72. * ui, 21. * ui, hud::col(ACCENT, 1.));
    hud::panel(20. * ui, h - 107. * ui, 320. * ui, 87. * ui, 8. * ui, Color::new(0.015, 0.035, 0.055, 0.9));
    text(&format!("HULL   {:.0} / {:.0}", sim.s.health, sim.max_health()), 35. * ui, h - 76. * ui, 22. * ui, WHITE);
    draw_rectangle(35. * ui, h - 59. * ui, 285. * ui, 11. * ui, Color::new(0.13, 0.19, 0.22, 1.));
    draw_rectangle(
        35. * ui,
        h - 59. * ui,
        285. * ui * (sim.s.health / sim.max_health()),
        11. * ui,
        hud::col(if sim.s.health < 30. { [1., 0.3, 0.2] } else { ACCENT }, 1.),
    );
    text(
        &format!(
            "SHIFT dash {}   /   RMB pulse {}",
            if sim.s.dash_cd == 0 { "READY".into() } else { format!("{:.1}s", sim.s.dash_cd as f32 / 60.) },
            if sim.s.pulse_cd == 0 { "READY".into() } else { format!("{:.1}s", sim.s.pulse_cd as f32 / 60.) }
        ),
        35. * ui,
        h - 29. * ui,
        15. * ui,
        hud::col([0.65, 0.8, 0.85], 1.),
    );
    let gx = w - 300. * ui;
    text(WEAPONS[sim.s.weapon], gx, h - 65. * ui, 20. * ui, WHITE);
    draw_rectangle(gx, h - 48. * ui, 270. * ui, 8. * ui, Color::new(0.13, 0.19, 0.22, 1.));
    draw_rectangle(
        gx,
        h - 48. * ui,
        270. * ui * sim.s.heat.min(1.),
        8. * ui,
        hud::col(if sim.s.overheat { [1., 0.3, 0.2] } else { [1., 0.7, 0.3] }, 1.),
    );
    text(
        if sim.s.overheat { "OVERHEATED / LET IT COOL" } else { "HEAT  /  Q swap weapon" },
        gx,
        h - 24. * ui,
        15. * ui,
        hud::col([0.7, 0.8, 0.8], 1.),
    );
    text(
        &format!("{} CIRCUITS", sim.s.perks.iter().sum::<u32>()),
        w * 0.5 - 50. * ui,
        h - 30. * ui,
        16. * ui,
        hud::col(ACCENT, 1.),
    );
    let cx = w * 0.5;
    let cy = h * 0.5;
    let gap = 5. * ui;
    let c = if hit > 0. { hud::col([1., 0.8, 0.3], 1.) } else { Color::new(0.8, 1., 0.96, 0.85) };
    for sign in [-1., 1.] {
        draw_line(cx + sign * gap, cy, cx + sign * (gap + 7. * ui), cy, 1.5 * ui, c);
        draw_line(cx, cy + sign * gap, cx, cy + sign * (gap + 7. * ui), 1.5 * ui, c);
    }
    if hit > 0. {
        for (dx, dy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            draw_line(cx + dx * 10. * ui, cy + dy * 10. * ui, cx + dx * 16. * ui, cy + dy * 16. * ui, 2. * ui, c);
        }
    }
    // Radar points convey rear threats without hiding the action.
    let rx = w - 95. * ui;
    let ry = 160. * ui;
    draw_circle(rx, ry, 57. * ui, Color::new(0.015, 0.035, 0.055, 0.85));
    draw_circle_lines(rx, ry, 57. * ui, 1. * ui, hud::col(ACCENT, 0.4));
    draw_triangle(vec2(rx, ry - 5. * ui), vec2(rx - 4. * ui, ry + 4. * ui), vec2(rx + 4. * ui, ry + 4. * ui), WHITE);
    for e in &sim.s.enemies {
        let delta = v(e.pos) - view.eye;
        let dx = delta.dot(view.right()) * 1.8 * ui;
        let dy = -delta.dot(view.dir()) * 1.8 * ui;
        if dx * dx + dy * dy < 51. * 51. * ui * ui {
            draw_circle(rx + dx, ry + dy, 2.4 * ui, hud::col([1., 0.45, 0.25], 1.));
        }
    }
    for e in &sim.s.enemies {
        if e.hp < e.max_hp && e.hp > 0. {
            if let Some(p) = view.project(v(e.pos) + vec3(0., 1.3, 0.), w, h) {
                if p.x > 0. && p.x < w && p.y > 0. && p.y < h {
                    draw_rectangle(p.x - 22. * ui, p.y, 44. * ui, 4. * ui, Color::new(0.1, 0.05, 0.05, 0.9));
                    draw_rectangle(
                        p.x - 22. * ui,
                        p.y,
                        44. * ui * e.hp / e.max_hp,
                        4. * ui,
                        hud::col([1., 0.5, 0.2], 1.),
                    );
                }
            }
        }
    }
}
#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let seed = life.seed();
    let unattended = life.options.unattended();
    let materials = Materials::load().expect("materials failed to compile");
    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let music_wav = Arc::new(OnceLock::new());
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), settings.music_level(), {
        let music_wav = music_wav.clone();
        let (title, tagline) = (identity.title.clone(), identity.tagline.clone());
        move || render_audio(music_wav, &title, &tagline)
    })
    .await;
    let quality = ShadowQuality::from_flag(&args).unwrap_or(None).unwrap_or(settings.shadow_quality);
    let mut shadows = Shadows::new(quality).with_range(30., 65.);
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut sim = Sim::new(seed);
    if !unattended && flag_value(&args, "--load").is_none() {
        match snapshot::load_from_slot(&mut sim, &life.slots, "expedition") {
            Ok(_) => fx.banner("EXPEDITION RESTORED", "Your workshop and expedition are saved", ACCENT),
            Err(vesper3d::viewer::devkit::SaveError::NotFound(_)) => {}
            Err(e) => fx.banner("SAVE COULD NOT LOAD", e.to_string(), [1., 0.6, 0.3]),
        }
    }
    life.load_flag_or_exit(&mut sim);
    let mut scene = build_scene(&sim);
    let (mut world, mut alpha, mut add, mut gun_batch) = (Batch::new(), Batch::new(), Batch::new(), Batch::new());
    let mut trails: Vec<Trail> = vec![];
    let mut hit: f32 = 0.;
    let mut recoil: f32 = 0.;
    let mut focus = 0usize;
    let mut queued = (0u8, false);
    let mut old_phase = sim.s.phase;
    let mut previous = sim.player.clone();
    let autopilot = has_flag(&args, "--autopilot");
    let vignette = hud::make_vignette();
    let mut weapon_target = render_target(screen_width() as u32, screen_height() as u32);
    weapon_target.texture.set_filter(FilterMode::Linear);
    loop {
        input.begin_frame_with_keyboard(
            &mut shell,
            sim.s.phase == Phase::Combat,
            unattended || platform::focused(),
            platform::keyboard(),
        );
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        sounds.update_music(dt, &[if sim.s.phase == Phase::Combat { 0.8 } else { 0.55 }]);
        let accepting = !shell.paused;
        let pad = input.gamepad();
        let mut choose = queued.0;
        let mut enter = queued.1;

        let mut edges = 0u32;
        let (held, look_delta) = if let Some(s) = life.script() {
            if s.starts("dash") {
                edges |= DASH;
            }
            if s.starts("jump") {
                edges |= JUMP;
            }
            if s.starts("pulse") {
                edges |= PULSE;
            }
            if s.starts("enter") {
                edges |= INTERACT;
            }
            if s.starts("weapon") {
                edges |= WEAPON;
            }
            for (i, c) in ["one", "two", "three", "four"].iter().enumerate() {
                if s.starts(c) {
                    choose = i as u8 + 1;
                }
            }
            (
                Held { forward: s.axis("fwd", "back"), right: s.axis("right", "left"), fire: s.held("fire") },
                [s.value("look", 0), s.value("look", 1)],
            )
        } else {
            if accepting {
                let m = input.movement(&shell);
                if input.pressed(KeyCode::LeftShift) || pad.pressed(Button::LeftTrigger) {
                    edges |= DASH;
                }
                if m.jump {
                    edges |= JUMP;
                }
                if is_mouse_button_pressed(MouseButton::Right) || pad.pressed(Button::RightTrigger) {
                    edges |= PULSE;
                }
                if input.pressed(KeyCode::E) {
                    enter = true;
                }
                if input.pressed(KeyCode::Q) || pad.pressed(Button::West) {
                    edges |= WEAPON;
                }
                for (i, k) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4].iter().enumerate() {
                    if input.pressed(*k) {
                        choose = i as u8 + 1;
                    }
                }
                if sim.s.phase != Phase::Combat {
                    let nav = input.menu_step();
                    let count = match sim.s.phase {
                        Phase::Camp => 5,
                        Phase::Draft => 3,
                        Phase::Gate => 2,
                        _ => 1,
                    };
                    if nav.down || nav.right {
                        focus = (focus + 1) % count;
                    }
                    if nav.up || nav.left {
                        focus = (focus + count - 1) % count;
                    }
                    if input.menu_select() {
                        match sim.s.phase {
                            Phase::Camp if focus == 0 => enter = true,
                            Phase::Camp => choose = focus as u8,
                            Phase::Draft | Phase::Gate => choose = focus as u8 + 1,
                            _ => enter = true,
                        }
                    }
                }
                (
                    Held {
                        forward: m.forward,
                        right: m.right,
                        fire: is_mouse_button_down(MouseButton::Left) || pad.triggers[1] > 0.3,
                    },
                    input.look_delta_with(&shell, dt),
                )
            } else {
                (Held::default(), [0., 0.])
            }
        };
        if enter {
            edges |= INTERACT;
        }
        if choose > 0 {
            edges |= (choose as u32) << 8;
        }
        life.feed(held, edges, look_delta);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let n = life.quick_save(&sim, "Rift Delver expedition");
            fx.banner(n.title, n.detail, n.color);
        }
        if load {
            let n = life.quick_load(&mut sim);
            fx.banner(n.title, n.detail, n.color);
            if n.ok {
                juice = Juice::default();
                previous = sim.player.clone();
                trails.clear();
            }
        }
        for _ in 0..life.ticks(dt, 1., accepting) {
            let t = life.take_tick();
            let choose = (t.edges >> 8) as u8;
            previous = sim.player.clone();
            let intention = Input {
                forward: t.held.forward,
                right: t.held.right,
                fire: t.held.fire,
                look: t.look,
                dash: t.pressed(DASH),
                jump: t.pressed(JUMP),
                pulse: t.pressed(PULSE),
                interact: t.pressed(INTERACT),
                weapon: t.pressed(WEAPON),
                choose,
            };
            sim.step(&if autopilot { rift_delver::autopilot(&sim) } else { intention });
            let events = sim.drain_events();
            let important =
                events.iter().any(|e| matches!(e, Event::Bought | Event::Banked { .. } | Event::Lost { .. }));
            for event in events {
                react(&event, &mut sounds, &mut fx, &mut juice, &mut trails, &mut hit, &mut recoil);
            }
            if !unattended && (important || sim.s.tick.is_multiple_of(1800) || sim.s.phase != old_phase) {
                if let Err(e) = snapshot::save_to_slot(&sim, &life.slots, "expedition", "Automatic expedition save") {
                    fx.banner("SAVE FAILED", e.to_string(), [1., 0.5, 0.2]);
                }
            }
            if sim.s.phase != old_phase {
                old_phase = sim.s.phase;
                previous = sim.player.clone();
                focus = 0;
                life.reset_input();
                trails.clear();
            }
        }
        if accepting {
            fx.update(dt);
            juice.update(dt);
            hit = (hit - dt).max(0.);
            recoil *= (-16. * dt).exp();
            for trail in &mut trails {
                trail.life -= dt;
            }
            trails.retain(|t| t.life > 0.);
        }
        if scene.biome != sim.biome() || scene.depth != sim.s.depth {
            scene = build_scene(&sim);
        }
        let mut look = Look::night();
        let (_, _, glow) = palette(sim.biome());
        look.fog_color = [0.10, 0.15, 0.21];
        look.fog_density = 0.012;
        look.ambient_sky = [0.48, 0.52, 0.64];
        look.key_color = [1., 0.82, 0.63];
        look.rim_color = glow;
        let pending = life.pending_look();
        let pose = sim.player.interpolated(&previous, life.alpha());
        let (shake, roll) = juice.camera_shake();
        let mut view = View::first_person(
            v(pose.position) + vec3(shake.0, shake.1, shake.2),
            pose.yaw + pending[0],
            (pose.pitch - pending[1]).clamp(-1.5, 1.5),
        );
        view.roll = roll;
        view.fov = (76. + juice.fov_kick).to_radians();
        if sim.s.phase == Phase::Camp {
            let a = time * 0.035;
            view = View::first_person(vec3(a.sin() * 10., 8., a.cos() * 18.), -a, -0.26);
        }
        clear_background(look.clear_color());
        world.clear();
        alpha.clear();
        add.clear();
        gun_batch.clear();
        shadows.begin_frame(&look, v(sim.player.position));
        world.add(
            &scene.rift,
            Mat4::from_rotation_y(time * 0.5) * Mat4::from_translation(vec3(0., 4., 0.)),
            Tint::NONE,
        );
        for e in &sim.s.enemies {
            let yaw = vesper3d::viewer::devkit::path::yaw_of(sim.player.position - e.pos);
            let bob = if e.kind == Kind::Drone { 0. } else { 0.04 * (time * 8. + e.id as f32).sin() };
            world.add(
                &scene.enemies[index(e.kind)],
                Mat4::from_translation(v(e.pos) + vec3(0., bob, 0.)) * Mat4::from_rotation_y(-yaw),
                Tint::flash(e.flash as f32 / 5.),
            );
            shadows.blob(vec3(e.pos.0, 0.05, e.pos.2), e.radius());
            if e.kind == Kind::Charger && e.timer < 42 && e.timer >= 18 {
                let mut t = Template::new();
                t.rod(
                    v(e.pos) + vec3(0., -0.55, 0.),
                    v(e.pos + e.dir * 6.) + vec3(0., -0.55, 0.),
                    0.05,
                    0.05,
                    [1., 0.3, 0.15],
                    1.,
                    5,
                );
                world.add(&t, Mat4::IDENTITY, Tint::NONE);
            }
        }
        for d in &sim.s.drops {
            world.add(
                &scene.drop,
                Mat4::from_translation(v(d.pos) + vec3(0., (time * 3. + d.value as f32).sin() * 0.1, 0.))
                    * Mat4::from_rotation_y(time),
                Tint::NONE,
            );
        }
        for b in &sim.s.bolts {
            world.add(&scene.bolt, Mat4::from_translation(v(b.pos)), Tint::NONE);
        }
        for trail in &trails {
            let mut t = Template::new();
            let start = if (trail.from - view.eye).length() < 1. {
                view.eye + view.right() * 0.18 - view.up() * 0.17 + view.dir() * 0.55
            } else {
                trail.from
            };
            t.rod(start, trail.to, 0.016, 0.007, ACCENT, 1., 5);
            world.add(&t, Mat4::IDENTITY, Tint::NONE);
        }
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        shadows.cast(|| {
            materials.draw_static(&scene.arena);
            world.draw();
        });
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for m in &scene.sky {
            draw_mesh(m);
        }
        set_camera(&view.camera_checked(0.08, 150.));
        materials.set_scene(&look, view.eye, time, 0.5);
        shadows.apply(&materials);
        materials.draw_static(&scene.arena);
        shadows.draw_decals(&materials);
        gl_use_material(&materials.world);
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        if weapon_target.texture.width() != screen_width() || weapon_target.texture.height() != screen_height() {
            weapon_target = render_target(screen_width() as u32, screen_height() as u32);
            weapon_target.texture.set_filter(FilterMode::Linear);
        }
        if sim.s.phase != Phase::Camp {
            let bob = if held.forward.abs() + held.right.abs() > 0.1 { (time * 11.).sin() * 0.009 } else { 0. };
            let gun_at = view.eye + view.right() * 0.23 - view.up() * (0.21 - bob) + view.dir() * (0.67 - recoil);
            let mat = Mat4::from_cols(
                view.right().extend(0.),
                view.up().extend(0.),
                (-view.dir()).extend(0.),
                gun_at.extend(1.),
            );
            gun_batch.add(&scene.guns[sim.s.weapon], mat * Mat4::from_scale(Vec3::splat(0.66)), Tint::NONE);
            let mut weapon_camera = view.camera_checked(0.05, 10.);
            weapon_camera.render_target = Some(weapon_target.clone());
            set_camera(&weapon_camera);
            clear_background(Color::new(0., 0., 0., 0.));
            gl_use_material(&materials.world);
            gun_batch.draw();
            gl_use_default_material();
            set_default_camera();
            draw_texture_ex(
                &weapon_target.texture,
                0.,
                0.,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    flip_y: true,
                    ..Default::default()
                },
            );
        }
        gl_use_default_material();
        set_default_camera();
        let ui = hud::ui_scale();
        hud::overlay(&vignette, Color::new(0.03, 0.05, 0.08, 0.35));
        if juice.flash > 0.01 {
            draw_rectangle(0., 0., screen_width(), screen_height(), hud::col(juice.flash_color, juice.flash * 0.3));
        }
        if sim.s.phase == Phase::Combat {
            draw_hud(&sim, &view, hit);
            hud::draw_popups(&fx.popups, &view, ui);
        }
        queued = menus(&sim, focus);
        hud::draw_banners(&fx.banners, ui);
        let controls: Vec<&str> = identity.controls.split(", ").collect();
        let outcome = shell.local_menu_with_options(
            &identity.title,
            &controls,
            AudioMenu { music_on: settings.music_on, sfx_on: settings.sfx_on, has_music: HAS_MUSIC },
            shadows.quality(),
        );
        if outcome.toggle_music {
            settings.toggle_music();
            sounds.music_volume = settings.music_level();
            settings.store(&settings_path);
        }
        if outcome.toggle_sfx {
            settings.toggle_sfx();
            sounds.sfx_volume = settings.sfx_level();
            settings.store(&settings_path);
        }
        if outcome.cycle_shadows {
            shadows.set_quality(shadows.quality().next());
            settings.shadow_quality = shadows.quality();
            settings.store(&settings_path);
        }
        if outcome.download_music {
            let n = save_music(&music_wav, &identity.title);
            fx.banner(n.title, n.detail, n.color);
        }
        if outcome.quit {
            break;
        }
        if let Some(path) = life.capture_path() {
            life.captured(&path, kit::capture::save_frame(&path).map_err(|e| e.to_string()));
        }
        if life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    if !unattended {
        if let Err(e) = snapshot::save_to_slot(&sim, &life.slots, "expedition", "Expedition saved on exit") {
            eprintln!("Could not save expedition: {e}");
        }
    }
    if let Some(report) = life.report() {
        println!("{report}");
    }
    if life.options.audible {
        let status = sounds.status();
        println!("audio: {status:?}");
        if let Err(e) = status.verify_playback(SOUNDS.iter().map(|p| p.variants()).sum(), 1) {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

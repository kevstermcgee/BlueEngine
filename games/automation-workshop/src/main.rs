//! The game-owned native workshop console. Rendering submits intentions; the library owns production.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use automation_workshop::{Command, Envelope, Workshop};
use macroquad::prelude::*;
use std::{collections::BTreeMap, path::PathBuf};
use vesper3d::{
    runtime::Simulation,
    viewer::{
        devkit::{flag_value, has_flag, parse_size, Lifecycle},
        game_client::{self, GameShell},
        game_input::ClientInput,
        identity::Identity,
        kit,
        savestate::SaveSlots,
    },
};

const IDENTITY: &str = include_str!("../assets/identity.json");
const WIDTH: f32 = 1200.;
const HEIGHT: f32 = 760.;
const INK: Color = Color::new(0.10, 0.15, 0.17, 1.);
const PAPER: Color = Color::new(0.94, 0.92, 0.85, 1.);
const TEAL: Color = Color::new(0.14, 0.47, 0.43, 1.);
const RUST: Color = Color::new(0.70, 0.29, 0.17, 1.);
const CUES: &[&str] = &[
    "smelt",
    "gear",
    "ore",
    "coal",
    "deliver",
    "cancel",
    "upgrade",
    "full",
    "recovery",
    "queue",
    "machine",
    "recipe",
    "upgrade_next",
    "item",
];

fn identity() -> Identity {
    Identity::parse(IDENTITY).expect("invalid identity; run python scripts/ship.py verify")
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
    let args: Vec<_> = std::env::args().collect();
    let (width, height) = flag_value(&args, "--size").and_then(parse_size).unwrap_or((1200, 760));
    conf.miniquad_conf.window_width = width.clamp(320, 7680) as i32;
    conf.miniquad_conf.window_height = height.clamp(240, 4320) as i32;
    if has_flag(&args, "--novsync") {
        conf.miniquad_conf.platform.swap_interval = Some(0);
    }
    conf
}

/// Stable game identity, independent of release/installation directory; --save-dir wins.
fn player_save_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).filter(|path| path.is_absolute());
    #[cfg(not(windows))]
    let root = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|path| path.is_absolute()).or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .map(|path| path.join(".local/share"))
    });
    root.map(|path| path.join("blueengine-workshop/saves"))
}

#[derive(Default)]
struct Selection {
    machine: usize,
    recipe: usize,
    upgrade: usize,
    item: usize,
}

fn selected<T>(map: &BTreeMap<String, T>, index: usize) -> Option<String> {
    map.keys().nth(index % map.len().max(1)).cloned()
}

fn named<T>(map: &BTreeMap<String, T>, name: &str) -> Option<String> {
    map.keys().find(|id| id.rsplit(':').next().is_some_and(|tail| tail.contains(name))).cloned()
}

fn queue(sim: &Workshop, machine: usize, recipe: Option<String>) -> Option<Command> {
    Some(Command::Queue { machine: selected(&sim.state.machines, machine)?, recipe: recipe? })
}

fn item_command(sim: &Workshop, name: &str, amount: u64, gather: bool) -> Option<Command> {
    let item = named(&sim.definitions.items, name)?;
    Some(if gather { Command::Gather { item, amount } } else { Command::Deliver { item, amount } })
}

/// A player's fill intention, still validated by the ordinary source command.
fn fill_source(sim: &Workshop) -> Option<Command> {
    let item = named(&sim.definitions.items, "ore")?;
    let capacity = *sim.state.inventory.capacities().get(&item)?;
    let available = sim.state.inventory.max_units().saturating_sub(sim.state.inventory.total_units());
    let amount = available.min(capacity.saturating_sub(sim.state.inventory.quantity(&item)));
    (amount > 0).then_some(Command::Gather { item, amount })
}

fn text(value: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(value, x, y, size, color);
}

fn status_lines(value: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        let next = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if !line.is_empty() && measure_text(&next, None, 18, 1.).width > WIDTH - 48. {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn button(value: &str, rect: Rect, enabled: bool, mouse: Vec2, pressed: bool) -> bool {
    let hovered = enabled && rect.contains(mouse);
    let color = if hovered {
        TEAL
    } else if enabled {
        INK
    } else {
        GRAY
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);
    text(value, rect.x + 10., rect.y + rect.h * 0.70, 18., PAPER);
    hovered && pressed
}

fn gear_icon(x: f32, y: f32, active: bool, tick: u64) {
    let angle = if active { tick as f32 / 60. } else { 0. };
    for n in 0..8 {
        let a = angle + n as f32 * std::f32::consts::TAU / 8.;
        draw_circle(x + a.cos() * 28., y + a.sin() * 28., 8., TEAL);
    }
    draw_circle(x, y, 28., TEAL);
    draw_circle(x, y, 12., PAPER);
    draw_circle(x, y, 4., INK);
}

/// Bounded state view. Every returned command goes through the normal authoritative input path.
fn draw_workshop(sim: &Workshop, choice: &mut Selection, interactive: bool, status: &str) -> Vec<Command> {
    let mut commands = Vec::new();
    let (mx, my) = mouse_position();
    let mouse = vec2(mx * WIDTH / screen_width(), my * HEIGHT / screen_height());
    let clicked = interactive && is_mouse_button_pressed(MouseButton::Left);
    set_camera(&Camera2D {
        target: vec2(WIDTH / 2., HEIGHT / 2.),
        zoom: vec2(2. / WIDTH, 2. / HEIGHT),
        ..Default::default()
    });
    clear_background(PAPER);
    draw_rectangle(0., 0., WIDTH, 110., INK);
    text("THE LITTLE AUTOMATION WORKSHOP", 24., 44., 30., PAPER);
    text("Two machines. One store. Every item has a job.", 24., 79., 20., Color::new(0.65, 0.83, 0.76, 1.));
    text(&format!("TICK {}", sim.state.tick), 1030., 44., 20., PAPER);
    text("SHARED STORE", 24., 148., 23., INK);
    text(
        &format!("{} / {} units", sim.state.inventory.total_units(), sim.state.inventory.max_units()),
        24.,
        176.,
        19.,
        TEAL,
    );
    let selected_item = choice.item % sim.definitions.items.len().max(1);
    let page_start = (selected_item / 8) * 8;
    for (index, (id, definition)) in sim.definitions.items.iter().enumerate().skip(page_start).take(8) {
        let y = 204. + (index - page_start) as f32 * 48.;
        if index == selected_item {
            draw_rectangle(16., y - 20., 314., 44., Color::new(0.82, 0.86, 0.77, 1.));
        }
        text(&definition.label, 24., y, 20., INK);
        text(&sim.state.inventory.quantity(id).to_string(), 242., y, 20., TEAL);
        if clicked && Rect::new(16., y - 20., 314., 44.).contains(mouse) {
            choice.item = index;
        }
    }
    text(
        &format!("Items {} / {}  [Down: next]", page_start / 8 + 1, sim.definitions.items.len().div_ceil(8)),
        24.,
        598.,
        16.,
        TEAL,
    );
    let item = selected(&sim.definitions.items, choice.item);
    if button("Gather 1", Rect::new(24., 616., 138., 34.), item.is_some(), mouse, clicked) {
        commands.push(Command::Gather { item: item.clone().unwrap(), amount: 1 });
    }
    if button("Deliver 1", Rect::new(174., 616., 138., 34.), item.is_some(), mouse, clicked) {
        commands.push(Command::Deliver { item: item.unwrap(), amount: 1 });
    }
    text("Gather ore/coal; deliver to free space.", 24., 681., 16., INK);

    let recipe = selected(&sim.definitions.recipes, choice.recipe);
    let upgrade = selected(&sim.definitions.upgrades, choice.upgrade);
    for (index, (id, machine)) in sim.state.machines.iter().enumerate().take(2) {
        let x = 355. + index as f32 * 410.;
        let selected_machine = index == choice.machine % sim.state.machines.len().max(1);
        draw_rectangle(x, 132., 392., 550., Color::new(0.88, 0.88, 0.80, 1.));
        draw_rectangle_lines(x, 132., 392., 550., if selected_machine { 4. } else { 1. }, TEAL);
        text(&format!("{}  [{}]", id.trim_start_matches("workshop:"), index + 1), x + 18., 167., 23., INK);
        let active = machine.queue.first();
        gear_icon(x + 53., 222., active.is_some() && machine.blocked.is_none(), sim.state.tick);
        let state = if machine.blocked.is_some() {
            "BLOCKED"
        } else if active.is_some() {
            "WORKING"
        } else {
            "IDLE"
        };
        text(state, x + 102., 211., 22., if machine.blocked.is_some() { RUST } else { TEAL });
        text(
            &format!("{} queued / {} upgrades", machine.queue.len(), machine.upgrades.len()),
            x + 102.,
            239.,
            17.,
            INK,
        );
        if let Some(job) = active {
            let name = sim.definitions.recipes.get(&job.recipe).map_or(job.recipe.as_str(), |rule| rule.label.as_str());
            text(name, x + 18., 287., 24., INK);
            text(&format!("{} logical ticks left", job.remaining_ticks), x + 18., 315., 18., TEAL);
            let progress = 1. - job.remaining_ticks as f32 / job.duration_ticks.max(1) as f32;
            draw_rectangle(x + 18., 330., 356., 14., PAPER);
            draw_rectangle(x + 18., 330., 356. * progress.clamp(0., 1.), 14., TEAL);
            if job.cancel_pending {
                text("Cancellation pending: escrow retained", x + 18., 370., 16., RUST);
            }
        } else {
            text("Choose a recipe, then queue a job.", x + 18., 295., 19., INK);
        }
        if let Some(blocked) = &machine.blocked {
            text(blocked.code, x + 18., 396., 18., RUST);
            text(blocked.resource.as_deref().unwrap_or("shared total capacity"), x + 18., 420., 17., RUST);
            text("Deliver stock to make room; escrow is safe.", x + 18., 444., 16., RUST);
        }
        let rule = recipe.as_ref().and_then(|key| sim.definitions.recipes.get(key));
        text("RECIPE   [Left / Right]", x + 18., 487., 15., TEAL);
        text(rule.map_or("No recipes", |rule| rule.label.as_str()), x + 18., 513., 20., INK);
        if button("<", Rect::new(x + 18., 530., 34., 32.), recipe.is_some(), mouse, clicked) {
            choice.recipe = choice.recipe.wrapping_add(sim.definitions.recipes.len().saturating_sub(1));
        }
        if button(">", Rect::new(x + 59., 530., 34., 32.), recipe.is_some(), mouse, clicked) {
            choice.recipe += 1;
        }
        if button("Queue", Rect::new(x + 103., 530., 118., 32.), recipe.is_some(), mouse, clicked) {
            choice.machine = index;
            commands.push(Command::Queue { machine: id.clone(), recipe: recipe.clone().unwrap() });
        }
        if button("Cancel", Rect::new(x + 232., 530., 140., 32.), active.is_some(), mouse, clicked) {
            choice.machine = index;
            commands.push(Command::Cancel { machine: id.clone() });
        }
        let rule = upgrade.as_ref().and_then(|key| sim.definitions.upgrades.get(key));
        text(rule.map_or("No upgrades", |rule| rule.label.as_str()), x + 18., 594., 18., INK);
        if button("Next", Rect::new(x + 18., 610., 75., 32.), upgrade.is_some(), mouse, clicked) {
            choice.upgrade += 1;
        }
        if button("Apply upgrade", Rect::new(x + 103., 610., 269., 32.), upgrade.is_some(), mouse, clicked) {
            choice.machine = index;
            commands.push(Command::Upgrade { machine: id.clone(), upgrade: upgrade.clone().unwrap() });
        }
    }
    let lines = status_lines(status);
    let top = 725. - lines.len() as f32 * 21.;
    draw_rectangle(16., top - 17., WIDTH - 32., lines.len() as f32 * 21. + 4., PAPER);
    for (index, line) in lines.iter().enumerate() {
        text(line, 24., top + index as f32 * 21., 18., RUST);
    }
    text("1/2 machine | Arrows recipe | Space queue | Backspace cancel | U upgrade | O/C gather | D deliver | F5/F9 save/load | Esc menu", 24., 744., 16., INK);
    commands
}

#[macroquad::main(window)]
async fn main() {
    vesper3d::viewer::startup::show().await;
    let args: Vec<String> = std::env::args().collect();
    let mut life = Lifecycle::<()>::start_or_exit(&args, CUES);
    if life.options.save_dir.is_none() {
        if let Some(path) = player_save_dir() {
            life.slots = SaveSlots::new(path);
        }
    }
    let mut sim = Workshop::standard();
    life.load_flag_or_exit(&mut sim);
    let mut serial = sim.state.last_command.get(&0).copied().unwrap_or(0);
    let mut choice = Selection::default();
    let mut pending = Vec::<Command>::new();
    let mut status = String::from("Queue jobs on both machines; gather sources and deliver products.");
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let identity = identity();
    loop {
        input.begin_frame_with_keyboard(
            &mut shell,
            false,
            life.options.unattended() || platform::focused(),
            platform::keyboard(),
        );
        let dt = life.begin_frame(input.frame_seconds());
        let accepting = shell.accepting_input();
        let mut commands = Vec::new();
        if accepting {
            if let Some(script) = life.script() {
                if script.starts("machine") {
                    choice.machine += 1;
                }
                if script.starts("recipe") {
                    choice.recipe += 1;
                }
                if script.starts("upgrade_next") {
                    choice.upgrade += 1;
                }
                if script.starts("item") {
                    choice.item += 1;
                }
                if script.starts("smelt") {
                    commands.extend(queue(&sim, 0, named(&sim.definitions.recipes, "smelt")));
                }
                if script.starts("gear") {
                    commands.extend(queue(&sim, 1, named(&sim.definitions.recipes, "gear")));
                }
                if script.starts("queue") {
                    commands.extend(queue(&sim, choice.machine, selected(&sim.definitions.recipes, choice.recipe)));
                }
                if script.starts("ore") {
                    commands.extend(item_command(&sim, "ore", 4, true));
                }
                if script.starts("coal") {
                    commands.extend(item_command(&sim, "coal", 2, true));
                }
                if script.starts("deliver") {
                    commands.extend(item_command(&sim, "plate", 1, false));
                }
                if script.starts("full") {
                    commands.extend(fill_source(&sim));
                }
                if script.starts("recovery") {
                    if let Some(item) = named(&sim.definitions.items, "plate") {
                        let available = sim.state.inventory.quantity(&item);
                        if available >= 3 {
                            commands.push(Command::Deliver { item, amount: available.min(4) });
                        } else if let Some(ore) = named(&sim.definitions.items, "ore") {
                            let amount = sim.state.inventory.quantity(&ore).min(3);
                            if amount > 0 {
                                commands.push(Command::Deliver { item: ore, amount });
                            }
                        }
                    }
                }
                if script.starts("cancel") {
                    if let Some(machine) = selected(&sim.state.machines, choice.machine) {
                        commands.push(Command::Cancel { machine });
                    }
                }
                if script.starts("upgrade") {
                    if let (Some(machine), Some(upgrade)) = (
                        selected(&sim.state.machines, choice.machine),
                        selected(&sim.definitions.upgrades, choice.upgrade),
                    ) {
                        commands.push(Command::Upgrade { machine, upgrade });
                    }
                }
            } else {
                if input.pressed(KeyCode::Key1) {
                    choice.machine = 0;
                }
                if input.pressed(KeyCode::Key2) {
                    choice.machine = 1;
                }
                if input.pressed(KeyCode::Right) {
                    choice.recipe += 1;
                }
                if input.pressed(KeyCode::Left) {
                    choice.recipe = choice.recipe.wrapping_add(sim.definitions.recipes.len().saturating_sub(1));
                }
                if input.pressed(KeyCode::Tab) {
                    choice.upgrade += 1;
                }
                if input.pressed(KeyCode::Down) {
                    choice.item += 1;
                }
                if input.pressed(KeyCode::Space) {
                    commands.extend(queue(&sim, choice.machine, selected(&sim.definitions.recipes, choice.recipe)));
                }
                if input.pressed(KeyCode::O) {
                    commands.extend(item_command(&sim, "ore", 4, true));
                }
                if input.pressed(KeyCode::C) {
                    commands.extend(item_command(&sim, "coal", 2, true));
                }
                if input.pressed(KeyCode::D) {
                    commands.extend(item_command(&sim, "plate", 1, false));
                }
                if input.pressed(KeyCode::Backspace) {
                    if let Some(machine) = selected(&sim.state.machines, choice.machine) {
                        commands.push(Command::Cancel { machine });
                    }
                }
                if input.pressed(KeyCode::U) {
                    if let (Some(machine), Some(upgrade)) = (
                        selected(&sim.state.machines, choice.machine),
                        selected(&sim.definitions.upgrades, choice.upgrade),
                    ) {
                        commands.push(Command::Upgrade { machine, upgrade });
                    }
                }
            }
        }
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let notice = life.quick_save(&sim, "Workshop production in progress");
            status = format!("{}: {}", notice.title, notice.detail);
        }
        if load {
            let notice = life.quick_load(&mut sim);
            status = format!("{}: {}", notice.title, notice.detail);
            if notice.ok {
                serial = sim.state.last_command.get(&0).copied().unwrap_or(0);
                pending.clear();
                commands.clear();
            }
        }
        pending.extend(commands);
        life.feed((), 0, [0.; 2]);
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            life.take_tick();
            let mut intentions = Vec::new();
            for command in pending.drain(..) {
                let Some(next_serial) = serial.checked_add(1) else {
                    status = "Player command serial exhausted; start a new workshop instance.".into();
                    break;
                };
                serial = next_serial;
                intentions.push(Envelope { seat: 0, serial, command });
            }
            sim.step(&intentions);
            if let Some(outcome) = sim.outcomes.last() {
                status = if outcome.ok { "Action accepted.".into() } else { outcome.detail.clone() };
            }
        }
        pending.extend(draw_workshop(&sim, &mut choice, accepting && life.script().is_none(), &status));
        set_default_camera();
        if shell.local_menu(&identity.title, &[identity.controls.as_str()]) || game_client::exit_requested() {
            break;
        }
        if let Some(path) = life.capture_path() {
            life.captured(&path, kit::capture::save_frame(&path).map_err(|error| error.to_string()));
        }
        if life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    match sim.inspect(&selected(&sim.state.machines, choice.machine).unwrap_or_default()) {
        Ok(report) => println!("workshop: {report}"),
        Err(error) => eprintln!("workshop inspection: {error}"),
    }
    if let Some(report) = life.report() {
        println!("{report}");
    }
}

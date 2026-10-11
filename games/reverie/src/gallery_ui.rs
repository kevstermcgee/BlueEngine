use macroquad::prelude::*;
use reverie::{
    collection::{room_seed, theme, Exhibit, MOTTO, PALETTES, THEMES},
    route_name, Sim, Target,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
use vesper3d::viewer::{
    devkit::{load_or_default, store_atomic, Settings},
    game_input::ClientInput,
};
const INK: Color = Color::new(0.035, 0.055, 0.048, 0.96);
const PAPER: Color = Color::new(0.93, 0.89, 0.79, 1.);
const BRASS: Color = Color::new(0.8, 0.66, 0.40, 1.);
const MUTED: Color = Color::new(0.57, 0.60, 0.53, 1.);
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub audio: Settings,
    pub footsteps: bool,
    pub gentle: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        let audio = Settings { music: 0.55, sfx: 0.6, ..Default::default() };
        Self { audio, footsteps: true, gentle: false }
    }
}
impl Preferences {
    pub fn load(p: &Path) -> Self {
        let mut s: Self = load_or_default(p);
        s.audio = s.audio.sanitized();
        s
    }
    pub fn store(&self, p: &Path) {
        if !store_atomic(p, self) {
            eprintln!("Could not store museum settings.");
        }
    }
}
#[derive(Clone)]
pub enum Page {
    Welcome,
    Pause,
    Map,
    Settings,
    Inspect(Exhibit),
}
pub struct Ui {
    pub page: Page,
    pub selection: usize,
    pub map_pan: Vec2,
    pub map_zoom: f32,
    pub serif: Font,
    pub sans: Font,
    pub notice: String,
    pub notice_time: f32,
    pub hint_time: f32,
}
#[derive(Default)]
pub struct Response {
    pub resume: bool,
    pub quit: bool,
    pub save: bool,
    pub load: bool,
    pub changed_settings: bool,
    pub fullscreen: bool,
}
impl Ui {
    pub fn new() -> Self {
        Self {
            page: Page::Welcome,
            selection: 0,
            map_pan: Vec2::ZERO,
            map_zoom: 1.,
            serif: load_ttf_font_from_bytes(include_bytes!("../assets/gallery-serif.ttf")).expect("gallery font"),
            sans: load_ttf_font_from_bytes(include_bytes!("../assets/gallery-sans.ttf")).expect("gallery sans font"),
            notice: String::new(),
            notice_time: 0.,
            hint_time: 0.,
        }
    }
    pub fn announce(&mut self, text: impl Into<String>) {
        self.notice = text.into();
        self.notice_time = 5.;
    }
    pub fn open(&mut self, page: Page) {
        self.page = page;
        self.selection = 0;
    }
    fn text(&self, text: &str, x: f32, y: f32, size: f32, col: Color, serif: bool) {
        draw_text_ex(
            text,
            x,
            y,
            TextParams {
                font: if serif { Some(&self.serif) } else { Some(&self.sans) },
                font_size: size as u16,
                color: col,
                ..Default::default()
            },
        );
    }
    fn centered(&self, text: &str, y: f32, size: f32, col: Color, serif: bool) {
        let font = if serif { Some(&self.serif) } else { Some(&self.sans) };
        let w = measure_text(text, font, size as u16, 1.).width;
        self.text(text, (screen_width() - w) / 2., y, size, col, serif);
    }
    pub fn hud(&mut self, sim: &Sim, dt: f32, transition: f32, gentle: bool) {
        self.notice_time = (self.notice_time - dt).max(0.);
        self.hint_time = (self.hint_time - dt).max(0.);
        let s = (screen_height() / 900.).clamp(0.65, 1.55);
        let w = screen_width();
        let h = screen_height();
        // Museum signage stays quiet; the paintings keep the screen.
        draw_rectangle(0., 0., w, 105. * s, Color::new(0.02, 0.035, 0.029, 0.55));
        self.text("R E V E R I E", 30. * s, 37. * s, 24. * s, PAPER, true);
        let title = if sim.depth() == 0 {
            format!("{}  /  {}", THEMES[sim.emotion()].to_uppercase(), route_name(&sim.archive.room))
        } else {
            format!("INSIDE A MEMORY  /  LAYER {:02}", sim.depth())
        };
        self.text(&title, 30. * s, 64. * s, 15. * s, BRASS, false);
        self.text(
            MOTTO[if sim.depth() == 0 { sim.emotion() } else { sim.world_emotion() }],
            30. * s,
            88. * s,
            13. * s,
            MUTED,
            false,
        );
        self.text(
            &format!("{:02} KEYS     {:02} ROOMS", sim.archive.keys.len(), sim.archive.visited.len()),
            w - 250. * s,
            40. * s,
            18. * s,
            PAPER,
            false,
        );
        self.text(
            &format!("{} painting worlds explored", sim.archive.worlds.len()),
            w - 250. * s,
            67. * s,
            13. * s,
            MUTED,
            false,
        );
        draw_circle(w / 2., h / 2., 2. * s, Color::new(0.96, 0.92, 0.79, 0.8));
        if let Some(target) = sim.target() {
            let (name, prompt) = match target {
                Target::Art(slot) => (
                    sim.art(slot).name,
                    if slot < 3 { "E  Step inside    Q  Read the story" } else { "E  Awaken    Q  Read the story" },
                ),
                Target::Nested => (sim.inner_art().name, "E  Enter the smaller painting    Q  Read the story"),
                Target::Door(b) => (
                    {
                        let mut next = sim.archive.room.clone();
                        next.push(b);
                        format!("The {} gallery", THEMES[theme(room_seed(sim.archive.seed, &next))])
                    },
                    if sim.has_key() {
                        "E  Open the next gallery"
                    } else {
                        "A key is hidden in one of this room's artworks"
                    },
                ),
                Target::Parent => ("The way you came".into(), "E  Return to the previous gallery"),
                Target::Return => ("The edge of the dream".into(), "E  Return through the frame"),
                Target::Key => ("A memory, made golden".into(), "E  Collect the room's key"),
            };
            draw_rectangle(w / 2. - 340. * s, h - 132. * s, 680. * s, 78. * s, INK);
            self.centered(&name, h - 99. * s, 22. * s, PAPER, true);
            self.centered(prompt, h - 71. * s, 15. * s, BRASS, false);
        }
        let footer = if sim.depth() == 0 {
            "WASD  Walk     E  Interact     Q  Artwork story     H  Hint     M  Map     ESC  Pause"
        } else {
            "WASD  Walk     E  Enter / collect     Q  Story     B  Return one layer     H  Hint     M  Map"
        };
        self.centered(footer, h - 22. * s, 13. * s, PAPER, false);
        if self.hint_time > 0. {
            draw_rectangle(0., h - 203. * s, w, 45. * s, INK);
            self.centered(sim.hint(), h - 174. * s, 17. * s, BRASS, false);
        }
        if self.notice_time > 0. {
            self.centered(&self.notice, 142. * s, 20. * s, PAPER, true);
        }
        if transition > 0. {
            let alpha = transition.clamp(0., 1.);
            draw_rectangle(0., 0., w, h, Color::new(0.94, 0.88, 0.68, alpha * alpha));
            if !gentle {
                for i in 0..30 {
                    let a = i as f32 * std::f32::consts::TAU / 30.;
                    let r = h * 0.45;
                    draw_line(
                        w / 2. + a.cos() * r,
                        h / 2. + a.sin() * r,
                        w / 2. + a.cos() * r * 1.35,
                        h / 2. + a.sin() * r * 1.35,
                        1.5 * s,
                        Color::new(1., 0.91, 0.68, alpha * 0.20),
                    );
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)] // Screen coordinates keep menu layout explicit.
    fn button(&self, label: &str, index: usize, x: f32, y: f32, width: f32, s: f32, input: &ClientInput) -> bool {
        let rect = Rect::new(x, y, width, 43. * s);
        let (mx, my) = mouse_position();
        let hover = rect.contains(vec2(mx, my));
        let chosen = self.selection == index;
        if chosen || hover {
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.14, 0.18, 0.14, 0.9));
            draw_rectangle(rect.x, rect.y, 2. * s, rect.h, BRASS);
        }
        self.text(label, x + 19. * s, y + 29. * s, 18. * s, if chosen || hover { PAPER } else { MUTED }, false);
        (hover && is_mouse_button_pressed(MouseButton::Left))
            || (chosen && (input.pressed(KeyCode::Enter) || input.menu_select()))
    }
    fn move_selection(&mut self, input: &ClientInput, count: usize) {
        let m = input.menu_step();
        if input.pressed(KeyCode::Up) || m.up {
            self.selection = (self.selection + count - 1) % count;
        }
        if input.pressed(KeyCode::Down) || m.down {
            self.selection = (self.selection + 1) % count;
        }
    }
    pub fn menu(&mut self, sim: &Sim, input: &ClientInput, prefs: &mut Preferences, has_save: bool) -> Response {
        let mut response = Response::default();
        let s = (screen_height() / 900.).clamp(0.65, 1.55);
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0., 0., w, h, Color::new(0.02, 0.035, 0.029, 0.84));
        let page = self.page.clone();
        match page {
            Page::Welcome | Page::Pause => {
                let x = 72. * s;
                let welcome = matches!(page, Page::Welcome);
                self.text("R E V E R I E", x, 145. * s, 62. * s, PAPER, true);
                self.text("A museum that dreams.", x, 191. * s, 23. * s, BRASS, true);
                self.text(
                    if welcome { "No closing time. No final room." } else { "Stay a little longer." },
                    x,
                    230. * s,
                    15. * s,
                    MUTED,
                    false,
                );
                self.move_selection(input, 7);
                let labels = [
                    if welcome { "Enter the museum" } else { "Continue wandering" },
                    "Explored map",
                    "Settings",
                    "Save this moment",
                    if has_save { "Return to saved moment" } else { "No saved moment yet" },
                    "How to wander",
                    "Leave the museum",
                ];
                for (i, label) in labels.iter().enumerate() {
                    if self.button(label, i, x, 270. * s + i as f32 * 48. * s, 330. * s, s, input) {
                        match i {
                            0 => response.resume = true,
                            1 => {
                                self.open(Page::Map);
                                self.map_pan = Vec2::ZERO;
                            }
                            2 => self.open(Page::Settings),
                            3 => response.save = true,
                            4 => {
                                if has_save {
                                    response.load = true;
                                    response.resume = true;
                                }
                            }
                            5 => self
                                .announce("Walk close. E enters or awakens. Q reveals a story. H gives a gentle clue."),
                            _ => response.quit = true,
                        }
                    }
                }
                let right = (w * 0.55).max(450. * s);
                self.text("Every frame is a threshold.", right, 320. * s, 26. * s, PAPER, true);
                for (i, line) in [
                    "Walk up to an artwork. Read its story.",
                    "Step into a painting and follow the light.",
                    "Some paintings contain smaller paintings.",
                    "One work in every room hides a golden key.",
                    "It opens both doors. Choose a feeling.",
                    "Your map remembers the paths you explore.",
                ]
                .iter()
                .enumerate()
                {
                    self.text(line, right, 363. * s + i as f32 * 34. * s, 17. * s, MUTED, false);
                }
                self.text("WASD / arrows    Walk", right, 610. * s, 15. * s, BRASS, false);
                self.text("E / A    Interact       Q / X    Read", right, 640. * s, 15. * s, BRASS, false);
                self.text("B    Leave painting       H    Hint", right, 670. * s, 15. * s, BRASS, false);
                self.text("M    Map       Esc / Start    Pause", right, 700. * s, 15. * s, BRASS, false);
            }
            Page::Settings => {
                let x = 90. * s;
                self.text("The way you wander", x, 140. * s, 40. * s, PAPER, true);
                self.text("Your choices are remembered between visits.", x, 183. * s, 16. * s, MUTED, false);
                self.move_selection(input, 8);
                let labels = [
                    format!("Music                         {}", if prefs.audio.music_on { "On" } else { "Off" }),
                    format!("Music volume                  {}%", (prefs.audio.music * 100.) as i32),
                    format!("Faint footsteps               {}", if prefs.footsteps { "On" } else { "Off" }),
                    format!("Artwork sounds                {}", if prefs.audio.sfx_on { "On" } else { "Off" }),
                    format!("Look sensitivity              {:.1}", prefs.audio.sensitivity),
                    format!("Gentle camera motion          {}", if prefs.gentle { "On" } else { "Off" }),
                    "Fullscreen                    F11".into(),
                    "Back to the museum menu".into(),
                ];
                for (i, label) in labels.iter().enumerate() {
                    let pressed = self.button(label, i, x, 228. * s + i as f32 * 50. * s, 450. * s, s, input);
                    let delta = if input.pressed(KeyCode::Left) || input.menu_step().left {
                        -1.
                    } else if input.pressed(KeyCode::Right) || input.menu_step().right {
                        1.
                    } else {
                        0.
                    };
                    if pressed || (self.selection == i && delta != 0.) {
                        let d = if delta == 0. { 1. } else { delta };
                        match i {
                            0 => {
                                prefs.audio.toggle_music();
                            }
                            1 => prefs.audio.music = (prefs.audio.music + d * 0.1).clamp(0., 1.),
                            2 => prefs.footsteps = !prefs.footsteps,
                            3 => {
                                prefs.audio.toggle_sfx();
                            }
                            4 => prefs.audio.sensitivity = (prefs.audio.sensitivity + d * 0.2).clamp(0.2, 4.),
                            5 => prefs.gentle = !prefs.gentle,
                            6 => response.fullscreen = true,
                            _ => self.open(Page::Pause),
                        }
                        response.changed_settings = true;
                    }
                }
                self.text("Left / Right adjusts the selected value.", x, 695. * s, 14. * s, MUTED, false);
            }
            Page::Inspect(art) => {
                let x = (w - 710. * s) / 2.;
                let y = 110. * s;
                draw_rectangle(x - 25. * s, y - 35. * s, 760. * s, 570. * s, INK);
                draw_line(x, y + 12. * s, x + 710. * s, y + 12. * s, 1., BRASS);
                self.text("THE COLLECTION", x, y, 14. * s, BRASS, false);
                let title = wrap(&art.name, Some(&self.serif), 33. * s, 670. * s);
                let mut ty = y + 63. * s;
                for line in title {
                    self.text(&line, x, ty, 33. * s, PAPER, true);
                    ty += 42. * s;
                }
                self.text(&format!("by {}", art.artist), x, ty + 5. * s, 22. * s, BRASS, true);
                self.text(&art.medium, x, ty + 38. * s, 14. * s, MUTED, false);
                ty += 100. * s;
                for line in wrap(&art.story, Some(&self.sans), 18. * s, 670. * s) {
                    self.text(&line, x, ty, 18. * s, PAPER, false);
                    ty += 28. * s;
                }
                self.text(
                    &format!("Emotional register: {}", THEMES[art.emotion]),
                    x,
                    y + 457. * s,
                    16. * s,
                    BRASS,
                    false,
                );
                self.selection = 0;
                if self.button("Return to the work", 0, x, y + 475. * s, 350. * s, s, input)
                    || input.pressed(KeyCode::Q)
                    || input.pressed(KeyCode::E)
                {
                    response.resume = true;
                }
            }
            Page::Map => {
                self.text("Where you have wandered", 45. * s, 68. * s, 32. * s, PAPER, true);
                self.text(
                    "Filled circles: galleries   Gold: key found   Small diamonds: painting worlds",
                    45. * s,
                    101. * s,
                    14. * s,
                    MUTED,
                    false,
                );
                let (zoom_x, zoom_y) = mouse_wheel();
                let _ = zoom_x;
                self.map_zoom = (self.map_zoom + zoom_y * 0.08).clamp(0.3, 2.5);
                let speed = 7. * s;
                let stick = input.gamepad().left_stick;
                self.map_pan += vec2(-stick[0], stick[1]) * speed;
                if input.down(KeyCode::Left) {
                    self.map_pan.x += speed;
                }
                if input.down(KeyCode::Right) {
                    self.map_pan.x -= speed;
                }
                if input.down(KeyCode::Up) {
                    self.map_pan.y += speed;
                }
                if input.down(KeyCode::Down) {
                    self.map_pan.y -= speed;
                }
                if is_mouse_button_down(MouseButton::Left) {
                    let d = vesper3d::viewer::game_input::mouse_pixels();
                    self.map_pan += vec2(d[0], d[1]);
                }
                let paths: Vec<_> = sim.archive.visited.iter().cloned().collect();
                let mut points = BTreeMap::new();
                // The explored tree has a stable order. Centre the current gallery and allow unlimited pan/zoom.
                let mut rank = BTreeMap::<usize, usize>::new();
                for path in &paths {
                    let r = rank.entry(path.len()).or_default();
                    points.insert(path.clone(), vec2(*r as f32 * 220., path.len() as f32 * 135.));
                    *r += 1;
                }
                for (path, p) in points.iter_mut() {
                    p.x -= ((rank[&path.len()] as f32 - 1.) * 220.) / 2.;
                }
                let origin = *points.get(&sim.archive.room).unwrap_or(&Vec2::ZERO);
                let transform = |p: Vec2| vec2(w / 2., h / 2.) + (p - origin) * s * self.map_zoom + self.map_pan;
                for (path, p) in &points {
                    if !path.is_empty() {
                        let parent = path[..path.len() - 1].to_vec();
                        if let Some(q) = points.get(&parent) {
                            let p = transform(*p);
                            let q = transform(*q);
                            draw_line(p.x, p.y, q.x, q.y, 2. * s, Color::new(0.35, 0.4, 0.32, 0.8));
                        }
                    }
                }
                for (path, position) in &points {
                    let p = transform(*position);
                    if p.y < 150. * s || p.y > h - 115. * s || p.x < 30. * s || p.x > w - 30. * s {
                        continue;
                    }
                    let active = *path == sim.archive.room;
                    let mood = theme(room_seed(sim.archive.seed, path));
                    let (col, _) = PALETTES[mood];
                    let col = Color::new(col[0], col[1], col[2], 1.);
                    if active {
                        draw_circle_lines(p.x, p.y, 24. * s * self.map_zoom, 2., PAPER);
                    }
                    draw_circle(
                        p.x,
                        p.y,
                        15. * s * self.map_zoom,
                        if sim.archive.keys.contains(path) { BRASS } else { col },
                    );
                    let label = format!("{} / {}", THEMES[mood], route_name(path));
                    self.text(&label, p.x - 80. * s, p.y + 38. * s * self.map_zoom, 13. * s, PAPER, false);
                    let prefix = format!("{}:", route_name(path));
                    let mut worlds: Vec<_> = sim.archive.worlds.iter().filter(|w| w.starts_with(&prefix)).collect();
                    worlds.sort();
                    for (i, world) in worlds.iter().enumerate() {
                        let q = p + vec2((i % 6) as f32 * 19. * s - 40. * s, 53. * s + (i / 6) as f32 * 16. * s);
                        draw_line(p.x, p.y + 16. * s, q.x, q.y, 1., Color::new(0.32, 0.38, 0.32, 0.55));
                        let diamond = |pt: Vec2| {
                            draw_triangle(pt + vec2(0., -4. * s), pt + vec2(4. * s, 0.), pt + vec2(0., 4. * s), col);
                            draw_triangle(pt + vec2(0., -4. * s), pt + vec2(-4. * s, 0.), pt + vec2(0., 4. * s), col);
                        };
                        diamond(q);
                        if **world == sim.world_id() {
                            draw_circle_lines(q.x, q.y, 7. * s, 1., PAPER);
                        }
                    }
                }
                // The sidebar records actual parent-child edges within this gallery's paintings.
                let prefix = format!("{}:", route_name(&sim.archive.room));
                let visits: Vec<Vec<u8>> = sim
                    .archive
                    .worlds
                    .iter()
                    .filter(|id| id.starts_with(&prefix))
                    .filter_map(|id| id.split_once(':').and_then(|(_, p)| serde_json::from_str(p).ok()))
                    .collect();
                if !visits.is_empty() {
                    let x = w - 300. * s;
                    draw_rectangle(x - 20. * s, 148. * s, 300. * s, h - 270. * s, INK);
                    self.text("Within the frames", x, 180. * s, 20. * s, PAPER, true);
                    self.text("This gallery's explored layers", x, 208. * s, 12. * s, MUTED, false);
                    let point = |p: &[u8]| {
                        vec2(
                            x + 35. * s + p.first().copied().unwrap_or(0) as f32 * 70. * s,
                            264. * s + (p.len() as f32 - 1. - sim.depth().saturating_sub(5) as f32) * 45. * s,
                        )
                    };
                    for path in &visits {
                        let pos = point(path);
                        if pos.y < 238. * s || pos.y > h - 135. * s {
                            continue;
                        }
                        if path.len() > 1 {
                            let parent = point(&path[..path.len() - 1]);
                            draw_line(pos.x, pos.y, parent.x, parent.y, 1.5 * s, BRASS);
                        }
                        draw_poly(pos.x, pos.y, 4, 7. * s, 45., BRASS);
                        if *path == sim.archive.inside {
                            draw_circle_lines(pos.x, pos.y, 13. * s, 2., PAPER);
                        }
                        if path.len() == 1 {
                            self.text(
                                &format!("Canvas {}", path[0] + 1),
                                pos.x - 25. * s,
                                pos.y - 18. * s,
                                11. * s,
                                MUTED,
                                false,
                            );
                        } else {
                            self.text(
                                &format!("Layer {}", path.len()),
                                pos.x + 12. * s,
                                pos.y + 4. * s,
                                11. * s,
                                MUTED,
                                false,
                            );
                        }
                    }
                }
                draw_rectangle(0., h - 100. * s, w, 100. * s, INK);
                self.text(
                    "Drag or use arrows to pan. Scroll to zoom. Only explored paths are drawn.",
                    45. * s,
                    h - 68. * s,
                    14. * s,
                    MUTED,
                    false,
                );
                self.selection = 0;
                if self.button("Back", 0, 45. * s, h - 51. * s, 200. * s, s, input) || input.pressed(KeyCode::M) {
                    self.open(Page::Pause);
                }
            }
        }
        if self.notice_time > 0. {
            self.centered(&self.notice, h - 38. * s, 16. * s, BRASS, false);
        }
        response
    }
}
fn wrap(text: &str, font: Option<&Font>, size: f32, width: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut current = String::new();
    for word in text.split_whitespace() {
        let test = if current.is_empty() { word.into() } else { format!("{current} {word}") };
        if measure_text(&test, font, size as u16, 1.).width > width && !current.is_empty() {
            lines.push(current);
            current = word.into();
        } else {
            current = test;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

//! Public Scene, font measurement, UiFrame/actions and AudioBank bindings; no engine special cases.
use super::*;
use vesper3d::two_d::{
    client::{Theme, UiFrame},
    draw::{self, Color, FontAsset, Scene},
    ui::{Action, Layout, Screen},
};
fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::new(r, g, b, 1.)
}
fn colors(style: u8) -> [Color; 4] {
    match style {
        0 => [
            rgb(0.91, 0.87, 0.75),
            rgb(0.19, 0.28, 0.26),
            rgb(0.65, 0.28, 0.17),
            rgb(0.99, 0.96, 0.88),
        ],
        1 => [
            rgb(0.13, 0.17, 0.18),
            rgb(0.85, 0.88, 0.75),
            rgb(0.93, 0.58, 0.19),
            rgb(0.23, 0.29, 0.29),
        ],
        _ => [
            rgb(0.23, 0.10, 0.45),
            rgb(1., 0.96, 0.78),
            rgb(1., 0.32, 0.43),
            rgb(0.31, 0.23, 0.67),
        ],
    }
}
fn lamp(style: u8, n: i32) -> Rect {
    match style {
        0 => Rect::new(400 + (n % 2) * 140, 155 + (n / 2) * 125, 110, 90),
        1 => Rect::new(75 + n * 175, 145, 135, 125),
        _ => Rect::new(335 + (n % 2) * 210, 70 + (n / 2) * 165, 185, 140),
    }
}
fn text(s: &mut Scene, label: impl Into<String>, x: i32, y: i32, size: f32, c: Color) {
    s.text_with_font(10, label, Point::new(x, y), size, c, "face");
}
fn ui_text(s: &mut Scene, label: impl Into<String>, x: i32, y: i32, size: f32, c: Color) {
    s.text_with_font(210, label, Point::new(x, y), size, c, "face");
}
impl<const S: u8> draw::Game for Lab<S> {
    fn fonts() -> &'static [FontAsset] {
        match S {
            0 => &[FontAsset {
                id: "face",
                file: "assets/fonts/LiberationSerif-Italic.ttf",
            }],
            1 => &[FontAsset {
                id: "face",
                file: "assets/fonts/LiberationMono-Regular.ttf",
            }],
            _ => &[FontAsset {
                id: "face",
                file: "assets/fonts/LiberationSans-Bold.ttf",
            }],
        }
    }
    fn theme() -> Theme {
        Theme {
            background: colors(S)[0],
            font: Some("face"),
            ..Theme::default()
        }
    }
    fn cue_particles() -> bool {
        false
    }
    fn device_input(&mut self, mut input: Intent) -> Intent {
        if input.action {
            if let Some(p) = input.pointer {
                input.pointer = (0..4)
                    .find(|n| lamp(S, *n).contains(p))
                    .map(|n| Point::new(n, -1));
                if input.pointer.is_none() {
                    input.action = false;
                }
            }
        }
        input
    }
    fn draw(&self, s: &mut Scene) {
        let [bg, ink, accent, panel] = colors(S);
        match S {
            0 => {
                s.rect(-90, Rect::new(35, 18, 730, 414), panel);
                for y in (56..410).step_by(22) {
                    s.rect(-80, Rect::new(57, y, 690, 1), rgb(0.81, 0.83, 0.73));
                }
                s.rect(-70, Rect::new(365, 20, 3, 410), bg);
                for y in (50..400).step_by(44) {
                    s.circle(-60, Point::new(43, y), 5., ink);
                }
                text(s, "Field notes", 80, 80, 40., ink);
                text(s, "No. 04 — light study", 80, 111, 19., ink);
                text(s, "A mark wakes its neighbour.", 80, 183, 22., ink);
                text(s, "Find the pair of gestures", 80, 220, 22., ink);
                text(s, "that illuminates every seed.", 80, 251, 22., ink);
                text(
                    s,
                    format!("Attempts remaining: {}", 4 - self.state.turns),
                    80,
                    330,
                    23.,
                    accent,
                );
            }
            1 => {
                s.rect(-90, Rect::new(25, 25, 750, 400), panel);
                for y in (30..420).step_by(6) {
                    s.rect(-80, Rect::new(30, y, 740, 1), rgb(0.25, 0.31, 0.31));
                }
                for (x, y) in [(40, 40), (760, 40), (40, 410), (760, 410)] {
                    s.circle(0, Point::new(x, y), 6., ink);
                    s.rect(1, Rect::new(x - 4, y - 1, 8, 2), bg);
                }
                text(s, "OPTICAL RELAY / FOUR CHANNELS", 70, 82, 26., ink);
                text(
                    s,
                    format!(
                        "CYCLES {:02} / 04    TARGET: ALL CHANNELS",
                        self.state.turns
                    ),
                    70,
                    116,
                    18.,
                    accent,
                );
                s.rect(2, Rect::new(55, 290, 690, 25), bg);
                text(s, "SELECT CHANNEL / CLOSE RELAY", 70, 309, 17., ink);
            }
            _ => {
                for n in 0..7 {
                    s.circle(
                        -80,
                        Point::new(45 + n * 110, 65 + (n % 3) * 130),
                        55.,
                        panel,
                    );
                }
                s.rect(0, Rect::new(35, 60, 255, 340), accent);
                text(s, "FLIP", 62, 130, 65., ink);
                text(s, "POP!", 57, 192, 64., ink);
                text(s, "LIGHT THE LOT", 57, 247, 22., ink);
                text(
                    s,
                    format!("{} TURNS LEFT", 4 - self.state.turns),
                    57,
                    291,
                    24.,
                    ink,
                );
                text(s, "TAP + NEIGHBOUR", 57, 351, 19., ink);
            }
        }
        for n in 0..4 {
            let r = lamp(S, n);
            let on = self.state.lamps & (1 << n) != 0;
            let selected = self.state.selected == n as u8;
            if S == 1 {
                s.circle(5, Point::new(r.x + r.w / 2, r.y + 58), 61., bg);
                s.circle(
                    6,
                    Point::new(r.x + r.w / 2, r.y + 58),
                    50.,
                    if on { accent } else { panel },
                );
                for a in 0..9 {
                    let angle = (a as f32 * 0.35) - 3.;
                    s.circle(
                        7,
                        Point::new(
                            r.x + 67 + (angle.cos() * 42.) as i32,
                            r.y + 58 + (angle.sin() * 42.) as i32,
                        ),
                        2.,
                        ink,
                    );
                }
                s.rect(8, Rect::new(r.x + 64, r.y + 28, 4, 32), ink);
            } else if S == 0 {
                s.circle(5, Point::new(r.x + 55, r.y + 43), 35., ink);
                s.circle(
                    6,
                    Point::new(r.x + 54, r.y + 42),
                    31.,
                    if on { accent } else { panel },
                );
                s.rect(7, Rect::new(r.x + 22, r.y + 70, 66, 2), ink);
            } else {
                let wobble = if on {
                    (self.state.tick as f32 * 0.1).sin() * 4.
                } else {
                    0.
                };
                s.rect(
                    4,
                    Rect::new(r.x + 6, r.y + 6, r.w, r.h),
                    rgb(0.09, 0.04, 0.22),
                );
                s.rect(
                    5,
                    Rect::new(r.x, r.y + (wobble as i32), r.w, r.h),
                    if on { accent } else { panel },
                );
                s.circle(6, Point::new(r.x + 55, r.y + 48), 10., ink);
                s.circle(6, Point::new(r.x + 128, r.y + 48), 10., ink);
                s.rect(
                    7,
                    Rect::new(r.x + 57, r.y + 80, 68, if on { 12 } else { 4 }),
                    ink,
                );
            }
            if selected {
                s.rect(9, Rect::new(r.x, r.y + r.h + 3, r.w, 3), accent);
            }
            text(
                s,
                format!("{} {}", n + 1, if on { "ON" } else { "OFF" }),
                r.x + 16,
                r.y + r.h - 5,
                19.,
                ink,
            );
        }
    }
    fn interface(&self, s: &mut Scene, frame: &UiFrame) -> Layout {
        let [bg, ink, accent, panel] = colors(S);
        let mut layout = Layout::default();
        ui_text(
            s,
            "Arrows / Space / click · Esc pause · R restart · K save · L load · M sound",
            35,
            446,
            15.,
            ink,
        );
        if !frame.notice.is_empty() {
            ui_text(s, frame.notice, 40, 421, 16., accent);
        }
        if frame.screen == Screen::Playing {
            return layout;
        }
        let (heading, primary, label) = match frame.screen {
            Screen::Start => ("Ready to begin?", Action::Start, "BEGIN"),
            Screen::Paused => ("Paused — take your time", Action::Resume, "RESUME"),
            Screen::Won => ("All four alight!", Action::Restart, "PLAY AGAIN"),
            _ => ("A fresh page awaits", Action::Restart, "RETRY"),
        };
        let actions = [
            (label, primary),
            ("SAVE", Action::Save),
            ("LOAD", Action::Load),
            ("QUIT", Action::Quit),
        ];
        match S {
            0 => {
                s.rect(200, Rect::new(58, 140, 300, 256), panel);
                ui_text(s, heading, 78, 173, 23., ink);
                for (n, (label, action)) in actions.iter().enumerate() {
                    let r = Rect::new(78, 190 + n as i32 * 45, 255, 35);
                    s.rect(202, r, if frame.selected == n { bg } else { panel });
                    ui_text(s, format!("{}  {label}", n + 1), 95, r.y + 25, 23., ink);
                    layout.button(r, *action);
                }
                ui_text(s, "Index / observations", 426, 70, 22., ink);
            }
            1 => {
                s.rect(200, Rect::new(45, 320, 710, 80), bg);
                let width = frame
                    .fonts
                    .measure(heading, Some("face"), 19.)
                    .expect("loaded font")
                    .width;
                ui_text(s, heading, (400. - width / 2.) as i32, 339, 19., accent);
                for (n, (label, action)) in actions.iter().enumerate() {
                    let r = Rect::new(65 + n as i32 * 175, 350, 145, 38);
                    s.rect(202, r, panel);
                    s.rect(
                        203,
                        Rect::new(r.x + 4, r.y + 4, 137, 2),
                        if frame.selected == n { accent } else { ink },
                    );
                    ui_text(s, *label, r.x + 12, r.y + 26, 17., ink);
                    layout.button(r, *action);
                }
                s.circle(
                    205,
                    Point::new(736, 335),
                    4.,
                    if (frame.elapsed * 2.) as i32 % 2 == 0 {
                        accent
                    } else {
                        panel
                    },
                );
            }
            _ => {
                s.rect(200, Rect::new(310, 45, 455, 370), bg);
                ui_text(s, heading, 332, 81, 25., ink);
                for (n, (label, action)) in actions.iter().enumerate() {
                    let r = Rect::new(333, 100 + n as i32 * 75, 408, 60);
                    s.rect(202, Rect::new(r.x + 5, r.y + 5, r.w, r.h), panel);
                    s.rect(203, r, if frame.selected == n { accent } else { panel });
                    let size = if frame.selected == n {
                        28. + (frame.elapsed * 4.).sin() * 1.5
                    } else {
                        28.
                    };
                    let width = frame
                        .fonts
                        .measure(label, Some("face"), size)
                        .expect("loaded font")
                        .width;
                    ui_text(
                        s,
                        *label,
                        r.x + (r.w as f32 / 2. - width / 2.) as i32,
                        r.y + 40,
                        size,
                        ink,
                    );
                    layout.button(r, *action);
                }
            }
        }
        layout
    }
}

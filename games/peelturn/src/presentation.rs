use super::*;
use draw::*;
const BG: Color = Color::new(0.035, 0.085, 0.105, 1.);
const PANEL: Color = Color::new(0.055, 0.13, 0.15, 1.);
const TILE_COLOR: Color = Color::new(0.085, 0.18, 0.20, 1.);
const LINE: Color = Color::new(0.14, 0.29, 0.30, 1.);
const MUTED: Color = Color::new(0.48, 0.65, 0.65, 1.);
const CREAM: Color = Color::new(0.96, 0.92, 0.81, 1.);
const COPPER: Color = Color::new(0.96, 0.55, 0.32, 1.);
const MINT: Color = Color::new(0.43, 0.88, 0.73, 1.);
const ROSE: Color = Color::new(0.98, 0.40, 0.41, 1.);
fn color(id: usize) -> Color {
    [
        COPPER,
        MINT,
        Color::new(0.66, 0.70, 0.97, 1.),
        Color::new(0.98, 0.78, 0.35, 1.),
    ][id % 4]
}
fn text(s: &mut Scene, value: impl Into<String>, x: i32, y: i32, size: f32, c: Color) {
    s.text(20, value, Point::new(x, y), size, c);
}
fn inset(r: Rect, amount: i32) -> Rect {
    Rect::new(
        r.x + amount,
        r.y + amount,
        r.w - amount * 2,
        r.h - amount * 2,
    )
}
fn arrow(s: &mut Scene, p: Point, dir: usize, c: Color, layer: i32) {
    let v = DIRS[dir];
    let side = [-v[1], v[0]];
    for t in -5..=6 {
        s.rect(
            layer,
            Rect::new(p.x + v[0] * t - 1, p.y + v[1] * t - 1, 3, 3),
            c,
        );
    }
    for t in 0..6 {
        for sign in [-1, 1] {
            s.rect(
                layer,
                Rect::new(
                    p.x + v[0] * (7 - t) + side[0] * t * sign - 1,
                    p.y + v[1] * (7 - t) + side[1] * t * sign - 1,
                    3,
                    3,
                ),
                c,
            );
        }
    }
}
fn words(s: &mut Scene, value: &str, x: i32, y: i32, width: usize, c: Color) {
    let mut line = String::new();
    let mut yy = y;
    for word in value.split_whitespace() {
        if line.len() + word.len() + 1 > width {
            text(s, &line, x, yy, 17., c);
            line.clear();
            yy += 19;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        text(s, line, x, yy, 17., c);
    }
}
impl draw::Game for Peelturn {
    fn show_hud() -> bool {
        false
    }
    fn start_automatically() -> bool {
        true
    }
    fn device_input(&mut self, mut input: Intent) -> Intent {
        use macroquad::prelude::{is_key_pressed, KeyCode};
        if is_key_pressed(KeyCode::H) {
            return Self::command(HELP);
        }
        if is_key_pressed(KeyCode::Backspace) {
            return Self::command(UNDO);
        }
        if is_key_pressed(KeyCode::Tab) {
            return Self::command(CYCLE);
        }
        if is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Enter) {
            input.action = true;
            input.pointer = None;
        }
        input
    }
    fn menu_status(&self) -> String {
        match self.state.board.result {
            ResultState::Jammed => "Clockwise exit blocked. Backspace to undo.",
            ResultState::EmptySockets => "Empty sockets remain. Backspace to undo.",
            ResultState::Trapped => "Every packet is trapped. Backspace to undo.",
            ResultState::Clear if self.state.level + 1 == levels().len() => {
                "Twelve boards solved. Every turn left a trace."
            }
            _ => "No timer. Backspace undoes a whole launch.",
        }
        .into()
    }
    fn draw(&self, s: &mut Scene) {
        let b = &self.state.board;
        let l = self.level();
        s.rect(-90, Rect::new(0, 0, 800, 450), BG);
        s.rect(-80, Rect::new(0, 0, 800, 55), PANEL);
        s.rect(-70, Rect::new(0, 54, 800, 1), LINE);
        text(s, "PEELTURN", 31, 38, 34., CREAM);
        text(s, "EVERY TURN LEAVES A TRACE", 400, 24, 13., MUTED);
        for n in 0..12 {
            s.rect(
                0,
                Rect::new(400 + n * 29, 36, 23, 5),
                if n < (self.state.level as i32) {
                    MINT
                } else if n == self.state.level as i32 {
                    COPPER
                } else {
                    LINE
                },
            );
        }
        s.rect(-50, Rect::new(30, 64, 322, 314), LINE);
        s.rect(-40, Rect::new(33, 67, 316, 308), PANEL);
        for y in 0..6 {
            for x in 0..6 {
                let r = cell_rect([x, y]);
                s.rect(-20, inset(r, 1), TILE_COLOR);
                s.rect(-19, Rect::new(r.x + 3, r.y + 3, 2, 2), LINE);
            }
        }
        for p in &l.walls {
            let r = cell_rect(*p);
            s.rect(0, inset(r, 2), BG);
            s.rect(1, Rect::new(r.x + 4, r.y + 3, 40, 36), LINE);
            s.rect(2, Rect::new(r.x + 7, r.y + 5, 34, 3), MUTED);
            s.rect(2, Rect::new(r.x + 10, r.y + 17, 28, 15), PANEL);
        }
        for p in &l.sockets {
            let c = packet_center(*p);
            let filled = b.deposits.iter().any(|d| d.cell == *p);
            s.circle(3, c, 18., if filled { MINT } else { COPPER });
            s.circle(4, c, 15., TILE_COLOR);
            if !filled {
                s.circle(5, c, 3., COPPER);
            }
            for (dx, dy) in [(0, -21), (0, 21), (-21, 0), (21, 0)] {
                s.rect(
                    5,
                    Rect::new(c.x + dx - 1, c.y + dy - 1, 3, 3),
                    if filled { MINT } else { COPPER },
                );
            }
        }
        if b.result == ResultState::Playing && b.flight.is_none() {
            if let Some(end) = b.first_impact(l) {
                let start = l.packets[b.selected].cell;
                let d = DIRS[b.dir];
                let mut p = start;
                while p != end {
                    p = [p[0] + d[0], p[1] + d[1]];
                    let c = packet_center(p);
                    s.circle(6, c, 2., MUTED);
                }
                let c = packet_center(end);
                s.circle(6, c, 12., CREAM);
                s.circle(7, c, 10., TILE_COLOR);
                arrow(s, c, (b.dir + 1) % 4, COPPER, 8);
            }
        }
        for d in &b.deposits {
            let c = packet_center(d.cell);
            s.circle(8, Point::new(c.x, c.y + 3), 11., BG);
            s.circle(9, c, 10., color(d.packet));
            s.circle(10, Point::new(c.x - 3, c.y - 3), 3., CREAM);
        }
        for (i, p) in l.packets.iter().enumerate() {
            if b.alive[i] {
                let c = packet_center(p.cell);
                if b.selected == i {
                    s.circle(10, c, 22., color(i));
                    s.circle(11, c, 20., TILE_COLOR);
                    let d = DIRS[b.dir];
                    arrow(
                        s,
                        Point::new(c.x + d[0] * 29, c.y + d[1] * 29),
                        b.dir,
                        CREAM,
                        16,
                    );
                }
                s.circle(12, Point::new(c.x, c.y + 3), 17., BG);
                s.circle(13, c, 17., color(i));
                text(s, p.beads.to_string(), c.x - 6, c.y + 7, 25., BG);
                text(
                    s,
                    ((b'A' + i as u8) as char).to_string(),
                    cell_rect(p.cell).x + 4,
                    cell_rect(p.cell).y + 12,
                    11.,
                    CREAM,
                );
            }
        }
        if let Some(f) = &b.flight {
            if f.left > 0 {
                let mut c = packet_center(f.cell);
                if f.wait == 0 && b.result == ResultState::Playing {
                    c.x += DIRS[f.dir][0] * TILE * f.progress as i32 / SLIDE as i32;
                    c.y += DIRS[f.dir][1] * TILE * f.progress as i32 / SLIDE as i32;
                }
                s.circle(13, Point::new(c.x, c.y + 3), 16., BG);
                s.circle(14, c, 16., color(f.packet));
                text(s, f.left.to_string(), c.x - 6, c.y + 7, 24., BG);
            }
            if f.wait > 0 {
                let c = packet_center(f.cell);
                let radius = 18. + (IMPACT - f.wait) as f32 * 0.55;
                s.circle(14, c, radius, Color::new(0.96, 0.92, 0.81, 0.15));
                text(s, format!("{}", f.impact), c.x - 5, c.y - 17, 19., CREAM);
            }
        }
        // Read-only legend and the selected packet's first-impact preview.
        text(
            s,
            format!("BOARD {:02} / 12", self.state.level + 1),
            395,
            85,
            15.,
            COPPER,
        );
        text(s, &l.name, 395, 116, 28., CREAM);
        words(s, &l.tip, 395, 141, 43, MUTED);
        text(
            s,
            format!("SOCKETS  {} / {}", b.filled(l), l.sockets.len()),
            395,
            195,
            18.,
            MINT,
        );
        text(s, format!("LAUNCHES  {}", b.launches), 600, 195, 16., MUTED);
        for (i, p) in l.packets.iter().enumerate() {
            let x = 395 + i as i32 * 85;
            let selected = b.selected == i && b.alive[i];
            s.rect(
                0,
                Rect::new(x, 209, 76, 49),
                if selected { color(i) } else { LINE },
            );
            s.rect(1, Rect::new(x + 2, 211, 72, 45), PANEL);
            text(
                s,
                ((b'A' + i as u8) as char).to_string(),
                x + 10,
                230,
                15.,
                if b.alive[i] { color(i) } else { MUTED },
            );
            text(
                s,
                if b.alive[i] {
                    format!("{}", p.beads)
                } else {
                    "--".into()
                },
                x + 40,
                244,
                27.,
                if b.alive[i] { CREAM } else { LINE },
            );
        }
        text(
            s,
            "FILL ALL SOCKETS. SPEND EVERY PACKET.",
            395,
            275,
            11.,
            MUTED,
        );
        let (label, detail, c) = match b.result {
            ResultState::Clear if self.state.level + 1 == levels().len() => (
                "TWELVE BOARDS COMPLETE".into(),
                "R retry this board / Backspace undo".into(),
                MINT,
            ),
            ResultState::Clear => (
                "BOARD COMPLETE".into(),
                "Space / Enter for the next board".into(),
                MINT,
            ),
            ResultState::Jammed => (
                "JAMMED".into(),
                "Backspace undo / R retry this board".into(),
                ROSE,
            ),
            ResultState::EmptySockets => (
                "SOCKETS STILL EMPTY".into(),
                "Backspace undo / R retry this board".into(),
                ROSE,
            ),
            ResultState::Trapped => (
                "NO PACKET CAN LEAVE".into(),
                "Backspace undo / R retry this board".into(),
                ROSE,
            ),
            _ if b.flight.is_some() => {
                let f = b.flight.as_ref().unwrap();
                (
                    format!("ROUTING PACKET {}", (b'A' + f.packet as u8) as char),
                    format!("{} beads left / {} impacts", f.left, f.impact),
                    color(f.packet),
                )
            }
            _ if b.rejected > 0 => (
                "DIRECTION BLOCKED".into(),
                "Nothing spent. Choose another arrow.".into(),
                ROSE,
            ),
            _ => {
                let direction = ["RIGHT", "DOWN", "LEFT", "UP"][b.dir];
                (
                    format!("AIM  {}", direction),
                    if b.first_impact(l).is_some() {
                        "Ring = first impact + clockwise exit".into()
                    } else {
                        "Blocked at launch. Choose another arrow.".into()
                    },
                    CREAM,
                )
            }
        };
        text(s, label, 395, 302, 21., c);
        text(s, detail, 395, 323, 16., MUTED);
        s.rect(
            0,
            Rect::new(395, 337, 352, 32),
            if b.result == ResultState::Clear {
                MINT
            } else {
                LINE
            },
        );
        text(
            s,
            if b.result == ResultState::Clear && self.state.level + 1 == levels().len() {
                "EVERY TURN LEFT A TRACE"
            } else if b.result == ResultState::Clear {
                "SPACE  /  NEXT BOARD"
            } else if b.flight.is_some() {
                "EVERY IMPACT LEAVES ONE BEAD"
            } else {
                "SPACE  /  LAUNCH SELECTED PACKET"
            },
            407,
            358,
            16.,
            if b.result == ResultState::Clear {
                BG
            } else {
                CREAM
            },
        );
        s.rect(0, Rect::new(30, 405, 740, 1), LINE);
        text(
            s,
            "Tab  packet     Arrows  aim     Space  launch     Backspace  undo",
            30,
            426,
            18.,
            CREAM,
        );
        text(
            s,
            "H rules   R retry   Esc pause   K save / L load   M sound   F fullscreen",
            30,
            446,
            14.,
            MUTED,
        );
        if self.state.help {
            s.rect(140, Rect::new(28, 62, 744, 315), BG);
            s.rect(141, Rect::new(35, 69, 730, 301), PANEL);
            let lines = [
                ("HOW TO PEEL", 30., COPPER),
                (
                    "Fill every socket and exhaust every packet to clear a board.",
                    20.,
                    CREAM,
                ),
                (
                    "Tab selects a packet. Arrows aim. Space launches its whole route.",
                    19.,
                    CREAM,
                ),
                (
                    "Slide to a wall, edge, packet or bead. Deposit ONE bead at the stop.",
                    19.,
                    MUTED,
                ),
                (
                    "If beads remain, turn CLOCKWISE and keep sliding automatically.",
                    19.,
                    MUTED,
                ),
                (
                    "Deposits stay forever. Passing over a socket does not fill it.",
                    19.,
                    MUTED,
                ),
                (
                    "A blocked clockwise exit jams. An initially blocked aim spends nothing.",
                    19.,
                    MUTED,
                ),
                (
                    "Jam, empty sockets at exhaustion, or all packets trapped = retry.",
                    19.,
                    MUTED,
                ),
                (
                    "Backspace undoes a whole launch. R retries this board. No limits.",
                    19.,
                    MINT,
                ),
                (
                    "H closes this guide. K / L save and load, even during a route.",
                    18.,
                    CREAM,
                ),
            ];
            for (i, (line, size, c)) in lines.iter().enumerate() {
                s.text(150, *line, Point::new(52, 105 + i as i32 * 27), *size, *c);
            }
        }
    }
}

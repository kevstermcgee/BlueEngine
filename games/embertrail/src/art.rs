//! Layered original pixel presentation. This module reads, and never advances, the simulation.
use super::*;
use vesper3d::two_d::draw::*;
fn rgb(n: u32) -> Color {
    Color::new(
        ((n >> 16) & 255) as f32 / 255.,
        ((n >> 8) & 255) as f32 / 255.,
        (n & 255) as f32 / 255.,
        1.,
    )
}
fn alpha(n: u32, a: f32) -> Color {
    let mut c = rgb(n);
    c.a = a;
    c
}
const PAPER: u32 = 0xf7edcc;
const GOLDEN: u32 = 0xf3c777;
const MINT: u32 = 0xa6e7c8;
// sky, horizon, far scenery, near scenery, grass, soil, stone, accent
const PALETTES: [[u32; 8]; 8] = [
    [
        0x263f50, 0x74918b, 0x465e64, 0x345655, 0x89ba83, 0x314945, 0x53706a, 0xf3c777,
    ],
    [
        0x1c243d, 0x454862, 0x30374f, 0x242f44, 0x9a9bc6, 0x303644, 0x546079, 0x98ddd5,
    ],
    [
        0x31474d, 0x98a58f, 0x607573, 0x455e5c, 0xc2c07f, 0x435249, 0x6f8578, 0xf3af71,
    ],
    [
        0x3b526e, 0xaacac9, 0x6c879d, 0x47687b, 0xe3e9d7, 0x456777, 0x94afba, 0xb6f1eb,
    ],
    [
        0x554657, 0xe3b78b, 0xa27b77, 0x795f61, 0xe4c080, 0x65504b, 0xaf927d, 0xffd88d,
    ],
    [
        0x497286, 0xb8d6ca, 0x91b6b8, 0x6b929b, 0xcce5c3, 0x426577, 0x7da4a8, 0xf7e1a2,
    ],
    [
        0x242d44, 0x705973, 0x463f5a, 0x30374a, 0xb896a4, 0x333a43, 0x69566e, 0xf5ba9e,
    ],
    [
        0x201f32, 0x5c455a, 0x3d3449, 0x282b3b, 0x928597, 0x2e303c, 0x565165, 0xffbd75,
    ],
];
fn sprite(
    s: &mut Scene,
    layer: i32,
    id: &'static str,
    data: &'static [u8],
    x: i32,
    y: i32,
    w: f32,
    h: f32,
    color: Color,
) {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    thread_local! {static TEXTURES: RefCell<BTreeMap<&'static str,Texture2D>>=RefCell::new(BTreeMap::new());}
    TEXTURES.with(|textures| {
        let mut textures = textures.borrow_mut();
        let texture = textures.entry(id).or_insert_with(|| {
            let texture =
                Texture2D::from_file_with_format(data, Some(macroquad::prelude::ImageFormat::Png));
            texture.set_filter(macroquad::prelude::FilterMode::Nearest);
            texture
        });
        s.sprite(
            layer,
            texture,
            Transform {
                position: [x as f32, y as f32],
                ..Default::default()
            },
            [w, h],
            None,
            color,
        );
    });
}
fn label(s: &mut Scene, layer: i32, text: impl Into<String>, x: i32, y: i32, size: f32, c: u32) {
    let text = text.into();
    s.text(
        layer,
        text.clone(),
        Point::new(x + 1, y + 2),
        size,
        alpha(0x14232e, 0.8),
    );
    s.text(layer + 1, text, Point::new(x, y), size, rgb(c));
}
fn star(s: &mut Scene, layer: i32, x: i32, y: i32, size: i32, c: Color) {
    s.rect(layer, Rect::new(x - size, y - 2, size * 2 + 2, 4), c);
    s.rect(layer, Rect::new(x - 2, y - size, 4, size * 2 + 2), c);
}
fn root_gate(s: &mut Scene, x: i32, y: i32, used: bool, tick: u32) {
    s.rect(8, Rect::new(x - 4, y + 10, 56, 38), rgb(0x263d40));
    s.rect(9, Rect::new(x + 2, y + 4, 44, 44), rgb(0x56786c));
    s.rect(10, Rect::new(x + 10, y + 14, 28, 34), rgb(0x172e39));
    s.rect(10, Rect::new(x + 4, y + 2, 40, 8), rgb(0x9fc59b));
    star(
        s,
        11,
        x + 24,
        y + 25,
        6,
        if used { rgb(GOLDEN) } else { rgb(MINT) },
    );
    if tick % 120 < 85 {
        label(s, 13, "v", x + 20, y - 4, 17., MINT);
    }
}
impl Embertrail {
    fn background(&self, s: &mut Scene, p: [u32; 8]) {
        let st = &self.state;
        let cam = st.camera;
        let world = st.course / 4;
        s.rect(-100, Rect::new(0, 0, 800, 450), rgb(p[0]));
        for band in 0..16 {
            let a = band as f32 / 15.;
            let top = rgb(p[0]);
            let bottom = rgb(p[1]);
            s.rect(
                -99,
                Rect::new(0, 52 + band * 20, 800, 21),
                Color::new(
                    top.r * (1. - a) + bottom.r * a,
                    top.g * (1. - a) + bottom.g * a,
                    top.b * (1. - a) + bottom.b * a,
                    1.,
                ),
            );
        }
        let moonx = 628 - cam / 24;
        s.circle(-90, Point::new(moonx, 123), 46., alpha(p[7], 0.07));
        s.circle(-89, Point::new(moonx, 123), 30., alpha(p[7], 0.13));
        s.circle(
            -88,
            Point::new(moonx, 123),
            23.,
            rgb(if world == 3 { 0xeaf6e1 } else { 0xf5d397 }),
        );
        if world == 1 || world >= 6 {
            s.circle(-87, Point::new(moonx + 12, 116), 23., rgb(p[0]));
        }
        for n in 0..18 {
            let x = (n * 83 - cam / 16).rem_euclid(820) - 10;
            let y = 65 + (n * 31) % 95;
            if world != 4 {
                star(
                    s,
                    -86,
                    x,
                    y,
                    if n % 3 == 0 { 2 } else { 1 },
                    alpha(PAPER, 0.25),
                );
            }
        }
        if world != 1 && world != 7 {
            for n in 0..5 {
                let x = (n * 217 - cam / 8).rem_euclid(1100) - 150;
                let y = 108 + (n * 47) % 95;
                for (dx, dy, w, h) in [
                    (0, 10, 100, 12),
                    (20, 0, 44, 14),
                    (38, -8, 32, 16),
                    (64, 5, 42, 14),
                ] {
                    s.rect(-80, Rect::new(x + dx, y + dy, w, h), alpha(PAPER, 0.07));
                }
            }
        }
        // Two independently scrolling silhouettes give the courses spatial depth.
        for (layer, divisor, color, base) in [(-70, 5, p[2], 342), (-60, 3, p[3], 375)] {
            for n in -1..9 {
                let x = n * 140 - (cam / divisor).rem_euclid(140);
                let height = 90 + ((n + 20) * 47) % 95;
                if world == 4 || world == 7 {
                    for tier in 0..4 {
                        s.rect(
                            layer,
                            Rect::new(
                                x + 20 - tier * 8,
                                base - height + tier * height / 4,
                                36 + tier * 16,
                                height / 4 + 2,
                            ),
                            rgb(color),
                        );
                    }
                    if world == 7 {
                        s.rect(
                            layer,
                            Rect::new(x + 12, base - height - 10, 8, 18),
                            rgb(color),
                        );
                        s.rect(
                            layer,
                            Rect::new(x + 60, base - height - 10, 8, 18),
                            rgb(color),
                        );
                    }
                } else if world == 1 {
                    s.rect(layer, Rect::new(x + 8, 53, 38, height / 2), rgb(color));
                    s.rect(layer, Rect::new(x + 45, 53, 22, height), rgb(color));
                    s.rect(
                        layer,
                        Rect::new(x, base - height / 2, 112, height / 2 + 60),
                        rgb(color),
                    );
                } else {
                    s.rect(
                        layer,
                        Rect::new(x + 56, base - height, 18, height),
                        rgb(color),
                    );
                    for (dx, dy, w, h) in [(8, 18, 112, 40), (24, -5, 88, 42), (39, -28, 56, 32)] {
                        s.rect(
                            layer,
                            Rect::new(x + dx, base - height + dy, w, h),
                            rgb(color),
                        );
                    }
                }
            }
        }
        for n in 0..22 {
            let x = (n * 59 - cam / 2).rem_euclid(840) - 20;
            let y = 210 + (n * 47 + st.tick as i32 / 12).rem_euclid(154);
            s.rect(
                -50,
                Rect::new(x, y, 2, 2),
                alpha(p[7], if n % 2 == 0 { 0.4 } else { 0.2 }),
            );
        }
        if st.secret_inside {
            s.rect(-40, Rect::new(0, 52, 800, 398), alpha(0x13283b, 0.75));
            for n in 0..14 {
                star(
                    s,
                    -35,
                    40 + n * 57,
                    128 + (n * 23) % 190,
                    2,
                    alpha(MINT, 0.3),
                );
            }
        }
    }
    fn world_art(&self, s: &mut Scene, p: [u32; 8]) {
        let st = &self.state;
        let cam = st.camera;
        let tick = st.tick;
        for t in &st.level.tiles {
            let r = Rect::new(t.r.x - cam, t.r.y, t.r.w, t.r.h);
            if r.w == 0 || r.x > 808 || r.x + r.w < -8 {
                continue;
            }
            match t.kind {
                0 => {
                    s.rect(0, r, rgb(p[5]));
                    s.rect(1, Rect::new(r.x, r.y, r.w, 5), rgb(p[4]));
                    s.rect(1, Rect::new(r.x, r.y + 5, r.w, 7), rgb(p[6]));
                    let start = r.x.max(0) / 32 * 32;
                    for x in (start..(r.x + r.w).min(810)).step_by(32) {
                        for (dx, dy) in [(7, 20), (20, 39), (3, 62)] {
                            s.rect(1, Rect::new(x + dx, r.y + dy, 4, 3), alpha(p[6], 0.7));
                        }
                        s.rect(
                            2,
                            Rect::new(x + 9, r.y + 12, 2, 10 + (x / 32).rem_euclid(3) * 7),
                            alpha(p[4], 0.45),
                        );
                        if st.course / 4 != 7 {
                            s.rect(2, Rect::new(x + 6, r.y - 4, 2, 4), rgb(p[4]));
                            s.rect(2, Rect::new(x + 10, r.y - 7, 2, 7), rgb(p[4]));
                        }
                    }
                    s.rect(1, Rect::new(r.x, r.y + 14, 3, r.h - 14), alpha(p[4], 0.25));
                }
                1 => {
                    s.rect(3, r, rgb(p[5]));
                    s.rect(4, Rect::new(r.x, r.y, r.w, 4), rgb(p[4]));
                    s.rect(4, Rect::new(r.x, r.y + 4, r.w, 4), rgb(p[6]));
                    for x in (r.x.max(-32)..(r.x + r.w).min(820)).step_by(32) {
                        s.rect(4, Rect::new(x + 4, r.y + 10, 24, 2), alpha(PAPER, 0.13));
                        if r.h > 32 {
                            s.rect(
                                4,
                                Rect::new(x + 30, r.y + 14, 2, r.h - 14),
                                alpha(p[6], 0.45),
                            );
                        }
                    }
                }
                2 | 3 | 4 => {
                    let base = if t.kind == 3 {
                        0xd4a55f
                    } else if t.kind == 2 {
                        0x9e6e63
                    } else {
                        0x706b61
                    };
                    s.rect(5, r, rgb(0x29393d));
                    s.rect(6, Rect::new(r.x + 2, r.y + 2, r.w - 4, r.h - 4), rgb(base));
                    s.rect(
                        7,
                        Rect::new(r.x + 3, r.y + 3, r.w - 6, 3),
                        alpha(PAPER, 0.45),
                    );
                    s.rect(
                        7,
                        Rect::new(r.x + 3, r.y + r.h - 5, r.w - 6, 3),
                        alpha(0x132b36, 0.35),
                    );
                    if t.kind == 3 {
                        star(
                            s,
                            8,
                            r.x + 16,
                            r.y + 16,
                            6,
                            rgb(if tick % 40 < 20 { PAPER } else { GOLDEN }),
                        );
                    } else if t.kind == 2 {
                        s.rect(7, Rect::new(r.x + 2, r.y + 15, 28, 2), rgb(0x714f50));
                        s.rect(7, Rect::new(r.x + 16, r.y + 2, 2, 13), rgb(0x714f50));
                    } else {
                        s.rect(8, Rect::new(r.x + 13, r.y + 13, 6, 6), rgb(0x454a49));
                    }
                }
                _ => (),
            }
        }
        for plat in &st.level.platforms {
            let r = Rect::new(plat.r.x - cam, plat.r.y, plat.r.w, plat.r.h);
            s.rect(6, r, rgb(p[5]));
            s.rect(7, Rect::new(r.x, r.y, r.w, 4), rgb(p[7]));
            for n in 0..4 {
                s.rect(7, Rect::new(r.x + n * 20 + 2, r.y + 6, 16, 3), rgb(p[6]));
            }
            if plat.vertical {
                s.rect(
                    -10,
                    Rect::new(plat.home.x - cam + 38, 100, 3, plat.home.y - 100),
                    alpha(p[6], 0.4),
                );
            }
        }
        for h in &st.level.hazards {
            let x = h.x - cam;
            for n in 0..h.w / 8 {
                s.rect(8, Rect::new(x + n * 8, h.y + 6, 8, 6), rgb(0x784b62));
                s.rect(9, Rect::new(x + n * 8 + 2, h.y, 4, 12), rgb(0xeaaa9e));
                s.rect(10, Rect::new(x + n * 8 + 3, h.y - 2, 2, 5), rgb(PAPER));
            }
        }
        for pickup in &st.level.pickups {
            if !pickup.active {
                continue;
            }
            let x = pickup.r.x - cam;
            let y = pickup.r.y;
            if !(-40..840).contains(&x) {
                continue;
            }
            let bob = if (tick / 14 + pickup.r.x as u32 / 16) % 2 == 0 {
                0
            } else {
                2
            };
            if pickup.kind == 0 {
                let width = if tick / 8 % 4 == 0 { 4 } else { 10 };
                s.rect(
                    10,
                    Rect::new(x + 8 - width / 2, y + bob + 3, width, 14),
                    rgb(0xa77b4a),
                );
                s.rect(
                    11,
                    Rect::new(x + 9 - width / 2, y + bob + 2, width - 2, 12),
                    rgb(GOLDEN),
                );
                s.rect(12, Rect::new(x + 7, y + bob + 4, 2, 6), rgb(PAPER));
            } else if pickup.kind == 5 {
                s.circle(9, Point::new(x + 8, y + 8 + bob), 15., alpha(MINT, 0.1));
                star(s, 11, x + 8, y + 8 + bob, 9, rgb(MINT));
                star(s, 12, x + 8, y + 8 + bob, 4, rgb(PAPER));
            } else {
                let c = match pickup.kind {
                    1 => MINT,
                    2 => 0xf4a76d,
                    3 => GOLDEN,
                    _ => 0xe4a4b1,
                };
                s.circle(9, Point::new(x + 12, y + 12 + bob), 18., alpha(c, 0.15));
                s.rect(10, Rect::new(x + 4, y + 6 + bob, 16, 17), rgb(0x263a42));
                s.rect(11, Rect::new(x + 6, y + 8 + bob, 12, 13), rgb(c));
                s.rect(12, Rect::new(x + 8, y + 2 + bob, 8, 6), rgb(c));
                star(s, 13, x + 12, y + 14 + bob, 4, rgb(PAPER));
            }
        }
        for e in &st.level.enemies {
            if !e.active {
                continue;
            }
            let x = e.r.x - cam;
            let y = e.r.y;
            if !(-80..880).contains(&x) {
                continue;
            }
            let c = if e.immunity > 0 && tick % 8 < 4 {
                rgb(0xffd49f)
            } else {
                WHITE
            };
            match e.kind {
                0 => sprite(
                    s,
                    15,
                    "mite",
                    include_bytes!("../assets/sprites/mite.png"),
                    x - 2,
                    y + if tick / 12 % 2 == 0 { 0 } else { -1 },
                    32.,
                    26.,
                    c,
                ),
                1 => {
                    if e.shell {
                        sprite(
                            s,
                            15,
                            "shell",
                            include_bytes!("../assets/sprites/shell.png"),
                            x - 2,
                            y - 12,
                            32.,
                            26.,
                            c,
                        )
                    } else {
                        sprite(
                            s,
                            15,
                            "beetle",
                            include_bytes!("../assets/sprites/beetle.png"),
                            x - 2,
                            y,
                            32.,
                            26.,
                            c,
                        )
                    }
                }
                2 => sprite(
                    s,
                    15,
                    "wisp",
                    include_bytes!("../assets/sprites/wisp.png"),
                    x - 2,
                    y,
                    32.,
                    26.,
                    c,
                ),
                3 => sprite(
                    s,
                    15,
                    "thorn",
                    include_bytes!("../assets/sprites/thorn.png"),
                    x - 2,
                    y,
                    32.,
                    56.,
                    c,
                ),
                4 => {
                    sprite(
                        s,
                        15,
                        "keeper",
                        include_bytes!("../assets/sprites/keeper.png"),
                        x - 6,
                        y - 6,
                        72.,
                        64.,
                        c,
                    );
                    for n in 0..e.hp {
                        s.rect(
                            16,
                            Rect::new(x + 4 + n as i32 * 9, y - 16, 6, 4),
                            rgb(GOLDEN),
                        );
                    }
                    label(
                        s,
                        30,
                        "HOLLOW KEEPER",
                        (x - 65).clamp(10, 640),
                        115,
                        16.,
                        GOLDEN,
                    );
                }
                _ => (),
            }
        }
        root_gate(s, st.level.secret.x - cam, 320, st.level.secret_used, tick);
        if st.secret_inside {
            root_gate(s, st.level.secret_return.x - cam, 320, true, tick);
        }
        let cp = st.level.checkpoint - cam;
        if (-60..860).contains(&cp) {
            s.rect(7, Rect::new(cp + 3, 274, 5, 94), rgb(p[6]));
            s.rect(8, Rect::new(cp - 5, 272, 22, 25), rgb(0x263b41));
            s.rect(
                9,
                Rect::new(cp - 2, 276, 16, 16),
                rgb(if st.checkpoint { GOLDEN } else { 0x7f9790 }),
            );
            if st.checkpoint {
                s.circle(6, Point::new(cp + 6, 284), 25., alpha(GOLDEN, 0.1));
            }
        }
        let gate = st.level.exit.x - cam;
        if gate < 850 {
            let locked = st.level.enemies.iter().any(|e| e.active && e.kind == 4);
            s.rect(10, Rect::new(gate - 8, 264, 64, 104), rgb(0x294447));
            s.rect(11, Rect::new(gate - 4, 260, 56, 12), rgb(p[4]));
            s.rect(11, Rect::new(gate + 3, 274, 42, 94), rgb(0x192f3a));
            s.rect(
                12,
                Rect::new(gate + 10, 279, 28, 89),
                alpha(if locked { 0x7f5e77 } else { MINT }, 0.25),
            );
            star(
                s,
                13,
                gate + 24,
                302,
                12,
                rgb(if locked { 0xa18794 } else { GOLDEN }),
            );
            label(
                s,
                20,
                if locked { "SEALED" } else { "DAWN GATE" },
                gate - 22,
                250,
                14.,
                PAPER,
            );
        }
        // First course signposts teach by placing instructions beside the relevant obstacle.
        if st.course == 0 && !st.secret_inside {
            for (x, y, text) in [
                (52, 294, "ARROWS: MOVE"),
                (338, 211, "SPACE / UP: JUMP"),
                (900, 202, "SHIFT: RUN"),
                (st.level.secret.x - 58, 308, "DOWN: SECRET"),
            ] {
                let sx = x - cam;
                if (-180..800).contains(&sx) {
                    label(s, 20, text, sx, y, 14., PAPER);
                }
            }
        }
    }
    fn actor_art(&self, s: &mut Scene) {
        let st = &self.state;
        let r = self.player();
        let x = r.x - st.camera;
        let y = r.y;
        for shot in &st.shots {
            let sx = shot.r.x - st.camera;
            let c = if shot.hostile { 0xe9949e } else { GOLDEN };
            s.circle(18, Point::new(sx + 5, shot.r.y + 5), 9., alpha(c, 0.18));
            star(s, 19, sx + 5, shot.r.y + 5, 4, rgb(c));
        }
        if st.dead > 0 {
            star(
                s,
                25,
                x + 11,
                y + 8 - (90 - st.dead) as i32 / 2,
                8,
                rgb(MINT),
            );
            return;
        }
        if st.shield > 0 && st.tick % 8 < 3 {
            return;
        }
        let frame = if !st.grounded {
            3
        } else if st.vx.unsigned_abs() > 100 {
            (st.tick / 7 % 3) as usize
        } else {
            0
        };
        let (id, data): (&'static str, &'static [u8]) = match (st.facing < 0, frame) {
            (false, 0) => ("mote0", include_bytes!("../assets/sprites/mote0.png")),
            (false, 1) => ("mote1", include_bytes!("../assets/sprites/mote1.png")),
            (false, 2) => ("mote2", include_bytes!("../assets/sprites/mote2.png")),
            (false, _) => ("mote3", include_bytes!("../assets/sprites/mote3.png")),
            (true, 0) => (
                "mote_left0",
                include_bytes!("../assets/sprites/mote_left0.png"),
            ),
            (true, 1) => (
                "mote_left1",
                include_bytes!("../assets/sprites/mote_left1.png"),
            ),
            (true, 2) => (
                "mote_left2",
                include_bytes!("../assets/sprites/mote_left2.png"),
            ),
            (true, _) => (
                "mote_left3",
                include_bytes!("../assets/sprites/mote_left3.png"),
            ),
        };
        s.circle(
            20,
            Point::new(x + 10, y + 19),
            23.,
            alpha(if st.form == 2 { 0xf9b075 } else { MINT }, 0.06),
        );
        let color = if st.invincible > 0 {
            rgb([0xffd189, MINT, 0xe3b1d3][(st.tick / 6 % 3) as usize])
        } else {
            WHITE
        };
        sprite(
            s,
            22,
            id,
            data,
            x - 5,
            y - 10,
            32.,
            if st.crouching { 28. } else { 40. },
            color,
        );
        if st.form > 0 {
            s.rect(
                23,
                Rect::new(x + 2, y + if st.crouching { 6 } else { 14 }, 16, 8),
                rgb(if st.form == 2 { 0xf4a76d } else { 0xb6edc9 }),
            );
            star(
                s,
                24,
                x + 10,
                y + if st.crouching { 10 } else { 18 },
                3,
                rgb(PAPER),
            );
        }
        if st.puff > 0 {
            for n in 0..5 {
                star(
                    s,
                    25,
                    x - 6 + n * 9,
                    y + 10 - (n % 2) * 8,
                    2,
                    alpha(GOLDEN, st.puff as f32 / 18.),
                );
            }
        }
    }
    fn hud(&self, s: &mut Scene) {
        let st = &self.state;
        let world = st.course / 4;
        s.rect(60, Rect::new(0, 0, 800, 53), rgb(0x182d38));
        s.rect(61, Rect::new(0, 51, 800, 2), rgb(0x526b69));
        label(s, 65, "EMBERTRAIL", 18, 26, 23., PAPER);
        label(
            s,
            65,
            format!(
                "{}-{}  {}",
                world + 1,
                st.course % 4 + 1,
                levels::WORLDS[world]
            ),
            20,
            44,
            12.,
            MINT,
        );
        label(s, 65, "SPARKS", 260, 19, 11., 0x8ba69f);
        label(s, 65, format!("{:03}", st.sparks), 275, 42, 21., GOLDEN);
        label(s, 65, "LIGHTS", 355, 19, 11., 0x8ba69f);
        label(s, 65, format!("{:02}", st.lives), 369, 42, 21., PAPER);
        label(s, 65, "SHARDS", 446, 19, 11., 0x8ba69f);
        for n in 0..3 {
            star(
                s,
                66,
                458 + n * 18,
                34,
                5,
                rgb(if n < (st.relics as i32) {
                    MINT
                } else {
                    0x52676c
                }),
            );
        }
        label(s, 65, "TIME", 537, 19, 11., 0x8ba69f);
        label(s, 65, format!("{}", st.timer / 60), 541, 42, 21., PAPER);
        label(s, 65, "SCORE", 616, 19, 11., 0x8ba69f);
        label(s, 65, format!("{:06}", st.score), 610, 42, 17., GOLDEN);
        s.rect(65, Rect::new(708, 10, 80, 32), rgb(0x345451));
        label(s, 66, "ATLAS", 724, 31, 15., MINT);
        s.rect(60, Rect::new(0, 419, 800, 31), rgb(0x182d38));
        s.rect(61, Rect::new(0, 419, 800, 1), rgb(0x526b69));
        label(
            s,
            65,
            "MOVE  < >    JUMP  SPACE/UP    RUN  SHIFT    CAST  DOWN+SPACE",
            17,
            440,
            12.,
            0xb9ccbe,
        );
        label(s, 65, "ESC PAUSE", 697, 440, 11., 0x8ba69f);
        if st.banner_ticks > 0 && st.transition == 0 {
            s.rect(50, Rect::new(16, 64, 768, 28), alpha(0x142a36, 0.85));
            label(s, 51, &st.banner, 28, 83, 14., PAPER);
        }
        let progress = if st.secret_inside {
            0
        } else {
            (self.player().x * 800 / st.level.width).clamp(0, 800)
        };
        s.rect(63, Rect::new(0, 416, progress, 3), rgb(MINT));
        if st.transition > 0 {
            s.rect(75, Rect::new(0, 52, 800, 367), alpha(0x142936, 0.65));
            label(s, 76, "A LITTLE BRIGHTER", 190, 189, 34., GOLDEN);
            label(
                s,
                76,
                format!("{}-{}  COURSE CLEAR", world + 1, st.course % 4 + 1),
                271,
                224,
                20.,
                PAPER,
            );
            label(
                s,
                76,
                format!("MOON SHARDS  {}/3", st.relics),
                285,
                255,
                18.,
                MINT,
            );
        }
        if st.atlas {
            self.atlas_art(s);
        }
    }
    fn atlas_art(&self, s: &mut Scene) {
        let st = &self.state;
        s.rect(80, Rect::new(0, 52, 800, 367), rgb(0x182d38));
        label(s, 81, "THE LANTERN ISLES", 28, 76, 21., PAPER);
        for world in 0..8 {
            let y = 86 + world * 35;
            label(
                s,
                81,
                format!("{:02}  {}", world + 1, levels::WORLDS[world as usize]),
                28,
                y + 18,
                12.,
                PALETTES[world as usize][7],
            );
            for course in 0..4 {
                let n = world as usize * 4 + course;
                let x = 269 + course as i32 * 112;
                let locked = n > st.unlocked;
                let cleared = st.cleared & (1 << n) != 0;
                s.rect(
                    81,
                    Rect::new(x, y, 102, 27),
                    rgb(if locked {
                        0x253840
                    } else if n == st.course {
                        0x587d6a
                    } else {
                        0x34534e
                    }),
                );
                label(
                    s,
                    82,
                    if locked {
                        "--".into()
                    } else {
                        format!("{}-{}", world + 1, course + 1)
                    },
                    x + 9,
                    y + 18,
                    14.,
                    if locked { 0x627674 } else { PAPER },
                );
                for shard in 0..st.best_relics[n] {
                    star(s, 83, x + 57 + shard as i32 * 12, y + 13, 3, rgb(MINT));
                }
                if cleared {
                    s.rect(83, Rect::new(x + 97, y + 2, 3, 23), rgb(GOLDEN));
                }
            }
        }
        label(
            s,
            81,
            "CLICK AN UNLOCKED COURSE TO REPLAY  |  ATLAS TO RETURN",
            28,
            401,
            13.,
            MINT,
        );
    }
}
impl draw::Game for Embertrail {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        if self.state.won {
            "Dawn returns! Thank you, Mote.".into()
        } else if self.state.lives == 0 {
            "Restart keeps your discoveries.".into()
        } else {
            "Space jumps. Hold Up for height.".into()
        }
    }
    fn draw(&self, s: &mut Scene) {
        let p = PALETTES[self.state.course / 4];
        self.background(s, p);
        self.world_art(s, p);
        self.actor_art(s);
        self.hud(s);
    }
}

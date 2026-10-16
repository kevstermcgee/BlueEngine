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
thread_local! {
    static UI_FONT: std::cell::RefCell<Option<macroquad::prelude::Font>> = const { std::cell::RefCell::new(None) };
    static SPRITE_MISSES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static SPRITE_EVENTS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
    static SPRITE_TEXTURES: std::cell::RefCell<std::collections::BTreeMap<&'static str,Texture2D>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}
fn text_metrics(value: &str, size: f32) -> (i32, i32, i32) {
    use macroquad::prelude::*;
    let scale = (screen_width() / 800.)
        .min(screen_height() / 450.)
        .max(0.01);
    let pixels = (size * scale).round().clamp(1., u16::MAX as f32) as u16;
    UI_FONT.with(|slot| {
        let mut font = slot.borrow_mut();
        let font = font.get_or_insert_with(|| {
            load_ttf_font_from_bytes(include_bytes!("../assets/fonts/DejaVuSans.ttf"))
                .expect("The bundled interface font is valid")
        });
        let measured = measure_text(value, Some(font), pixels, 1.);
        (
            (measured.width / scale).ceil() as i32,
            (measured.height / scale).ceil() as i32,
            (measured.offset_y / scale).ceil() as i32,
        )
    })
}
fn text_lines(value: &str, width: i32, size: f32) -> Vec<String> {
    let mut result = Vec::new();
    for paragraph in value.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.into()
            } else {
                format!("{line} {word}")
            };
            if text_metrics(&candidate, size).0 <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                result.push(std::mem::take(&mut line));
            }
            if text_metrics(word, size).0 <= width {
                line = word.to_owned();
                continue;
            }
            // Filenames and error paths may contain no spaces. Split them by
            // measured glyphs rather than letting a single word escape its box.
            for ch in word.chars() {
                let mut candidate = line.clone();
                candidate.push(ch);
                if !line.is_empty() && text_metrics(&candidate, size).0 > width {
                    result.push(std::mem::take(&mut line));
                }
                line.push(ch);
            }
        }
        result.push(line);
    }
    result
}
#[allow(clippy::too_many_arguments)]
fn text_box(
    s: &mut Scene,
    layer: i32,
    value: &str,
    bounds: Rect,
    size: f32,
    line_height: i32,
    color: Color,
    centered: bool,
) {
    if bounds.w <= 3 || bounds.h < line_height {
        return;
    }
    let mut lines = text_lines(value, bounds.w, size);
    let max_lines = (bounds.h / line_height) as usize;
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        let last = lines.last_mut().unwrap();
        while !last.is_empty() && text_metrics(&format!("{last}…"), size).0 > bounds.w {
            last.pop();
        }
        last.push('…');
    }
    let (_, glyph_height, ascent) = text_metrics("Ag", size);
    let baseline = bounds.y + ascent + (line_height - glyph_height).max(0) / 2;
    for (row, line) in lines.iter().enumerate() {
        let width = text_metrics(line, size).0;
        let x = bounds.x + if centered { (bounds.w - width) / 2 } else { 0 };
        let y = baseline + row as i32 * line_height;
        s.text(
            layer,
            line,
            Point::new(x + 1, y + 1),
            size,
            alpha(0x061b25, 0.9),
        );
        s.text(layer + 1, line, Point::new(x, y), size, color);
    }
}
// Stable cell hashes keep scenery attached to its place in the world. Camera
// movement only changes projection; it never chooses a new tree or rock shape.
fn cell_seed(cell: i32, salt: u32) -> u32 {
    let mut n = (cell as u32).wrapping_mul(0x45d9f3b) ^ salt.wrapping_mul(0x9e3779b9);
    n ^= n >> 16;
    n = n.wrapping_mul(0x45d9f3b);
    n ^ (n >> 16)
}
// sky, horizon, far scenery, near scenery, grass, soil, stone, accent
const PALETTES: [[u32; 8]; 8] = [
    [
        0x203d50, 0xa6b5a0, 0x587c7b, 0x365d5b, 0xa3cc8d, 0x304c48, 0x648475, 0xf8ce85,
    ],
    [
        0x171f37, 0x585b81, 0x3e4768, 0x2b3554, 0xafa5d5, 0x303449, 0x6d7695, 0xb4ede0,
    ],
    [
        0x344c53, 0xc1ba8c, 0x7b8875, 0x4c6c61, 0xd3c887, 0x455548, 0x87937a, 0xffbb7c,
    ],
    [
        0x354e70, 0xb7dcd8, 0x789cad, 0x4f7a8c, 0xf1f1db, 0x426576, 0x9fbcc4, 0xc4fff2,
    ],
    [
        0x695068, 0xf4c48c, 0xb18a83, 0x865f68, 0xf5d08c, 0x67504e, 0xb79881, 0xffe2a0,
    ],
    [
        0x437994, 0xc6e6d7, 0x9cbdc2, 0x6e9eac, 0xe0eec4, 0x426678, 0x8eb5b5, 0xffe6a8,
    ],
    [
        0x262f4d, 0x99758e, 0x655473, 0x3e435c, 0xd3a7b3, 0x373d4b, 0x826780, 0xffc9aa,
    ],
    [
        0x22233b, 0x866178, 0x574760, 0x35334c, 0xb9a0b1, 0x343340, 0x74687c, 0xffbd7f,
    ],
];
#[allow(clippy::too_many_arguments)] // Sprite placement keeps the renderer's explicit layer/geometry/tint parameters.
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
    SPRITE_TEXTURES.with(|textures| {
        let mut textures = textures.borrow_mut();
        let texture = textures.entry(id).or_insert_with(|| {
            SPRITE_MISSES.with(|count| count.set(count.get() + 1));
            SPRITE_EVENTS.with(|events| events.borrow_mut().push(id));
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
    if !(-80..880).contains(&x) {
        return;
    }
    // An open hollow root, with a real stair perspective descending into darkness.
    // Its silhouette and disappearing steps carry the interaction, without a sign.
    s.circle(7, Point::new(x + 24, y + 26), 31., alpha(MINT, 0.045));
    s.circle(8, Point::new(x + 24, y + 21), 27., rgb(0x1c3238));
    s.circle(9, Point::new(x + 24, y + 21), 24., rgb(0x648578));
    s.circle(10, Point::new(x + 24, y + 23), 18., rgb(0x142b36));
    s.rect(8, Rect::new(x - 5, y + 26, 58, 22), rgb(0x294c46));
    s.rect(10, Rect::new(x + 6, y + 22, 36, 26), rgb(0x142b36));
    s.rect(11, Rect::new(x + 7, y + 40, 34, 8), rgb(0x799489));
    s.rect(11, Rect::new(x + 11, y + 34, 26, 5), rgb(0x617b76));
    s.rect(11, Rect::new(x + 15, y + 29, 18, 4), rgb(0x45615f));
    s.rect(11, Rect::new(x + 18, y + 25, 12, 3), rgb(0x304b50));
    for (dx, dy) in [(-6, 30), (-1, 18), (45, 20), (49, 34)] {
        s.rect(11, Rect::new(x + dx, y + dy, 3, 18 - dy / 3), rgb(0x8dad8b));
    }
    s.rect(12, Rect::new(x + 4, y - 1, 15, 3), rgb(0xa6c698));
    s.rect(12, Rect::new(x + 32, y + 1, 10, 3), rgb(0x8daf8c));
    let c = if used { GOLDEN } else { MINT };
    for dx in [0, 47] {
        s.rect(12, Rect::new(x + dx, y + 29, 3, 9), rgb(0xb1a575));
        s.circle(11, Point::new(x + dx + 1, y + 27), 8., alpha(c, 0.13));
        s.rect(
            13,
            Rect::new(x + dx - 1, y + 22 + (tick / 25 % 2) as i32, 5, 6),
            rgb(c),
        );
    }
}
fn home(s: &mut Scene, x: i32, y: i32, restored: bool) {
    // A hollow old lantern tree: the missing Dawnheart leaves its circular socket cold.
    let glow = if restored { GOLDEN } else { 0x9fc8cd };
    s.circle(
        -21,
        Point::new(x + 57, y + 63),
        81.,
        alpha(glow, if restored { 0.08 } else { 0.025 }),
    );
    s.rect(-20, Rect::new(x + 13, y + 26, 88, 107), rgb(0x203e43));
    s.rect(-19, Rect::new(x + 20, y + 27, 75, 102), rgb(0x506b5b));
    for (dx, h) in [(7, 30), (23, 39), (79, 36), (96, 24)] {
        s.rect(
            -20,
            Rect::new(x + dx, y + 124 - h, 9, h + 10),
            rgb(0x294846),
        );
    }
    for (dx, dy, w, h) in [(5, 20, 108, 15), (13, 9, 89, 15), (29, 1, 58, 14)] {
        s.rect(
            -18,
            Rect::new(x + dx, y + dy, w, h),
            rgb(if restored { 0x84ad77 } else { 0x73968a }),
        );
        s.rect(
            -17,
            Rect::new(x + dx + 4, y + dy, w - 9, 3),
            rgb(if restored { 0xb1cf8e } else { 0xabc3b0 }),
        );
    }
    for dx in [26, 40, 79, 88] {
        s.rect(
            -18,
            Rect::new(x + dx, y + 39, 2, 78 - dx / 4),
            alpha(0xc0bb8b, 0.24),
        );
    }
    s.circle(-17, Point::new(x + 58, y + 57), 19., rgb(0x283f42));
    s.circle(
        -16,
        Point::new(x + 58, y + 57),
        15.,
        rgb(if restored { 0xc99560 } else { 0x416371 }),
    );
    s.circle(-15, Point::new(x + 58, y + 57), 11., rgb(0x203941));
    if restored {
        star(s, -14, x + 58, y + 57, 9, rgb(GOLDEN));
        star(s, -13, x + 58, y + 57, 4, rgb(PAPER));
    } else {
        for (dx, dy) in [(-7, -5), (3, -1), (-2, 7)] {
            s.rect(
                -14,
                Rect::new(x + 58 + dx, y + 57 + dy, 4, 2),
                rgb(0x7ca0a0),
            );
        }
    }
    s.circle(-17, Point::new(x + 58, y + 98), 18., rgb(0x243c40));
    s.rect(-17, Rect::new(x + 40, y + 97, 36, 36), rgb(0x243c40));
    s.rect(
        -16,
        Rect::new(x + 45, y + 100, 26, 33),
        rgb(if restored { 0x725c45 } else { 0x345057 }),
    );
    s.rect(-15, Rect::new(x + 47, y + 104, 2, 27), alpha(PAPER, 0.15));
    s.rect(-15, Rect::new(x + 64, y + 113, 3, 3), rgb(GOLDEN));
    s.rect(-14, Rect::new(x + 34, y + 130, 49, 4), rgb(0x91a78a));
    for dx in [22, 86] {
        s.rect(-15, Rect::new(x + dx, y + 83, 7, 12), rgb(glow));
        s.rect(
            -14,
            Rect::new(x + dx + 2, y + 85, 3, 7),
            rgb(if restored { PAPER } else { 0xacc6c9 }),
        );
    }
}
impl Embertrail {
    pub fn release_presentation_assets() {
        // GPU handles must be released while Macroquad's context still exists,
        // rather than from thread-local destructors after the native window ends.
        SPRITE_TEXTURES.with(|textures| textures.borrow_mut().clear());
        UI_FONT.with(|font| *font.borrow_mut() = None);
    }
    pub fn presentation_asset_misses() -> u32 {
        SPRITE_MISSES.with(|count| count.get())
    }
    pub fn take_presentation_asset_events() -> Vec<&'static str> {
        SPRITE_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
    }
    pub fn prewarm_presentation_assets() {
        let assets: &[(&str, &[u8])] = &[
            ("mote0", include_bytes!("../assets/sprites/mote0.png")),
            ("mote1", include_bytes!("../assets/sprites/mote1.png")),
            ("mote2", include_bytes!("../assets/sprites/mote2.png")),
            ("mote3", include_bytes!("../assets/sprites/mote3.png")),
            (
                "mote_left0",
                include_bytes!("../assets/sprites/mote_left0.png"),
            ),
            (
                "mote_left1",
                include_bytes!("../assets/sprites/mote_left1.png"),
            ),
            (
                "mote_left2",
                include_bytes!("../assets/sprites/mote_left2.png"),
            ),
            (
                "mote_left3",
                include_bytes!("../assets/sprites/mote_left3.png"),
            ),
            ("mite", include_bytes!("../assets/sprites/mite.png")),
            ("beetle", include_bytes!("../assets/sprites/beetle.png")),
            ("shell", include_bytes!("../assets/sprites/shell.png")),
            ("wisp", include_bytes!("../assets/sprites/wisp.png")),
            ("thorn", include_bytes!("../assets/sprites/thorn.png")),
            ("villager", include_bytes!("../assets/sprites/villager.png")),
            ("dewmane", include_bytes!("../assets/sprites/boss0.png")),
            ("chimeback", include_bytes!("../assets/sprites/boss1.png")),
            ("brack", include_bytes!("../assets/sprites/boss2.png")),
            ("rimehart", include_bytes!("../assets/sprites/boss3.png")),
            ("solarch", include_bytes!("../assets/sprites/boss4.png")),
            ("tempest", include_bytes!("../assets/sprites/boss5.png")),
            (
                "briar_matron",
                include_bytes!("../assets/sprites/boss6.png"),
            ),
            ("vesper", include_bytes!("../assets/sprites/boss7.png")),
        ];
        let mut scratch = Scene::default();
        for &(id, data) in assets {
            sprite(&mut scratch, -1000, id, data, -1000, -1000, 1., 1., WHITE);
        }
        let glyphs = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~…—×";
        for size in [16., 17., 18., 20., 21., 22., 24., 28., 30., 32., 48.] {
            text_metrics(glyphs, size);
        }
    }
    pub fn set_ui_font(font: macroquad::prelude::Font) {
        UI_FONT.with(|slot| *slot.borrow_mut() = Some(font));
    }
    pub fn ui_text_width(value: &str, size: f32) -> i32 {
        text_metrics(value, size).0
    }
    pub fn ui_text_lines(value: &str, width: i32, size: f32) -> Vec<String> {
        text_lines(value, width, size)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw_ui_text(
        s: &mut Scene,
        layer: i32,
        value: &str,
        bounds: Rect,
        size: f32,
        line_height: i32,
        color: Color,
        centered: bool,
    ) {
        text_box(s, layer, value, bounds, size, line_height, color, centered);
    }
    /// A living view of the village for the title. Wall time only animates
    /// presentation; opening a menu never advances rules, timers or story.
    pub fn draw_title_backdrop(&self, s: &mut Scene, seconds: f32) {
        let seconds = if seconds.is_finite() {
            seconds.max(0.)
        } else {
            0.
        };
        let p = PALETTES[0];
        let top = rgb(0x122b41);
        let horizon = rgb(0x778e87);
        s.rect(-100, Rect::new(0, 0, 800, 450), top);
        for band in 0..32 {
            let a = band as f32 / 31.;
            s.rect(
                -99,
                Rect::new(0, 46 + band * 11, 800, 12),
                Color::new(
                    top.r * (1. - a) + horizon.r * a,
                    top.g * (1. - a) + horizon.g * a,
                    top.b * (1. - a) + horizon.b * a,
                    1.,
                ),
            );
        }
        for (radius, a) in [(75., 0.025), (51., 0.05), (35., 0.08)] {
            s.circle(-96, Point::new(688, 85), radius, alpha(GOLDEN, a));
        }
        s.circle(-95, Point::new(688, 85), 25., rgb(0xffdd9c));
        s.circle(-94, Point::new(700, 76), 24., rgb(0x18334a));
        for n in 0..22 {
            let seed = cell_seed(n, 151);
            let a = 0.15 + ((seconds * 0.7 + n as f32).sin() + 1.) * 0.1;
            star(
                s,
                -92,
                (seed % 800) as i32,
                25 + (seed / 800 % 165) as i32,
                1,
                alpha(PAPER, a),
            );
        }
        for n in -1i32..4 {
            let x = ((n as f32 * 278. + seconds * 3.).rem_euclid(1112.) - 160.) as i32;
            let y = 127 + n.rem_euclid(3) * 19;
            for (dx, dy, w, h) in [(0, 8, 132, 9), (21, 0, 80, 11), (41, -7, 40, 11)] {
                s.rect(-85, Rect::new(x + dx, y + dy, w, h), alpha(PAPER, 0.065));
            }
        }
        for (layer, spacing, base, color) in
            [(-70i32, 163, 366, 0x365b60), (-60, 207, 384, 0x274b4f)]
        {
            for n in -1..6 {
                let seed = cell_seed(n, layer.unsigned_abs());
                let x = n * spacing;
                let h = 100 + (seed % 86) as i32;
                s.rect(layer, Rect::new(x + 64, base - h, 17, h), rgb(color));
                for (dx, dy, radius) in [(29, 28, 37.), (64, 7, 46.), (99, 29, 35.)] {
                    s.circle(layer, Point::new(x + dx, base - h + dy), radius, rgb(color));
                }
                s.rect(
                    layer + 1,
                    Rect::new(x + 68, base - 56, 2, 43),
                    alpha(MINT, 0.08),
                );
            }
        }
        // The empty circular socket is the visual promise behind the adventure.
        home(s, 17, 234, false);
        home(s, 671, 234, false);
        let pulse = 0.5 + 0.5 * (seconds * 1.7).sin();
        for x in [42, 696] {
            s.circle(
                -12,
                Point::new(x, 323),
                27.,
                alpha(GOLDEN, 0.035 + pulse * 0.035),
            );
            s.rect(-11, Rect::new(x - 2, 320, 5, 7), rgb(0xe6b876));
        }
        // A restrained central scrim supports the client's centered story and buttons.
        s.rect(-8, Rect::new(152, 35, 496, 299), alpha(0x102733, 0.3));
        s.rect(0, Rect::new(0, FLOOR, 800, 82), rgb(p[5]));
        s.rect(1, Rect::new(0, FLOOR, 800, 4), rgb(p[4]));
        s.rect(1, Rect::new(0, FLOOR + 4, 800, 7), rgb(p[6]));
        for n in 0..26 {
            let seed = cell_seed(n, 43);
            let x = n * 32 + 7;
            s.rect(2, Rect::new(x, FLOOR - 4, 2, 5), rgb(p[4]));
            s.rect(2, Rect::new(x + 4, FLOOR - 7, 2, 8), rgb(p[4]));
            s.rect(
                2,
                Rect::new(x + 11, FLOOR + 22 + (seed % 3) as i32 * 6, 5, 3),
                alpha(p[6], 0.45),
            );
            if n % 4 == 0 {
                s.rect(2, Rect::new(x + 12, FLOOR - 4, 1, 5), rgb(p[6]));
                s.rect(3, Rect::new(x + 10, FLOOR - 6, 5, 3), rgb(GOLDEN));
            }
        }
        let bob = (seconds * 2.1).sin().round() as i32;
        s.rect(18, Rect::new(665, FLOOR - 1, 46, 4), alpha(0x102733, 0.4));
        sprite(
            s,
            22,
            "mote_left0",
            include_bytes!("../assets/sprites/mote_left0.png"),
            655,
            292 + bob,
            64.,
            80.,
            WHITE,
        );
        s.circle(
            20,
            Point::new(687, 309 + bob),
            24.,
            alpha(MINT, 0.07 + pulse * 0.04),
        );
        star(s, 23, 689, 295 + bob, 3, alpha(PAPER, 0.5 + pulse * 0.35));
        // Floating sparks and gently swaying fireflies visibly live while the title waits.
        for n in 0..20 {
            let seed = cell_seed(n, 212);
            let x = (seed % 790) as i32 + (seconds * 0.8 + n as f32).sin().mul_add(5., 0.) as i32;
            let travel =
                (seconds * (8. + (seed % 6) as f32) + (seed / 790 % 280) as f32).rem_euclid(300.);
            let y = 402 - travel as i32;
            let a = (travel / 32.).min(1.) * ((300. - travel) / 40.).min(1.) * 0.55;
            s.circle(25, Point::new(x, y), 6., alpha(GOLDEN, a * 0.12));
            star(s, 26, x, y, 1, alpha(GOLDEN, a));
        }
    }
    fn grotto_palette(&self) -> [u32; 8] {
        let world = self.state.level.grotto.archetype as usize;
        [
            [
                0x182c32, 0x426058, 0x304841, 0x263d36, 0xbbc58c, 0x374d40, 0x718568, 0xffd28d,
            ],
            [
                0x151f36, 0x3e5368, 0x303d59, 0x243346, 0xa7cad5, 0x313d51, 0x617f96, 0xbcefee,
            ],
            [
                0x202f34, 0x5f6357, 0x3d514c, 0x2c3e3e, 0xddbc84, 0x454b43, 0x8f8c70, 0xffbd7e,
            ],
            [
                0x1c3447, 0x5d8190, 0x3c596b, 0x2f4d5b, 0xd3e8d8, 0x3a5660, 0x8caeb5, 0xffd79c,
            ],
            [
                0x352d3f, 0x936c65, 0x654950, 0x473d4c, 0xf0ce99, 0x5c4848, 0xa38a76, 0xffdf9d,
            ],
            [
                0x193442, 0x54818a, 0x355963, 0x294953, 0xc8e5d0, 0x35565e, 0x83adb1, 0xd4fff0,
            ],
            [
                0x292b3f, 0x70596d, 0x484052, 0x343949, 0xc5a7a4, 0x45414a, 0x866c7e, 0xf3b4b5,
            ],
            [
                0x20263b, 0x5d536a, 0x3c394e, 0x2d3045, 0xc5ac9e, 0x3b3b4a, 0x817789, 0xffc28a,
            ],
        ][world]
    }
    fn grotto_background(&self, s: &mut Scene, p: [u32; 8]) {
        let st = &self.state;
        let room = &st.level.grotto;
        let world = room.archetype as usize;
        let variant = u32::from(room.variant);
        let local_cam = st.camera - room.bounds.x;
        s.rect(-100, Rect::new(0, 0, 800, 450), rgb(p[0]));
        for band in 0..24 {
            let a = band as f32 / 23.;
            let top = rgb(p[0]);
            let bottom = rgb(p[1]);
            s.rect(
                -99,
                Rect::new(0, 48 + band * 17, 800, 18),
                Color::new(
                    top.r * (1. - a) + bottom.r * a,
                    top.g * (1. - a) + bottom.g * a,
                    top.b * (1. - a) + bottom.b * a,
                    1.,
                ),
            );
        }
        // These chambers have their own spatial rhythm. Identity is local world
        // geometry, so a camera pan never replaces a visible pipe, root or gear.
        let spacing = [176, 192, 204, 184, 208, 196, 168, 222][world];
        let start = (local_cam / 3).div_euclid(spacing) - 1;
        for cell in start..=start + 6 {
            let seed = cell_seed(cell, 211 + world as u32 * 37 + variant * 19);
            let x = cell * spacing - local_cam / 3;
            let depth = 95 + (seed % 90) as i32;
            match world {
                0 => {
                    // A root stepwell: branching old timber and suspended seed lanterns.
                    s.rect(-72, Rect::new(x + 40, 52, 24, 326), rgb(p[2]));
                    s.rect(-71, Rect::new(x + 44, 52, 4, 290), alpha(p[4], 0.17));
                    for (dx, y, w) in [(-16, 128, 85), (12, 219, 95), (-5, 305, 81)] {
                        s.rect(-72, Rect::new(x + dx, y, w, 12), rgb(p[2]));
                        s.rect(-71, Rect::new(x + dx + 5, y, 24, 2), alpha(p[4], 0.2));
                    }
                    s.rect(-68, Rect::new(x + 117, 58, 2, depth), rgb(p[3]));
                    s.circle(-67, Point::new(x + 118, 58 + depth), 12., rgb(0x776f4c));
                    s.circle(-66, Point::new(x + 118, 58 + depth), 7., rgb(p[7]));
                    s.circle(
                        -69,
                        Point::new(x + 118, 58 + depth),
                        31.,
                        alpha(p[7], 0.055),
                    );
                }
                1 => {
                    // The echo waterway is cut by dark arches and mineral curtains.
                    s.circle(-73, Point::new(x + 80, 192), 63., rgb(p[2]));
                    s.rect(-73, Rect::new(x + 17, 190, 126, 187), rgb(p[2]));
                    s.circle(-72, Point::new(x + 80, 192), 47., rgb(p[3]));
                    s.rect(-72, Rect::new(x + 33, 190, 94, 187), rgb(p[3]));
                    for n in 0..4 {
                        s.rect(
                            -69,
                            Rect::new(x + 40 + n * 21, 54, 8, depth / 2 + n % 2 * 24),
                            rgb(p[6]),
                        );
                        s.rect(
                            -68,
                            Rect::new(x + 42 + n * 21, 55, 2, depth / 2),
                            alpha(p[7], 0.26),
                        );
                    }
                    let drip = 90 + ((st.tick / 2 + seed) % 230) as i32;
                    s.rect(-65, Rect::new(x + 93, drip, 2, 9), alpha(p[7], 0.5));
                    s.rect(-66, Rect::new(x + 37, 326, 88, 2), alpha(p[7], 0.2));
                }
                2 => {
                    // Copper pumps: oxidized pipes, turning flywheels and small pistons.
                    for (dx, y, w, h) in [
                        (16, 112, 150, 12),
                        (24, 112, 12, 242),
                        (145, 112, 12, 142),
                        (33, 254, 122, 11),
                    ] {
                        s.rect(-72, Rect::new(x + dx, y, w, h), rgb(p[2]));
                        s.rect(
                            -71,
                            Rect::new(x + dx + 2, y + 2, (w - 4).max(2), 2),
                            alpha(p[7], 0.18),
                        );
                    }
                    let cx = x + 89;
                    let cy = 210;
                    s.circle(-69, Point::new(cx, cy), 30., rgb(p[3]));
                    s.circle(-68, Point::new(cx, cy), 25., rgb(p[6]));
                    s.circle(-67, Point::new(cx, cy), 20., rgb(p[3]));
                    for spoke in 0..6 {
                        let angle = (st.tick as f32 * 0.012
                            + spoke as f32 * std::f32::consts::TAU / 6.
                            + variant as f32)
                            .sin_cos();
                        for step in (4..23).step_by(4) {
                            s.rect(
                                -66,
                                Rect::new(
                                    cx + (angle.0 * step as f32) as i32 - 1,
                                    cy + (angle.1 * step as f32) as i32 - 1,
                                    3,
                                    3,
                                ),
                                rgb(p[6]),
                            );
                        }
                    }
                    let piston =
                        (st.tick as f32 * 0.035 + cell as f32).sin().mul_add(7., 0.) as i32;
                    s.rect(-65, Rect::new(x + 139, 286 + piston, 24, 12), rgb(p[6]));
                    s.rect(-65, Rect::new(x + 148, 265, 5, 28 + piston), rgb(p[6]));
                }
                3 => {
                    // Warm wells under the frost orchard: pale ice above, amber springs below.
                    for n in 0..4 {
                        s.rect(
                            -73,
                            Rect::new(x + n * 29, 55, 17, 37 + (seed % 31) as i32 + n * 13),
                            rgb(p[2]),
                        );
                    }
                    s.circle(-72, Point::new(x + 76, 345), 47., rgb(p[3]));
                    s.rect(-71, Rect::new(x + 26, 337, 102, 6), alpha(p[7], 0.25));
                    s.rect(-70, Rect::new(x + 43, 334, 68, 2), alpha(p[7], 0.4));
                    for n in 0..3 {
                        let rise = ((st.tick / 3 + n * 37 + seed) % 96) as i32;
                        s.circle(
                            -67,
                            Point::new(x + 43 + n as i32 * 24, 327 - rise),
                            9.,
                            alpha(PAPER, 0.1 * (1. - rise as f32 / 96.)),
                        );
                    }
                }
                4 => {
                    // Sunglass reliquaries: carved pillars and small windows catching warm light.
                    s.rect(-73, Rect::new(x + 35, 58, 26, 308), rgb(p[2]));
                    s.rect(-72, Rect::new(x + 25, 72, 47, 10), rgb(p[6]));
                    s.rect(-72, Rect::new(x + 25, 350, 47, 13), rgb(p[6]));
                    s.rect(-71, Rect::new(x + 42, 86, 3, 256), alpha(p[7], 0.16));
                    for tier in 0..5 {
                        s.rect(
                            -73,
                            Rect::new(x + 95 + tier * 5, 97 + tier * 8, 71 - tier * 10, 9),
                            rgb(p[2]),
                        );
                    }
                    s.rect(-72, Rect::new(x + 114, 122, 29, 57), rgb(p[3]));
                    star(s, -69, x + 128, 148, 11, alpha(p[7], 0.55));
                    for beam in 0..3 {
                        s.rect(
                            -70,
                            Rect::new(x + 111 + beam * 11, 174, 5, 190),
                            alpha(p[7], 0.035),
                        );
                    }
                }
                5 => {
                    // Cloud cisterns: hanging tanks, chains and water disappearing into deep gaps.
                    s.circle(-73, Point::new(x + 79, 160), 50., rgb(p[2]));
                    s.rect(-73, Rect::new(x + 29, 154, 100, 105), rgb(p[2]));
                    s.rect(-72, Rect::new(x + 35, 191, 88, 54), alpha(p[7], 0.08));
                    for dx in [26, 126] {
                        for y in (57..148).step_by(12) {
                            s.rect(-69, Rect::new(x + dx, y, 4, 8), rgb(p[6]));
                        }
                    }
                    s.rect(-70, Rect::new(x + 22, 252, 116, 8), rgb(p[6]));
                    for n in 0..4 {
                        let fall = 266 + ((st.tick * 2 + seed + n * 23) % 150) as i32;
                        s.rect(
                            -68,
                            Rect::new(x + 54 + n as i32 * 15, fall, 2, 14),
                            alpha(p[7], 0.25),
                        );
                    }
                }
                6 => {
                    // A sheltered nursery: sleeping seed pods, old roots and pink sap lights.
                    s.rect(-73, Rect::new(x + 24, 56, 9, 312), rgb(p[2]));
                    s.rect(-72, Rect::new(x + 31, 127, 88, 6), rgb(p[2]));
                    s.rect(
                        -72,
                        Rect::new(x + 113, 127, 3, 67 + (seed % 43) as i32),
                        rgb(p[2]),
                    );
                    let py = 184 + (seed % 43) as i32;
                    s.circle(-70, Point::new(x + 114, py), 22., rgb(p[3]));
                    s.circle(-69, Point::new(x + 114, py), 15., rgb(p[6]));
                    s.rect(-68, Rect::new(x + 109, py - 9, 3, 18), alpha(p[7], 0.45));
                    for n in 0..3 {
                        s.rect(
                            -71,
                            Rect::new(x + 49 + n * 13, 310 - n * 12, 4, 58 + n * 12),
                            rgb(p[2]),
                        );
                        s.circle(
                            -70,
                            Point::new(x + 51 + n * 13, 310 - n * 12),
                            7.,
                            rgb(p[6]),
                        );
                    }
                }
                _ => {
                    // The Regent's undercroft: counterweights and slow, chained clockwork.
                    let cx = x + 94;
                    let cy = 206;
                    s.circle(-73, Point::new(cx, cy), 61., rgb(p[2]));
                    s.circle(-72, Point::new(cx, cy), 48., rgb(p[3]));
                    for spoke in 0..10 {
                        let angle = (st.tick as f32 * 0.006
                            + spoke as f32 * std::f32::consts::TAU / 10.
                            + variant as f32)
                            .sin_cos();
                        for radius in (8..60).step_by(6) {
                            s.rect(
                                -71,
                                Rect::new(
                                    cx + (angle.0 * radius as f32) as i32 - 2,
                                    cy + (angle.1 * radius as f32) as i32 - 2,
                                    5,
                                    5,
                                ),
                                rgb(p[6]),
                            );
                        }
                    }
                    s.circle(-70, Point::new(cx, cy), 11., rgb(p[7]));
                    let weight_y = 150
                        + (st.tick as f32 * 0.018 + cell as f32)
                            .sin()
                            .mul_add(17., 0.) as i32;
                    for y in (57..weight_y).step_by(11) {
                        s.rect(-69, Rect::new(x + 188, y, 4, 7), rgb(p[6]));
                    }
                    s.rect(-68, Rect::new(x + 175, weight_y, 28, 47), rgb(p[2]));
                    s.rect(
                        -67,
                        Rect::new(x + 180, weight_y + 4, 4, 38),
                        alpha(p[7], 0.25),
                    );
                }
            }
        }
        // Course variants have different little skylight patterns and mote trails.
        for n in 0..15 {
            let seed = cell_seed(n, world as u32 * 19 + variant * 31 + 81);
            let x = (seed % 850) as i32 - 25 - local_cam / 2;
            let y = 82 + ((seed / 850 + st.tick / 9) % 260) as i32;
            s.rect(-45, Rect::new(x, y, 2, 2), alpha(p[7], 0.24));
        }
        let reward = room.reward;
        if !self.grotto_reward_collected() {
            let x = reward.x - st.camera + reward.w / 2;
            let y = reward.y + reward.h / 2;
            s.circle(-41, Point::new(x, y), 32., alpha(p[7], 0.08));
            s.circle(-40, Point::new(x, y), 19., alpha(p[7], 0.06));
        }
    }
    fn background(&self, s: &mut Scene, p: [u32; 8]) {
        let st = &self.state;
        if st.secret_inside {
            self.grotto_background(s, p);
            return;
        }
        let cam = if st.won { 0 } else { st.camera };
        let world = if st.won { 0 } else { st.course / 4 };
        s.rect(-100, Rect::new(0, 0, 800, 450), rgb(p[0]));
        let top = rgb(p[0]);
        let bottom = rgb(p[1]);
        for band in 0..32 {
            let a = (band as f32 / 31.).powf(1.2);
            s.rect(
                -99,
                Rect::new(0, 30 + band * 12, 800, 13),
                Color::new(
                    top.r * (1. - a) + bottom.r * a,
                    top.g * (1. - a) + bottom.g * a,
                    top.b * (1. - a) + bottom.b * a,
                    1.,
                ),
            );
        }
        let moonx = 658 - cam / 42;
        if world != 1 {
            for (radius, opacity) in [(70., 0.025), (49., 0.045), (33., 0.08)] {
                s.circle(-94, Point::new(moonx, 123), radius, alpha(p[7], opacity));
            }
            s.circle(
                -93,
                Point::new(moonx, 123),
                25.,
                rgb(if world == 3 { 0xe4ffed } else { 0xffdf9e }),
            );
            if world >= 6 {
                s.circle(-92, Point::new(moonx + 13, 114), 25., rgb(p[0]));
            }
        }
        if world == 3 {
            for n in 0..17 {
                let x = n * 61 - cam / 55;
                s.rect(
                    -92,
                    Rect::new(x, 55 + n % 4 * 8, 31, 90 + n % 3 * 13),
                    alpha(MINT, 0.045),
                );
            }
        }
        // Each layer enumerates actual world cells. Crossing a cell boundary
        // adds scenery offscreen instead of changing a visible tree's identity.
        for cell in (cam / 18 / 88 - 2)..=(cam / 18 / 88 + 11) {
            let seed = cell_seed(cell, world as u32 + 10);
            let x = cell * 88 - cam / 18;
            let y = 62 + (seed % 122) as i32;
            if world != 4 && world != 5 && world != 2 {
                star(
                    s,
                    -91,
                    x,
                    y,
                    if seed.is_multiple_of(5) { 2 } else { 1 },
                    alpha(PAPER, 0.18 + (seed % 3) as f32 * 0.07),
                );
            }
        }
        if world != 1 && world != 7 {
            for cell in (cam / 11 / 220 - 2)..=(cam / 11 / 220 + 5) {
                let seed = cell_seed(cell, 71 + world as u32);
                let x = cell * 220 - cam / 11;
                let y = 121 + (seed % 60) as i32;
                for (dx, dy, w, h) in [(0, 13, 135, 11), (22, 4, 75, 16), (39, -4, 43, 12)] {
                    s.rect(
                        -85,
                        Rect::new(x + dx, y + dy, w, h),
                        alpha(PAPER, if world == 5 { 0.19 } else { 0.07 }),
                    );
                }
            }
        }
        for (layer, divisor, spacing, color, base) in
            [(-75, 7, 170, p[2], 350), (-60, 4, 202, p[3], 385)]
        {
            let start = (cam / divisor).div_euclid(spacing) - 2;
            for cell in start..=start + 8 {
                let seed = cell_seed(cell, world as u32 + divisor as u32 * 19);
                let x = cell * spacing - cam / divisor;
                let height = 92 + (seed % 102) as i32;
                match world {
                    1 => {
                        // Echo Root: mineral curtains above, giant fungi below.
                        s.rect(layer, Rect::new(x + 28, 40, 21, height / 2), rgb(color));
                        s.rect(
                            layer,
                            Rect::new(x + 33, 40, 11, height / 2 + 31),
                            rgb(color),
                        );
                        s.rect(
                            layer,
                            Rect::new(x + 86, base - height / 2, 14, height),
                            rgb(color),
                        );
                        s.circle(
                            layer,
                            Point::new(x + 94, base - height / 2),
                            47.,
                            rgb(color),
                        );
                        s.rect(
                            layer + 1,
                            Rect::new(x + 47, base - height / 2 + 1, 94, 16),
                            rgb(color),
                        );
                        s.rect(
                            layer + 2,
                            Rect::new(x + 61, base - height / 2 + 16, 63, 2),
                            alpha(p[7], 0.13),
                        );
                    }
                    4 | 7 => {
                        // Broken sun temples become the Regent's bell towers.
                        let width = if world == 7 { 67 } else { 51 };
                        s.rect(
                            layer,
                            Rect::new(x + 35, base - height, width, height),
                            rgb(color),
                        );
                        s.rect(
                            layer,
                            Rect::new(x + 27, base - height, width + 16, 10),
                            rgb(color),
                        );
                        s.rect(
                            layer,
                            Rect::new(x + 30, base - 13, width + 10, 13),
                            rgb(color),
                        );
                        if world == 7 {
                            for k in 0..3 {
                                s.rect(
                                    layer,
                                    Rect::new(x + 36 + k * 24, base - height - 13, 12, 16),
                                    rgb(color),
                                );
                            }
                        } else {
                            // Fractured lintels and fluted columns belong to the sun temples.
                            s.rect(
                                layer,
                                Rect::new(x + 18, base - height - 6, 91, 7),
                                rgb(color),
                            );
                            s.rect(
                                layer,
                                Rect::new(x + 103, base - height + 4, 11, height - 4),
                                rgb(color),
                            );
                            s.rect(
                                layer + 1,
                                Rect::new(x + 43, base - height + 17, 4, height - 37),
                                alpha(p[1], 0.12),
                            );
                            s.rect(
                                layer + 1,
                                Rect::new(x + 70, base - height + 17, 3, height - 37),
                                alpha(p[1], 0.12),
                            );
                            s.rect(
                                layer + 1,
                                Rect::new(x + 27, base - height - 4, 26, 2),
                                alpha(p[7], 0.2),
                            );
                        }
                        s.rect(
                            layer + 1,
                            Rect::new(x + 54, base - height + 28, 11, 28),
                            alpha(p[0], 0.5),
                        );
                        if world == 7 {
                            s.circle(
                                layer,
                                Point::new(x + 68, base - height + 7),
                                29.,
                                rgb(color),
                            );
                            s.rect(
                                layer + 1,
                                Rect::new(x + 59, base - height + 4, 19, 29),
                                alpha(p[0], 0.5),
                            );
                            s.rect(
                                layer + 2,
                                Rect::new(x + 66, base - height + 9, 5, 16),
                                alpha(p[7], 0.25),
                            );
                        }
                    }
                    5 => {
                        // Floating gardens and seed windmills above a cloud sea.
                        let island_y = base - height / 2;
                        for tier in 0..6 {
                            s.rect(
                                layer,
                                Rect::new(
                                    x + 11 + tier * 10,
                                    island_y + tier * 7,
                                    126 - tier * 20,
                                    8,
                                ),
                                rgb(color),
                            );
                        }
                        for (dx, w, h) in [(38, 3, 24), (75, 4, 29), (91, 2, 16)] {
                            s.rect(layer, Rect::new(x + dx, island_y + 22, w, h), rgb(color));
                        }
                        s.circle(layer, Point::new(x + 32, island_y - 9), 12., rgb(color));
                        s.circle(layer, Point::new(x + 48, island_y - 13), 15., rgb(color));
                        s.rect(
                            layer,
                            Rect::new(x + 70, base - height, 8, height / 2),
                            rgb(color),
                        );
                        for (dx, dy, w, h) in [(-27, -4, 62, 7), (0, -30, 7, 62)] {
                            s.rect(
                                layer,
                                Rect::new(x + 70 + dx, base - height + dy, w, h),
                                rgb(color),
                            );
                        }
                        s.rect(
                            layer + 1,
                            Rect::new(x + 13, island_y - 2, 122, 4),
                            alpha(p[4], 0.3),
                        );
                    }
                    2 => {
                        for reed in 0..5 {
                            let h = height / 2 + reed * 11;
                            s.rect(layer, Rect::new(x + reed * 21, base - h, 3, h), rgb(color));
                            s.rect(
                                layer,
                                Rect::new(x - 3 + reed * 21, base - h, 9, 22),
                                rgb(color),
                            );
                        }
                        s.circle(layer, Point::new(x + 100, base - 17), 40., rgb(color));
                        s.rect(
                            layer + 1,
                            Rect::new(x + 66, base - 19, 73, 3),
                            alpha(p[7], 0.1),
                        );
                    }
                    _ => {
                        // Thick orchard trunks with asymmetric, rounded crowns.
                        s.rect(
                            layer,
                            Rect::new(x + 70, base - height + 12, 19, height),
                            rgb(color),
                        );
                        s.rect(layer, Rect::new(x + 57, base - 10, 47, 20), rgb(color));
                        s.rect(
                            layer,
                            Rect::new(x + 47, base - height + 45, 70, 9),
                            rgb(color),
                        );
                        for (dx, dy, radius) in
                            [(29, 28, 38.), (63, 6, 47.), (100, 29, 36.), (59, 38, 42.)]
                        {
                            s.circle(
                                layer,
                                Point::new(x + dx, base - height + dy),
                                radius,
                                rgb(color),
                            );
                        }
                        if world == 3 {
                            s.rect(
                                layer + 1,
                                Rect::new(x + 15, base - height + 4, 44, 3),
                                alpha(PAPER, 0.2),
                            );
                            s.rect(
                                layer + 1,
                                Rect::new(x + 66, base - height - 30, 31, 3),
                                alpha(PAPER, 0.2),
                            );
                        }
                        s.rect(
                            layer + 1,
                            Rect::new(x + 74, base - 64, 3, 45),
                            alpha(p[1], 0.08),
                        );
                    }
                }
            }
        }
        // Soft horizon fog stays in scenery space, leaving collision surfaces crisp.
        for (y, a) in [(308, 0.04), (328, 0.045), (348, 0.035)] {
            s.rect(-52, Rect::new(0, y, 800, 21), alpha(p[1], a));
        }
        for cell in (cam / 2 / 61 - 2)..=(cam / 2 / 61 + 15) {
            let seed = cell_seed(cell, world as u32 + 161);
            let x = cell * 61 - cam / 2 + (seed % 31) as i32;
            let y = 115 + ((seed / 31 + st.tick / if world == 3 { 3 } else { 11 }) % 230) as i32;
            let c = if world == 3 { PAPER } else { p[7] };
            s.rect(
                -48,
                Rect::new(x, y, 2, if world == 3 { 2 } else { 1 }),
                alpha(c, 0.3),
            );
        }
    }
    fn world_art(&self, s: &mut Scene, p: [u32; 8]) {
        let st = &self.state;
        let cam = st.camera;
        let tick = st.tick;
        if !st.secret_inside {
            if st.course == 0 && cam < 180 {
                home(s, -6 - cam, 234, false);
            }
            let nx = self.npc_x() - cam;
            if (-60..860).contains(&nx) {
                sprite(
                    s,
                    12,
                    "villager",
                    include_bytes!("../assets/sprites/villager.png"),
                    nx - 5,
                    328,
                    32.,
                    42.,
                    WHITE,
                );
                if (self.player().x - self.npc_x()).abs() < 170 {
                    let name = story::NPC_NAMES[st.course / 4];
                    let width = text_metrics(name, 16.).0 + 18;
                    let left = (nx - width / 2 + 12).clamp(12, 788 - width);
                    s.rect(18, Rect::new(left, 296, width, 29), alpha(0x102733, 0.96));
                    text_box(
                        s,
                        19,
                        name,
                        Rect::new(left + 8, 299, width - 16, 23),
                        16.,
                        23,
                        rgb(PAPER),
                        false,
                    );
                }
            }
            // A chained bell hangs over each guardian's arena, rooted in the same architecture.
            if st.course % 4 == 3 {
                let bx = st.level.exit.x - cam - 415;
                if bx > -300 && bx < 1050 {
                    for dx in [-82, 126] {
                        s.rect(-19, Rect::new(bx + dx, 151, 18, 217), rgb(p[3]));
                        s.rect(-18, Rect::new(bx + dx + 3, 155, 3, 210), alpha(p[4], 0.22));
                        s.rect(-18, Rect::new(bx + dx - 6, 147, 30, 11), rgb(p[6]));
                    }
                    s.rect(-17, Rect::new(bx - 90, 143, 242, 11), rgb(p[3]));
                    for dx in [5, 57] {
                        for cy in (155..207).step_by(9) {
                            s.rect(-16, Rect::new(bx + dx, cy, 4, 6), rgb(p[6]));
                        }
                    }
                    s.circle(-15, Point::new(bx + 33, 213), 33., rgb(0x283c43));
                    s.rect(-15, Rect::new(bx, 211, 66, 35), rgb(0x283c43));
                    s.circle(-14, Point::new(bx + 33, 214), 28., rgb(0x927958));
                    s.rect(-14, Rect::new(bx + 5, 214, 56, 28), rgb(0x927958));
                    s.rect(-13, Rect::new(bx - 4, 239, 74, 7), rgb(GOLDEN));
                    s.rect(-13, Rect::new(bx + 14, 202, 4, 24), rgb(0xd7b77e));
                    s.rect(-13, Rect::new(bx + 28, 247, 10, 9), rgb(GOLDEN));
                    if !self.boss_active() {
                        s.circle(-20, Point::new(bx + 33, 222), 72., alpha(GOLDEN, 0.045));
                    }
                }
            }
        }
        for t in &st.level.tiles {
            let r = Rect::new(t.r.x - cam, t.r.y, t.r.w, t.r.h);
            if r.w == 0 || r.x > 808 || r.x + r.w < -8 {
                continue;
            }
            match t.kind {
                0 => {
                    s.rect(0, r, rgb(p[5]));
                    s.rect(1, Rect::new(r.x, r.y + 11, r.w, 6), alpha(0x162e37, 0.3));
                    // Geological details are anchored to world cells, never to screen x.
                    let first = t.r.x.div_euclid(32);
                    let last = (t.r.x + t.r.w).div_euclid(32);
                    for cell in first..=last {
                        let wx = cell * 32;
                        if wx < t.r.x || wx + 28 > t.r.x + t.r.w {
                            continue;
                        }
                        let x = wx - cam;
                        if !(-36..836).contains(&x) {
                            continue;
                        }
                        let seed = cell_seed(cell, st.course as u32 / 4 + 38);
                        for (dx, dy) in [(5, 27), (20, 48), (9, 74)] {
                            s.rect(1, Rect::new(x + dx, r.y + dy, 7, 3), alpha(p[6], 0.45));
                            s.rect(
                                1,
                                Rect::new(x + dx + 1, r.y + dy - 1, 4, 1),
                                alpha(p[4], 0.11),
                            );
                        }
                        let root_h = 12 + (seed % 4) as i32 * 9;
                        s.rect(2, Rect::new(x + 9, r.y + 9, 2, root_h), alpha(p[4], 0.28));
                        s.rect(2, Rect::new(x + 11, r.y + 15, 6, 2), alpha(p[4], 0.19));
                        if st.course / 4 != 7
                            && (!st.secret_inside || [0, 3, 6].contains(&(st.course / 4)))
                        {
                            s.rect(3, Rect::new(x + 7, r.y - 3, 2, 4), rgb(p[4]));
                            s.rect(
                                3,
                                Rect::new(x + 11, r.y - 6 - (seed % 3) as i32, 2, 7),
                                rgb(p[4]),
                            );
                            if seed.is_multiple_of(5) {
                                s.rect(3, Rect::new(x + 21, r.y - 6, 1, 7), rgb(p[6]));
                                s.rect(4, Rect::new(x + 19, r.y - 8, 5, 3), rgb(p[7]));
                                s.rect(4, Rect::new(x + 21, r.y - 9, 1, 4), rgb(PAPER));
                            }
                        }
                    }
                    s.rect(3, Rect::new(r.x, r.y, r.w, 3), rgb(p[4]));
                    s.rect(2, Rect::new(r.x, r.y + 3, r.w, 6), rgb(p[6]));
                    s.rect(2, Rect::new(r.x, r.y + 10, 3, r.h - 10), alpha(p[4], 0.25));
                    s.rect(
                        2,
                        Rect::new(r.x + r.w - 3, r.y + 10, 3, r.h - 10),
                        alpha(0x142d38, 0.45),
                    );
                }
                1 => {
                    s.rect(3, r, rgb(p[5]));
                    s.rect(4, Rect::new(r.x, r.y, r.w, 3), rgb(p[4]));
                    s.rect(4, Rect::new(r.x, r.y + 3, r.w, 5), rgb(p[6]));
                    let first = t.r.x.div_euclid(32);
                    let last = (t.r.x + t.r.w).div_euclid(32);
                    for cell in first..=last {
                        let wx = cell * 32;
                        if wx < t.r.x || wx + 28 > t.r.x + t.r.w {
                            continue;
                        }
                        let x = wx - cam;
                        if !(-36..836).contains(&x) {
                            continue;
                        }
                        s.rect(4, Rect::new(x + 3, r.y + 12, 23, 2), alpha(PAPER, 0.16));
                        if r.h > 32 {
                            s.rect(4, Rect::new(x + 29, r.y + 9, 2, r.h - 10), alpha(p[6], 0.4));
                            s.rect(4, Rect::new(x + 8, r.y + 34, 13, 2), alpha(p[6], 0.4));
                        }
                    }
                    s.rect(
                        4,
                        Rect::new(r.x, r.y + r.h - 3, r.w, 3),
                        alpha(0x102b35, 0.3),
                    );
                }
                5 => {
                    // Floor-flush bellows: a mint cap, visible coil, and rising
                    // motes signal an automatic upward spring without a label.
                    s.rect(5, Rect::new(r.x + 2, r.y - 1, r.w - 4, 5), rgb(0x243d45));
                    for n in 0..3 {
                        s.rect(
                            6,
                            Rect::new(r.x + 11 + n % 2 * 4, r.y - 4 - n * 4, 18, 3),
                            rgb(0xd7b77a),
                        );
                    }
                    s.rect(7, Rect::new(r.x + 2, r.y - 15, r.w - 4, 5), rgb(0x274c48));
                    s.rect(8, Rect::new(r.x + 4, r.y - 15, r.w - 8, 3), rgb(MINT));
                    for n in 0..2 {
                        let rise = ((tick / 3 + n * 14) % 30) as i32;
                        star(
                            s,
                            8,
                            r.x + r.w / 2,
                            r.y - 23 - rise,
                            1,
                            alpha(MINT, 0.5 * (1. - rise as f32 / 30.)),
                        );
                    }
                }
                2..=4 => {
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
            if r.x > 880 || r.x + r.w < -80 {
                continue;
            }
            // Brass-bound root ferries belong to the same bell machinery as the wardens.
            s.rect(5, Rect::new(r.x + 5, r.y + r.h, r.w - 10, 3), rgb(0x1b353d));
            s.rect(6, r, rgb(p[5]));
            s.rect(7, Rect::new(r.x, r.y, r.w, 3), rgb(p[7]));
            s.rect(7, Rect::new(r.x, r.y + 3, r.w, 3), rgb(p[6]));
            for n in 0..4 {
                s.rect(7, Rect::new(r.x + n * 20 + 2, r.y + 6, 16, 3), rgb(p[6]));
            }
            for dx in [2, r.w - 8] {
                s.rect(8, Rect::new(r.x + dx, r.y, 6, r.h), rgb(0xb29968));
                s.rect(9, Rect::new(r.x + dx + 2, r.y + 5, 2, 2), rgb(PAPER));
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
            if x > 880 || x + h.w < -80 {
                continue;
            }
            if st.secret_inside && st.level.grotto.archetype == 1 {
                s.rect(7, Rect::new(x, h.y, h.w, h.h), rgb(0x405d79));
                s.rect(8, Rect::new(x, h.y, h.w, 3), rgb(0x9fd9e2));
                for n in 0..h.w / 16 {
                    let pulse = (tick / 12 + n as u32) % 3;
                    s.rect(
                        9,
                        Rect::new(x + n * 16 + 3, h.y - 2 - pulse as i32, 6, 2),
                        rgb(0xd4f0e5),
                    );
                }
                continue;
            }
            if st.secret_inside && [2, 7].contains(&st.level.grotto.archetype) {
                s.rect(7, Rect::new(x, h.y, h.w, h.h), rgb(0x403940));
                s.rect(8, Rect::new(x, h.y, h.w, 3), rgb(0xf5b17b));
                for n in 0..h.w / 10 {
                    s.rect(
                        9,
                        Rect::new(x + n * 10 + 3, h.y + 3, 3, h.h - 3),
                        rgb(0xa06f63),
                    );
                    let rise = ((tick / 2 + n as u32 * 17) % 30) as i32;
                    s.circle(
                        8,
                        Point::new(x + n * 10 + 4, h.y - rise),
                        4.,
                        alpha(0xffd6a9, 0.18 * (1. - rise as f32 / 30.)),
                    );
                }
                continue;
            }
            // The Regent's curse grows as ivory-tipped briars, never loose metal spikes.
            s.rect(7, Rect::new(x, h.y + 9, h.w, 4), rgb(0x403e4e));
            for n in 0..h.w / 8 {
                s.rect(8, Rect::new(x + n * 8, h.y + 7, 8, 5), rgb(0x8d586f));
                s.rect(9, Rect::new(x + n * 8 + 2, h.y + 2, 4, 10), rgb(0xe2a2a3));
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
            let bob = if (tick / 14 + pickup.r.x as u32 / 16).is_multiple_of(2) {
                0
            } else {
                2
            };
            if pickup.kind == 0 {
                let width = if (tick / 8).is_multiple_of(4) { 4 } else { 10 };
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
                    y + if (tick / 12).is_multiple_of(2) { 0 } else { -1 },
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
                    let world = st.course / 4;
                    let phase = self.boss_phase(e);
                    let tell = phase == BossPhase::Telegraph;
                    let open = phase == BossPhase::Recover;
                    let c = if e.immunity > 0 && tick % 8 < 4 {
                        rgb(PAPER)
                    } else {
                        c
                    };
                    if tell || open {
                        let glow = if open { MINT } else { 0xffbd87 };
                        s.circle(
                            12,
                            Point::new(x + e.r.w / 2, y + e.r.h / 2),
                            e.r.w as f32 * 0.8,
                            alpha(glow, if tick % 20 < 10 { 0.09 } else { 0.045 }),
                        );
                    }
                    let (id, data): (&'static str, &'static [u8]) = match world {
                        0 => ("dewmane", include_bytes!("../assets/sprites/boss0.png")),
                        1 => ("chimeback", include_bytes!("../assets/sprites/boss1.png")),
                        2 => ("brack", include_bytes!("../assets/sprites/boss2.png")),
                        3 => ("rimehart", include_bytes!("../assets/sprites/boss3.png")),
                        4 => ("solarch", include_bytes!("../assets/sprites/boss4.png")),
                        5 => ("tempest", include_bytes!("../assets/sprites/boss5.png")),
                        6 => (
                            "briar_matron",
                            include_bytes!("../assets/sprites/boss6.png"),
                        ),
                        _ => ("vesper", include_bytes!("../assets/sprites/boss7.png")),
                    };
                    let shake = if tell { ((tick / 4 % 3) as i32) - 1 } else { 0 };
                    sprite(
                        s,
                        15,
                        id,
                        data,
                        x - 12 + shake,
                        y - 24,
                        (e.r.w + 24) as f32,
                        (e.r.h + 28) as f32,
                        c,
                    );
                    // The cursed collar opens in mint during the safe punish window.
                    let collar_y = y + e.r.h * 55 / 84;
                    s.rect(
                        17,
                        Rect::new(x + e.r.w / 2 - 12, collar_y, 24, 5),
                        rgb(if open { MINT } else { 0xa97c60 }),
                    );
                    star(
                        s,
                        18,
                        x + e.r.w / 2,
                        collar_y + 2,
                        if open { 5 } else { 2 },
                        rgb(if open { PAPER } else { GOLDEN }),
                    );
                    if open {
                        // Put the invitation to stomp above the actual collision crown.
                        star(s, 18, x + e.r.w / 2, y - 12, 5, rgb(MINT));
                        s.rect(18, Rect::new(x + e.r.w / 2 - 13, y - 13, 3, 2), rgb(MINT));
                        s.rect(18, Rect::new(x + e.r.w / 2 + 11, y - 13, 3, 2), rgb(MINT));
                    }
                    if tell {
                        for n in 0..3 {
                            s.rect(
                                17,
                                Rect::new(x + e.r.w / 2 - 13 + n * 12, y - 34 - (n % 2) * 5, 3, 7),
                                rgb(GOLDEN),
                            );
                        }
                    }
                    if e.phase > 0 && !st.secret_inside {
                        let top = if st.banner_ticks > 0 {
                            46 + 14 + text_lines(&st.banner, 748, 18.).len().min(3) as i32 * 24 + 8
                        } else {
                            74
                        };
                        s.rect(45, Rect::new(120, top, 560, 70), alpha(0x102733, 0.96));
                        text_box(
                            s,
                            46,
                            story::BOSS_NAMES[world],
                            Rect::new(137, top + 8, 365, 28),
                            22.,
                            28,
                            rgb(GOLDEN),
                            false,
                        );
                        let text = if open {
                            "Collar open"
                        } else if tell {
                            "Watch the tell"
                        } else {
                            "Keep moving"
                        };
                        text_box(
                            s,
                            46,
                            text,
                            Rect::new(520, top + 10, 145, 24),
                            16.,
                            24,
                            rgb(if open { MINT } else { PAPER }),
                            false,
                        );
                        text_box(
                            s,
                            46,
                            story::BOSS_EPITHETS[world],
                            Rect::new(137, top + 38, 400, 24),
                            17.,
                            24,
                            rgb(PAPER),
                            false,
                        );
                        for n in 0..e.hp {
                            s.rect(
                                47,
                                Rect::new(551 + n as i32 * 16, top + 44, 11, 7),
                                rgb(if open { MINT } else { GOLDEN }),
                            );
                        }
                    }
                }
                _ => (),
            }
        }
        if st.secret_inside {
            let entry = self.grotto_entry_gate();
            root_gate(s, entry.x - cam, entry.y, true, tick);
            root_gate(s, st.level.secret_return.x - cam, 320, true, tick);
        } else {
            root_gate(s, st.level.secret.x - cam, 320, st.level.secret_used, tick);
        }
        let cp = st.level.checkpoint - cam;
        if !st.secret_inside && (-60..860).contains(&cp) {
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
        if !st.secret_inside && gate < 850 {
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
                if locked { "Sealed" } else { "Dawn gate" },
                gate - 22,
                250,
                16.,
                PAPER,
            );
        }
        // The opening trail teaches the three controller gestures beside safe ground.
        if st.course == 0 && !st.secret_inside {
            for (x, y, text, c) in [
                (241, 280, "Left stick: Move", PAPER),
                (391, 215, "A: Jump", MINT),
                (920, 240, "X: Cast", 0xa9dce8),
            ] {
                let sx = x - cam;
                if (-180..800).contains(&sx) {
                    s.rect(
                        19,
                        Rect::new(sx - 8, y - 22, text_metrics(text, 16.).0 + 18, 29),
                        alpha(0x102733, 0.96),
                    );
                    text_box(
                        s,
                        20,
                        text,
                        Rect::new(sx, y - 20, text_metrics(text, 16.).0 + 2, 25),
                        16.,
                        25,
                        rgb(c),
                        false,
                    );
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
            let c = if shot.hostile {
                [
                    0xc6d58c, 0xb7a7e4, 0xe8a77e, 0xc1f7ed, 0xffd78e, 0xc0e9ec, 0xeaaaaf, 0xf0a18e,
                ][st.course / 4]
            } else {
                GOLDEN
            };
            s.circle(18, Point::new(sx + 5, shot.r.y + 5), 9., alpha(c, 0.18));
            if shot.hostile {
                s.circle(18, Point::new(sx + 5, shot.r.y + 5), 5., rgb(0x34414b));
            }
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
        let color = if st.shield > 0 && st.tick % 8 < 3 {
            // Invulnerability pulses translucently so Mote never disappears.
            alpha(0xffffff, 0.48)
        } else if st.invincible > 0 {
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
        let compact = |n: u32| {
            if n >= 1_000_000 {
                format!("{:.1}m", n as f64 / 1_000_000.)
            } else if n >= 10_000 {
                format!("{}k", n / 1000)
            } else {
                format!("{n:03}")
            }
        };
        s.rect(60, Rect::new(8, 6, 267, 33), alpha(0x102733, 0.96));
        text_box(
            s,
            65,
            &format!("{}-{}", world + 1, st.course % 4 + 1),
            Rect::new(19, 10, 44, 26),
            18.,
            26,
            rgb(PAPER),
            false,
        );
        star(s, 65, 76, 22, 4, rgb(GOLDEN));
        text_box(
            s,
            65,
            &compact(st.sparks),
            Rect::new(88, 10, 45, 26),
            18.,
            26,
            rgb(GOLDEN),
            false,
        );
        s.rect(65, Rect::new(143, 13, 8, 2), rgb(MINT));
        s.rect(65, Rect::new(140, 17, 14, 15), rgb(MINT));
        s.rect(66, Rect::new(144, 20, 6, 8), rgb(0x142a36));
        text_box(
            s,
            65,
            &format!("{:02}", st.lives),
            Rect::new(163, 10, 31, 26),
            18.,
            26,
            rgb(PAPER),
            false,
        );
        for n in 0..3 {
            star(
                s,
                66,
                211 + n * 23,
                23,
                4,
                rgb(if n < st.relics as i32 { MINT } else { 0x52676c }),
            );
        }
        s.rect(60, Rect::new(471, 6, 245, 33), alpha(0x102733, 0.96));
        let seconds = st.timer / 60;
        text_box(
            s,
            65,
            &format!("{}:{:02}", seconds / 60, seconds % 60),
            Rect::new(484, 10, 62, 26),
            18.,
            26,
            rgb(PAPER),
            false,
        );
        text_box(
            s,
            65,
            &format!("Score {}", compact(st.score)),
            Rect::new(555, 10, 149, 26),
            18.,
            26,
            rgb(GOLDEN),
            false,
        );
        s.rect(65, ATLAS_BUTTON, rgb(0x294b48));
        text_box(
            s,
            66,
            "Map",
            Rect::new(737, 8, 51, 24),
            18.,
            24,
            rgb(MINT),
            true,
        );
        if st.banner_ticks > 0 && st.transition == 0 {
            let count = text_lines(&st.banner, 748, 18.).len().min(3);
            let height = 14 + count as i32 * 24;
            s.rect(50, Rect::new(12, 46, 776, height), alpha(0x102733, 0.96));
            text_box(
                s,
                51,
                &st.banner,
                Rect::new(26, 53, 748, count as i32 * 24),
                18.,
                24,
                rgb(PAPER),
                false,
            );
        }
        if st.transition > 0 {
            s.rect(75, Rect::new(104, 136, 592, 158), alpha(0x142936, 0.96));
            text_box(
                s,
                76,
                if st.course % 4 == 3 {
                    "A bell calls home"
                } else {
                    "A little brighter"
                },
                Rect::new(124, 152, 552, 43),
                32.,
                43,
                rgb(GOLDEN),
                true,
            );
            text_box(
                s,
                76,
                &format!("{}-{}  Course clear", world + 1, st.course % 4 + 1),
                Rect::new(124, 203, 552, 32),
                24.,
                32,
                rgb(PAPER),
                true,
            );
            text_box(
                s,
                76,
                &format!("Moon shards  {}/3", st.relics),
                Rect::new(124, 247, 552, 28),
                20.,
                28,
                rgb(MINT),
                true,
            );
        }
        if st.atlas {
            self.atlas_art(s);
        }
    }
    fn atlas_art(&self, s: &mut Scene) {
        let st = &self.state;
        s.rect(80, Rect::new(0, 38, 800, 412), rgb(0x182d38));
        text_box(
            s,
            81,
            "The Lantern Isles",
            Rect::new(28, 47, 330, 31),
            24.,
            31,
            rgb(PAPER),
            false,
        );
        text_box(
            s,
            81,
            "A: Travel    B: Return",
            Rect::new(433, 50, 306, 24),
            16.,
            24,
            rgb(MINT),
            true,
        );
        for world in 0..8 {
            let y = 86 + world * 35;
            text_box(
                s,
                81,
                &format!("{:02}  {}", world + 1, levels::WORLDS[world as usize]),
                Rect::new(28, y, 227, 34),
                16.,
                17,
                rgb(PALETTES[world as usize][7]),
                false,
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
                    } else if n == st.map_selection {
                        0x526e5e
                    } else {
                        0x34534e
                    }),
                );
                let value = if locked {
                    "--".into()
                } else {
                    format!("{}-{}", world + 1, course + 1)
                };
                text_box(
                    s,
                    82,
                    &value,
                    Rect::new(x + 9, y + 2, 40, 23),
                    18.,
                    23,
                    rgb(if locked { 0x82968f } else { PAPER }),
                    false,
                );
                for shard in 0..st.best_relics[n] {
                    star(s, 83, x + 57 + shard as i32 * 12, y + 13, 3, rgb(MINT));
                }
                if cleared {
                    s.rect(83, Rect::new(x + 97, y + 2, 3, 23), rgb(GOLDEN));
                }
                if n == st.map_selection {
                    for edge in [
                        Rect::new(x, y, 102, 2),
                        Rect::new(x, y + 25, 102, 2),
                        Rect::new(x, y, 2, 27),
                        Rect::new(x + 100, y, 2, 27),
                    ] {
                        s.rect(84, edge, rgb(GOLDEN));
                    }
                }
            }
        }
        let selected = st.map_selection;
        text_box(
            s,
            81,
            &format!(
                "{}-{}  {}",
                selected / 4 + 1,
                selected % 4 + 1,
                levels::NAMES[selected]
            ),
            Rect::new(28, 382, 711, 50),
            18.,
            25,
            rgb(PAPER),
            false,
        );
    }
}
impl draw::Game for Embertrail {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        if self.state.won {
            "The Dawnheart is home. Morning finds everyone.".into()
        } else if self.state.lives == 0 {
            "Restart keeps your discoveries.".into()
        } else {
            "Left stick moves - A jumps - X casts - Menu pauses".into()
        }
    }
    fn draw(&self, s: &mut Scene) {
        if self.state.won {
            let p = PALETTES[0];
            self.background(s, p);
            s.rect(0, Rect::new(0, FLOOR, 800, 82), rgb(p[5]));
            s.rect(1, Rect::new(0, FLOOR, 800, 4), rgb(p[4]));
            for x in [31, 341, 648] {
                home(s, x, 234, true);
            }
            for x in [188, 266, 480, 559] {
                sprite(
                    s,
                    12,
                    "villager",
                    include_bytes!("../assets/sprites/villager.png"),
                    x,
                    328,
                    32.,
                    42.,
                    WHITE,
                );
            }
            sprite(
                s,
                22,
                "mote0",
                include_bytes!("../assets/sprites/mote0.png"),
                385,
                330,
                32.,
                40.,
                WHITE,
            );
            for n in 0..12 {
                let phase = (self.state.tick / 3 + n * 31) % 130;
                star(
                    s,
                    30,
                    90 + n as i32 * 57,
                    360 - phase as i32,
                    2,
                    alpha(GOLDEN, 0.55),
                );
            }
            return;
        }
        let p = if self.state.secret_inside {
            self.grotto_palette()
        } else {
            PALETTES[self.state.course / 4]
        };
        self.background(s, p);
        self.world_art(s, p);
        self.actor_art(s);
        self.hud(s);
    }
}

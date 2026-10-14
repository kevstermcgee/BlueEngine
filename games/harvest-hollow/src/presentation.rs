//! Read-only, code-native pixel art for the farm. All farm rules live in lib.rs.
use crate::{crop_days, rainy, Garden, State, Tile};
use macroquad::prelude::{is_key_pressed, KeyCode};
use vesper3d::two_d::{
    draw::{self, Color, Scene},
    Intent, Point, Rect,
};

const OX: i32 = 16;
const OY: i32 = 54;
const TILE: i32 = 26;
const W: usize = 22;
const H: usize = 13;
const FARM_SCALE: f32 = 1.23;
const FARM_X: i32 = 48;
const FARM_Y: i32 = 24;
const CREAM: Color = rgb(247, 231, 184);
const INK: Color = rgb(67, 57, 47);
const EDGE: Color = rgb(76, 65, 49);
const WOOD: Color = rgb(150, 104, 61);
const LEAF: Color = rgb(73, 119, 65);
const DEEP: Color = rgb(42, 82, 51);
const GOLD: Color = rgb(237, 186, 82);

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255., g as f32 / 255., b as f32 / 255., 1.)
}
fn r(s: &mut Scene, l: i32, x: i32, y: i32, w: i32, h: i32, c: Color) {
    let rect = if (0..500).contains(&l) {
        let left = x.max(OX);
        let top = y.max(OY);
        let right = (x + w).min(OX + 572);
        let bottom = (y + h).min(OY + 338);
        if right <= left || bottom <= top {
            return;
        }
        Rect::new(left, top, right - left, bottom - top)
    } else {
        Rect::new(x, y, w, h)
    };
    let rect = if (0..500).contains(&l) {
        let p = farm_point(Point::new(rect.x, rect.y));
        Rect::new(
            p.x,
            p.y,
            (rect.w as f32 * FARM_SCALE).round() as i32,
            (rect.h as f32 * FARM_SCALE).round() as i32,
        )
    } else {
        rect
    };
    s.rect(layer(l), rect, c);
}
fn farm_point(p: Point) -> Point {
    Point::new(
        FARM_X + ((p.x - OX) as f32 * FARM_SCALE).round() as i32,
        FARM_Y + ((p.y - OY) as f32 * FARM_SCALE).round() as i32,
    )
}
// The shared start/pause panel owns 200/201, so every game item stays below it.
fn layer(l: i32) -> i32 {
    if l >= 1000 {
        l - 900
    } else if l >= 0 {
        l / 5
    } else {
        l
    }
}
fn text(s: &mut Scene, l: i32, x: i32, y: i32, size: f32, msg: impl Into<String>, c: Color) {
    let (p, size) = if (0..500).contains(&l) {
        (farm_point(Point::new(x, y)), size * FARM_SCALE)
    } else {
        (Point::new(x, y), size)
    };
    s.text(layer(l), msg, p, size, c);
}
fn frame(s: &mut Scene, l: i32, x: i32, y: i32, w: i32, h: i32, c: Color) {
    r(s, l, x, y, w, h, EDGE);
    r(s, l, x + 2, y + 2, w - 4, h - 4, WOOD);
    r(s, l, x + 4, y + 4, w - 8, h - 8, c);
    r(s, l, x + 4, y + 4, w - 8, 1, rgb(248, 219, 162));
}
fn outline(s: &mut Scene, l: i32, x: i32, y: i32, w: i32, h: i32, c: Color) {
    r(s, l, x, y, w, 2, c);
    r(s, l, x, y + h - 2, w, 2, c);
    r(s, l, x, y, 2, h, c);
    r(s, l, x + w - 2, y, 2, h, c);
}
fn disc(s: &mut Scene, l: i32, x: i32, y: i32, radius: i32, c: Color) {
    for dy in (-radius..=radius).step_by(3) {
        let half = ((radius * radius - dy * dy).max(0) as f32).sqrt() as i32;
        r(s, l, x - half, y + dy, (half * 2).max(2), 3, c);
    }
}
fn flower(s: &mut Scene, l: i32, x: i32, y: i32, col: Color) {
    r(s, l, x, y + 2, 2, 6, LEAF);
    r(s, l, x - 3, y - 1, 3, 3, col);
    r(s, l, x + 2, y - 1, 3, 3, col);
    r(s, l, x, y - 4, 2, 3, col);
    r(s, l, x, y + 1, 2, 3, col);
    r(s, l, x, y - 1, 2, 2, rgb(250, 209, 95));
}
fn grass(s: &mut Scene, l: i32, x: i32, y: i32, light: bool) {
    let c = if light {
        rgb(143, 164, 88)
    } else {
        rgb(103, 142, 75)
    };
    r(s, l, x, y, 2, 4, c);
    r(s, l, x - 3, y + 1, 2, 2, c);
    r(s, l, x + 3, y - 2, 2, 5, c);
}

fn cabin(s: &mut Scene) {
    let x = OX + 26;
    let y = OY + 2;
    let l = 130;
    r(s, 4, x - 5, y + 71, 112, 10, rgb(93, 125, 68));
    r(s, l, x + 4, y + 35, 98, 43, rgb(102, 70, 50));
    r(s, l, x + 7, y + 34, 92, 39, rgb(211, 173, 109));
    for row in 0..6 {
        r(s, l, x + 8, y + 38 + row * 6, 90, 1, rgb(173, 132, 81));
    }
    r(s, l, x + 10, y + 36, 5, 38, WOOD);
    r(s, l, x + 90, y + 36, 5, 38, WOOD);
    r(s, l, x + 2, y + 75, 102, 5, rgb(132, 111, 80));
    r(s, l, x + 6, y + 75, 96, 2, rgb(169, 151, 109));
    r(s, l, x + 43, y + 47, 24, 29, rgb(84, 64, 45));
    r(s, l, x + 46, y + 49, 18, 25, rgb(130, 89, 51));
    r(s, l, x + 48, y + 52, 2, 21, rgb(173, 117, 66));
    r(s, l, x + 59, y + 63, 3, 3, GOLD);
    for wx in [21, 73] {
        r(s, l, x + wx, y + 47, 18, 17, rgb(106, 76, 48));
        r(s, l, x + wx + 2, y + 49, 14, 13, rgb(115, 169, 173));
        r(s, l, x + wx + 3, y + 49, 4, 5, rgb(185, 211, 187));
        r(s, l, x + wx + 8, y + 48, 2, 16, rgb(200, 161, 98));
        r(s, l, x + wx + 1, y + 55, 16, 2, rgb(200, 161, 98));
        r(s, l, x + wx - 2, y + 65, 22, 5, rgb(125, 79, 46));
        flower(s, l, x + wx + 3, y + 63, rgb(218, 113, 128));
        flower(s, l, x + wx + 12, y + 63, rgb(229, 180, 112));
    }
    r(s, l, x + 75, y + 2, 13, 27, rgb(108, 80, 64));
    r(s, l, x + 73, y + 2, 17, 5, rgb(174, 132, 92));
    r(s, l, x + 78, y + 9, 5, 9, rgb(162, 113, 77));
    for n in 0..8 {
        let top = y + 7 + n * 4;
        let left = x + 27 - n * 4;
        r(s, l, left - 3, top - 1, 56 + n * 8, 6, rgb(103, 66, 56));
        r(
            s,
            l,
            left,
            top,
            50 + n * 8,
            4,
            if n % 2 == 0 {
                rgb(175, 96, 76)
            } else {
                rgb(163, 83, 66)
            },
        );
        for seam in 0..(4 + n / 2) {
            r(
                s,
                l,
                left + 7 + seam * 13 + n % 2 * 5,
                top,
                2,
                4,
                rgb(128, 69, 59),
            );
        }
    }
    r(s, l, x - 6, y + 38, 116, 5, rgb(103, 65, 49));
    r(s, l, x - 4, y + 38, 112, 2, rgb(225, 154, 98));
    text(s, l + 1, x + 39, y + 46, 8., "HOME", rgb(242, 218, 167));
}

fn pond(s: &mut Scene, state: &State) {
    let x = OX + 17 * TILE;
    let y = OY + TILE;
    r(s, 3, x - 6, y + 5, 106, 69, rgb(170, 158, 99));
    r(s, 3, x + 2, y - 3, 88, 85, rgb(170, 158, 99));
    r(s, 3, x - 3, y + 7, 100, 65, rgb(82, 130, 133));
    r(s, 3, x + 5, y + 1, 84, 77, rgb(82, 130, 133));
    r(s, 3, x + 2, y + 11, 89, 53, rgb(100, 158, 158));
    r(s, 3, x + 11, y + 5, 70, 67, rgb(100, 158, 158));
    for n in 0..12 {
        let px = x + 11 + (n * 23 % 75);
        let py = y + 8 + (n * 17 % 55);
        let off = ((state.tick / 22 + n as u32) % 3) as i32;
        r(s, 4, px + off, py, 9 + n % 5, 1, rgb(150, 190, 170));
        r(s, 4, px + off + 3, py + 2, 5, 1, rgb(131, 179, 167));
    }
    for (px, py) in [(x + 18, y + 51), (x + 71, y + 18)] {
        r(s, 5, px, py, 13, 5, rgb(65, 113, 69));
        r(s, 5, px + 3, py - 2, 8, 3, rgb(88, 137, 74));
        r(s, 5, px + 6, py, 2, 3, rgb(146, 176, 95));
        flower(s, 5, px + 5, py - 2, rgb(236, 172, 173));
    }
    for n in 0..5 {
        let px = x + 2 + n * 19;
        r(s, 8, px, y + 75, 2, 8, DEEP);
        r(s, 8, px + 3, y + 70, 2, 14, LEAF);
        r(s, 8, px + 3, y + 68, 3, 5, rgb(123, 88, 54));
    }
    if state.bridge {
        r(s, 9, x - 7, y + 39, 114, 24, rgb(101, 82, 55));
        for n in 0..11 {
            r(s, 9, x - 6 + n * 10, y + 40, 8, 21, rgb(191, 152, 88));
            r(s, 9, x - 5 + n * 10, y + 42, 1, 17, rgb(220, 182, 113));
        }
        r(s, 10, x - 8, y + 37, 116, 3, rgb(230, 189, 118));
        r(s, 10, x - 8, y + 60, 116, 3, rgb(137, 97, 57));
    }
}

fn stand(s: &mut Scene, x: i32, y: i32, shipping: bool) {
    let l = y + 46;
    r(s, 5, x - 2, y + 29, 34, 6, rgb(85, 121, 64));
    if shipping {
        r(s, l, x - 1, y + 6, 31, 25, rgb(107, 75, 46));
        r(s, l, x + 1, y + 8, 27, 21, rgb(183, 134, 69));
        r(s, l, x - 3, y + 3, 35, 7, rgb(205, 157, 87));
        r(s, l, x - 3, y + 9, 35, 2, rgb(112, 77, 43));
        r(s, l, x + 3, y + 11, 2, 18, rgb(222, 175, 101));
        r(s, l, x + 24, y + 11, 2, 18, rgb(139, 92, 48));
        r(s, l, x + 11, y + 9, 8, 5, rgb(84, 66, 45));
        r(s, l, x + 1, y + 17, 27, 11, rgb(99, 72, 45));
        text(s, l, x + 2, y + 26, 9., "SHIP", CREAM);
    } else {
        r(s, l, x - 1, y + 2, 3, 32, WOOD);
        r(s, l, x + 26, y + 2, 3, 32, WOOD);
        r(s, l, x - 4, y + 20, 37, 13, rgb(161, 109, 62));
        r(s, l, x - 2, y + 20, 33, 3, rgb(231, 192, 111));
        for n in 0..3 {
            r(s, l, x + n * 10, y + 13, 8, 7, rgb(232, 211, 155));
            r(
                s,
                l,
                x + n * 10 + 2,
                y + 14,
                4,
                3,
                [rgb(203, 96, 109), rgb(233, 150, 68), rgb(233, 177, 72)][n as usize],
            );
        }
        r(s, l, x - 7, y - 3, 43, 9, rgb(73, 104, 64));
        for n in 0..5 {
            r(
                s,
                l,
                x - 7 + n * 9,
                y - 3,
                9,
                7,
                if n % 2 == 0 {
                    rgb(219, 210, 152)
                } else {
                    rgb(112, 145, 78)
                },
            );
        }
        text(s, l, x - 1, y + 31, 9., "SEEDS", CREAM);
    }
}

fn crop(s: &mut Scene, l: i32, x: i32, y: i32, tile: &Tile, tick: u32) {
    if tile.crop == 0 {
        return;
    }
    let mature = tile.growth >= crop_days(tile.crop);
    let stage = if mature {
        3
    } else if tile.growth == 0 {
        0
    } else if tile.growth + 1 >= crop_days(tile.crop) {
        2
    } else {
        1
    };
    let dark = rgb(47, 100, 52);
    let light = rgb(135, 172, 85);
    if stage == 0 {
        r(s, l, x - 1, y - 1, 3, 5, dark);
        r(s, l, x - 5, y - 4, 4, 3, LEAF);
        r(s, l, x + 2, y - 3, 4, 3, light);
        return;
    }
    r(s, l, x - 7, y + 6, 15, 3, rgb(96, 70, 49));
    r(s, l, x - 1, y - 5, 3, 11, dark);
    for (dx, dy, ww) in [(-7, -5, 6), (2, -8, 6), (-5, -11, 5), (2, -1, 5)] {
        r(
            s,
            l,
            x + dx,
            y + dy,
            ww,
            4,
            if dx < 0 { LEAF } else { light },
        );
    }
    if stage >= 2 {
        match tile.crop {
            1 => {
                r(s, l, x - 6, y + 1, 12, 7, rgb(174, 72, 101));
                r(s, l, x - 4, y - 1, 8, 11, rgb(217, 106, 130));
                r(s, l, x - 4, y + 1, 3, 3, rgb(245, 157, 162));
                r(s, l, x, y + 9, 2, 3, CREAM);
            }
            2 => {
                r(s, l, x - 5, y + 1, 10, 5, rgb(233, 153, 63));
                r(s, l, x - 3, y + 5, 6, 3, rgb(225, 130, 50));
                r(s, l, x - 1, y + 8, 2, 4, rgb(186, 102, 47));
                r(s, l, x - 3, y + 1, 2, 5, rgb(251, 192, 87));
            }
            _ => {
                r(s, l, x - 9, y, 18, 9, rgb(200, 110, 45));
                r(s, l, x - 6, y - 3, 12, 14, rgb(229, 144, 55));
                r(s, l, x - 2, y - 4, 4, 15, rgb(243, 168, 65));
                r(s, l, x - 6, y, 2, 7, rgb(248, 188, 84));
                r(s, l, x, y - 8, 3, 5, rgb(107, 102, 50));
            }
        }
    }
    if mature && (tick / 24 + x as u32).is_multiple_of(4) {
        r(s, l + 1, x + 9, y - 16, 2, 7, CREAM);
        r(s, l + 1, x + 6, y - 13, 8, 1, CREAM);
    }
}

fn tree(s: &mut Scene, x: i32, y: i32, hp: u8, tick: u32) {
    let l = y + 5;
    r(s, 6, x - 18, y + 4, 38, 6, rgb(80, 115, 65));
    r(s, 6, x - 11, y + 9, 27, 4, rgb(90, 123, 66));
    r(s, l, x - 5, y - 24, 11, 31, rgb(89, 65, 45));
    r(s, l, x - 2, y - 26, 6, 32, rgb(147, 104, 61));
    r(s, l, x - 1, y - 20, 2, 17, rgb(181, 130, 76));
    r(s, l, x - 9, y + 4, 8, 4, rgb(109, 77, 47));
    r(s, l, x + 3, y + 4, 7, 4, rgb(109, 77, 47));
    disc(s, l, x, y - 40, 23, DEEP);
    disc(s, l, x - 13, y - 29, 17, DEEP);
    disc(s, l, x + 14, y - 30, 18, DEEP);
    disc(s, l, x - 3, y - 44, 19, rgb(80, 125, 63));
    disc(s, l, x - 17, y - 34, 13, rgb(86, 131, 65));
    disc(s, l, x + 12, y - 34, 16, rgb(75, 117, 59));
    disc(s, l, x - 4, y - 29, 15, rgb(85, 128, 61));
    for n in 0..12 {
        let px = x - 20 + n * 13 % 38;
        let py = y - 51 + n * 7 % 31;
        r(
            s,
            l,
            px,
            py,
            4 + n % 3,
            3,
            if n % 3 == 0 {
                rgb(146, 163, 78)
            } else {
                rgb(105, 143, 65)
            },
        );
        r(s, l, px + 2, py + 3, 3, 2, rgb(70, 112, 58));
    }
    if hp < 3 {
        r(s, l + 1, x - 3, y - 3, 7, 2, rgb(233, 180, 111));
        if (tick / 20).is_multiple_of(2) {
            r(s, l + 1, x - 2, y - 7, 5, 2, rgb(221, 171, 105));
        }
    }
}
fn rock(s: &mut Scene, x: i32, y: i32, stump: bool) {
    let l = y + 3;
    r(s, 6, x - 11, y + 4, 23, 5, rgb(87, 119, 64));
    if stump {
        r(s, l, x - 9, y - 5, 18, 13, rgb(123, 86, 50));
        r(s, l, x - 11, y - 1, 22, 7, rgb(123, 86, 50));
        r(s, l, x - 8, y - 7, 16, 7, rgb(210, 167, 93));
        r(s, l, x - 5, y - 5, 10, 3, rgb(159, 115, 59));
        r(s, l, x - 2, y - 5, 4, 2, rgb(223, 184, 108));
        r(s, l, x - 5, y + 2, 2, 5, rgb(159, 109, 59));
    } else {
        r(s, l, x - 10, y - 3, 21, 10, rgb(99, 109, 94));
        r(s, l, x - 7, y - 9, 15, 15, rgb(137, 146, 127));
        r(s, l, x - 4, y - 11, 10, 12, rgb(153, 159, 139));
        r(s, l, x - 5, y - 8, 8, 3, rgb(188, 187, 159));
        r(s, l, x + 4, y - 2, 5, 5, rgb(116, 126, 106));
        r(s, l, x - 11, y + 5, 23, 2, rgb(84, 99, 82));
    }
}

fn player(s: &mut Scene, state: &State) {
    let x = OX + state.player.x + state.player.w / 2;
    let y = OY + state.player.y + state.player.h;
    let l = y + 7;
    let bob = if (state.tick / 32).is_multiple_of(2) {
        0
    } else {
        1
    };
    r(s, 7, x - 9, y - 1, 18, 4, rgb(74, 109, 66));
    r(s, l, x - 6, y - 5, 5, 6, rgb(84, 62, 48));
    r(s, l, x + 2, y - 5, 5, 6, rgb(84, 62, 48));
    r(s, l, x - 6, y - 16 + bob, 13, 13, rgb(47, 99, 106));
    r(s, l, x - 9, y - 16 + bob, 4, 9, rgb(244, 217, 170));
    r(s, l, x + 7, y - 16 + bob, 4, 9, rgb(244, 217, 170));
    r(s, l, x - 5, y - 17 + bob, 3, 7, rgb(161, 193, 170));
    r(s, l, x + 3, y - 17 + bob, 3, 7, rgb(161, 193, 170));
    r(s, l, x - 4, y - 10 + bob, 9, 2, rgb(75, 135, 136));
    r(s, l, x - 1, y - 10 + bob, 2, 2, GOLD);
    r(s, l, x - 6, y - 29 + bob, 13, 13, rgb(117, 74, 47));
    r(s, l, x - 5, y - 27 + bob, 11, 10, rgb(236, 188, 132));
    r(s, l, x + 3, y - 24 + bob, 4, 5, rgb(244, 203, 149));
    if state.facing.y >= 0 {
        r(s, l, x - 3 + state.facing.x, y - 23 + bob, 2, 2, INK);
        r(s, l, x + 3 + state.facing.x, y - 23 + bob, 2, 2, INK);
        r(s, l, x - 1, y - 19 + bob, 3, 1, rgb(164, 104, 77));
    } else {
        r(s, l, x - 5, y - 23 + bob, 11, 6, rgb(125, 79, 48));
    }
    r(s, l, x - 13, y - 31 + bob, 27, 5, rgb(125, 89, 46));
    r(s, l, x - 12, y - 32 + bob, 25, 4, rgb(233, 189, 99));
    r(s, l, x - 8, y - 39 + bob, 17, 8, rgb(217, 169, 79));
    r(s, l, x - 6, y - 41 + bob, 13, 3, rgb(237, 199, 115));
    r(s, l, x - 8, y - 34 + bob, 17, 3, rgb(139, 101, 57));
    r(s, l, x - 7, y - 39 + bob, 3, 5, rgb(246, 211, 135));
    if state.action_tick > 0 && state.tick.saturating_sub(state.action_tick) < 12 {
        let dx = state.facing.x * 14;
        let dy = state.facing.y * 9;
        tool_icon(s, l + 2, x + dx - 8, y - 19 + dy, state.tool);
    }
}

fn tool_icon(s: &mut Scene, l: i32, x: i32, y: i32, kind: u8) {
    let metal = rgb(154, 182, 173);
    match kind {
        0 => {
            r(s, l, x + 7, y + 8, 4, 12, rgb(104, 126, 65));
            r(s, l, x + 2, y + 4, 6, 7, rgb(105, 153, 75));
            r(s, l, x + 10, y, 7, 9, rgb(143, 174, 89));
            r(s, l, x + 11, y + 1, 4, 3, rgb(184, 193, 115));
            r(s, l, x + 5, y + 18, 10, 3, rgb(153, 108, 61));
        }
        1 | 4 | 5 | 6 => {
            r(s, l, x + 8, y + 6, 3, 15, rgb(129, 85, 49));
            r(s, l, x + 8, y + 8, 1, 11, rgb(204, 153, 85));
            if kind == 1 {
                r(s, l, x + 2, y + 4, 13, 3, metal);
                r(s, l, x + 2, y + 4, 3, 7, rgb(108, 139, 135));
            } else if kind == 4 {
                r(s, l, x + 3, y + 3, 12, 7, metal);
                r(s, l, x + 1, y + 5, 5, 7, rgb(187, 203, 185));
                r(s, l, x + 1, y + 5, 2, 7, CREAM);
            } else if kind == 5 {
                r(s, l, x + 1, y + 4, 17, 3, metal);
                r(s, l, x, y + 7, 3, 4, rgb(108, 139, 135));
                r(s, l, x + 16, y + 7, 3, 4, rgb(108, 139, 135));
            } else {
                r(s, l, x + 2, y + 2, 15, 8, rgb(153, 110, 68));
                r(s, l, x + 3, y + 2, 13, 3, rgb(221, 173, 107));
            }
        }
        2 => {
            r(s, l, x + 1, y + 3, 17, 18, rgb(143, 108, 61));
            r(s, l, x + 3, y + 2, 13, 17, rgb(232, 213, 159));
            r(s, l, x + 5, y + 2, 9, 3, CREAM);
            r(s, l, x + 8, y + 7, 3, 8, LEAF);
            r(s, l, x + 5, y + 7, 4, 3, rgb(142, 166, 82));
            r(s, l, x + 10, y + 6, 4, 3, LEAF);
        }
        _ => {
            r(s, l, x + 3, y + 8, 12, 12, rgb(82, 142, 158));
            r(s, l, x + 5, y + 10, 8, 7, rgb(124, 177, 180));
            outline(s, l, x + 13, y + 9, 7, 9, rgb(82, 142, 158));
            r(s, l, x, y + 4, 5, 9, rgb(103, 160, 171));
            r(s, l, x - 2, y + 2, 5, 4, rgb(159, 197, 193));
            r(s, l, x + 6, y + 5, 6, 3, rgb(57, 108, 128));
        }
    }
}

fn terrain(s: &mut Scene, state: &State) {
    r(s, 0, OX, OY, 572, 338, rgb(124, 153, 80));
    for gy in 0..H {
        for gx in 0..W {
            let i = gy * W + gx;
            let x = OX + gx as i32 * TILE;
            let y = OY + gy as i32 * TILE;
            let Some(tile) = state.tiles.get(i) else {
                continue;
            };
            if (gx * 7 + gy * 11) % 9 == 0 {
                r(s, 1, x, y, 26, 26, rgb(128, 156, 82));
            }
            for n in 0..3 {
                let px = x + 4 + (gx as i32 * 13 + gy as i32 * 7 + n * 9) % 18;
                let py = y + 4 + (gx as i32 * 9 + gy as i32 * 13 + n * 7) % 18;
                grass(s, 2, px, py, n == 0);
            }
            if tile.soil {
                let base = if tile.wet {
                    rgb(115, 78, 53)
                } else {
                    rgb(158, 112, 67)
                };
                r(s, 5, x + 1, y + 1, 24, 24, rgb(112, 85, 50));
                r(s, 5, x + 2, y + 2, 22, 21, base);
                for row in 0..3 {
                    r(
                        s,
                        5,
                        x + 4,
                        y + 5 + row * 6,
                        18,
                        1,
                        if tile.wet {
                            rgb(101, 69, 49)
                        } else {
                            rgb(130, 90, 53)
                        },
                    );
                    r(
                        s,
                        5,
                        x + 7 + row * 3,
                        y + 7 + row * 6,
                        3,
                        1,
                        if tile.wet {
                            rgb(151, 117, 77)
                        } else {
                            rgb(185, 139, 82)
                        },
                    );
                }
                if tile.wet && (state.tick / 35 + i as u32).is_multiple_of(7) {
                    r(s, 6, x + 19, y + 6, 3, 1, rgb(166, 180, 155));
                    r(s, 6, x + 20, y + 5, 1, 3, rgb(166, 180, 155));
                }
            }
            crop(s, y + 40, x + 13, y + 13, tile, state.tick);
        }
    }
    // A worn footpath links the cabin, seed stand and shipping box.
    r(s, 3, OX + 48, OY + 78, 20, 31, rgb(191, 170, 111));
    r(s, 3, OX + 48, OY + 102, 350, 19, rgb(191, 170, 111));
    r(s, 3, OX + 210, OY + 62, 16, 44, rgb(191, 170, 111));
    r(s, 3, OX + 312, OY + 59, 17, 48, rgb(191, 170, 111));
    for n in 0..44 {
        r(
            s,
            4,
            OX + 52 + n * 17 % 342,
            OY + 105 + n * 13 % 14,
            3,
            2,
            if n % 2 == 0 {
                rgb(171, 149, 94)
            } else {
                rgb(218, 195, 130)
            },
        );
    }
    pond(s, state);
    // Split-rail boundary leaves the playable field unobstructed.
    for n in 0..22 {
        let x = OX + n * TILE + 2;
        for y in [OY + 4, OY + 328] {
            r(s, 7, x, y, 25, 3, rgb(162, 126, 72));
            r(s, 7, x, y + 5, 25, 3, rgb(138, 102, 62));
            r(s, 7, x + 9, y - 3, 4, 13, rgb(114, 87, 53));
            r(s, 7, x + 9, y - 3, 3, 3, rgb(223, 188, 111));
        }
    }
    for n in 0..12 {
        let y = OY + 12 + n * TILE;
        for x in [OX + 1, OX + 562] {
            r(s, 7, x, y, 3, 25, rgb(148, 114, 67));
            r(s, 7, x + 5, y, 3, 25, rgb(132, 101, 60));
            r(s, 7, x - 2, y + 9, 11, 4, rgb(173, 136, 79));
        }
    }
    for n in 0..18 {
        let x = OX + 15 + n * 53 % 540;
        let y = OY + 21 + n * 31 % 305;
        let gx = ((x - OX) / TILE) as usize;
        let gy = ((y - OY) / TILE) as usize;
        if state
            .tiles
            .get(gy * W + gx)
            .is_some_and(|t| !t.soil && t.obstacle == 0)
            && !(gx > 16 && gy < 4)
        {
            flower(
                s,
                8,
                x,
                y,
                if n % 2 == 0 {
                    rgb(233, 215, 153)
                } else {
                    rgb(186, 150, 181)
                },
            );
        }
    }
    for gy in 0..H {
        for gx in 0..W {
            let Some(tile) = state.tiles.get(gy * W + gx) else {
                continue;
            };
            let x = OX + gx as i32 * TILE + 13;
            let y = OY + gy as i32 * TILE + 20;
            match tile.obstacle {
                1 => tree(s, x, y, tile.hp, state.tick),
                2 => rock(s, x, y, false),
                3 => {
                    let l = y + 3;
                    for (dx, dy) in [(-6, 0), (-2, -6), (4, -3)] {
                        r(s, l, x + dx, y + dy - 7, 3, 10, DEEP);
                        r(s, l, x + dx - 3, y + dy - 5, 7, 3, LEAF);
                        r(s, l, x + dx + 1, y + dy - 9, 4, 3, rgb(155, 170, 91));
                    }
                }
                4 => rock(s, x, y, true),
                _ => {}
            }
        }
    }
    cabin(s);
    stand(s, OX + 8 * TILE, OY + TILE, false);
    stand(s, OX + 12 * TILE, OY + TILE, true);
    if state.sprinkler {
        let x = OX + 10 * TILE + 13;
        let y = OY + 8 * TILE + 13;
        r(s, y + 44, x - 2, y - 6, 5, 12, rgb(96, 127, 135));
        r(s, y + 44, x - 7, y - 8, 15, 4, rgb(163, 187, 174));
        r(s, y + 44, x - 4, y + 5, 9, 3, rgb(94, 114, 106));
        if state.tick % 120 < 40 {
            for n in 0..6 {
                let off = (state.tick % 40) as i32 / 2 + n * 5;
                r(
                    s,
                    y + 45,
                    x - off,
                    y - 11 + n % 2 * 3,
                    2,
                    2,
                    rgb(176, 212, 202),
                );
                r(
                    s,
                    y + 45,
                    x + off,
                    y - 11 + n % 2 * 3,
                    2,
                    2,
                    rgb(176, 212, 202),
                );
            }
        }
    }
    player(s, state);
    // Butterflies and fireflies are decorative, deterministic and never own rules.
    for n in 0..3 {
        let x = OX + 100 + ((state.tick / 6 + n * 149) % 410) as i32;
        let y = OY + 128 + ((state.tick / 13 + n * 47) % 153) as i32;
        let color = [rgb(248, 213, 112), rgb(217, 175, 182), rgb(185, 204, 171)][n as usize];
        let flap = if (state.tick / 8).is_multiple_of(2) {
            3
        } else {
            1
        };
        r(s, 480, x - flap, y, flap, 3, color);
        r(s, 480, x + 1, y, flap, 3, color);
        r(s, 480, x, y, 1, 4, rgb(116, 107, 72));
    }
    if rainy(state.day) {
        r(s, 490, OX, OY, 572, 338, Color::new(0.23, 0.34, 0.40, 0.17));
        for n in 0..65 {
            let x = OX + 9 + ((n * 43 + state.tick as i32 / 3) % 552);
            let y = OY + 4 + ((n * 71 + state.tick as i32 * 3) % 321);
            r(s, 491, x, y, 1, 8, Color::new(0.77, 0.86, 0.85, 0.5));
            r(
                s,
                491,
                x - 1,
                y + 4,
                1,
                4,
                Color::new(0.77, 0.86, 0.85, 0.3),
            );
        }
    }
    let evening = ((state.clock.saturating_sub(4800)) as f32 / 5700.).clamp(0., 0.42);
    if evening > 0. {
        r(
            s,
            495,
            OX,
            OY,
            572,
            338,
            Color::new(0.17, 0.20, 0.34, evening),
        );
        for wx in [OX + 47, OX + 99] {
            r(
                s,
                496,
                wx,
                OY + 51,
                12,
                10,
                Color::new(1., 0.77, 0.35, evening),
            );
        }
    }
    if state.action_tick > 0 && state.tick.saturating_sub(state.action_tick) < 18 {
        let elapsed = state.tick.saturating_sub(state.action_tick) as i32;
        let x = OX + state.action_at.x * TILE + 13;
        let y = OY + state.action_at.y * TILE + 10;
        for n in 0..7 {
            let dx = (n * 7 % 15 - 7) * (elapsed + 4) / 8;
            let dy = elapsed / 2 + n * 3 % 8;
            r(
                s,
                498,
                x + dx,
                y - dy,
                3,
                3,
                if state.tool == 3 {
                    rgb(161, 209, 206)
                } else {
                    GOLD
                },
            );
        }
    }
}

fn drawer_button(s: &mut Scene, y: i32, label: impl Into<String>) {
    frame(s, 1080, 572, y, 200, 26, rgb(239, 218, 169));
    text(s, 1082, 583, y + 18, 14., label, INK);
}

impl draw::Game for Garden {
    fn draw<'a>(&'a self, s: &mut Scene<'a>) {
        let state = &self.state;
        r(s, -10, 0, 0, 800, 450, rgb(43, 38, 33));
        terrain(s, state);

        // A small status ribbon leaves the farm as the main thing on screen.
        r(s, 1000, 0, 0, 800, 22, rgb(48, 42, 35));
        r(s, 1000, 0, 21, 800, 1, rgb(116, 96, 70));
        text(s, 1002, 12, 16, 16., "Harvest Hollow", CREAM);
        let minutes = 6 * 60 + state.clock * 16 * 60 / crate::DAY_TICKS;
        let hour = (minutes / 60) % 24;
        let hour12 = if hour.is_multiple_of(12) {
            12
        } else {
            hour % 12
        };
        let kind = state.seed_kind.min(2) as usize;
        let seed_names = ["Radish", "Carrot", "Pumpkin"];
        text(s, 1002, 140, 15, 12., format!("Day {}", state.day), CREAM);
        text(
            s,
            1002,
            182,
            15,
            12.,
            format!(
                "{}:{:02}{}",
                hour12,
                minutes % 60 / 10 * 10,
                if hour < 12 { "a" } else { "p" }
            ),
            rgb(207, 193, 166),
        );
        text(s, 1002, 237, 15, 12., format!("{}g", state.gold), GOLD);
        text(
            s,
            1002,
            294,
            15,
            12.,
            format!("E {}", state.energy),
            rgb(181, 206, 132),
        );
        text(
            s,
            1002,
            348,
            15,
            12.,
            format!("W {}", state.water),
            rgb(156, 204, 207),
        );
        text(
            s,
            1002,
            404,
            15,
            12.,
            format!("{} {}", seed_names[kind], state.seeds[kind]),
            CREAM,
        );
        text(
            s,
            1002,
            486,
            15,
            11.,
            if rainy(state.day) { "Rain" } else { "Spring" },
            rgb(192, 178, 151),
        );
        text(
            s,
            1002,
            545,
            15,
            11.,
            format!("Wood {}  Stone {}", state.wood, state.stone),
            rgb(207, 193, 166),
        );
        frame(
            s,
            1000,
            709,
            2,
            77,
            19,
            if self.panel_open {
                rgb(249, 225, 166)
            } else {
                rgb(225, 206, 155)
            },
        );
        text(s, 1002, 718, 15, 12., "Farm [I]", INK);

        // Compact tools sit over the bottom fence; the optional drawer owns supplies.
        r(s, 1000, 0, 422, 800, 28, Color::new(0.17, 0.15, 0.13, 0.97));
        let tools = ["Smart", "Hoe", "Seed", "Water", "Axe", "Pick", "Build"];
        for (n, name) in tools.iter().enumerate() {
            let x = 12 + n as i32 * 66;
            let selected = state.tool == n as u8;
            r(
                s,
                1000,
                x,
                425,
                63,
                23,
                if selected {
                    rgb(239, 220, 166)
                } else {
                    rgb(183, 177, 131)
                },
            );
            if selected {
                r(s, 1001, x, 445, 63, 3, GOLD);
            }
            tool_icon(s, 1002, x + 3, 426, n as u8);
            text(s, 1002, x + 25, 441, 12., *name, INK);
            text(
                s,
                1002,
                x + 55,
                432,
                8.,
                format!("{}", n + 1),
                rgb(124, 103, 65),
            );
        }
        text(
            s,
            1002,
            493,
            435,
            11.,
            "WASD  /  Click or E  /  Tab crop",
            rgb(227, 216, 193),
        );
        text(
            s,
            1002,
            493,
            447,
            10.,
            "K save   L load   N music   M sound   Esc pause",
            rgb(193, 179, 153),
        );

        if state.tick < state.notice_until && !state.notice.is_empty() {
            let mut msg: String = state.notice.chars().take(96).collect();
            if state.notice.chars().count() > 96 {
                msg.push_str("...");
            }
            r(s, 700, 60, 396, 680, 21, Color::new(0.18, 0.15, 0.12, 0.92));
            r(s, 701, 60, 396, 2, 21, GOLD);
            text(s, 702, 70, 411, 12., msg, CREAM);
        }

        if self.panel_open {
            frame(s, 1080, 560, 35, 224, 333, rgb(241, 222, 180));
            text(s, 1082, 572, 57, 19., "Farm & supplies", INK);
            frame(s, 1080, 750, 42, 24, 20, rgb(215, 184, 135));
            text(s, 1082, 758, 56, 13., "X", INK);
            drawer_button(
                s,
                68,
                format!("{} seeds: {}   [Tab]", seed_names[kind], state.seeds[kind]),
            );
            drawer_button(
                s,
                102,
                format!("Buy 5 seeds  /  {} gold", crate::PRICES[kind] * 5),
            );
            drawer_button(
                s,
                136,
                format!("Ship harvest  ({})", state.harvest.iter().sum::<u32>()),
            );
            drawer_button(
                s,
                170,
                if !state.sprinkler {
                    "Build sprinkler  [B]"
                } else if !state.bridge {
                    "Build pond deck  [B]"
                } else {
                    "Upgrades complete"
                },
            );
            text(
                s,
                1082,
                579,
                207,
                11.,
                if !state.sprinkler {
                    "20 wood + 8 stone + 90 gold"
                } else if !state.bridge {
                    "18 wood + 5 stone"
                } else {
                    "Your farm is flourishing."
                },
                rgb(128, 96, 63),
            );
            drawer_button(s, 214, "Sleep until morning");
            drawer_button(s, 248, "Farm guide & goals");
            text(
                s,
                1082,
                578,
                292,
                13.,
                format!(
                    "{} crops shipped  /  {} cleared",
                    state.shipped, state.cleared
                ),
                rgb(115, 89, 57),
            );
            text(
                s,
                1082,
                578,
                314,
                12.,
                format!(
                    "{}: {} watered days to grow",
                    seed_names[kind],
                    crop_days(kind as u8 + 1)
                ),
                rgb(100, 112, 65),
            );
            text(
                s,
                1082,
                578,
                337,
                11.,
                "Water daily. Dry crops wait safely.",
                rgb(115, 96, 69),
            );
            text(
                s,
                1082,
                578,
                352,
                11.,
                "Click the pond to refill your can.",
                rgb(115, 96, 69),
            );
        }
    }

    fn device_input(&mut self, mut input: Intent) -> Intent {
        if is_key_pressed(KeyCode::E) || is_key_pressed(KeyCode::Space) {
            input.pointer = None;
            input.action = true;
        }
        if is_key_pressed(KeyCode::I) {
            self.panel_open = !self.panel_open;
            input.action = false;
            input.pointer = None;
            return input;
        }
        if input.action {
            if let Some(p) = input.pointer {
                if Rect::new(709, 2, 77, 20).contains(p) {
                    self.panel_open = !self.panel_open;
                    input.action = false;
                    input.pointer = None;
                    return input;
                }
                if self.panel_open && Rect::new(560, 35, 224, 333).contains(p) {
                    if Rect::new(750, 42, 24, 20).contains(p) {
                        self.panel_open = false;
                        input.action = false;
                        input.pointer = None;
                        return input;
                    }
                    let virtual_y = if (572..772).contains(&p.x) {
                        match p.y {
                            68..=93 => Some(174),
                            102..=127 => Some(204),
                            136..=161 => Some(240),
                            170..=195 => Some(278),
                            214..=239 => Some(315),
                            248..=273 => Some(353),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    input.pointer = virtual_y.map(|y| Point::new(688, y));
                    input.action = virtual_y.is_some();
                    if virtual_y == Some(315) {
                        self.panel_open = false;
                    }
                    return input;
                }
                if Rect::new(12, 425, 462, 23).contains(p) {
                    let tool = ((p.x - 12) / 66).clamp(0, 6);
                    input.pointer = Some(Point::new(56 + tool * 80, 420));
                } else if Rect::new(400, 0, 83, 22).contains(p) {
                    input.pointer = Some(Point::new(688, 174));
                } else if Rect::new(FARM_X, FARM_Y, 704, 398).contains(p) {
                    input.pointer = Some(Point::new(
                        OX + ((p.x - FARM_X) as f32 / FARM_SCALE) as i32,
                        OY + ((p.y - FARM_Y) as f32 / FARM_SCALE) as i32,
                    ));
                } else {
                    input.pointer = None;
                    input.action = false;
                }
            }
        }
        let keys = [
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Key6,
            KeyCode::Key7,
        ];
        for (n, key) in keys.iter().enumerate() {
            if is_key_pressed(*key) {
                input.pointer = Some(Point::new(56 + n as i32 * 80, 420));
                input.action = true;
            }
        }
        if is_key_pressed(KeyCode::Tab) {
            input.pointer = Some(Point::new(688, 174));
            input.action = true;
        }
        if is_key_pressed(KeyCode::B) {
            input.pointer = Some(Point::new(688, 278));
            input.action = true;
        }
        if is_key_pressed(KeyCode::E) || is_key_pressed(KeyCode::Space) {
            input.pointer = None;
            input.action = true;
        }
        input
    }

    fn show_hud() -> bool {
        false
    }

    fn menu_status(&self) -> String {
        "Click land to farm. Sleep to grow.".to_string()
    }
}

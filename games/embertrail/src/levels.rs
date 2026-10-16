//! Explicit authored section sequences. No random generation determines course layouts.
use super::*;
pub const WORLDS: [&str; 8] = [
    "DEWLIGHT GARDENS",
    "ECHO ROOT CAVERNS",
    "COPPER MARSH",
    "FROSTBLOOM ORCHARD",
    "SUNGLASS RUINS",
    "CLOUDSEED SKYWAYS",
    "DUSKWEAVE WOODS",
    "THE HOLLOW CITADEL",
];
pub const NAMES: [&str; 32] = [
    "First little light",
    "The root bridges",
    "Over the garden wall",
    "Dewmane's stolen bell",
    "Under the old oak",
    "Crystal footsteps",
    "The hidden river",
    "Chimeback's echo court",
    "Reeds and rivets",
    "Seed-shell crossing",
    "Marshlight machinery",
    "Brack and the silent mill",
    "Snowfall orchard",
    "Slippery branches",
    "Aurora over ice",
    "Rimehart's frozen bell",
    "Dunes of glass",
    "Sunken stairways",
    "A mirage of bridges",
    "Solarch's glass crown",
    "Cloudseed ascent",
    "Between the windmills",
    "The floating orchard",
    "Tempest over the skyways",
    "Lanterns after dusk",
    "Thornwoven trails",
    "The lost grove",
    "The Briar Matron",
    "Door of the hollow",
    "Gears of night",
    "The last ember",
    "Vesper, the Bell Regent",
];
pub const BOSS_COURT: i32 = 880;
// 0 meadow, 1 steps, 2 gap, 3 bridge, 4 towers, 5 canopy, 6 thorns,
// 7 sprint, 8 hidden root vault, 9 lift, 10 creature gauntlet.
const ROUTES: [&[u8]; 32] = [
    &[0, 1, 0, 2, 8, 1],
    &[0, 2, 3, 1, 8, 2, 3],
    &[1, 4, 5, 2, 8, 4, 5],
    &[0, 1, 2, 10, 8, 1],
    &[0, 4, 1, 8, 2, 5, 4],
    &[1, 9, 4, 5, 8, 9, 2],
    &[2, 3, 9, 2, 8, 3, 10],
    &[4, 1, 10, 9, 8, 2],
    &[0, 6, 3, 1, 8, 6, 2],
    &[10, 2, 10, 3, 8, 10, 4],
    &[1, 9, 3, 9, 8, 6, 9],
    &[6, 10, 1, 3, 8, 9],
    &[0, 2, 1, 7, 8, 2, 5],
    &[1, 5, 3, 7, 8, 4, 2],
    &[2, 3, 5, 9, 8, 7, 5],
    &[1, 6, 7, 10, 8, 3],
    &[0, 4, 6, 2, 8, 4, 7],
    &[4, 1, 9, 4, 8, 1, 6],
    &[3, 7, 3, 9, 8, 5, 2],
    &[6, 4, 10, 7, 8, 1],
    &[1, 9, 5, 3, 8, 9, 5],
    &[3, 2, 9, 3, 8, 7, 9],
    &[5, 4, 9, 5, 8, 3, 10],
    &[3, 9, 10, 5, 8, 2],
    &[0, 6, 5, 10, 8, 6, 2],
    &[6, 7, 4, 6, 8, 3, 10],
    &[5, 10, 9, 6, 8, 4, 7],
    &[6, 10, 4, 7, 8, 9],
    &[4, 6, 10, 7, 8, 9, 3, 4],
    &[9, 4, 6, 9, 8, 10, 7, 3],
    &[7, 10, 3, 6, 8, 4, 9, 10],
    &[6, 10, 9, 7, 8, 3, 4, 10],
];
fn tile(l: &mut Level, x: i32, y: i32, w: i32, h: i32, kind: u8, content: u8) {
    l.tiles.push(Tile {
        r: Rect::new(x, y, w, h),
        kind,
        content,
    });
}
fn coin(l: &mut Level, x: i32, y: i32, kind: u8) {
    l.pickups.push(Pickup {
        r: Rect::new(x, y, 16, 20),
        kind,
        active: true,
    });
}
fn enemy(l: &mut Level, x: i32, y: i32, kind: u8, phase: u32) {
    let (w, h) = if kind == 4 {
        (58, 56)
    } else if kind == 3 {
        (28, 56)
    } else {
        (28, 26)
    };
    l.enemies.push(Enemy {
        r: Rect::new(x, y, w, h),
        kind,
        dir: -1,
        vy: 0,
        phase,
        active: true,
        shell: false,
        rolling: false,
        hp: 1,
        immunity: 0,
    });
}
fn moving(l: &mut Level, x: i32, y: i32, vertical: bool, span: i32, phase: u32) {
    let r = Rect::new(x, y, 80, 14);
    l.platforms.push(Platform {
        r,
        home: Point::new(x, y),
        span,
        vertical,
        phase,
        previous: r,
    });
}
pub fn build(course: usize) -> Level {
    let route = ROUTES[course];
    let world = course / 4;
    let boss = course % 4 == 3;
    let width = 320 + route.len() as i32 * 384 + if boss { BOSS_COURT } else { 352 };
    let mut l = Level {
        width,
        tiles: Vec::new(),
        pickups: Vec::new(),
        enemies: Vec::new(),
        platforms: Vec::new(),
        hazards: Vec::new(),
        exit: Rect::new(width - 78, 272, 48, 96),
        checkpoint: if boss {
            width - BOSS_COURT + 16
        } else {
            320 + (route.len() / 2) as i32 * 384
        },
        secret: Rect::default(),
        secret_return: Rect::new(width + 800, 320, 40, 48),
        secret_used: false,
    };
    tile(&mut l, 0, FLOOR, 320, 96, 0, 0);
    for x in [170, 206, 242] {
        coin(&mut l, x, 334, 0);
    }
    for (n, section) in route.iter().enumerate() {
        let x = 320 + n as i32 * 384;
        let gap = match section {
            2 | 7 => Some((128, 96)),
            3 => Some((96, 192)),
            _ => None,
        };
        if let Some((start, size)) = gap {
            tile(&mut l, x, FLOOR, start, 96, 0, 0);
            tile(
                &mut l,
                x + start + size,
                FLOOR,
                384 - start - size,
                96,
                0,
                0,
            );
            for k in 0..5 {
                coin(&mut l, x + 90 + k * 38, 260 + (k - 2).abs() * 16, 0);
            }
        } else {
            tile(&mut l, x, FLOOR, 384, 96, 0, 0);
        }
        match section {
            0 => {
                tile(&mut l, x + 128, 260, 32, 32, 3, 1);
                tile(&mut l, x + 160, 260, 32, 32, 2, 0);
                tile(&mut l, x + 192, 260, 32, 32, 3, 0);
                enemy(
                    &mut l,
                    x + 300,
                    342,
                    if world > 1 { 1 } else { 0 },
                    n as u32 * 37,
                );
                for k in 0..3 {
                    coin(&mut l, x + 126 + k * 36, 220, 0);
                }
            }
            1 => {
                tile(&mut l, x + 96, 336, 64, 32, 1, 0);
                tile(&mut l, x + 160, 304, 64, 64, 1, 0);
                tile(&mut l, x + 224, 272, 64, 96, 1, 0);
                for k in 0..4 {
                    coin(&mut l, x + 100 + k * 48, 290 - k * 22, 0);
                }
                if world > 0 {
                    enemy(&mut l, x + 305, 342, 1, n as u32 * 21);
                }
            }
            2 => {
                enemy(&mut l, x + 302, 342, 0, n as u32 * 31);
                tile(
                    &mut l,
                    x + 32,
                    250,
                    32,
                    32,
                    3,
                    if n % 3 == 0 { 2 } else { 0 },
                );
            }
            3 => {
                moving(&mut l, x + 96, 316, false, 144, n as u32 * 31);
                tile(&mut l, x + 16, 300, 64, 14, 1, 0);
                tile(&mut l, x + 304, 270, 64, 14, 1, 0);
            }
            4 => {
                for (dx, y) in [(64, 304), (176, 248), (288, 304)] {
                    tile(&mut l, x + dx, y, 64, FLOOR - y, 1, 0);
                    coin(&mut l, x + dx + 22, y - 32, 0);
                }
                if world > 0 {
                    enemy(&mut l, x + 190, 208, 2, n as u32 * 28);
                }
            }
            5 => {
                tile(&mut l, x + 48, 300, 96, 16, 1, 0);
                tile(&mut l, x + 160, 240, 96, 16, 1, 0);
                tile(&mut l, x + 288, 284, 80, 16, 1, 0);
                for k in 0..5 {
                    coin(&mut l, x + 80 + k * 52, 206, 0);
                }
                enemy(&mut l, x + 188, 204, 2, n as u32 * 29);
            }
            6 => {
                l.hazards.push(Rect::new(x + 112, 356, 64, 12));
                l.hazards.push(Rect::new(x + 248, 356, 48, 12));
                tile(&mut l, x + 92, 278, 104, 16, 1, 0);
                tile(&mut l, x + 232, 240, 80, 16, 1, 0);
                tile(&mut l, x + 32, 258, 32, 32, 3, 1);
                enemy(&mut l, x + 310, 342, 0, n as u32 * 43);
            }
            7 => {
                tile(&mut l, x + 64, 304, 32, 64, 1, 0);
                tile(&mut l, x + 272, 304, 32, 64, 1, 0);
                if world > 3 {
                    l.hazards.push(Rect::new(x + 320, 356, 32, 12));
                }
                tile(&mut l, x + 240, 212, 32, 32, 3, 3);
            }
            8 => {
                l.secret = Rect::new(x + 84, 320, 48, 48);
                tile(&mut l, x + 192, 270, 32, 32, 3, 1);
                tile(&mut l, x + 224, 270, 32, 32, 2, 0);
                tile(&mut l, x + 256, 270, 32, 32, 3, 2);
                tile(&mut l, x + 208, 174, 32, 32, 3, 4);
                for k in 0..4 {
                    coin(&mut l, x + 186 + k * 32, 238, 0);
                }
            }
            9 => {
                moving(&mut l, x + 72, 338, true, 144, n as u32 * 37);
                moving(&mut l, x + 232, 308, true, 96, 120 + n as u32 * 13);
                tile(&mut l, x + 176, 192, 64, 16, 1, 0);
                for k in 0..3 {
                    coin(&mut l, x + 192, 158 + k * 42, 0);
                }
            }
            10 => {
                enemy(&mut l, x + 110, 342, 1, n as u32 * 19);
                enemy(&mut l, x + 240, 342, 0, n as u32 * 23);
                if world > 1 {
                    enemy(&mut l, x + 316, FLOOR - 56, 3, n as u32 * 41);
                }
                tile(
                    &mut l,
                    x + 176,
                    258,
                    32,
                    32,
                    3,
                    if world > 3 { 3 } else { 1 },
                );
                tile(&mut l, x + 208, 258, 32, 32, 2, 0);
            }
            _ => unreachable!(),
        }
        // Each stage has exactly three optional moon shards, in reachable upper routes.
        if n == 1 || n == 3 || n == route.len() - 1 {
            let sx = x + 246;
            let sy = if *section == 1 || *section == 4 {
                216
            } else {
                174
            };
            if sy == 174 {
                tile(&mut l, x + 210, 258, 80, 12, 1, 0);
            }
            coin(&mut l, sx, sy, 5);
        }
    }
    let end = 320 + route.len() as i32 * 384;
    tile(&mut l, end, FLOOR, width - end, 96, 0, 0);
    if boss {
        // Clear, roomy bell courts. Side terraces offer readable escape routes,
        // while the uninterrupted floor lets every guardian finish its charge.
        tile(&mut l, end + 138, 280, 76, 16, 1, 0);
        tile(&mut l, width - 310, 280, 76, 16, 1, 0);
        coin(&mut l, end + 78, 334, 1);
        tile(&mut l, end + 16, 246, 32, 32, 3, 2);
        enemy(&mut l, end + 540, 312, 4, 0);
        let guardian = l.enemies.last_mut().unwrap();
        guardian.hp = 3 + u8::from(world >= 3) + u8::from(world >= 6) + u8::from(world == 7);
        if world == 7 {
            guardian.r = Rect::new(end + 540, FLOOR - 72, 72, 72);
        }
    } else {
        for k in 0..4 {
            tile(
                &mut l,
                end + 32 + k * 32,
                FLOOR - 32 * (k + 1),
                32,
                32 * (k + 1),
                1,
                0,
            );
        }
    }
    // A separate, camera-bounded secret grotto, reached only via the hollow root.
    tile(&mut l, width + 128, FLOOR, 736, 96, 0, 0);
    tile(&mut l, width + 96, 80, 32, 384, 1, 0);
    tile(&mut l, width + 864, 80, 32, 384, 1, 0);
    tile(&mut l, width + 128, 80, 736, 32, 1, 0);
    for row in 0..3 {
        for col in 0..7 {
            coin(&mut l, width + 280 + col * 52, 208 + row * 44, 0);
        }
    }
    tile(&mut l, width + 600, 306, 64, 16, 1, 0);
    coin(&mut l, width + 620, 264, 4);
    l
}

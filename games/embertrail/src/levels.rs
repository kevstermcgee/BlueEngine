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
pub const GROTTO_NAMES: [&str; 8] = [
    "Root Stepwell",
    "Echo Waterway",
    "Copper Pumpworks",
    "Frostbloom Warmwell",
    "Sunglass Reliquary",
    "Cloudseed Cistern",
    "Briar Nursery",
    "Citadel Undercroft",
];
pub fn grotto_meta(course: usize, width: i32) -> Grotto {
    let world = course / 4;
    let room_width = [928, 1120, 960, 1056, 1184, 1024, 1152, 1248][world];
    let x = width + 128;
    let reward_floor = [208, 184, 208, 176, 176, 224, 208, 144][world];
    Grotto {
        bounds: Rect::new(x, 40, room_width, 424),
        entrance: Point::new(x + 96, 338),
        reward: Rect::new(x + room_width - 220, reward_floor - 36, 20, 24),
        archetype: world as u8,
        variant: (course % 4) as u8,
    }
}
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
fn spring(l: &mut Level, x: i32) {
    // Non-solid, floor-flush contact pad. Its visible coil sits above this trigger.
    tile(l, x, FLOOR - 4, 44, 8, 5, 0);
}
fn build_grotto(l: &mut Level, course: usize) {
    let room = l.grotto.clone();
    let x = room.bounds.x;
    let w = room.bounds.w;
    let v = i32::from(room.variant);
    let shift = v * 20;
    let phase = course as u32 * 37;
    tile(l, x - 24, 40, 24, 424, 1, 0);
    tile(l, x + w, 40, 24, 424, 1, 0);
    tile(l, x, 40, w, 16, 1, 0);
    if room.archetype == 5 {
        // The cistern's two short gaps have an ordinary jump route as well as springs/lifts.
        tile(l, x, FLOOR, 320, 96, 0, 0);
        tile(l, x + 432, FLOOR, 184, 96, 0, 0);
        tile(l, x + 744, FLOOR, w - 744, 96, 0, 0);
    } else {
        tile(l, x, FLOOR, w, 96, 0, 0);
    }
    match room.archetype {
        0 => {
            tile(l, x + 180 + shift, 304, 88, 16, 1, 0);
            tile(l, x + 336 + shift / 2, 256 - v * 8, 88, 16, 1, 0);
            moving(l, x + 500 - shift / 2, 332, true, 140 + v * 8, phase);
            tile(l, x + w - 260, 208, 116, 16, 1, 0);
            if v % 2 == 1 {
                tile(l, x + 284, 202, 64, 12, 1, 0);
            }
        }
        1 => {
            l.hazards.push(Rect::new(x + 274 + shift, 356, 96, 12));
            l.hazards.push(Rect::new(x + 610 - shift / 2, 356, 112, 12));
            tile(l, x + 202 + shift / 2, 286, 100, 16, 1, 0);
            tile(l, x + 382 + shift, 230, 88, 16, 1, 0);
            moving(l, x + 490, 272 - v * 8, false, 184, phase);
            moving(l, x + 740 - shift, 324, true, 160, phase + 90);
            tile(l, x + w - 260, 184, 116, 16, 1, 0);
        }
        2 => {
            l.hazards.push(Rect::new(x + 302 + shift / 2, 356, 104, 12));
            tile(l, x + 184, 294 - v * 8, 96, 16, 1, 0);
            moving(l, x + 340 + shift, 252, false, 128, phase);
            moving(l, x + 558 - shift / 2, 330, true, 136 + v * 12, phase + 100);
            tile(l, x + w - 260, 208, 116, 16, 1, 0);
            enemy(l, x + 570 + shift, 342, 1, phase);
        }
        3 => {
            spring(l, x + 226 + shift);
            spring(l, x + 542 - shift / 2);
            tile(l, x + 352 + shift, 216 - v * 8, 96, 16, 1, 0);
            tile(l, x + 608 + shift / 2, 176, 92, 16, 1, 0);
            tile(l, x + w - 260, 176, 116, 16, 1, 0);
            moving(l, x + 394, 322, false, 128 + shift, phase);
        }
        4 => {
            for (dx, y) in [(176, 304), (326, 248), (474, 192)] {
                tile(l, x + dx + shift / 2, y - (v % 2) * 8, 80, 16, 1, 0);
            }
            moving(l, x + 596 - shift / 2, 222, false, 176, phase);
            tile(l, x + 830 + shift / 2, 236, 88, 16, 1, 0);
            tile(l, x + w - 260, 176, 116, 16, 1, 0);
            enemy(l, x + 700 + shift, 342, 1, phase);
        }
        5 => {
            spring(l, x + 246 + shift / 2);
            spring(l, x + 534 - shift / 2);
            moving(l, x + 424, 330, true, 140 + v * 8, phase);
            moving(l, x + 632, 290 - v * 10, false, 150, phase + 80);
            tile(l, x + 548 + shift, 190, 88, 16, 1, 0);
            tile(l, x + w - 260, 224, 116, 16, 1, 0);
        }
        6 => {
            l.hazards.push(Rect::new(x + 278 + shift, 356, 48, 12));
            l.hazards.push(Rect::new(x + 672 - shift, 356, 64, 12));
            tile(l, x + 200 + shift / 2, 286, 110, 16, 1, 0);
            tile(l, x + 390 + shift, 218, 104, 16, 1, 0);
            moving(l, x + 584, 286, false, 148 + v * 8, phase);
            tile(l, x + w - 260, 208, 116, 16, 1, 0);
            enemy(l, x + 510 - shift / 2, FLOOR - 56, 3, phase);
            enemy(l, x + 800 + shift, 342, 0, phase + 60);
        }
        7 => {
            moving(l, x + 216 + shift / 2, 334, true, 126 + v * 8, phase);
            spring(l, x + 426 + shift);
            moving(l, x + 560 - shift / 2, 304, true, 144, phase + 80);
            l.hazards.push(Rect::new(x + 690 + shift / 2, 356, 64, 12));
            tile(l, x + 754, 224 - v * 8, 96, 16, 1, 0);
            moving(l, x + 884 - shift / 2, 284, true, 120 + v * 8, phase + 150);
            tile(l, x + w - 260, 144, 116, 16, 1, 0);
            enemy(l, x + 866 + shift, 342, 1, phase);
        }
        _ => unreachable!(),
    }
    l.secret_return = Rect::new(x + w - 82, 320, 48, 48);
    l.pickups.push(Pickup {
        r: room.reward,
        kind: 4,
        active: true,
    });
    // Coins trace the climb, while a few floor sparks lead toward the far stairs.
    for n in 0..6 {
        coin(l, x + 160 + n * 112 + (v % 2) * 16, 292 - (n % 3) * 38, 0);
    }
    coin(l, room.reward.x - 36, room.reward.y + 4, 0);
    coin(l, room.reward.x + 36, room.reward.y + 4, 0);
    if room.archetype >= 1 {
        coin(l, x + 146, 334, 1);
    }
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
        grotto: grotto_meta(course, width),
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
    build_grotto(&mut l, course);
    l
}

//! Rendering-free screw geometry. Integer poses and fixed-point continuous sweep bounds.
use crate::sweep_table::SIN;
use serde::{Deserialize, Serialize};
pub const LOCAL: [[i32; 3]; 5] = [[0, 0, 0], [1, 0, 0], [2, 0, 0], [0, 1, 0], [0, 0, 1]];
pub const Q: i64 = 1_048_576;
pub const SAMPLES: i32 = 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pose {
    pub center: [i32; 3],
    pub basis: [[i32; 3]; 3],
}
impl Default for Pose {
    fn default() -> Self {
        Self {
            center: [0, 0, 0],
            basis: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
        }
    }
}
fn rotate(mut v: [i32; 3], axis: usize, sign: i32) -> [i32; 3] {
    let a = (axis + 1) % 3;
    let b = (axis + 2) % 3;
    let old = v[a];
    v[a] = -sign * v[b];
    v[b] = sign * old;
    v
}
impl Pose {
    pub fn offset(self, p: [i32; 3]) -> [i32; 3] {
        std::array::from_fn(|j| (0..3).map(|k| p[k] * self.basis[k][j]).sum())
    }
    pub fn cells(self) -> [[i32; 3]; 5] {
        LOCAL.map(|p| {
            let v = self.offset(p);
            std::array::from_fn(|j| self.center[j] + v[j])
        })
    }
    pub fn screw(self, axis: usize, sign: i32) -> Self {
        let mut p = self;
        p.center[axis] += sign;
        p.basis = p.basis.map(|v| rotate(v, axis, sign));
        p
    }
    pub fn valid(self) -> bool {
        let dot = |a: [i32; 3], b: [i32; 3]| (0..3).map(|j| a[j] * b[j]).sum::<i32>();
        self.center.iter().all(|n| (-4..=4).contains(n))
            && self
                .basis
                .iter()
                .all(|v| v.iter().all(|n| (-1..=1).contains(n)) && dot(*v, *v) == 1)
            && dot(self.basis[0], self.basis[1]) == 0
            && dot(self.basis[1], self.basis[2]) == 0
            && dot(self.basis[0], self.basis[2]) == 0
            && self.basis[0][0]
                * (self.basis[1][1] * self.basis[2][2] - self.basis[1][2] * self.basis[2][1])
                - self.basis[0][1]
                    * (self.basis[1][0] * self.basis[2][2] - self.basis[1][2] * self.basis[2][0])
                + self.basis[0][2]
                    * (self.basis[1][0] * self.basis[2][1] - self.basis[1][1] * self.basis[2][0])
                == 1
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub center: [i32; 3],
    pub size: [i32; 3],
}
impl Block {
    pub const fn new(center: [i32; 3], size: [i32; 3]) -> Self {
        Self { center, size }
    }
}
pub const WALLS: [Block; 6] = [
    Block::new([-9, 0, 0], [9, 24, 24]),
    Block::new([9, 0, 0], [9, 24, 24]),
    Block::new([0, -8, 0], [24, 9, 24]),
    Block::new([0, 9, 0], [24, 9, 24]),
    Block::new([0, 0, -9], [24, 24, 9]),
    Block::new([0, 0, 9], [24, 24, 9]),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Obstruction {
    pub block: usize,
    pub sample: i32,
    pub cube: usize,
}
// SAT at an interval midpoint, padded by a bound on ALL vertex motion within it.
// A vertex is <= 2.599 grid units from the screw axis. Quarter-turn speed plus
// axial translation is < 5.1 units/step; 6 is a conservative Lipschitz bound.
// Recursion certifies clear intervals, rejects actual overlaps and conservatively
// rejects sub-1/1024 intervals with <0.003 grid units unresolved clearance.
fn overlap(
    pose: Pose,
    offset: [i32; 3],
    axis: usize,
    sign: i32,
    t: i32,
    padding: [i64; 2],
    block: Block,
) -> bool {
    let [pad, axial_pad] = padding;
    let a = (axis + 1) % 3;
    let b = (axis + 2) % 3;
    let s = SIN[t as usize] as i64 * sign as i64;
    let c = SIN[(SAMPLES - t) as usize] as i64;
    let mut pos = pose.center.map(|n| n as i64 * Q);
    pos[axis] += offset[axis] as i64 * Q + sign as i64 * t as i64 * Q / SAMPLES as i64;
    pos[a] += c * offset[a] as i64 - s * offset[b] as i64;
    pos[b] += s * offset[a] as i64 + c * offset[b] as i64;
    let delta: [i64; 3] = std::array::from_fn(|j| pos[j] - block.center[j] as i64 * Q);
    let h = block.size.map(|n| n as i64 * Q / 2);
    if delta[axis].abs() >= h[axis] + Q / 2 + axial_pad {
        return false;
    }
    let extent = (c.abs() + s.abs()) / 2;
    if delta[a].abs() >= h[a] + extent + pad || delta[b].abs() >= h[b] + extent + pad {
        return false;
    }
    for (u, v) in [(c, s), (-s, c)] {
        let distance = (delta[a] * u + delta[b] * v).abs();
        let radius = Q * Q / 2 + h[a] * u.abs() + h[b] * v.abs() + pad * (u.abs() + v.abs());
        if distance >= radius {
            return false;
        }
    }
    true
}
fn interval(
    p: Pose,
    off: [i32; 3],
    axis: usize,
    sign: i32,
    block: Block,
    lo: i32,
    hi: i32,
) -> Option<i32> {
    let mid = (lo + hi) / 2;
    let pad = 3 * Q * (hi - lo) as i64 / SAMPLES as i64 + 4; // LUT rounding and integer division
    let axial_pad = (hi - mid).max(mid - lo) as i64 * Q / SAMPLES as i64;
    if !overlap(p, off, axis, sign, mid, [pad, axial_pad], block) {
        return None;
    }
    if hi - lo <= 1 {
        return Some(lo);
    }
    interval(p, off, axis, sign, block, lo, mid)
        .or_else(|| interval(p, off, axis, sign, block, mid, hi))
}
pub fn sweep(pose: Pose, axis: usize, sign: i32, blocks: &[Block]) -> Option<Obstruction> {
    let mut first = None;
    for (index, block) in blocks.iter().chain(WALLS.iter()).enumerate() {
        for (cube, off) in LOCAL.map(|p| pose.offset(p)).iter().enumerate() {
            if let Some(sample) = interval(pose, *off, axis, sign, *block, 0, SAMPLES) {
                let hit = Obstruction {
                    block: index,
                    sample,
                    cube,
                };
                if first.is_none_or(|f: Obstruction| sample < f.sample) {
                    first = Some(hit);
                }
            }
        }
    }
    first
}
pub fn pose_clear(pose: Pose, blocks: &[Block]) -> bool {
    LOCAL.iter().all(|p| {
        blocks
            .iter()
            .chain(WALLS.iter())
            .all(|b| !overlap(pose, pose.offset(*p), 0, 1, 0, [0, 0], *b))
    })
}
pub const ROUTES: [&[(usize, i32)]; 6] = [
    &[(0, 1)],
    &[(0, 1), (1, 1)],
    &[(2, -1), (0, 1), (2, 1)],
    &[(1, 1), (2, 1), (0, 1), (1, -1)],
    &[(0, 1), (1, 1), (0, -1), (1, -1)],
    &[
        (1, 1),
        (1, 1),
        (2, 1),
        (0, -1),
        (1, -1),
        (0, 1),
        (2, -1),
        (1, -1),
    ],
];
pub const NAMES: [&str; 6] = [
    "THE FIRST THREAD",
    "ORDER MATTERS",
    "UNDER THE BEAM",
    "THE HIGH ROAD",
    "THE CLOSED LOOP",
    "THE CORKSCREW VAULT",
];
pub const LESSONS: [&str; 6] = [
    "One turn. One unit. Match the gold socket.",
    "X then Y differs from Y then X. Try both orders.",
    "A safe endpoint can hide a collision during the turn.",
    "Rise above the columns, then return to socket height.",
    "Return to the starting center with a different orientation.",
    "Climb into the upper passage. Reorient. Come home.",
];
pub fn goal(chamber: usize) -> Pose {
    ROUTES[chamber]
        .iter()
        .fold(Pose::default(), |p, (a, s)| p.screw(*a, *s))
}
pub fn blocks(chamber: usize) -> Vec<Block> {
    match chamber {
        0 => vec![],
        1 => vec![Block::new([-2, 1, -1], [1, 3, 1])],
        2 => vec![Block::new([0, 2, 1], [3, 1, 1])],
        3 => vec![
            Block::new([0, -2, -1], [1, 1, 1]),
            Block::new([-1, -1, -2], [1, 1, 1]),
            Block::new([-2, -1, 0], [1, 1, 1]),
            Block::new([3, -1, -2], [1, 3, 1]),
        ],
        4 => vec![Block::new([-1, -1, -2], [1, 3, 1])],
        _ => vec![
            Block::new([0, -2, -1], [1, 1, 1]),
            Block::new([-1, -1, -2], [1, 1, 1]),
            Block::new([-2, -1, 0], [1, 1, 1]),
            Block::new([3, -1, -2], [1, 3, 1]),
            Block::new([0, 3, -2], [3, 1, 1]),
        ],
    }
}

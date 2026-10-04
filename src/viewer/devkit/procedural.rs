//! Endless-world building blocks: seeded chunk identity, bounded streaming, local coordinates and day time.
//! No geometry, biome rules, devices or wall clock. See docs/PROCEDURAL_WORLDS.md and Leo.
use super::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ChunkId {
    pub x: i64,
    pub z: i64,
}
impl ChunkId {
    pub fn offset(self, x: i64, z: i64) -> Result<Self, String> {
        Ok(Self {
            x: self.x.checked_add(x).ok_or("chunk x overflow")?,
            z: self.z.checked_add(z).ok_or("chunk z overflow")?,
        })
    }
    /// Stable across visiting order and cache eviction. Salt separates independent content streams.
    pub fn seed(self, world_seed: u64, salt: u64) -> u64 {
        fn mix(mut n: u64) -> u64 {
            n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            n ^ (n >> 31)
        }
        mix(world_seed
            ^ mix(self.x as u64)
            ^ mix((self.z as u64).wrapping_add(0x9e37_79b9_7f4a_7c15))
            ^ mix(salt))
    }
    pub fn rng(self, world_seed: u64, salt: u64) -> Rng {
        Rng::new(self.seed(world_seed, salt))
    }
}

/// A global position without converting a huge global coordinate to f32. Local metres in [0, size).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldPoint {
    pub chunk: ChunkId,
    pub local: [f32; 2],
}
fn valid_size(size: f32) -> Result<(), String> {
    if !size.is_finite() || !(1. ..=4096.).contains(&size) {
        return Err("chunk size must be finite, 1..4096 metres".into());
    }
    Ok(())
}
impl WorldPoint {
    /// Normalize after a bounded local move. Teleports set integer chunk identity directly.
    pub fn new(mut chunk: ChunkId, mut local: [f32; 2], size: f32) -> Result<Self, String> {
        valid_size(size)?;
        for (axis, value) in local.iter_mut().enumerate() {
            if !value.is_finite() || value.abs() > size * 1_000_000. {
                return Err("local position must be finite and within one million chunks".into());
            }
            let carry = (f64::from(*value) / f64::from(size)).floor() as i64;
            let mut rest = (f64::from(*value) - carry as f64 * f64::from(size)) as f32;
            // Rounding a tiny negative position can produce size exactly.
            let extra = i64::from(rest >= size);
            if extra != 0 {
                rest = 0.;
            }
            chunk = if axis == 0 {
                chunk.offset(carry + extra, 0)?
            } else {
                chunk.offset(0, carry + extra)?
            };
            *value = rest;
        }
        Ok(Self { chunk, local })
    }
    /// Rendering/collision coordinates near an integer origin; distant conversions fail explicitly.
    pub fn relative(self, origin: ChunkId, size: f32) -> Result<[f32; 2], String> {
        valid_size(size)?;
        if self
            .local
            .iter()
            .any(|v| !v.is_finite() || !(0. ..size).contains(v))
        {
            return Err("world point is not normalized".into());
        }
        let dx = self
            .chunk
            .x
            .checked_sub(origin.x)
            .ok_or("relative x overflow")?;
        let dz = self
            .chunk
            .z
            .checked_sub(origin.z)
            .ok_or("relative z overflow")?;
        if dx.unsigned_abs() > 64 || dz.unsigned_abs() > 64 {
            return Err("choose a nearby origin (at most 64 chunks away)".into());
        }
        Ok([
            dx as f32 * size + self.local[0],
            dz as f32 * size + self.local[1],
        ])
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChunkChanges {
    pub added: Vec<ChunkId>,
    pub removed: Vec<ChunkId>,
}
/// Deterministic bounded square cache, at most 289 chunks. Stationary updates allocate nothing.
pub struct ChunkCache<T> {
    radius: i64,
    center: Option<ChunkId>,
    chunks: BTreeMap<ChunkId, T>,
}
impl<T> ChunkCache<T> {
    pub fn new(radius: u8) -> Result<Self, String> {
        if radius > 8 {
            return Err("stream radius must be 0..8 chunks".into());
        }
        Ok(Self {
            radius: i64::from(radius),
            center: None,
            chunks: BTreeMap::new(),
        })
    }
    pub fn chunks(&self) -> &BTreeMap<ChunkId, T> {
        &self.chunks
    }
    pub fn center(&self) -> Option<ChunkId> {
        self.center
    }
    /// Generate missing chunks before committing: overflow or generation failure leaves the old cache intact.
    pub fn update(
        &mut self,
        center: ChunkId,
        mut generate: impl FnMut(ChunkId) -> Result<T, String>,
    ) -> Result<ChunkChanges, String> {
        if self.center == Some(center) {
            return Ok(ChunkChanges::default());
        }
        let mut wanted = BTreeSet::new();
        for z in -self.radius..=self.radius {
            for x in -self.radius..=self.radius {
                wanted.insert(center.offset(x, z)?);
            }
        }
        let mut additions = Vec::new();
        for &id in &wanted {
            if !self.chunks.contains_key(&id) {
                additions.push((id, generate(id)?));
            }
        }
        let changes = ChunkChanges {
            added: additions.iter().map(|(id, _)| *id).collect(),
            removed: self
                .chunks
                .keys()
                .filter(|id| !wanted.contains(id))
                .copied()
                .collect(),
        };
        self.chunks.retain(|id, _| wanted.contains(id));
        self.chunks.extend(additions);
        self.center = Some(center);
        Ok(changes)
    }
}

/// Seam-continuous smooth value noise at a scale of 1..64 chunks. Integer lattice avoids far-world precision loss.
pub fn field(seed: u64, point: WorldPoint, size: f32, scale: u16) -> Result<f32, String> {
    point.relative(point.chunk, size)?;
    if !(1..=64).contains(&scale) {
        return Err("field scale must be 1..64 chunks".into());
    }
    let scale = i64::from(scale);
    let cell = ChunkId {
        x: point.chunk.x.div_euclid(scale),
        z: point.chunk.z.div_euclid(scale),
    };
    let fraction = |n: i64, local: f32| {
        let t = (n.rem_euclid(scale) as f32 + local / size) / scale as f32;
        t * t * (3. - 2. * t)
    };
    let x = fraction(point.chunk.x, point.local[0]);
    let z = fraction(point.chunk.z, point.local[1]);
    let value = |id: ChunkId| (id.seed(seed, 0) >> 40) as f32 / (1u64 << 24) as f32;
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    Ok(lerp(
        lerp(value(cell), value(cell.offset(1, 0)?), x),
        lerp(value(cell.offset(0, 1)?), value(cell.offset(1, 1)?), x),
        z,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayCycle {
    ticks_per_day: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayTime {
    /// Completed cycles; the player-facing ordinal is days + 1.
    pub days: u64,
    /// [0,1): midnight=0, sunrise=0.25, noon=0.5, sunset=0.75.
    pub phase: f32,
}
impl DayCycle {
    pub fn new(ticks_per_day: u32) -> Result<Self, String> {
        if ticks_per_day == 0 {
            return Err("day cycle needs at least one tick".into());
        }
        Ok(Self { ticks_per_day })
    }
    pub fn at(self, tick: u64) -> DayTime {
        let duration = u64::from(self.ticks_per_day);
        let phase = (tick % duration) as f64 / f64::from(self.ticks_per_day);
        DayTime {
            days: tick / duration,
            phase: (phase as f32).min(f32::from_bits(1f32.to_bits() - 1)),
        }
    }
}

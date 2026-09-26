use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(from = "[f32; 3]", into = "[f32; 3]")]
pub struct V(pub f32, pub f32, pub f32);
impl From<[f32; 3]> for V {
    fn from(a: [f32; 3]) -> Self {
        Self(a[0], a[1], a[2])
    }
}
impl From<V> for [f32; 3] {
    fn from(v: V) -> Self {
        [v.0, v.1, v.2]
    }
}
impl Add for V {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self(self.0 + b.0, self.1 + b.1, self.2 + b.2)
    }
}
impl Sub for V {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }
}
impl Mul<f32> for V {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        Self(self.0 * b, self.1 * b, self.2 * b)
    }
}
impl Mul for V {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        Self(self.0 * b.0, self.1 * b.1, self.2 * b.2)
    }
}
impl Div<f32> for V {
    type Output = Self;
    fn div(self, b: f32) -> Self {
        self * (1.0 / b)
    }
}
impl Neg for V {
    type Output = Self;
    fn neg(self) -> Self {
        -1.0 * self
    }
}
impl Mul<V> for f32 {
    type Output = V;
    fn mul(self, b: V) -> V {
        b * self
    }
}
impl V {
    pub const ZERO: Self = Self(0., 0., 0.);
    pub const ONE: Self = Self(1., 1., 1.);
    pub fn dot(self, b: Self) -> f32 {
        self.0 * b.0 + self.1 * b.1 + self.2 * b.2
    }
    pub fn cross(self, b: Self) -> Self {
        Self(
            self.1 * b.2 - self.2 * b.1,
            self.2 * b.0 - self.0 * b.2,
            self.0 * b.1 - self.1 * b.0,
        )
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn norm(self) -> Self {
        self / self.length().max(1e-12)
    }
    /// Return a unit-length vector, or the zero vector when `self` is zero.
    ///
    /// This is the conventional-name alias for [`Self::norm`].
    pub fn normalize(self) -> Self {
        self.norm()
    }
    pub fn min(self, b: Self) -> Self {
        Self(self.0.min(b.0), self.1.min(b.1), self.2.min(b.2))
    }
    pub fn max(self, b: Self) -> Self {
        Self(self.0.max(b.0), self.1.max(b.1), self.2.max(b.2))
    }
    pub fn axis(self, i: usize) -> f32 {
        [self.0, self.1, self.2][i]
    }
    pub fn lerp(self, b: Self, t: f32) -> Self {
        self * (1. - t) + b * t
    }
    pub fn finite(self) -> bool {
        self.0.is_finite() && self.1.is_finite() && self.2.is_finite()
    }
    pub fn reflect(self, n: Self) -> Self {
        self - n * (2. * self.dot(n))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Mat {
    pub x: V,
    pub y: V,
    pub z: V,
    pub p: V,
}
impl Mat {
    pub fn identity() -> Self {
        Self {
            x: V(1., 0., 0.),
            y: V(0., 1., 0.),
            z: V(0., 0., 1.),
            p: V::ZERO,
        }
    }
    pub fn vector(self, v: V) -> V {
        self.x * v.0 + self.y * v.1 + self.z * v.2
    }
    pub fn point(self, v: V) -> V {
        self.vector(v) + self.p
    }
    pub fn compose(self, b: Self) -> Self {
        Self {
            x: self.vector(b.x),
            y: self.vector(b.y),
            z: self.vector(b.z),
            p: self.point(b.p),
        }
    }
    pub fn trs(p: V, r: V, s: V) -> Self {
        let (sx, cx) = r.0.to_radians().sin_cos();
        let (sy, cy) = r.1.to_radians().sin_cos();
        let (sz, cz) = r.2.to_radians().sin_cos();
        Self {
            x: V(cz * cy, sz * cy, -sy) * s.0,
            y: V(cz * sy * sx - sz * cx, sz * sy * sx + cz * cx, cy * sx) * s.1,
            z: V(cz * sy * cx + sz * sx, sz * sy * cx - cz * sx, cy * cx) * s.2,
            p,
        }
    }
    pub fn inverse(self) -> Self {
        let a = self.y.cross(self.z);
        let b = self.z.cross(self.x);
        let c = self.x.cross(self.y);
        let d = self.x.dot(a);
        let mut m = Self {
            x: V(a.0, b.0, c.0) / d,
            y: V(a.1, b.1, c.1) / d,
            z: V(a.2, b.2, c.2) / d,
            p: V::ZERO,
        };
        m.p = -m.vector(self.p);
        m
    }
    pub fn normal_from_inverse(self, n: V) -> V {
        V(self.x.dot(n), self.y.dot(n), self.z.dot(n)).norm()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub o: V,
    pub d: V,
}
impl Ray {
    pub fn at(self, t: f32) -> V {
        self.o + self.d * t
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub lo: V,
    pub hi: V,
}
impl Bounds {
    pub fn empty() -> Self {
        Self {
            lo: V::ONE * f32::INFINITY,
            hi: V::ONE * f32::NEG_INFINITY,
        }
    }
    pub fn include(self, p: V) -> Self {
        Self {
            lo: self.lo.min(p),
            hi: self.hi.max(p),
        }
    }
    pub fn union(self, b: Self) -> Self {
        self.include(b.lo).include(b.hi)
    }
    pub fn hit(self, r: Ray, mut near: f32, mut far: f32) -> bool {
        for i in 0..3 {
            let d = r.d.axis(i);
            let o = r.o.axis(i);
            if d.abs() < 1e-12 {
                if o < self.lo.axis(i) || o > self.hi.axis(i) {
                    return false;
                }
                continue;
            }
            let a = (self.lo.axis(i) - o) / d;
            let b = (self.hi.axis(i) - o) / d;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if far < near {
                return false;
            }
        }
        true
    }
}

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}
pub fn random(seed: u32) -> f32 {
    (hash(seed) >> 8) as f32 / 16777216.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_affine() {
        let m = Mat::trs(V(2., 4., -3.), V(32., 74., 12.), V(2., 0.4, 3.));
        let p = V(5., -2., 0.2);
        assert!((m.inverse().point(m.point(p)) - p).length() < 1e-4);
    }
    #[test]
    fn normalize_is_zero_safe_norm_alias() {
        assert_eq!(V::ZERO.normalize(), V::ZERO);
        assert_eq!(V(3., 0., 4.).normalize(), V(3., 0., 4.).norm());
        assert!((V(3., 0., 4.).normalize().length() - 1.).abs() < 1e-6);
    }
    #[test]
    fn slab_parallel_boundary() {
        let b = Bounds {
            lo: V::ZERO,
            hi: V::ONE,
        };
        assert!(b.hit(
            Ray {
                o: V(0., 0.5, -1.),
                d: V(0., 0., 1.)
            },
            0.,
            100.
        ));
        assert!(!b.hit(
            Ray {
                o: V(-0.1, 0.5, -1.),
                d: V(0., 0., 1.)
            },
            0.,
            100.
        ));
    }
}

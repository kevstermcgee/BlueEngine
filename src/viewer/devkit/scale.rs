//! Size checks a headless agent can run: how big is this thing, in metres, next to a person?
//!
//! Procedural geometry fails quietly in scale. A logarithmic shell whose growth factor is a little
//! off comes out three metres across instead of ten centimetres, and nothing errors; only a person
//! looking at the window notices. The engine unit is one metre (`+Y` up), so a size in the code is a
//! size in the world, and this module turns that into assertions and readable comparisons:
//!
//! ```
//! use vesper3d::viewer::devkit::{Bounds, HUMAN_HEIGHT};
//! // The points of a hand-held shell, in metres.
//! let shell = Bounds::of([[0., 0., 0.], [0.11, 0.05, 0.07]]).unwrap();
//! shell.expect_longest("conch", 0.05..=0.30).unwrap(); // holdable in one hand
//! let giant = Bounds::of([[0., 0., 0.], [3.4, 1.5, 2.1]]).unwrap();
//! let err = giant.expect_longest("conch", 0.05..=0.30).unwrap_err();
//! assert!(err.contains("scale by about 0.09"), "{err}");
//! assert!(HUMAN_HEIGHT > 1.5);
//! ```
//!
//! The drawing half (wireframe boxes and a person-sized silhouette) is `kit::gizmo`.
use std::ops::RangeInclusive;

/// Height of an adult standing, in metres.
pub const HUMAN_HEIGHT: f32 = 1.75;
/// Height of a standing adult's eyes; the stock first-person camera sits near here.
pub const EYE_HEIGHT: f32 = 1.6;
/// A standard interior door (height, width).
pub const DOOR: (f32, f32) = (2.05, 0.9);
/// The top of a dining table.
pub const TABLE_HEIGHT: f32 = 0.75;
/// The longest an object stays comfortable in one hand.
pub const ONE_HAND_LONGEST: f32 = 0.30;

/// Things whose size everyone knows, smallest first, for [`describe_length`].
const REFERENCES: [(&str, f32); 8] = [
    ("a coin", 0.024),
    ("a phone", 0.15),
    ("a shoebox", 0.33),
    ("a table top's height", TABLE_HEIGHT),
    ("a door's width", DOOR.1),
    ("a person", HUMAN_HEIGHT),
    ("a door", DOOR.0),
    ("a car", 4.5),
];

/// An axis-aligned box in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// Smallest corner.
    pub min: [f32; 3],
    /// Largest corner.
    pub max: [f32; 3],
}

impl Bounds {
    /// The box around `points`; `None` when there are none or any coordinate is not finite (which is
    /// itself a bug worth surfacing).
    pub fn of(points: impl IntoIterator<Item = [f32; 3]>) -> Option<Self> {
        let mut bounds: Option<Bounds> = None;
        for p in points {
            if p.iter().any(|c| !c.is_finite()) {
                return None;
            }
            bounds = Some(match bounds {
                None => Bounds { min: p, max: p },
                Some(b) => Bounds {
                    min: [b.min[0].min(p[0]), b.min[1].min(p[1]), b.min[2].min(p[2])],
                    max: [b.max[0].max(p[0]), b.max[1].max(p[1]), b.max[2].max(p[2])],
                },
            });
        }
        bounds
    }
    /// Extent along each axis.
    pub fn size(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
    /// The middle of the box.
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
    /// The largest extent.
    pub fn longest(&self) -> f32 {
        self.size().into_iter().fold(0., f32::max)
    }
    /// The box around both.
    pub fn union(&self, other: &Bounds) -> Bounds {
        Bounds {
            min: [
                self.min[0].min(other.min[0]),
                self.min[1].min(other.min[1]),
                self.min[2].min(other.min[2]),
            ],
            max: [
                self.max[0].max(other.max[0]),
                self.max[1].max(other.max[1]),
                self.max[2].max(other.max[2]),
            ],
        }
    }
    /// One line an agent can read: `conch: 0.11 x 0.05 x 0.07 m, longest 0.11 m (about a phone)`.
    pub fn describe(&self, name: &str) -> String {
        let s = self.size();
        format!(
            "{name}: {:.2} x {:.2} x {:.2} m, longest {:.2} m ({})",
            s[0],
            s[1],
            s[2],
            self.longest(),
            describe_length(self.longest())
        )
    }
    /// Fails, with the correction, when the longest extent is outside `allowed` metres.
    pub fn expect_longest(&self, name: &str, allowed: RangeInclusive<f32>) -> Result<(), String> {
        let longest = self.longest();
        if allowed.contains(&longest) {
            return Ok(());
        }
        let target = if longest > *allowed.end() {
            *allowed.end()
        } else {
            *allowed.start()
        };
        Err(format!(
            "{}; expected {:.2}..{:.2} m: scale by about {:.2}",
            self.describe(name),
            allowed.start(),
            allowed.end(),
            target / longest.max(f32::MIN_POSITIVE)
        ))
    }
}

/// A length in metres as a comparison with something familiar: `about 1.0 x a person`.
pub fn describe_length(metres: f32) -> String {
    if !metres.is_finite() || metres <= 0. {
        return "no size".into();
    }
    let (name, size) = REFERENCES
        .iter()
        .copied()
        .min_by(|a, b| {
            (a.1.ln() - metres.ln())
                .abs()
                .total_cmp(&(b.1.ln() - metres.ln()).abs())
        })
        .unwrap_or(("a person", HUMAN_HEIGHT));
    format!("about {:.1} x {name}", metres / size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_cover_every_point_and_reject_garbage() {
        let b = Bounds::of([[1., 2., 3.], [-1., 5., 0.], [0., 0., 9.]]).unwrap();
        assert_eq!((b.min, b.max), ([-1., 0., 0.], [1., 5., 9.]));
        assert_eq!(b.size(), [2., 5., 9.]);
        assert_eq!(b.longest(), 9.);
        assert!(Bounds::of([]).is_none());
        assert!(Bounds::of([[0., f32::NAN, 0.]]).is_none());
    }

    #[test]
    fn a_metres_long_shell_is_reported_with_its_fix() {
        let shell = Bounds::of([[0., 0., 0.], [3., 1., 1.]]).unwrap();
        let err = shell.expect_longest("shell", 0.05..=0.30).unwrap_err();
        assert!(err.contains("scale by about 0.10"), "{err}");
        let tiny = Bounds::of([[0., 0., 0.], [0.005, 0.005, 0.005]]).unwrap();
        assert!(tiny
            .expect_longest("grain", 0.05..=0.30)
            .unwrap_err()
            .contains("scale by about 10.00"));
        assert!(Bounds::of([[0., 0., 0.], [0.1, 0.1, 0.1]])
            .unwrap()
            .expect_longest("ok", 0.05..=0.30)
            .is_ok());
    }

    #[test]
    fn lengths_compare_with_familiar_things() {
        assert!(describe_length(1.75).contains("a person"));
        assert!(describe_length(0.16).contains("a phone"));
        assert!(describe_length(f32::NAN).contains("no size"));
    }
}

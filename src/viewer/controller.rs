use super::profile::ControllerProfile;
use crate::math::{Ray, V};

pub const EYE_HEIGHT: f32 = 1.68;
pub const RADIUS: f32 = 0.23;
pub const STANDING_HEIGHT: f32 = 1.80;
pub const CROUCH_HEIGHT: f32 = 1.10;
pub const JUMP_HEIGHT: f32 = 0.35;
pub const WALK_SPEED: f32 = 3.2;
pub const SPRINT_SPEED: f32 = 5.6;
pub const CROUCH_SPEED: f32 = 1.3;
/// Tallest ledge, in metres, the controller steps up onto while grounded (stairs, curbs); anything higher
/// blocks. [`super::reach::STEP_HEIGHT`] is the path planner's slightly stricter 0.22.
pub const STEP_HEIGHT: f32 = 0.221;
/// Default gravity in m/s^2. A jump's launch speed follows it, so `jump_height` stays true at any gravity.
pub const GRAVITY: f32 = 12.;
/// Default decay rate (per second) of an impulse's horizontal push; see [`Controller::apply_impulse`].
pub const PUSH_DRAG: f32 = 3.;

/// Playable body profile shared by rendering and fixed-step movement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CharacterKind {
    #[default]
    Scientist,
    Feta,
}
impl CharacterKind {
    pub fn standing_height(self) -> f32 {
        if self == Self::Feta {
            0.30
        } else {
            STANDING_HEIGHT
        }
    }
    pub fn radius(self) -> f32 {
        if self == Self::Feta {
            0.16
        } else {
            RADIUS
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Movement {
    pub forward: f32,
    pub right: f32,
    pub sprint: bool,
    /// A press edge, not a held key. An airborne press is ignored.
    pub jump: bool,
    pub crouch: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(
    all(feature = "schema-generation", not(target_arch = "wasm32")),
    derive(schemars::JsonSchema)
)]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::collider))]
pub struct Collider {
    pub min: V,
    pub max: V,
}
impl Collider {
    pub fn blocks(&self, p: V) -> bool {
        self.overlaps_body(p, 0., STANDING_HEIGHT, RADIUS)
    }
    /// True when a body of `radius` and `height` standing with its feet at `feet` and centred (in x/z)
    /// on `p` intersects this box. This is the exact test [`Controller`] uses, so a game can ask the same
    /// question (spawn checks, projectile hulls) without re-implementing it.
    pub fn overlaps_body(&self, p: V, feet: f32, height: f32, radius: f32) -> bool {
        self.max.1 > feet + 0.0001
            && self.min.1 < feet + height - 0.0001
            && self.overlaps_xz(p, radius)
    }
    pub fn overlaps_xz(&self, p: V, radius: f32) -> bool {
        let x = p.0.clamp(self.min.0, self.max.0);
        let z = p.2.clamp(self.min.2, self.max.2);
        (p.0 - x).powi(2) + (p.2 - z).powi(2) < radius * radius
    }
    pub fn contains(&self, p: V) -> bool {
        p.0 >= self.min.0 - 0.01
            && p.0 <= self.max.0 + 0.01
            && p.1 >= self.min.1 - 0.01
            && p.1 <= self.max.1 + 0.01
            && p.2 >= self.min.2 - 0.01
            && p.2 <= self.max.2 + 0.01
    }
}

#[derive(Clone, Debug)]
pub struct Controller {
    pub position: V,
    pub yaw: f32,
    pub pitch: f32,
    kind: CharacterKind,
    profile: ControllerProfile,
    velocity: V,
    feet: f32,
    body_height: f32,
    vertical_velocity: f32,
    grounded: bool,
    gravity: f32,
    floor: Option<f32>,
    push: V,
    push_drag: f32,
}

fn no_push(push: &V) -> bool {
    *push == V::ZERO
}

/// Complete movement state for authoritative reconciliation and network serialization.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ControllerState {
    #[serde(rename = "p")]
    pub position: V,
    #[serde(rename = "y")]
    pub yaw: f32,
    #[serde(rename = "t")]
    pub pitch: f32,
    #[serde(rename = "v")]
    pub velocity: V,
    #[serde(rename = "f")]
    pub feet: f32,
    #[serde(rename = "h")]
    pub body_height: f32,
    #[serde(rename = "j")]
    pub vertical_velocity: f32,
    #[serde(rename = "g")]
    pub grounded: bool,
    /// Horizontal push from [`Controller::apply_impulse`]; omitted from the wire while zero.
    #[serde(rename = "k", default, skip_serializing_if = "no_push")]
    pub push: V,
}

/// Alias for game compatibility with Feta code.
pub type KinematicState = ControllerState;

impl Controller {
    /// Current locomotion dimensions and speeds, also used by physical route planning.
    pub fn profile(&self) -> ControllerProfile {
        self.profile
    }

    pub fn network_state(&self) -> ControllerState {
        ControllerState {
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            velocity: self.velocity,
            feet: self.feet,
            body_height: self.body_height,
            vertical_velocity: self.vertical_velocity,
            grounded: self.grounded,
            push: self.push,
        }
    }

    pub fn kinematic_state(&self) -> ControllerState {
        self.network_state()
    }

    pub fn restore_network_state(&mut self, state: &ControllerState) {
        self.position = state.position;
        self.yaw = state.yaw;
        self.pitch = state.pitch;
        self.velocity = state.velocity;
        self.feet = state.feet;
        self.body_height = state.body_height;
        self.vertical_velocity = state.vertical_velocity;
        self.grounded = state.grounded;
        self.push = state.push;
    }

    pub fn restore_kinematic_state(&mut self, state: &ControllerState) {
        self.restore_network_state(state);
    }
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            position: V(0., EYE_HEIGHT, 4.6),
            yaw: -0.10,
            pitch: -0.035,
            kind: CharacterKind::Scientist,
            profile: ControllerProfile::default(),
            velocity: V::ZERO,
            feet: 0.,
            body_height: STANDING_HEIGHT,
            vertical_velocity: 0.,
            grounded: true,
            gravity: GRAVITY,
            floor: Some(0.),
            push: V::ZERO,
            push_drag: PUSH_DRAG,
        }
    }
}
impl Controller {
    /// Spawn a character at the normal map entrance, with matching eye/body height.
    pub fn for_character(kind: CharacterKind) -> Self {
        Self::for_character_at(kind, V(0., 0., 4.6), -0.10)
            .expect("Built-in character profile and spawn are valid")
    }
    /// Spawn a rendered character at explicit feet coordinates and yaw.
    pub fn for_character_at(kind: CharacterKind, feet: V, yaw: f32) -> crate::Result<Self> {
        let profile = if kind == CharacterKind::Feta {
            ControllerProfile {
                height: 0.30,
                crouched_height: 0.20,
                radius: 0.16,
                eye_height: 0.22,
                walk_speed: 5.6,
                sprint_speed: 5.6,
                crouch_speed: 2.8,
                ..ControllerProfile::default()
            }
        } else {
            ControllerProfile::default()
        };
        if !feet.finite() || !yaw.is_finite() {
            return Err("Nonfinite spawn".into());
        }
        Ok(Self {
            kind,
            profile,
            body_height: profile.height,
            position: profile.eye_at(feet),
            feet: feet.1,
            yaw,
            pitch: 0.,
            ..Self::default()
        })
    }
    /// Create a generic controller at a feet position, with no rendering metadata.
    pub fn for_profile(profile: ControllerProfile, feet: V, yaw: f32) -> crate::Result<Self> {
        profile.validate()?;
        if !feet.finite() || !yaw.is_finite() {
            return Err("Nonfinite spawn".into());
        }
        Ok(Self {
            profile,
            body_height: profile.height,
            position: profile.eye_at(feet),
            feet: feet.1,
            yaw,
            pitch: 0.,
            ..Self::default()
        })
    }
    pub fn character_kind(&self) -> CharacterKind {
        self.kind
    }

    pub fn velocity(&self) -> V {
        self.velocity
    }

    /// Interpolate presentation only; current look stays immediate.
    pub fn interpolated(&self, previous: &Self, alpha: f32) -> Self {
        let mut pose = self.clone();
        let alpha = alpha.clamp(0., 1.);
        pose.position = previous.position.lerp(self.position, alpha);
        pose.feet = previous.feet + (self.feet - previous.feet) * alpha;
        pose.body_height = previous.body_height + (self.body_height - previous.body_height) * alpha;
        pose
    }
    pub fn feet_height(&self) -> f32 {
        self.feet
    }
    pub fn body_height(&self) -> f32 {
        self.body_height
    }
    pub fn is_grounded(&self) -> bool {
        self.grounded
    }
    pub fn is_crouched(&self) -> bool {
        self.body_height < self.profile.height - 0.01
    }
    pub fn vertical_velocity(&self) -> f32 {
        self.vertical_velocity
    }
    /// Overwrite the eye position and the vertical state. This is the supported way to apply an external
    /// *vertical* change (a moving platform, a teleport, reconciliation with an authority); for a shove
    /// or knockback use [`Controller::apply_impulse`], which keeps the push alive for a moment.
    /// [`Controller::restore_network_state`] overwrites the complete state instead.
    pub fn set_physics_state(&mut self, pos: V, vert_vel: f32, grounded: bool) {
        self.position = pos;
        self.feet = pos.1 - (self.body_height - self.profile.head_margin());
        self.vertical_velocity = vert_vel;
        self.grounded = grounded;
    }
    /// A kinematic box moved from `from` to `to` this tick (a game mover; a pure translation): a grounded body
    /// standing on top of it rides along, so lifts, elevators and moving platforms carry whoever is on them, in
    /// every direction including down. Nothing else is needed for a box that slides into a body or rises into
    /// it, because ordinary collision already pushes the body out; what collision cannot do is keep a body on
    /// top of a box that moves away from under it, which is why the body used to sink through a rising lift.
    ///
    /// Call it once per mover per tick, after the mover has moved and before the controller updates.
    pub fn ride(&mut self, from: &Collider, to: &Collider) {
        let delta = to.min - from.min;
        if delta == V::ZERO {
            return;
        }
        let standing = self.grounded
            && (self.feet - from.max.1).abs() <= 0.05
            && from.overlaps_xz(self.position, self.profile.radius);
        if standing {
            self.position = self.position + delta;
            self.feet += delta.1;
            self.vertical_velocity = 0.;
        }
    }
    pub fn direction(&self) -> V {
        V(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
    pub fn ray(&self) -> Ray {
        Ray {
            o: self.position,
            d: self.direction(),
        }
    }
    /// Discard horizontal momentum, including any push from [`Controller::apply_impulse`].
    pub fn stop(&mut self) {
        self.velocity = V(0., 0., 0.);
        self.push = V::ZERO;
    }
    /// Gravity in m/s^2 (default [`GRAVITY`]).
    pub fn gravity(&self) -> f32 {
        self.gravity
    }
    /// Change gravity for this controller (finite, `0.5..=100`; anything else is ignored). The launch
    /// speed of a jump follows it, so `jump_height` stays the same while the jump gets quicker or floatier.
    pub fn set_gravity(&mut self, gravity: f32) {
        if gravity.is_finite() && (0.5..=100.).contains(&gravity) {
            self.gravity = gravity;
        }
    }
    /// Height of the implicit ground plane (default `Some(0.)`).
    pub fn floor(&self) -> Option<f32> {
        self.floor
    }
    /// Set the implicit ground plane. `Some(y)` is an infinite floor at `y` metres; `None` removes it, so
    /// only colliders support the body and a player who walks off the last tile keeps falling. A game with
    /// pits, voids or an arena floating in the sky wants `None` (or a floor far below) instead of lifting
    /// its whole world above y = 0. A non-finite height is ignored.
    pub fn set_floor(&mut self, floor: Option<f32>) {
        if floor.is_none_or(f32::is_finite) {
            self.floor = floor;
        }
    }
    /// The horizontal push currently applied by [`Controller::apply_impulse`], in m/s.
    pub fn push_velocity(&self) -> V {
        self.push
    }
    /// How fast a push decays, per second (default [`PUSH_DRAG`]; `0..=50`, others ignored). At the default
    /// a push loses about 95% of its speed in one second; `0` keeps it until something blocks it.
    pub fn set_push_drag(&mut self, per_second: f32) {
        if per_second.is_finite() && (0.0..=50.).contains(&per_second) {
            self.push_drag = per_second;
        }
    }
    /// An instantaneous velocity change in m/s: knockback, a dash, a jump pad, an explosion.
    ///
    /// The horizontal part (x, z) becomes a *push* that is added to walking and decays with the push
    /// drag, so it is not erased by the ~0.1 s that input smoothing takes to converge on the walk
    /// speed; walls stop it. The vertical part is added to the vertical velocity and, when it is upward,
    /// leaves the ground. Non-finite impulses are ignored. Impulses are local state of the simulation
    /// that applies them: an online game must apply them where the authority runs.
    pub fn apply_impulse(&mut self, impulse: V) {
        if !impulse.finite() {
            return;
        }
        self.push = self.push + V(impulse.0, 0., impulse.2);
        self.vertical_velocity += impulse.1;
        if impulse.1 > 0. {
            self.grounded = false;
        }
    }
    /// True when this controller's body, placed at `at` (x/z) with its current feet height and body
    /// height, would intersect any of `colliders`.
    pub fn blocked_at(&self, at: V, colliders: &[Collider]) -> bool {
        colliders
            .iter()
            .any(|c| c.overlaps_body(at, self.feet, self.body_height, self.profile.radius))
    }
    pub fn look(&mut self, dx: f32, dy: f32, sensitivity: f32, invert: bool) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.yaw = (self.yaw + dx * sensitivity).rem_euclid(std::f32::consts::TAU);
        self.pitch =
            (self.pitch - dy * sensitivity * if invert { -1. } else { 1. }).clamp(-1.50, 1.50);
    }
    pub fn step(
        &mut self,
        forward: f32,
        right: f32,
        sprint: bool,
        dt: f32,
        colliders: &[Collider],
    ) {
        self.update(
            Movement {
                forward,
                right,
                sprint,
                ..Default::default()
            },
            dt,
            colliders,
        );
    }
    // Grounded step-up for stairs, with a full standing/crouched headroom check.
    fn move_horizontal(&mut self, target: V, colliders: &[Collider]) -> bool {
        let blocked = colliders
            .iter()
            .any(|c| c.overlaps_body(target, self.feet, self.body_height, self.profile.radius));
        if !blocked {
            self.position = target;
            return true;
        }
        if !self.grounded {
            return false;
        }
        let mut top = self.feet;
        for c in colliders
            .iter()
            .filter(|c| c.overlaps_body(target, self.feet, self.body_height, self.profile.radius))
        {
            if c.max.1 - self.feet > STEP_HEIGHT {
                return false;
            }
            top = top.max(c.max.1);
        }
        if colliders
            .iter()
            .any(|c| c.overlaps_body(target, top, self.body_height, self.profile.radius))
        {
            return false;
        }
        self.feet = top;
        self.position = V(
            target.0,
            top + self.body_height - self.profile.head_margin(),
            target.2,
        );
        true
    }
    // A dropped/moving prop can introduce overlap without player movement. Recover
    // horizontally before the normal sweep; never teleport through another collider.
    fn recover_overlap(&mut self, colliders: &[Collider]) {
        let overlaps =
            |c: &Collider, p| c.overlaps_body(p, self.feet, self.body_height, self.profile.radius);
        let mut best = None;
        let mut distance = f32::INFINITY;
        for c in colliders.iter().filter(|c| overlaps(c, self.position)) {
            let margin = self.profile.radius + 0.001;
            let xs = [self.position.0, c.min.0 - margin, c.max.0 + margin];
            let zs = [self.position.2, c.min.2 - margin, c.max.2 + margin];
            for x in xs {
                for z in zs {
                    let candidate = V(x, self.position.1, z);
                    let d = (candidate - self.position).length();
                    if d <= 4. && d < distance && !colliders.iter().any(|c| overlaps(c, candidate))
                    {
                        // The recovery segment must not cross an unrelated wall.
                        let steps = (d / (self.profile.radius * 0.5)).ceil() as usize;
                        if (1..=steps).any(|i| {
                            let p = self.position.lerp(candidate, i as f32 / steps as f32);
                            colliders
                                .iter()
                                .any(|c| !overlaps(c, self.position) && overlaps(c, p))
                        }) {
                            continue;
                        }
                        distance = d;
                        best = Some(candidate);
                    }
                }
            }
        }
        if let Some(position) = best {
            self.position = position;
            self.velocity = V(0., 0., 0.);
            self.push = V::ZERO;
        }
    }
    pub fn update(&mut self, input: Movement, dt: f32, colliders: &[Collider]) {
        let Movement {
            forward,
            right,
            sprint,
            jump,
            crouch,
        } = input;
        if !dt.is_finite() || dt <= 0. || !forward.is_finite() || !right.is_finite() {
            return;
        }
        self.recover_overlap(colliders);
        let dt = dt.min(0.1);
        let length = (forward * forward + right * right).sqrt().max(1.);
        let direction = V(
            self.yaw.sin() * forward + self.yaw.cos() * right,
            0.,
            -self.yaw.cos() * forward + self.yaw.sin() * right,
        ) / length;
        if jump && self.grounded {
            self.vertical_velocity = (2. * self.gravity * self.profile.jump_height).sqrt();
            self.grounded = false;
        }
        let steps = (dt / 0.008).ceil() as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let target_height = if crouch {
                self.profile.crouched_height
            } else {
                self.profile.height
            };
            let mut next_height =
                self.body_height + (target_height - self.body_height).clamp(-4. * h, 4. * h);
            // Expand only into free headroom; release crouch under a beam safely.
            if next_height > self.body_height {
                for c in colliders
                    .iter()
                    .filter(|c| c.overlaps_xz(self.position, self.profile.radius))
                {
                    if c.min.1 >= self.feet + self.body_height - 0.0001 {
                        next_height = next_height.min((c.min.1 - self.feet).max(self.body_height));
                    }
                }
            }
            self.body_height = next_height;
            let desired = direction
                * if crouch || self.is_crouched() {
                    self.profile.crouch_speed
                } else if sprint {
                    self.profile.sprint_speed
                } else {
                    self.profile.walk_speed
                };
            self.velocity = self.velocity.lerp(desired, 1. - (-18. * h).exp());
            // A push (knockback) rides on top of walking and decays; a wall stops both.
            self.push = self.push * (-self.push_drag * h).exp();
            if self.push.length() < 0.01 {
                self.push = V::ZERO;
            }
            let d = (self.velocity + self.push) * h;
            let px = self.position + V(d.0, 0., 0.);
            if !self.move_horizontal(px, colliders) {
                self.velocity.0 = 0.;
                self.push.0 = 0.;
            }
            let pz = self.position + V(0., 0., d.2);
            if !self.move_horizontal(pz, colliders) {
                self.velocity.2 = 0.;
                self.push.2 = 0.;
            }
            // Swept vertical movement catches ceilings and landings even on long frames.
            let dy = self.vertical_velocity * h - 0.5 * self.gravity * h * h;
            self.vertical_velocity -= self.gravity * h;
            let mut next_feet = self.feet + dy;
            self.grounded = false;
            if dy > 0. {
                for c in colliders
                    .iter()
                    .filter(|c| c.overlaps_xz(self.position, self.profile.radius))
                {
                    if self.feet + self.body_height <= c.min.1 + 0.0001
                        && next_feet + self.body_height >= c.min.1
                    {
                        next_feet = next_feet.min(c.min.1 - self.body_height);
                        self.vertical_velocity = 0.;
                    }
                }
            } else {
                let mut support = self.floor.unwrap_or(f32::NEG_INFINITY);
                for c in colliders
                    .iter()
                    .filter(|c| c.overlaps_xz(self.position, self.profile.radius))
                {
                    if self.feet >= c.max.1 - 0.0001 && next_feet <= c.max.1 {
                        support = support.max(c.max.1);
                    }
                }
                if next_feet <= support {
                    next_feet = support;
                    self.vertical_velocity = 0.;
                    self.grounded = true;
                }
            }
            self.feet = next_feet;
            self.position.1 = self.feet + self.body_height - self.profile.head_margin();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_recovery_preserves_feet_and_avoids_adjacent_wall() {
        let prop = Collider {
            min: V(-0.3, 0.05, -0.3),
            max: V(0.3, 0.5, 0.3),
        };
        let wall = Collider {
            min: V(-0.6, 0., -3.),
            max: V(-0.4, 3., 3.),
        };
        let colliders = [prop, wall];
        let mut player = Controller::for_profile(Default::default(), V(0., 0., 0.), 0.).unwrap();
        player.update(Default::default(), 1. / 60., &colliders);
        assert_eq!(player.feet_height(), 0.);
        assert!(player.position.0 >= 0., "crossed adjacent wall");
        assert!(!colliders.iter().any(|c| c.overlaps_body(
            player.position,
            player.feet,
            player.body_height,
            player.profile.radius
        )));
        let recovered = player.position;
        player.update(Default::default(), 1. / 60., &colliders);
        assert_eq!(player.position, recovered, "recovery jittered");
    }

    #[test]
    fn fully_enclosed_overlap_does_not_teleport_through_walls() {
        let colliders = [
            Collider {
                min: V(-0.3, 0., -0.3),
                max: V(0.3, 1., 0.3),
            },
            Collider {
                min: V(-1., 0., -1.),
                max: V(-0.4, 3., 1.),
            },
            Collider {
                min: V(0.4, 0., -1.),
                max: V(1., 3., 1.),
            },
            Collider {
                min: V(-1., 0., -1.),
                max: V(1., 3., -0.4),
            },
            Collider {
                min: V(-1., 0., 0.4),
                max: V(1., 3., 1.),
            },
        ];
        let mut player = Controller::for_profile(Default::default(), V(0., 0., 0.), 0.).unwrap();
        let before = player.position;
        player.update(Default::default(), 1. / 60., &colliders);
        assert_eq!(player.position, before);
    }
    #[test]
    fn sprint_is_normalized_and_returns_smoothly_to_walking() {
        for hz in [30, 60, 144] {
            let mut c = Controller::default();
            for _ in 0..hz {
                c.update(
                    Movement {
                        forward: 1.,
                        right: 1.,
                        sprint: true,
                        ..Default::default()
                    },
                    1. / hz as f32,
                    &[],
                );
            }
            assert!((c.velocity.length() - SPRINT_SPEED).abs() < 0.001);
            c.update(
                Movement {
                    forward: 1.,
                    right: 1.,
                    ..Default::default()
                },
                1. / hz as f32,
                &[],
            );
            assert!(c.velocity.length() > WALK_SPEED && c.velocity.length() < SPRINT_SPEED);
            for _ in 0..hz {
                c.update(
                    Movement {
                        forward: 1.,
                        right: 1.,
                        ..Default::default()
                    },
                    1. / hz as f32,
                    &[],
                );
            }
            assert!((c.velocity.length() - WALK_SPEED).abs() < 0.001);
            for _ in 0..hz {
                c.update(
                    Movement {
                        forward: 1.,
                        right: 1.,
                        sprint: true,
                        crouch: true,
                        ..Default::default()
                    },
                    1. / hz as f32,
                    &[],
                );
            }
            assert!((c.velocity.length() - CROUCH_SPEED).abs() < 0.001);
        }
    }
    #[test]
    fn small_jump_lands_and_is_frame_rate_independent() {
        for hz in [30, 60, 144] {
            let mut c = Controller::default();
            let mut peak = 0_f32;
            for i in 0..hz {
                c.update(
                    Movement {
                        jump: i == 0,
                        ..Default::default()
                    },
                    1. / hz as f32,
                    &[],
                );
                peak = peak.max(c.feet);
            }
            assert!((peak - JUMP_HEIGHT).abs() < 0.004, "{hz} Hz peak {peak}");
            assert!(c.is_grounded());
            assert_eq!(c.feet, 0.);
            assert_eq!(c.position.1, EYE_HEIGHT);
        }
    }
    #[test]
    fn cannot_double_jump() {
        let mut c = Controller::default();
        c.update(
            Movement {
                jump: true,
                ..Default::default()
            },
            0.1,
            &[],
        );
        let mut expected = c.clone();
        c.update(
            Movement {
                jump: true,
                ..Default::default()
            },
            0.1,
            &[],
        );
        expected.update(Movement::default(), 0.1, &[]);
        assert_eq!(c.position, expected.position);
    }
    #[test]
    fn jump_hits_ceiling_and_lands_without_penetration() {
        let beam = Collider {
            min: V(-2., 1.94, 2.),
            max: V(2., 2.2, 6.),
        };
        let mut c = Controller::default();
        for i in 0..30 {
            c.update(
                Movement {
                    jump: i == 0,
                    ..Default::default()
                },
                0.1,
                std::slice::from_ref(&beam),
            );
            assert!(c.feet + c.body_height <= 1.9401);
        }
        assert!(c.is_grounded());
        assert_eq!(c.feet, 0.);
    }
    #[test]
    fn crouch_is_smooth_slower_and_returns_to_standing() {
        let mut c = Controller {
            yaw: 0.,
            ..Default::default()
        };
        c.update(
            Movement {
                crouch: true,
                ..Default::default()
            },
            1. / 60.,
            &[],
        );
        assert!(c.position.1 < EYE_HEIGHT && c.position.1 > 1.5);
        for _ in 0..60 {
            c.update(
                Movement {
                    forward: 1.,
                    crouch: true,
                    sprint: true,
                    ..Default::default()
                },
                1. / 60.,
                &[],
            );
        }
        assert!(
            (c.position.1 - (CROUCH_HEIGHT - ControllerProfile::default().head_margin())).abs()
                < 0.001
        );
        assert!(4.6 - c.position.2 < 1.4);
        assert_eq!(c.feet, 0.);
        for _ in 0..60 {
            c.update(Movement::default(), 1. / 60., &[]);
        }
        assert_eq!(c.position.1, EYE_HEIGHT);
        assert!(!c.is_crouched());
    }
    #[test]
    fn cannot_stand_through_low_overhang_and_can_exit_it() {
        let beam = Collider {
            min: V(-1., 1.3, 3.),
            max: V(1., 1.6, 6.),
        };
        let mut c = Controller::default();
        for _ in 0..30 {
            c.update(
                Movement {
                    crouch: true,
                    ..Default::default()
                },
                1. / 60.,
                &[],
            );
        }
        for _ in 0..30 {
            c.update(Movement::default(), 1. / 60., std::slice::from_ref(&beam));
        }
        assert!((c.body_height - 1.3).abs() < 0.001);
        assert!(c.is_crouched());
        for _ in 0..180 {
            c.update(
                Movement {
                    right: 1.,
                    ..Default::default()
                },
                1. / 60.,
                std::slice::from_ref(&beam),
            );
        }
        assert!(!c.is_crouched());
        assert_eq!(c.position.1, EYE_HEIGHT);
    }
    #[test]
    fn lands_on_low_ledge_then_falls_when_walking_off() {
        let ledge = Collider {
            min: V(-1., 0., 4.0),
            max: V(1., 0.2, 4.4),
        };
        let mut c = Controller {
            yaw: 0.,
            ..Default::default()
        };
        c.position.2 = 4.8;
        c.update(
            Movement {
                jump: true,
                ..Default::default()
            },
            0.1,
            std::slice::from_ref(&ledge),
        );
        for _ in 0..10 {
            c.update(
                Movement {
                    forward: 1.,
                    ..Default::default()
                },
                1. / 60.,
                std::slice::from_ref(&ledge),
            );
        }
        c.stop();
        for _ in 0..60 {
            c.update(Movement::default(), 1. / 60., std::slice::from_ref(&ledge));
        }
        assert!((c.feet - 0.2).abs() < 0.001);
        assert!(c.is_grounded());
        for _ in 0..120 {
            c.update(
                Movement {
                    right: 1.,
                    ..Default::default()
                },
                1. / 60.,
                std::slice::from_ref(&ledge),
            );
        }
        assert_eq!(c.feet, 0.);
        assert!(c.is_grounded());
    }
    #[test]
    fn airborne_crouch_and_pause_do_not_teleport_feet() {
        let mut c = Controller::default();
        c.update(
            Movement {
                jump: true,
                ..Default::default()
            },
            0.1,
            &[],
        );
        let v = c.vertical_velocity;
        c.stop();
        assert_eq!(c.vertical_velocity, v);
        let mut standing = c.clone();
        c.update(
            Movement {
                crouch: true,
                ..Default::default()
            },
            0.1,
            &[],
        );
        standing.update(Movement::default(), 0.1, &[]);
        assert_eq!(c.feet, standing.feet);
        assert!(c.position.1 < standing.position.1);
    }
    fn walk(f: f32, r: f32, hz: usize) -> Controller {
        let mut c = Controller {
            yaw: 0.,
            ..Default::default()
        };
        for _ in 0..hz {
            c.step(f, r, false, 1. / hz as f32, &[]);
        }
        c
    }
    #[test]
    fn diagonal_and_frame_rate_independent() {
        let start = Controller::default().position;
        let a = (walk(1., 0., 60).position - start).length();
        let b = (walk(1., 1., 60).position - start).length();
        assert!((a - b).abs() < 0.001);
        assert!((a - (walk(1., 0., 144).position - start).length()).abs() < 0.02);
    }
    #[test]
    fn forward_tracks_yaw_and_ignores_pitch() {
        let mut c = Controller {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 1.4,
            ..Default::default()
        };
        let p = c.position;
        for _ in 0..60 {
            c.step(1., 0., false, 1. / 60., &[]);
        }
        assert!(c.position.0 > p.0 + 2.);
        assert!((c.position.2 - p.2).abs() < 0.001);
        assert_eq!(c.position.1, EYE_HEIGHT);
    }
    #[test]
    fn collision_slides_and_long_frames_do_not_tunnel() {
        let wall = Collider {
            min: V(-100., 0., 2.),
            max: V(100., 3., 2.1),
        };
        let mut c = Controller {
            yaw: 0.,
            ..Default::default()
        };
        for _ in 0..100 {
            c.step(1., 0.5, true, 0.1, std::slice::from_ref(&wall));
        }
        assert!(c.position.2 >= 2.1 + RADIUS - 0.001);
        assert!(c.position.0 > 3.);
    }
    #[test]
    fn mouse_is_bounded_and_reversible() {
        let mut c = Controller::default();
        c.look(40., 20., 0.002, false);
        assert!(c.pitch < -0.035);
        c.look(0., 100000., 0.002, false);
        assert_eq!(c.pitch, -1.5);
        c.look(0., 100000., 0.002, true);
        assert_eq!(c.pitch, 1.5);
        assert!((c.direction().length() - 1.).abs() < 0.001);
    }
    #[test]
    fn stop_discards_momentum() {
        let mut c = walk(1., 0., 60);
        c.stop();
        let p = c.position;
        c.step(0., 0., false, 0.1, &[]);
        assert_eq!(c.position, p);
    }
    #[test]
    fn invalid_delta_is_ignored() {
        let mut c = Controller::default();
        let p = c.position;
        c.step(1., 0., false, f32::NAN, &[]);
        assert_eq!(p, c.position);
    }
}

#[cfg(test)]
mod character_tests {
    use super::*;
    #[test]
    fn feta_natural_speed_matches_scientist_sprint() {
        let mut rat = Controller::for_character(CharacterKind::Feta);
        let mut human = Controller::default();
        for _ in 0..120 {
            rat.step(1., 0., false, 1. / 60., &[]);
            human.step(1., 0., true, 1. / 60., &[]);
        }
        assert!((rat.position.2 - human.position.2).abs() < 0.001);
        assert!((rat.position.1 - 0.22).abs() < 0.001);
        assert!(!rat.is_crouched());
    }
    #[test]
    fn feta_fits_low_passage_but_still_collides_with_walls() {
        let beam = Collider {
            min: V(-2., 0.4, 2.),
            max: V(2., 2., 3.),
        };
        let wall = Collider {
            min: V(-2., 0., 0.),
            max: V(2., 2., 0.1),
        };
        let mut rat = Controller::for_character(CharacterKind::Feta);
        let mut human = Controller::default();
        rat.yaw = 0.;
        human.yaw = 0.;
        for _ in 0..120 {
            rat.step(1., 0., false, 1. / 60., &[beam.clone(), wall.clone()]);
            human.step(1., 0., true, 1. / 60., &[beam.clone(), wall.clone()]);
        }
        assert!(rat.position.2 < 2. && rat.position.2 >= 0.25);
        assert!(human.position.2 > 3.);
    }
}

#[cfg(test)]
mod external_force_tests {
    use super::*;

    const TICK: f32 = 1. / 60.;

    fn run(c: &mut Controller, movement: Movement, ticks: usize, colliders: &[Collider]) {
        for _ in 0..ticks {
            c.update(movement, TICK, colliders);
        }
    }
    fn platform() -> Collider {
        Collider {
            min: V(-1., 0., -1.),
            max: V(1., 1., 1.),
        }
    }
    /// Standing on the platform top, facing +x.
    fn on_platform() -> Controller {
        Controller::for_profile(
            Default::default(),
            V(0., 1., 0.),
            std::f32::consts::FRAC_PI_2,
        )
        .unwrap()
    }
    const FORWARD: Movement = Movement {
        forward: 1.,
        right: 0.,
        sprint: false,
        jump: false,
        crouch: false,
    };

    #[test]
    fn walking_off_a_platform_lands_on_the_default_floor() {
        let mut c = on_platform();
        run(&mut c, Movement::default(), 30, &[platform()]);
        assert!(
            c.is_grounded() && (c.feet_height() - 1.).abs() < 1e-4,
            "starts on the platform"
        );
        run(&mut c, FORWARD, 180, &[platform()]);
        assert!(c.position.0 > 3., "walked off the edge");
        assert_eq!(
            (c.feet_height(), c.is_grounded()),
            (0., true),
            "the implicit floor at y = 0 catches the fall"
        );
    }

    #[test]
    fn without_a_floor_the_player_falls_into_the_void() {
        let mut c = on_platform();
        c.set_floor(None);
        assert_eq!(c.floor(), None);
        run(&mut c, Movement::default(), 30, &[platform()]);
        assert!(c.is_grounded(), "colliders still support the body");
        run(&mut c, FORWARD, 240, &[platform()]);
        assert!(
            c.feet_height() < -10. && !c.is_grounded(),
            "fell to {} and kept going",
            c.feet_height()
        );
    }

    #[test]
    fn a_floor_can_sit_below_zero() {
        let mut c = on_platform();
        c.set_floor(Some(-3.));
        run(&mut c, FORWARD, 240, &[platform()]);
        assert_eq!((c.feet_height(), c.is_grounded()), (-3., true));
        c.set_floor(Some(f32::NAN));
        assert_eq!(c.floor(), Some(-3.), "a non-finite floor is ignored");
    }

    fn air_time_and_peak(gravity: f32) -> (usize, f32) {
        let mut c = Controller::default();
        c.set_gravity(gravity);
        let (mut peak, mut ticks) = (0f32, 0);
        c.update(
            Movement {
                jump: true,
                ..Default::default()
            },
            1. / 240.,
            &[],
        );
        while !c.is_grounded() && ticks < 4000 {
            c.update(Movement::default(), 1. / 240., &[]);
            peak = peak.max(c.feet_height());
            ticks += 1;
        }
        (ticks, peak)
    }

    #[test]
    fn gravity_changes_the_arc_but_not_the_jump_height() {
        let (normal, peak) = air_time_and_peak(GRAVITY);
        let (light, light_peak) = air_time_and_peak(GRAVITY / 2.);
        let (heavy, heavy_peak) = air_time_and_peak(GRAVITY * 2.);
        for p in [peak, light_peak, heavy_peak] {
            assert!((p - JUMP_HEIGHT).abs() < 0.01, "peak {p}");
        }
        assert!(
            light as f32 > normal as f32 * 1.3 && (heavy as f32) < normal as f32 * 0.8,
            "{light} {normal} {heavy}"
        );
        let mut c = Controller::default();
        for bad in [f32::NAN, 0., -5., 1000.] {
            c.set_gravity(bad);
            assert_eq!(c.gravity(), GRAVITY, "{bad} is ignored");
        }
    }

    #[test]
    fn an_impulse_carries_the_player_and_decays() {
        let mut c = Controller {
            yaw: 0.,
            ..Default::default()
        };
        let start = c.position.0;
        c.apply_impulse(V(6., 0., 0.));
        assert_eq!(c.push_velocity(), V(6., 0., 0.));
        run(&mut c, Movement::default(), 60, &[]);
        // Integral of 6 * e^(-3t) over one second is 6/3 * (1 - e^-3) = 1.9 m.
        assert!(
            (c.position.0 - start - 1.9).abs() < 0.12,
            "travelled {}",
            c.position.0 - start
        );
        run(&mut c, Movement::default(), 240, &[]);
        assert_eq!(c.push_velocity(), V::ZERO, "the push dies out");
        let rest = c.position;
        run(&mut c, Movement::default(), 30, &[]);
        assert_eq!(c.position, rest);
    }

    #[test]
    fn a_softer_drag_carries_further_and_zero_drag_lasts_until_blocked() {
        let travel = |drag: f32| {
            let mut c = Controller {
                yaw: 0.,
                ..Default::default()
            };
            c.set_push_drag(drag);
            let start = c.position.0;
            c.apply_impulse(V(4., 0., 0.));
            run(&mut c, Movement::default(), 120, &[]);
            c.position.0 - start
        };
        assert!(travel(1.) > travel(3.) * 1.5);
        assert!(travel(0.) > 7.5, "no drag: 4 m/s for two seconds");
        let mut c = Controller::default();
        c.set_push_drag(f32::NAN);
        c.set_push_drag(-1.);
        c.apply_impulse(V(1., 0., 0.));
        run(&mut c, Movement::default(), 30, &[]);
        assert!(
            c.push_velocity().0 < 1.,
            "the default drag is untouched by bad values"
        );
    }

    #[test]
    fn a_wall_stops_a_push() {
        let wall = Collider {
            min: V(1., 0., -5.),
            max: V(1.2, 3., 5.),
        };
        let mut c = Controller {
            yaw: 0.,
            position: V(0., EYE_HEIGHT, 0.),
            ..Default::default()
        };
        c.apply_impulse(V(10., 0., 0.));
        run(
            &mut c,
            Movement::default(),
            120,
            std::slice::from_ref(&wall),
        );
        assert!(
            c.position.0 <= 1. - RADIUS + 0.01 && c.position.0 > 0.5,
            "stopped at {}",
            c.position.0
        );
        assert_eq!(c.push_velocity().0, 0.);
    }

    #[test]
    fn a_vertical_impulse_launches_and_the_player_lands_again() {
        let mut c = Controller::default();
        c.apply_impulse(V(0., 5., 0.));
        assert!(!c.is_grounded());
        assert_eq!(
            c.push_velocity(),
            V::ZERO,
            "the vertical part is not a push"
        );
        run(&mut c, Movement::default(), 15, &[]);
        assert!(c.feet_height() > 0.5, "launched to {}", c.feet_height());
        run(&mut c, Movement::default(), 120, &[]);
        assert_eq!((c.feet_height(), c.is_grounded()), (0., true));
    }

    #[test]
    fn stop_and_bad_impulses_clear_or_ignore_the_push() {
        let mut c = Controller::default();
        c.apply_impulse(V(f32::NAN, 1., 0.));
        c.apply_impulse(V(f32::INFINITY, 0., 0.));
        assert_eq!((c.push_velocity(), c.vertical_velocity()), (V::ZERO, 0.));
        c.apply_impulse(V(2., 0., 1.));
        c.apply_impulse(V(1., 0., 0.));
        assert_eq!(c.push_velocity(), V(3., 0., 1.), "impulses add up");
        c.stop();
        assert_eq!(c.push_velocity(), V::ZERO);
    }

    #[test]
    fn network_state_carries_the_push_and_stays_compact_without_one() {
        let mut c = Controller::default();
        let quiet = serde_json::to_string(&c.network_state()).unwrap();
        assert!(
            !quiet.contains("\"k\""),
            "an idle push costs no bytes: {quiet}"
        );
        c.apply_impulse(V(1., 0., 2.));
        let json = serde_json::to_string(&c.network_state()).unwrap();
        assert!(json.contains("\"k\""));
        let mut restored = Controller::default();
        restored.restore_network_state(&serde_json::from_str(&json).unwrap());
        assert_eq!(restored.push_velocity(), V(1., 0., 2.));
        let older: ControllerState = serde_json::from_str(&quiet).unwrap();
        assert_eq!(
            older.push,
            V::ZERO,
            "payloads from before the push field still parse"
        );
    }

    #[test]
    fn the_body_overlap_test_is_public() {
        let crate_box = Collider {
            min: V(1., 0., -1.),
            max: V(2., 1., 1.),
        };
        assert!(crate_box.overlaps_body(V(1.1, 0., 0.), 0., STANDING_HEIGHT, RADIUS));
        assert!(
            !crate_box.overlaps_body(V(1.1, 1.5, 0.), 1.5, STANDING_HEIGHT, RADIUS),
            "feet above the box"
        );
        let c = Controller::for_profile(Default::default(), V(0., 0., 0.), 0.).unwrap();
        assert!(c.blocked_at(V(1.1, 0., 0.), std::slice::from_ref(&crate_box)));
        assert!(!c.blocked_at(V(-1., 0., 0.), std::slice::from_ref(&crate_box)));
    }
}

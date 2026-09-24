//! Loose-prop rigid bodies and shared pickup/drop behavior, independent of graphics.
//! Static map documents stay unchanged. Catalog-material nodes inside semantic prop
//! bounds become runtime bodies; architectural materials and wall art stay fixed.
use super::{
    controller::{Collider as PlayerCollider, Controller},
    room::Room,
};
use crate::{
    geometry::{Compiled, Instance, Part, Primitive, World},
    math::{Mat, Ray as ViewRay, V},
    scene::Track,
};
use rapier3d::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

const STEP: f32 = 1. / 120.;
pub const PICKUP_REACH: f32 = 2.;

/// One semantic prop, composed of its original visual primitives.
pub struct PropBody {
    pub id: String,
    pub label: String,
    pub origin: V,
    pub local_world: World,
    pub transform: Mat,
    handle: RigidBodyHandle,
    entity: usize,
    radius: f32,
}
/// A local physics scene. The owner must call `advance` only while gameplay is active.
/// One body per player can be held; bidirectional ownership rejects contention.
pub struct PropPhysics {
    pub props: Vec<PropBody>,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: BroadPhaseMultiSap,
    narrow: NarrowPhase,
    joints: ImpulseJointSet,
    multi: MultibodyJointSet,
    ccd: CCDSolver,
    static_colliders: Vec<PlayerCollider>,
    held_by_player: HashMap<u64, usize>,
    player_by_held: HashMap<usize, u64>,
    debt: f32,
}
fn vector(v: V) -> Vector<Real> {
    Vector::new(v.0, v.1, v.2)
}
fn value(v: &Vector<Real>) -> V {
    V(v.x, v.y, v.z)
}
fn matrix(p: &Isometry<Real>) -> Mat {
    Mat {
        x: value(&(p.rotation * Vector::x())),
        y: value(&(p.rotation * Vector::y())),
        z: value(&(p.rotation * Vector::z())),
        p: value(&p.translation.vector),
    }
}
fn contains(c: &PlayerCollider, lo: V, hi: V) -> bool {
    (0..3).all(|a| lo.axis(a) >= c.min.axis(a) - 0.003 && hi.axis(a) <= c.max.axis(a) + 0.003)
}
fn shape(instance: &Instance, origin: V) -> Option<SharedShape> {
    let m = instance.inverse.inverse();
    let mut points = vec![];
    match &instance.shape {
        Primitive::Sphere => {
            for j in 0..=8 {
                for i in 0..12 {
                    let a = i as f32 * std::f32::consts::TAU / 12.;
                    let b = j as f32 * std::f32::consts::PI / 8.;
                    points.push(m.point(V(a.cos() * b.sin(), b.cos(), a.sin() * b.sin())));
                }
            }
        }
        Primitive::Cylinder | Primitive::Cone => {
            for i in 0..12 {
                let a = i as f32 * std::f32::consts::TAU / 12.;
                points.push(m.point(V(a.cos(), -1., a.sin())));
                points.push(m.point(if matches!(instance.shape, Primitive::Cone) {
                    V(0., 1., 0.)
                } else {
                    V(a.cos(), 1., a.sin())
                }));
            }
        }
        Primitive::Triangle(p, _) => {
            let p = p.map(|v| Point::from(vector(m.point(v) - origin)));
            return Some(SharedShape::triangle(p[0], p[1], p[2]));
        }
        Primitive::Box => {
            for x in [-1., 1.] {
                for y in [-1., 1.] {
                    for z in [-1., 1.] {
                        points.push(m.point(V(x, y, z)));
                    }
                }
            }
        }
    }
    SharedShape::convex_hull(
        &points
            .into_iter()
            .map(|p| Point::from(vector(p - origin)))
            .collect::<Vec<_>>(),
    )
}
fn moved(instance: &Instance, transform: Mat, origin: V) -> Instance {
    Instance::new(
        &Part {
            shape: instance.shape.clone(),
            local: instance.inverse.inverse(),
            material: instance.material.clone(),
            limb: 0,
        },
        transform.compose(Mat::trs(-origin, V::ZERO, V::ONE)),
    )
}
impl PropPhysics {
    /// Split catalog props out of static geometry once. Existing semantic IDs survive.
    /// Legacy map exports are recognized by their catalog materials and contained bounds.
    /// No map file is rewritten; unsupported custom materials remain static.
    pub fn new(room: &mut Room) -> crate::Result<Self> {
        let mut this = Self {
            props: vec![],
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: BroadPhaseMultiSap::new(),
            narrow: NarrowPhase::new(),
            joints: ImpulseJointSet::new(),
            multi: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            static_colliders: vec![],
            held_by_player: HashMap::new(),
            player_by_held: HashMap::new(),
            debt: 0.,
        };
        let source = &room.compiled.scene;
        let mut used = HashSet::new();
        // Small semantic bounds claim their own contents before larger furniture.
        let mut order: Vec<usize> = (0..room.entities.len()).collect();
        order.sort_by(|a, b| {
            let size = |i: usize| {
                let d = room.entities[i].bounds.max - room.entities[i].bounds.min;
                d.0 * d.1 * d.2
            };
            size(*a).total_cmp(&size(*b))
        });
        for entity in order {
            let e = &room.entities[entity];
            let h = (e.bounds.max - e.bounds.min) * 0.5;
            if h.length() > 1.2 || h.0.min(h.2) < 0.06 && h.1 > 0.25 {
                continue;
            }
            let mut scene = source.clone();
            scene.nodes.clear();
            for n in &source.nodes {
                if used.contains(&n.id)
                    || n.parent.is_some()
                    || !(n.material.starts_with("prop-") || n.material.starts_with("decor-"))
                {
                    continue;
                }
                if let (Track::Fixed(p), Track::Fixed(s), Track::Fixed(r)) =
                    (&n.pos, &n.scale, &n.rot)
                {
                    let m = Mat::trs(*p, *r, *s);
                    let mut lo = V::ONE * f32::INFINITY;
                    let mut hi = V::ONE * f32::NEG_INFINITY;
                    for x in [-1., 1.] {
                        for y in [-1., 1.] {
                            for z in [-1., 1.] {
                                let p = m.point(V(x, y, z));
                                lo = lo.min(p);
                                hi = hi.max(p);
                            }
                        }
                    }
                    if contains(&e.bounds, lo, hi) {
                        scene.nodes.push(n.clone());
                    }
                }
            }
            if scene.nodes.is_empty() {
                continue;
            }
            let local_world = Compiled::new(scene.clone(), Path::new("."))?.at(0.);
            let origin = (e.bounds.min + e.bounds.max) * 0.5;
            let handle = this.bodies.insert(
                RigidBodyBuilder::dynamic()
                    .translation(vector(origin))
                    .linear_damping(0.12)
                    .angular_damping(0.4)
                    .ccd_enabled(true)
                    .build(),
            );
            for instance in &local_world.instances {
                if let Some(shape) = shape(instance, origin) {
                    this.colliders.insert_with_parent(
                        ColliderBuilder::new(shape)
                            .density(160.)
                            .friction(0.65)
                            .restitution(0.12)
                            .build(),
                        handle,
                        &mut this.bodies,
                    );
                }
            }
            used.extend(scene.nodes.iter().map(|n| n.id.clone()));
            this.props.push(PropBody {
                id: e.id.clone(),
                label: e.label.clone(),
                origin,
                local_world,
                transform: Mat::trs(origin, V::ZERO, V::ONE),
                handle,
                entity,
                radius: h.length(),
            });
        }
        let mut scene = source.clone();
        scene.nodes.retain(|n| !used.contains(&n.id));
        room.world = Compiled::new(scene, Path::new("."))?.at(0.);
        room.colliders.retain(|c| {
            !this.props.iter().any(|p| {
                let b = &room.entities[p.entity].bounds;
                ((b.min - c.min).length() < 0.005 && (b.max - c.max).length() < 0.005)
                    || p.local_world.instances.iter().any(|part| {
                        (part.bounds.lo - c.min).length() < 0.005
                            && (part.bounds.hi - c.max).length() < 0.005
                    })
            })
        });
        this.static_colliders = room.colliders.clone();
        // Collision follows visible surfaces, so objects land on tabletops instead of
        // the conservative full-volume character collision boxes below the table.
        for instance in &room.world.instances {
            if let Some(shape) = shape(instance, V::ZERO) {
                this.colliders
                    .insert(ColliderBuilder::new(shape).friction(0.7).build());
            }
        }
        // The movement world treats y=0 as ground, including open garden areas.
        this.colliders.insert(
            ColliderBuilder::cuboid(1000., 0.1, 1000.)
                .translation(vector(V(0., -0.1, 0.)))
                .friction(0.7)
                .build(),
        );
        this.sync(room);
        Ok(this)
    }
    pub fn held(&self) -> Option<&PropBody> {
        self.held_for_player(0).map(|i| &self.props[i])
    }

    pub fn held_for_player(&self, player_id: u64) -> Option<usize> {
        self.held_by_player.get(&player_id).copied()
    }

    pub fn holder_of(&self, prop_idx: usize) -> Option<u64> {
        self.player_by_held.get(&prop_idx).copied()
    }

    pub fn is_prop_held(&self, prop_idx: usize) -> bool {
        self.player_by_held.contains_key(&prop_idx)
    }

    pub fn held_index(&self) -> Option<usize> {
        self.held_for_player(0)
    }

    /// Aim at the nearest visible prop. Fixed surfaces and other props occlude pickup.
    pub fn target(&self, room: &Room, ray: ViewRay) -> Option<usize> {
        if !ray.o.finite() || !ray.d.finite() {
            return None;
        }
        let mut distance = room
            .world
            .hit(ray, PICKUP_REACH, false)
            .map_or(PICKUP_REACH, |h| h.t);
        let mut result = None;
        for (i, p) in self.props.iter().enumerate() {
            for part in &p.local_world.instances {
                if let Some((t, _)) = moved(part, p.transform, p.origin).intersect(ray, distance) {
                    if t < distance {
                        distance = t;
                        result = Some(i);
                    }
                }
            }
        }
        result
    }

    /// Raycast against all dynamic props within max_distance, returning closest hit prop index and distance.
    pub fn hit_prop(&self, ray: ViewRay, max_distance: f32) -> Option<(usize, f32)> {
        if !ray.o.finite() || !ray.d.finite() {
            return None;
        }
        let mut distance = max_distance;
        let mut result = None;
        for (i, p) in self.props.iter().enumerate() {
            for part in &p.local_world.instances {
                if let Some((t, _)) = moved(part, p.transform, p.origin).intersect(ray, distance) {
                    if t < distance {
                        distance = t;
                        result = Some((i, t));
                    }
                }
            }
        }
        result
    }

    /// One E press picks up the aimed prop, or drops the current one (for player 0).
    pub fn toggle(&mut self, room: &Room, ray: ViewRay) -> bool {
        self.toggle_for_player(0, room, ray)
    }

    /// Multi-player interaction: pick up aimed prop or drop held prop for a specific player ID.
    /// Contention resolution: returns false if the target prop is already held by another player.
    pub fn toggle_for_player(&mut self, player_id: u64, room: &Room, ray: ViewRay) -> bool {
        if self.held_by_player.contains_key(&player_id) {
            self.drop_for_player(player_id);
            return true;
        }
        let Some(i) = self.target(room, ray) else {
            return false;
        };
        // Contention resolution: Cannot take a prop already held by another player
        if self.player_by_held.contains_key(&i) {
            return false;
        }
        self.held_by_player.insert(player_id, i);
        self.player_by_held.insert(i, player_id);
        let b = &mut self.bodies[self.props[i].handle];
        b.set_gravity_scale(0., true);
        b.wake_up(true);
        true
    }

    /// Release in place for player 0, retaining bounded carry momentum and restoring gravity.
    pub fn drop_held(&mut self) {
        self.drop_for_player(0);
    }

    /// Release in place for a specific player ID.
    pub fn drop_for_player(&mut self, player_id: u64) {
        if let Some(i) = self.held_by_player.remove(&player_id) {
            self.player_by_held.remove(&i);
            let b = &mut self.bodies[self.props[i].handle];
            b.set_gravity_scale(1., true);
            let v = *b.linvel();
            b.set_linvel(v.cap_magnitude(4.), true);
        }
    }

    /// Authoritatively synchronize a held prop for a player (used by network client reconciliation).
    pub fn set_held_for_player(&mut self, player_id: u64, prop_idx: usize) {
        if prop_idx >= self.props.len() {
            return;
        }
        if self.held_by_player.get(&player_id) == Some(&prop_idx) {
            return;
        }
        if self.held_by_player.contains_key(&player_id) {
            self.drop_for_player(player_id);
        }
        if let Some(other) = self.player_by_held.remove(&prop_idx) {
            self.held_by_player.remove(&other);
        }
        self.held_by_player.insert(player_id, prop_idx);
        self.player_by_held.insert(prop_idx, player_id);
        let b = &mut self.bodies[self.props[prop_idx].handle];
        b.set_gravity_scale(0., true);
        b.wake_up(true);
    }

    /// Pause clears only accumulated time; held objects and poses stay frozen.
    pub fn pause(&mut self) {
        self.debt = 0.;
    }

    /// Fixed 120 Hz rigid-body steps, capped to 16 ticks after a stall (single-player convenience).
    pub fn advance(&mut self, dt: f32, player: &Controller, room: &mut Room) {
        let mut map = HashMap::new();
        map.insert(0, player);
        self.step_simulation_with_players(dt, &map, room);
    }

    /// Step the simulation without requiring players (for empty server or static ticks).
    pub fn step_simulation(&mut self, dt: f32, room: &mut Room) {
        let empty = HashMap::new();
        self.step_simulation_with_players(dt, &empty, room);
    }

    /// Step simulation with multiple authoritative player poses for velocity servos and contention.
    pub fn step_simulation_with_players(
        &mut self,
        dt: f32,
        players: &HashMap<u64, &Controller>,
        room: &mut Room,
    ) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        self.debt = (self.debt + dt).min(STEP * 16.);
        while self.debt + 1e-6 >= STEP {
            self.debt = (self.debt - STEP).max(0.);
            let mut to_drop = Vec::new();
            for (&player_id, &i) in &self.held_by_player {
                if let Some(player) = players.get(&player_id) {
                    let prop = &self.props[i];
                    let target = player.position
                        + player.direction() * (0.65 + prop.radius)
                        + V(0., 0.08, 0.);
                    let b = &mut self.bodies[prop.handle];
                    let delta = vector(target) - b.translation();
                    if delta.norm() > 4. {
                        to_drop.push(player_id);
                    } else {
                        // Velocity servo keeps the object physical while carrying
                        b.set_linvel((delta * 12.).cap_magnitude(8.), true);
                        b.set_angvel(*b.angvel() * 0.85, true);
                    }
                } else {
                    to_drop.push(player_id);
                }
            }
            for pid in to_drop {
                self.drop_for_player(pid);
            }
            self.pipeline.step(
                &Vector::new(0., -9.81, 0.),
                &IntegrationParameters {
                    dt: STEP,
                    ..Default::default()
                },
                &mut self.islands,
                &mut self.broad,
                &mut self.narrow,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.joints,
                &mut self.multi,
                &mut self.ccd,
                None,
                &(),
                &(),
            );
        }
        self.sync(room);
    }

    pub fn active_and_sleeping_counts(&self) -> (usize, usize) {
        let mut active = 0;
        let mut sleeping = 0;
        for p in &self.props {
            if let Some(b) = self.bodies.get(p.handle) {
                if b.is_sleeping() {
                    sleeping += 1;
                } else {
                    active += 1;
                }
            }
        }
        (active, sleeping)
    }

    pub fn prop_position(&self, i: usize) -> Option<V> {
        self.props
            .get(i)
            .and_then(|p| self.bodies.get(p.handle).map(|b| value(b.translation())))
    }

    pub fn prop_linear_velocity(&self, i: usize) -> Option<V> {
        self.props
            .get(i)
            .and_then(|p| self.bodies.get(p.handle).map(|b| value(b.linvel())))
    }

    pub fn is_prop_sleeping(&self, i: usize) -> bool {
        self.props
            .get(i)
            .and_then(|p| self.bodies.get(p.handle).map(|b| b.is_sleeping()))
            .unwrap_or(true)
    }

    /// Apply an external linear impulse to a prop body by index.
    pub fn apply_impulse(&mut self, i: usize, impulse: V) {
        if let Some(p) = self.props.get(i) {
            if let Some(b) = self.bodies.get_mut(p.handle) {
                b.apply_impulse(vector(impulse), true);
                b.wake_up(true);
            }
        }
    }

    /// Set a prop's translation directly by semantic ID (e.g. from network replication).
    pub fn set_prop_position(&mut self, id: &str, pos: V) -> bool {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            if let Some(body) = self.bodies.get_mut(p.handle) {
                let mut iso = *body.position();
                iso.translation.vector = vector(pos);
                body.set_position(iso, true);
                p.transform = matrix(&iso);
                return true;
            }
        }
        false
    }

    pub fn prop_rotation(&self, i: usize) -> Option<[f32; 4]> {
        self.props.get(i).and_then(|p| {
            self.bodies.get(p.handle).map(|b| {
                let q = b.position().rotation;
                [q.i, q.j, q.k, q.w]
            })
        })
    }

    pub fn prop_angular_velocity(&self, i: usize) -> Option<V> {
        self.props
            .get(i)
            .and_then(|p| self.bodies.get(p.handle).map(|b| value(b.angvel())))
    }

    pub fn set_prop_transform_and_vel(
        &mut self,
        id: &str,
        pos: V,
        rot: [f32; 4],
        linvel: V,
        angvel: V,
        sleeping: bool,
    ) -> bool {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            if let Some(body) = self.bodies.get_mut(p.handle) {
                let q = rapier3d::na::UnitQuaternion::new_normalize(rapier3d::na::Quaternion::new(
                    rot[3], rot[0], rot[1], rot[2],
                ));
                let mut iso = *body.position();
                iso.translation.vector = vector(pos);
                iso.rotation = q;
                body.set_position(iso, true);
                body.set_linvel(vector(linvel), true);
                body.set_angvel(vector(angvel), true);
                if sleeping {
                    body.sleep();
                } else {
                    body.wake_up(true);
                }
                p.transform = matrix(&iso);
                return true;
            }
        }
        false
    }

    pub fn sync(&mut self, room: &mut Room) {
        let any_active = self
            .props
            .iter()
            .any(|p| self.bodies.get(p.handle).is_some_and(|b| !b.is_sleeping()));
        if !any_active && !room.dynamic_world.instances.is_empty() {
            return;
        }
        room.colliders.clone_from(&self.static_colliders);
        let held_indices: HashSet<usize> = self.player_by_held.keys().copied().collect();
        let mut instances = vec![];
        for (i, p) in self.props.iter_mut().enumerate() {
            p.transform = matrix(self.bodies[p.handle].position());
            let parts: Vec<_> = p
                .local_world
                .instances
                .iter()
                .map(|v| moved(v, p.transform, p.origin))
                .collect();
            let mut lo = V::ONE * f32::INFINITY;
            let mut hi = V::ONE * f32::NEG_INFINITY;
            for part in &parts {
                lo = lo.min(part.bounds.lo);
                hi = hi.max(part.bounds.hi);
            }
            room.entities[p.entity].bounds = PlayerCollider { min: lo, max: hi };
            if !held_indices.contains(&i) {
                for handle in self.bodies[p.handle].colliders() {
                    let a = self.colliders[*handle].compute_aabb();
                    room.colliders.push(PlayerCollider {
                        min: V(a.mins.x, a.mins.y, a.mins.z),
                        max: V(a.maxs.x, a.maxs.y, a.maxs.z),
                    });
                }
                instances.extend(parts);
            }
        }
        room.dynamic_world = World::new(instances);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::{controller::CharacterKind, interaction::Action, props, room::Entity};
    fn fixture(positions: &[V]) -> Room {
        let mut scene = crate::scene::Scene::default();
        scene.nodes.clear();
        let mut entities = vec![];
        let mut colliders = vec![];
        for (i, origin) in positions.iter().enumerate() {
            let mut prop = props::scene(props::PropKind::CerealBox);
            // Standalone prop scenes have only this prop's nodes.
            let id = format!("test-{i}");
            for (j, n) in prop.nodes.iter_mut().enumerate() {
                n.id = format!("{id}/{j}");
                if let Track::Fixed(p) = n.pos {
                    n.pos = Track::Fixed(p + *origin);
                }
            }
            scene.nodes.extend(prop.nodes);
            scene.materials.extend(prop.materials);
            let bounds = PlayerCollider {
                min: *origin - V(0.14, 0., 0.07),
                max: *origin + V(0.14, 0.44, 0.07),
            };
            colliders.push(bounds.clone());
            entities.push(Entity {
                id,
                label: "Cereal box".into(),
                bounds,
                action: Action::Inspect,
            });
        }
        let compiled = Compiled::new(scene, Path::new(".")).unwrap();
        Room {
            name: "Physics fixture".into(),
            simple_geometry: true,
            world: compiled.at(0.),
            dynamic_world: World::new(vec![]),
            compiled,
            colliders,
            entities,
            spatial: None,
        }
    }
    fn run(p: &mut PropPhysics, r: &mut Room, seconds: f32) {
        for _ in 0..(seconds * 120.) as usize {
            p.advance(STEP, &Controller::default(), r);
        }
    }
    #[test]
    fn dropping_a_prop_inside_either_character_recovers_and_allows_walking() {
        for kind in [CharacterKind::Scientist, CharacterKind::Feta] {
            let mut player = Controller::for_character(kind);
            let mut room = fixture(&[player.position + V(0., 0., -1.)]);
            let mut physics = PropPhysics::new(&mut room).unwrap();
            physics.set_held_for_player(0, 0);
            let body = &mut physics.bodies[physics.props[0].handle];
            body.set_translation(
                vector(V(
                    player.position.0,
                    player.feet_height() + 0.22,
                    player.position.2,
                )),
                true,
            );
            physics.drop_held();
            physics.advance(STEP, &player, &mut room);
            let before = player.position;
            assert!(room.colliders.iter().any(|c| c.contains(V(
                before.0,
                player.feet_height() + 0.1,
                before.2
            ))));
            player.update(Default::default(), 1. / 60., &room.colliders);
            assert!(
                (player.position - before).length() > 0.1,
                "did not escape dropped prop"
            );
            let recovered = player.position;
            for _ in 0..60 {
                player.update(
                    crate::viewer::controller::Movement {
                        forward: 1.,
                        ..Default::default()
                    },
                    1. / 60.,
                    &room.colliders,
                );
            }
            assert!(
                (player.position - recovered).length() > 1.,
                "player remained trapped"
            );
        }
    }
    #[test]
    fn falling_prop_lands_and_sleeps_without_static_ghost() {
        let mut room = fixture(&[V(0., 2., 0.)]);
        let mut p = PropPhysics::new(&mut room).unwrap();
        assert_eq!(p.props.len(), 1);
        assert!(room.world.instances.is_empty());
        run(&mut p, &mut room, 6.);
        let b = &p.bodies[p.props[0].handle];
        assert!(
            b.translation().y > 0.05 && b.translation().y < 0.25,
            "{:?}",
            b.translation()
        );
        assert!(b.linvel().norm() < 0.03);
        assert!(room.entities[0].bounds.min.1 > -0.02);
        assert!(room
            .hit(
                ViewRay {
                    o: value(b.translation()) + V(0., 0., 1.),
                    d: V(0., 0., -1.)
                },
                2.
            )
            .is_some());
        assert!(room
            .hit(
                ViewRay {
                    o: V(0., 2.2, 1.),
                    d: V(0., 0., -1.)
                },
                2.
            )
            .is_none());
    }
    #[test]
    fn impact_transfers_momentum_and_topples_another_small_prop() {
        let mut room = fixture(&[V(0., 0.23, -0.75), V(0., 0., 0.)]);
        let mut p = PropPhysics::new(&mut room).unwrap();
        let first = p.props.iter().position(|v| v.id == "test-0").unwrap();
        let second = p.props.iter().position(|v| v.id == "test-1").unwrap();
        p.bodies[p.props[first].handle].set_linvel(Vector::new(0., 0., 4.), true);
        run(&mut p, &mut room, 2.);
        let b = &p.bodies[p.props[second].handle];
        assert!(
            b.translation().z > 0.1,
            "no transferred momentum: {:?}",
            b.translation()
        );
        assert!(
            b.rotation().angle() > 0.2,
            "no rotation: {:?}",
            b.rotation()
        );
    }
    #[test]
    fn both_characters_pick_up_carry_drop_and_cannot_pick_through_wall() {
        for kind in [CharacterKind::Feta, CharacterKind::Scientist] {
            let player = Controller::for_character(kind);
            let origin = V(
                player.position.0,
                player.position.1 - 0.22,
                player.position.2 - 1.,
            );
            let mut room = fixture(&[origin]);
            let mut p = PropPhysics::new(&mut room).unwrap();
            let ray = ViewRay {
                o: player.position,
                d: V(0., 0., -1.),
            };
            assert!(p.toggle(&room, ray));
            assert!(p.held().is_some());
            for _ in 0..120 {
                p.advance(STEP, &player, &mut room);
            }
            let before = p.props[0].transform.p;
            assert!(p.toggle(&room, ray));
            assert!(p.held().is_none());
            run(&mut p, &mut room, 2.);
            assert!(p.props[0].transform.p.1 < before.1);
            let box_part = Part {
                shape: Primitive::Box,
                local: Mat::identity(),
                material: crate::scene::Material::default(),
                limb: 0,
            };
            room.world = World::new(vec![Instance::new(
                &box_part,
                Mat::trs(player.position + V(0., 0., -0.2), V::ZERO, V(1., 2., 0.05)),
            )]);
            assert!(!p.toggle(&room, ray));
        }
    }
    #[test]
    fn fixed_steps_are_frame_rate_independent_and_pause_has_no_catchup() {
        let mut end = vec![];
        for hz in [30, 60, 144] {
            let mut room = fixture(&[V(0., 2., 0.)]);
            let mut p = PropPhysics::new(&mut room).unwrap();
            for _ in 0..hz {
                p.advance(1. / hz as f32, &Controller::default(), &mut room);
            }
            p.pause();
            let before = p.props[0].transform.p;
            p.advance(f32::NAN, &Controller::default(), &mut room);
            assert_eq!(before, p.props[0].transform.p);
            end.push(before);
        }
        assert!((end[0] - end[1]).length() < 0.005 && (end[1] - end[2]).length() < 0.005);
    }
    #[test]
    fn carried_prop_cannot_tunnel_through_a_wall() {
        let mut room = fixture(&[V(0., 0.5, 0.)]);
        let n = crate::scene::Node {
            id: "wall".into(),
            shape: crate::scene::Shape::Box,
            material: "prop-yellow".into(),
            pos: Track::Fixed(V(0., 1., -0.4)),
            scale: Track::Fixed(V(2., 1., 0.025)),
            ..Default::default()
        };
        let mut scene = room.compiled.scene.clone();
        scene.nodes.push(n);
        room.compiled = Compiled::new(scene, Path::new(".")).unwrap();
        room.world = room.compiled.at(0.);
        let mut p = PropPhysics::new(&mut room).unwrap();
        let ray = ViewRay {
            o: V(0., 0.7, 1.),
            d: V(0., 0., -1.),
        };
        assert!(p.toggle(&room, ray));
        let mut player = Controller::default();
        player.position = V(0., 0.7, -0.1);
        player.yaw = 0.;
        player.pitch = 0.;
        for _ in 0..240 {
            p.advance(STEP, &player, &mut room);
        }
        assert!(p.props[0].transform.p.2 > -0.4, "carried prop crossed wall");
        p.drop_held();
        run(&mut p, &mut room, 2.);
        assert!(p.props[0].transform.p.2 > -0.4, "dropped prop crossed wall");
    }
    #[test]
    fn moving_furniture_removes_original_component_proxies() {
        let mut room = super::super::room::build().unwrap();
        let mut physics = PropPhysics::new(&mut room).unwrap();
        let index = physics
            .props
            .iter()
            .position(|p| p.id == "be2-table")
            .unwrap();
        let prop = &physics.props[index];
        for part in &prop.local_world.instances {
            assert!(
                !physics.static_colliders.iter().any(|c| {
                    (c.min - part.bounds.lo).length() < 0.005
                        && (c.max - part.bounds.hi).length() < 0.005
                }),
                "Original furniture parts must not remain fixed after extraction"
            );
        }
        let original = prop.origin;
        physics.bodies[prop.handle].set_translation(vector(original + V(8., 0., 0.)), true);
        physics.advance(STEP, &Controller::default(), &mut room);
        assert!(room
            .colliders
            .iter()
            .any(|c| c.contains(original + V(8., 0.35, 0.))));
        assert!(!room
            .colliders
            .iter()
            .any(|c| c.contains(original + V(0., 0.35, 0.))));
    }

    #[test]
    fn house_props_extract_and_all_shipped_maps_load_physics() {
        let mut room = super::super::maps::build(super::super::maps::MapId::House).unwrap();
        let p = PropPhysics::new(&mut room).unwrap();
        assert!(p.props.iter().any(|p| p.id == "house-apple"));
        assert!(p.props.iter().any(|p| p.id == "house-cereal"));
        assert!(!p
            .props
            .iter()
            .any(|p| p.id.contains("framed") || p.id == "house-fridge"));
        for name in ["house", "school-wing", "office", "convenience-store"] {
            let path = format!("assets/maps/starters/{name}.json");
            let mut room = super::super::authoring::MapDocument::load(Path::new(&path))
                .unwrap()
                .build()
                .unwrap();
            let p = PropPhysics::new(&mut room).unwrap();
            assert!(!p.props.is_empty(), "{name}");
        }
    }
}

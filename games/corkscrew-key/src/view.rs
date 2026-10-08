//! Read-only native 3D presentation; camera controls never alter world axes or poses.
use crate::*;
use macroquad::prelude::{
    is_key_pressed, is_mouse_button_down, mouse_position, mouse_wheel, KeyCode, MouseButton,
};
use vesper3d::portable::draw::*;
const NAVY: Color = Color::new(0.035, 0.055, 0.085, 1.);
const PANEL: Color = Color::new(0.065, 0.095, 0.14, 1.);
const MUTED: Color = Color::new(0.48, 0.61, 0.68, 1.);
const AXES: [Color; 3] = [
    Color::new(1., 0.39, 0.2, 1.),
    Color::new(0.3, 0.88, 0.76, 1.),
    Color::new(0.58, 0.55, 1., 1.),
];
const LEGAL: Color = Color::new(0.28, 0.95, 0.57, 1.);
const BLOCKED: Color = Color::new(1., 0.24, 0.32, 1.);
pub struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
    last: Option<(f32, f32)>,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.70,
            pitch: 0.45,
            distance: 13.5,
            last: None,
        }
    }
}
impl Orbit {
    fn eye(&self) -> [f32; 3] {
        [
            self.distance * self.yaw.sin() * self.pitch.cos(),
            0.5 + self.distance * self.pitch.sin(),
            self.distance * self.yaw.cos() * self.pitch.cos(),
        ]
    }
    pub fn project(&self, point: [f32; 3]) -> Point {
        use macroquad::math::Mat4;
        let matrix = Mat4::perspective_rh_gl(42_f32.to_radians(), 550. / 332., 0.1, 60.)
            * Mat4::look_at_rh(
                Vec3::from_array(self.eye()),
                Vec3::new(0., 0.4, 0.),
                Vec3::Y,
            );
        let clip = matrix * Vec3::from_array(point).extend(1.);
        let ndc = clip.truncate() / clip.w;
        Point::new(
            242 + ((ndc.x + 1.) * 275.) as i32,
            52 + ((1. - ndc.y) * 166.) as i32,
        )
    }
}
fn turn(v: [f32; 3], axis: usize, angle: f32) -> [f32; 3] {
    let mut p = v;
    let a = (axis + 1) % 3;
    let b = (axis + 2) % 3;
    let (s, c) = angle.sin_cos();
    p[a] = c * v[a] - s * v[b];
    p[b] = s * v[a] + c * v[b];
    p
}
fn moving(pose: Pose, axis: usize, sign: i32, t: f32, local: [f32; 3]) -> [f32; 3] {
    let v = std::array::from_fn(|j| (0..3).map(|k| local[k] * pose.basis[k][j] as f32).sum());
    let rotated = turn(v, axis, sign as f32 * t * std::f32::consts::FRAC_PI_2);
    std::array::from_fn(|j| {
        rotated[j] + pose.center[j] as f32 + if j == axis { sign as f32 * t } else { 0. }
    })
}
fn tint(c: Color, k: f32) -> Color {
    Color::new((c.r * k).min(1.), (c.g * k).min(1.), (c.b * k).min(1.), 1.)
}
fn face(mesh: &mut Mesh, points: &[[f32; 3]], color: Color) {
    let start = mesh.vertices.len() as u16;
    for p in points {
        mesh.vertices
            .push(Vertex::new(p[0], p[1], p[2], 0., 0., color));
    }
    for n in 1..points.len() - 1 {
        mesh.indices
            .extend([start, start + n as u16, start + n as u16 + 1]);
    }
}
// Chamfered cubes, with directional face shading and bright bevels. Color belongs
// to a LOCAL arm and therefore rotates with the rigid key throughout animation.
fn key_mesh(pose: Pose, axis: usize, sign: i32, t: f32) -> Mesh {
    let mut mesh = Mesh {
        vertices: vec![],
        indices: vec![],
        texture: None,
    };
    let colors = [GOLD, AXES[0], AXES[0], AXES[1], AXES[2]];
    for (index, cell) in LOCAL.iter().enumerate() {
        let transform = |p: [f32; 3]| {
            moving(
                pose,
                axis,
                sign,
                t,
                std::array::from_fn(|j| cell[j] as f32 + p[j]),
            )
        };
        let h = 0.5;
        let inset = 0.435;
        for axis_face in 0..3 {
            let a = (axis_face + 1) % 3;
            let b = (axis_face + 2) % 3;
            for side in [-1., 1.] {
                let points = [
                    [-inset, -inset],
                    [inset, -inset],
                    [inset, inset],
                    [-inset, inset],
                ]
                .map(|uv| {
                    let mut v = [0.; 3];
                    v[axis_face] = side * h;
                    v[a] = uv[0];
                    v[b] = uv[1];
                    transform(v)
                });
                let normal = moving(
                    pose,
                    axis,
                    sign,
                    t,
                    std::array::from_fn(|j| if j == axis_face { side } else { 0. }),
                );
                let origin = moving(pose, axis, sign, t, [0.; 3]);
                let light = (0.71 + (normal[1] - origin[1]) * 0.22 + (normal[2] - origin[2]) * 0.1)
                    .clamp(0.42, 1.);
                face(&mut mesh, &points, tint(colors[index], light));
            }
        }
        for a in 0..3 {
            let b = (a + 1) % 3;
            let c = (a + 2) % 3;
            for sa in [-1., 1.] {
                for sb in [-1., 1.] {
                    let points = [
                        (h, inset, -inset),
                        (h, inset, inset),
                        (inset, h, inset),
                        (inset, h, -inset),
                    ]
                    .map(|(x, y, z)| {
                        let mut v = [0.; 3];
                        v[a] = sa * x;
                        v[b] = sb * y;
                        v[c] = z;
                        transform(v)
                    });
                    face(&mut mesh, &points, tint(colors[index], 1.08));
                }
            }
        }
        for x in [-1., 1.] {
            for y in [-1., 1.] {
                for z in [-1., 1.] {
                    let points = [
                        [x * h, y * inset, z * inset],
                        [x * inset, y * h, z * inset],
                        [x * inset, y * inset, z * h],
                    ]
                    .map(transform);
                    face(&mut mesh, &points, tint(colors[index], 0.85));
                }
            }
        }
    }
    mesh
}
fn line(world: &mut World, a: [f32; 3], b: [f32; 3], width: f32, color: Color) {
    // Arbitrary segments use a tiny square prism instead of depth-free 2D lines.
    let va = Vec3::from_array(a);
    let vb = Vec3::from_array(b);
    let d = (vb - va).normalize();
    let right = d
        .cross(if d.y.abs() < 0.9 { Vec3::Y } else { Vec3::X })
        .normalize()
        * width
        / 2.;
    let up = d.cross(right).normalize() * width / 2.;
    let verts = [
        va - right - up,
        va + right - up,
        va + right + up,
        va - right + up,
        vb - right - up,
        vb + right - up,
        vb + right + up,
        vb - right + up,
    ];
    let mut mesh = Mesh {
        vertices: vec![],
        indices: vec![],
        texture: None,
    };
    for ids in [
        [0, 1, 2, 3],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ] {
        face(&mut mesh, &ids.map(|i| verts[i].to_array()), color);
    }
    world.mesh(mesh);
}
fn outline(
    world: &mut World,
    pose: Pose,
    axis: usize,
    sign: i32,
    t: f32,
    width: f32,
    color: Color,
) {
    for cube in LOCAL {
        for a in 0..3 {
            let b = (a + 1) % 3;
            let c = (a + 2) % 3;
            for u in [-0.51, 0.51] {
                for v in [-0.51, 0.51] {
                    let mut start = cube.map(|n| n as f32);
                    start[a] -= 0.51;
                    start[b] += u;
                    start[c] += v;
                    let mut end = start;
                    end[a] += 1.02;
                    line(
                        world,
                        moving(pose, axis, sign, t, start),
                        moving(pose, axis, sign, t, end),
                        width,
                        color,
                    );
                }
            }
        }
    }
}
fn button(scene: &mut Scene, rect: Rect, label: &str, selected: bool, color: Color) {
    scene.rect(20, rect, if selected { tint(color, 0.22) } else { PANEL });
    scene.rect(21, Rect::new(rect.x, rect.y, 3, rect.h), color);
    scene.text(
        22,
        label,
        Point::new(rect.x + 12, rect.y + rect.h - 9),
        15.,
        if selected { color } else { WHITE },
    );
}
impl Game for Corkscrew {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        if self.state.won {
            "Six sockets. A new way to think in space.".into()
        } else {
            "Turn + travel. Match gold. No timer. Undo freely.".into()
        }
    }
    fn device_input(&mut self, input: Intent) -> Intent {
        let (mx, my) = mouse_position();
        if is_mouse_button_down(MouseButton::Right) {
            if let Some((x, y)) = self.camera.last {
                self.camera.yaw -= (mx - x) * 0.007;
                self.camera.pitch = (self.camera.pitch + (my - y) * 0.006).clamp(-0.95, 1.25);
            }
            self.camera.last = Some((mx, my));
        } else {
            self.camera.last = None;
        }
        self.camera.distance = (self.camera.distance - mouse_wheel().1 * 0.7).clamp(7., 23.);
        for (key, code) in [
            (KeyCode::Key1, 1),
            (KeyCode::Key2, 2),
            (KeyCode::Key3, 3),
            (KeyCode::Right, 4),
            (KeyCode::Left, 5),
            (KeyCode::Space, 6),
            (KeyCode::Z, 7),
            (KeyCode::H, 8),
        ] {
            if is_key_pressed(key) {
                return command(code);
            }
        }
        if input.action && input.pointer.is_none() {
            command(6)
        } else {
            input
        }
    }
    fn draw(&self, scene: &mut Scene) {
        let state = &self.state;
        scene.rect(-10, Rect::new(0, 0, 800, 450), NAVY);
        scene.rect(
            10,
            Rect::new(0, 52, 238, 354),
            Color::new(0.047, 0.073, 0.108, 1.),
        );
        scene.text(20, "CORKSCREW KEY", Point::new(24, 32), 25., WHITE);
        scene.text(
            20,
            format!("{:02} / 06    {}", state.chamber + 1, NAMES[state.chamber]),
            Point::new(300, 30),
            15.,
            GOLD,
        );
        for a in 0..3 {
            button(
                scene,
                Rect::new(24 + a as i32 * 104, 74, 98, 30),
                ["1  X", "2  Y", "3  Z"][a],
                state.axis == a,
                AXES[a],
            );
        }
        button(
            scene,
            Rect::new(24, 119, 98, 29),
            "Left  -",
            state.sign < 0,
            AXES[state.axis],
        );
        button(
            scene,
            Rect::new(130, 119, 98, 29),
            "Right +",
            state.sign > 0,
            AXES[state.axis],
        );
        let hit = self.preview();
        let legal = hit.is_none();
        button(
            scene,
            Rect::new(24, 166, 204, 38),
            if state.docked {
                if state.chamber == 5 {
                    "SPACE  FINISH"
                } else {
                    "SPACE  NEXT"
                }
            } else {
                "SPACE  SCREW STEP"
            },
            true,
            if state.docked {
                GOLD
            } else if legal {
                LEGAL
            } else {
                BLOCKED
            },
        );
        button(scene, Rect::new(24, 216, 98, 29), "Z  Undo", false, TEAL);
        button(
            scene,
            Rect::new(130, 216, 98, 29),
            "H  Hint",
            state.hint,
            MUTED,
        );
        scene.text(
            20,
            format!(
                "Axis {}   {} quarter-turn",
                ["X", "Y", "Z"][state.axis],
                if state.sign > 0 { "+" } else { "-" }
            ),
            Point::new(24, 274),
            15.,
            AXES[state.axis],
        );
        scene.text(
            20,
            if state.docked {
                "SOCKET MATCHED"
            } else if state.flash > 0 {
                "BLOCKED - POSE UNCHANGED"
            } else if legal {
                "PATH CLEAR"
            } else {
                "PATH BLOCKED"
            },
            Point::new(24, 300),
            15.,
            if state.docked {
                GOLD
            } else if legal {
                LEGAL
            } else {
                BLOCKED
            },
        );
        scene.text(
            20,
            format!("Moves {}   Undo unlimited", self.moves()),
            Point::new(24, 326),
            14.,
            MUTED,
        );
        let center = state.pose.center;
        scene.text(
            20,
            format!("Center [{}, {}, {}]", center[0], center[1], center[2]),
            Point::new(24, 348),
            14.,
            WHITE,
        );
        let target = goal(state.chamber).center;
        scene.text(
            20,
            format!("Socket [{}, {}, {}]", target[0], target[1], target[2]),
            Point::new(24, 369),
            14.,
            GOLD,
        );
        scene.text(
            20,
            "WORLD AXES / CAMERA INDEPENDENT",
            Point::new(300, 392),
            12.,
            MUTED,
        );
        scene.text(20, LESSONS[state.chamber], Point::new(24, 420), 15., WHITE);
        scene.text(
            20,
            "Right-drag orbit   Scroll zoom   K save / L load   Esc pause",
            Point::new(24, 443),
            14.,
            MUTED,
        );
        if state.hint {
            let hint = if state.docked {
                "Matched! Space opens the next chamber.".into()
            } else if let Some((a, s)) = self.hint_move() {
                format!(
                    "Try {} {} then SPACE",
                    ["X", "Y", "Z"][a],
                    if s > 0 { "+" } else { "-" }
                )
            } else {
                "Explore freely; R restores the hint route.".into()
            };
            scene.rect(22, Rect::new(298, 348, 474, 26), PANEL);
            scene.text(23, hint, Point::new(306, 367), 14., TEAL);
        }
        let mut world = World::new(self.camera.eye(), [0., 0.4, 0.]);
        world.camera.fovy = 42_f32.to_radians();
        world.camera.z_near = 0.1;
        world.camera.z_far = 60.;
        world.cube(
            [0., -3.65, 0.],
            [9.6, 0.3, 9.6],
            Color::new(0.09, 0.13, 0.18, 1.),
        );
        for i in -4..=4 {
            world.cube([i as f32, -3.487, 0.], [0.016, 0.01, 9.], tint(MUTED, 0.38));
            world.cube([0., -3.486, i as f32], [9., 0.01, 0.016], tint(MUTED, 0.38));
        }
        // Chamber boundary: a light open cage, never an opaque wall hiding the puzzle.
        for x in [-4.5, 4.5] {
            for z in [-4.5, 4.5] {
                line(
                    &mut world,
                    [x, -3.5, z],
                    [x, 4.5, z],
                    0.024,
                    tint(MUTED, 0.4),
                );
            }
        }
        for y in [-3.5, 4.5] {
            for edge in [-4.5, 4.5] {
                line(
                    &mut world,
                    [-4.5, y, edge],
                    [4.5, y, edge],
                    0.024,
                    tint(MUTED, 0.4),
                );
                line(
                    &mut world,
                    [edge, y, -4.5],
                    [edge, y, 4.5],
                    0.024,
                    tint(MUTED, 0.4),
                );
            }
        }
        let obstacles = blocks(state.chamber);
        for (n, b) in obstacles.iter().enumerate() {
            let obstructed = hit.is_some_and(|h| h.block == n);
            let color = if obstructed {
                BLOCKED
            } else {
                Color::new(0.19, 0.27, 0.34, 1.)
            };
            world.cube(b.center.map(|v| v as f32), b.size.map(|v| v as f32), color);
            let p = b.center.map(|v| v as f32);
            let half = b.size.map(|v| v as f32 / 2.);
            for a in 0..3 {
                let u = (a + 1) % 3;
                let v = (a + 2) % 3;
                for s in [-1., 1.] {
                    for t in [-1., 1.] {
                        let mut start = p;
                        start[a] -= half[a];
                        start[u] += half[u] * s;
                        start[v] += half[v] * t;
                        let mut end = start;
                        end[a] += half[a] * 2.;
                        line(&mut world, start, end, 0.028, tint(color, 1.7));
                    }
                }
            }
        }
        let axis_color = AXES[state.axis];
        // Fixed world axes anchored below the toy. Camera orbit cannot rotate them.
        for a in 0..3 {
            let start = [-3.3, -3.3, 3.3];
            let mut end = start;
            end[a] += 1.6;
            line(
                &mut world,
                start,
                end,
                if state.axis == a { 0.07 } else { 0.04 },
                AXES[a],
            );
            world.sphere(end, 0.10, AXES[a]);
            let label = self.camera.project(end);
            scene.text(
                12,
                ["X", "Y", "Z"][a],
                Point::new(label.x + 5, label.y + 5),
                14.,
                AXES[a],
            );
        }
        let goal_pose = goal(state.chamber);
        outline(&mut world, goal_pose, 0, 1, 0., 0.038, GOLD);
        if state.turning == 0 && !state.docked {
            let color = if legal { LEGAL } else { BLOCKED };
            outline(
                &mut world, state.pose, state.axis, state.sign, 1., 0.019, color,
            );
            // Ghost shape at each quarter of the path exposes the swept arm geometry.
            for t in [0.25, 0.5, 0.75] {
                outline(
                    &mut world,
                    state.pose,
                    state.axis,
                    state.sign,
                    t,
                    0.008,
                    tint(color, 0.45),
                );
            }
            for cell in LOCAL {
                let mut prev = moving(
                    state.pose,
                    state.axis,
                    state.sign,
                    0.,
                    cell.map(|v| v as f32),
                );
                for n in 1..=20 {
                    let next = moving(
                        state.pose,
                        state.axis,
                        state.sign,
                        n as f32 / 20.,
                        cell.map(|v| v as f32),
                    );
                    line(&mut world, prev, next, 0.012, tint(color, 0.78));
                    prev = next;
                }
            }
            if let Some(h) = hit {
                let local = LOCAL[h.cube].map(|n| n as f32);
                let at = moving(
                    state.pose,
                    state.axis,
                    state.sign,
                    h.sample as f32 / SAMPLES as f32,
                    local,
                );
                world.sphere(at, 0.12, BLOCKED);
                if h.block >= obstacles.len() {
                    let mut end = at;
                    end[state.axis] += 0.4;
                    line(&mut world, at, end, 0.06, BLOCKED);
                }
            }
        }
        if state.turning > 0 {
            let t = 1. - state.turning as f32 / TURN_TICKS as f32;
            let smooth = t * t * (3. - 2. * t);
            world.mesh(key_mesh(
                state.from,
                state.last_axis,
                state.last_sign,
                smooth,
            ));
        } else {
            world.mesh(key_mesh(state.pose, 0, 1, 0.));
        }
        // Center beacon clarifies translation while the differently colored arms show orientation.
        let pos = if state.turning > 0 {
            let t = 1. - state.turning as f32 / TURN_TICKS as f32;
            moving(
                state.from,
                state.last_axis,
                state.last_sign,
                t * t * (3. - 2. * t),
                [0.; 3],
            )
        } else {
            state.pose.center.map(|v| v as f32)
        };
        line(
            &mut world,
            [pos[0], -3.48, pos[2]],
            [pos[0], pos[1] - 0.55, pos[2]],
            0.012,
            tint(axis_color, 0.55),
        );
        world.cube(
            [pos[0], -3.46, pos[2]],
            [0.55, 0.022, 0.55],
            tint(GOLD, 0.42),
        );
        scene.world(0, Rect::new(242, 52, 550, 332), world);
    }
}

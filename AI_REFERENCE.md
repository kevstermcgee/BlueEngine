# Vesper3D — AI reference

Create one JSON scene, validate it, render a contact sheet, then an MP4.
`vesper3d validate scene.json`
`vesper3d contact scene.json board.png --quality draft`
`vesper3d render scene.json movie.mp4 --quality high`

Minimal scene: `{"nodes":[{"id":"friend","shape":"robot"}]}`

Units: meters, seconds, degrees; Y up, characters face +Z. Primitive sphere/box/cylinder/cone span -1..1; torus outer radius 1, tube .22 by default, around Y. Robot feet at 0, height 2.45. Tree height 3; rocket 2.77. Crystal extends Y=-.8..1.4.

Root (all optional): version=1, size=[1280,720], fps=30, duration=6, camera, world, materials={}, lights=[default studio light], nodes=[], audio=null. Unknown fields are errors. Colors are linear RGB [0..1].

Node: id (required unique string), shape="group"|"sphere"|"box"|"cylinder"|"cone"|"torus"|"crystal"|"robot"|"tree"|"rocket"|"mesh", material="default", parent=null, pos=[0,0,0], rot=[0,0,0], scale=[1,1,1], ease="smooth", motion={}, visible=null, mesh=null, tube=.22 (torus tube radius, .005 to .45), repeat={}.
pos/rot/scale accept a vector OR [[time,[x,y,z]],...]. Keys strictly increase; values hold outside the key range. ease: linear, smooth, smoother, hold. Euler rotations interpolate numerically (use 0 to 360 for a full revolution). Parent transforms and visibility are inherited. Transform order: scale, X/Y/Z rotation, translation. Scale must stay positive.

Motion: bob=0 (vertical amplitude), frequency=1 (Hz), phase=0 (degrees), spin=[0,0,0] (degrees/sec), orbit=0 (XZ radius around pos), orbit_speed=30 (degrees/sec), walk=0, wave=0. walk and wave animate robot joints; set to 1. All procedural motion is an absolute function of time, so frames can be requested in any order.

Repeat: count=1 (max 4096), step=[0,0,0], ring=0 (XZ radius), jitter=[0,0,0] (seeded +/- extent), seed=1. Copies this node's own geometry; children are not copied. Offsets are in node-local units and inherit its scale/rotation. Ring, step and jitter can be combined. Example: a tree with repeat={"count":12,"ring":4} creates a grove from one node.

Material: color=[1,1,1], roughness=.4 (min .04), metallic=0, emission=0, checker=null (alternate color on world XZ unit squares). Define once under materials and reuse by name. Emission adds glow but does not illuminate neighbors: add a light.

Camera: pos=[7,4.5,9], target=[0,1,0] (both accept tracks), fov=42 (vertical degrees), orbit=0 (degrees/sec around target), aperture=0 (lens radius), focus=10 (distance along view axis), ease="smooth". Use high/ultra for depth of field.

Light: pos=[-4,8,5] (accepts tracks), color=[1,.88,.72], power=90, radius=1.2 (soft shadow source). lights replaces default list; use [] for ambient only.
World: sky=[.025,.045,.09], horizon=[.16,.22,.28], ambient=.24, fog=.012, exposure=1.15, bloom=.22.

Audio: optional local audio filename relative to scene. Audio starts at zero, is padded/truncated to video duration. Mesh: local OBJ path (positions and polygon faces; convex polygons fan triangulated; flat normals, one material per node). Assets must remain inside the scene directory, including resolved symlinks. No network fetching or script execution.

Other commands: frame scene.json still.png --time 2; bench scene.json --time 2; doctor; reference. Quality draft=1, standard=4, high=9, ultra=25 primary samples/pixel, with increasing shadow/AO/reflection samples. --width 640 preserves aspect, --threads 8 limits CPU workers, --overwrite permits replacing an existing completed output, --ffmpeg PATH selects encoder. PNG rendering needs no FFmpeg. MP4 uses H.264/yuv420p and optional AAC. Frame count=ceil(duration*fps). Progress is JSON stderr; final status is JSON stdout. Ctrl+C cancels and preserves the previous output. Output is committed only after success.

Limits: scene JSON 8 MiB, 4096 nodes, 16 parent levels, 16 lights, 1024 materials, 4096 keys/track, 200000 expanded primitives, OBJ 32 MiB/100000 triangles. Even image dimensions 16..3840, up to 8294400 pixels. 1..120 fps, duration .01..3600 sec. Use draft contact sheets before expensive high-resolution renders.

BE2 reusable props: viewer::props::scene(PropKind) creates a standalone scene. PropKind is CerealBox, Chair, Table or Apple. Origin is floor-level, Y-up. Standalone JSON assets are in assets/props. No changes to the version-1 scene contract.

## BE2 editable maps (separate document contract)

The graphical and headless BE2 runtimes accept `--map FILE`. This is a versioned map document with `schema_version: 1`, `name`, `scene`, named `colliders`, and `entities`; it is not a raw Vesper scene. `scene` uses the inherited schema but map v1 restricts it to static unparented box/sphere/cylinder/cone nodes, fixed positive transforms, inspect-only entities, no repeats, imported meshes or external audio. Colliders contain min/max vectors. Entity fields are id, label, bounds, action (`inspect`). Spawn remains the existing default controller spawn.

Use `be2-tools export-house` to obtain a valid example and `apply` for checked transactions. The full workflow, commands, constraints and output preservation semantics are in tools/README.md; patch structure is in tools/patch.schema.json. `export-scene` converts only visual geometry back to a raw Vesper scene for the offline renderer. Runtime names/labels are now owned strings, so custom maps do not leak static allocations. No changes were made to the inherited scene JSON schema.

# 0004: One authoring contract and compatibility names

Status: Accepted. Extends 0002; historical sources remain available.

## Context
Blue inherited Vesper scene documents (offline visuals/animation), MapDocument v1
(validated playable geometry, collision and semantic IDs), and procedural Rust maps.
They are related layers, not interchangeable files. The latter two construct the same
Room; Vesper alone cannot describe playable collision or entities.

## Decision
Use MapDocument as the portable authoring boundary. SceneBuilder is an owned,
transactional adapter to its existing edits, not another scene format or an ECS.
Native export-lab makes the development map available to the same tools as the house.
Keep procedural reference maps and standalone Vesper assets. Do not rewrite legacy
maps or introduce a scripting language to deliver a smaller prototype interface.

The project/repository is BlueEngine; keep the Cargo package `be2`, library `vesper3d`
and executable names for compatibility. Preserve original history, license and
attribution. BLUE_ARCHITECTURE, BLUE_V1_* and VESPER_* are historical references;
BE2_ARCHITECTURE is current, ARCHITECTURE/AI_REFERENCE cover the offline renderer.

## Consequences
AI authors can generate one validated document and load it in both runtimes. Owned
string IDs avoid retaining mesh/physics borrows across updates. SceneBuilder boxes
are static; catalog props use the existing size/material extraction convention.
Large furniture and wall art stay fixed. Explicit arbitrary rigid-body components,
custom spawn profiles and gameplay documents remain future schema work. The builder
does not hide those limitations or claim to provide a complete game framework.

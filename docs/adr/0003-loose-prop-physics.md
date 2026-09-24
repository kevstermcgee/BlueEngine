# 0003: Runtime loose-prop rigid bodies

Status: Accepted, 2026-09-23

## Context
Both characters need E pickup/drop and dropped props must tumble, settle and knock other props over. Existing props are baked static primitives with semantic bounds.

## Decision
Use pinned Rapier 0.26.1 in the shared Rust library with fixed 120 Hz stepping, compound convex proxies and CCD. Separate catalog prop nodes from static geometry once at client startup, recognizing the existing material/bounds convention so shipped v1 maps remain compatible. Reuse local meshes under rigid transforms. Carrying uses a bounded velocity servo on an ordinary dynamic body; scenery can block it. E selection is occlusion-aware and drop restores gravity.

## Consequences
This adds a physics dependency and runtime state but leaves saved map schemas and semantic IDs intact. Generic custom materials stay fixed until given an explicit future authoring contract. Convex proxies approximate curved surfaces; objects do not break apart. Single-local-player carry state does not implement network ownership. Physics tests run without the renderer; the existing multi-player movement benchmark remains unchanged.

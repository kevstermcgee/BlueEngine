# 0002: Explicit static map documents

Status: Accepted; retrospective description of the current code.

## Context

Content authors need to inspect and change maps without loading engine source or rewriting gameplay. Visual geometry, collision bounds and interaction entities have different responsibilities.

## Decision

Keep the procedural Rust House as the default. Both runtimes explicitly load custom documents through `--map`. [MapDocument](../../src/viewer/authoring.rs) v1 supports validated static primitives and inspection entities. Patch batches operate on a clone, validate and compile before saving to a new destination. Existing outputs are preserved.

Keep visual, collision and semantic components separate, with explicit IDs and edit selections. The source-free [authoring interface](../../tools/AUTHORING.md) discovers supported operations; new gameplay still requires Rust work.

## Consequences

An export is an editable snapshot, not a change to the built-in map. Export-generated room/collider IDs may change after procedural source edits; custom IDs must remain stable. Removing a visual alone can leave collision behind. Routes, rays, audits and captures check different properties and must be chosen for the edit. Schema v1 is not an arbitrary game scripting or networking API.

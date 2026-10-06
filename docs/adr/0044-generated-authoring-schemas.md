# ADR 0044: Generate authoring schemas from opt-in Rust types

Status: accepted, subject to the existing complete engine/CI verification.

## Context

GameDocument and patch JSON schemas duplicated existing Serde shapes. MCP
tools/list contained thirteen handwritten input shapes. Asset-pack schemas
described a Python catalog contract without a typed Rust authoring definition.
Schema updates required locating and reconciling these copies. A generated
shape must retain v1 bounds, strict objects, nullable fields and semantic gates.

## Decision

Use pinned schemars behind native-only `schema-generation` and a separate
`schema-validation` feature. Existing Rust types derive metadata only with the
feature enabled. Type-local transform hooks retain existing v1 constraints;
game limits come from existing constants. A typed asset-pack authoring contract
generates the catalog schema without changing runtime model/collider loading.

`be2.py schemas --write` uses the real Cargo artifact, existing stateless input
identity and atomic replacement. `--check` runs exact committed-output parity,
valid fixtures and independently invalid cases. The full checker includes this
suite and feature-specific Clippy in addition to every prior gate.

MCP argument structs produce a committed schema bundle embedded in normal
builds. Keep the current transport and dispatch: no rmcp replacement is needed
to remove handwritten shape duplication. Direct tools remain accessible.

## Consequences

Maintainers edit Rust shape definitions and regenerate rather than reconstruct
JSON shapes. Constraints still need explicit Rust hooks and meaningful tests.
The output is larger because it includes type documentation, defaults and
numeric formats; output-size improvement is not claimed. The optional validator
adds maintenance-check compilation cost, isolated from ordinary game builds.

Schema parity does not certify gameplay, references, visuals or packaging.
Published v1 `$defs` anchors are preserved from generated component shapes;
the standalone asset `vec3` fragment retains its historical cardinality.
The existing runtime validators and shipping checks remain mandatory. Explicit
null support in presentation and optional rules is preserved. Asset vectors
now express the three components already required by typed/runtime validation;
MCP count/seed advertise the unsigned native range, including full u64 seeds.
No new gameplay capability, scripting language or verification cache is added.

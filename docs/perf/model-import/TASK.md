# Furnished-scene source-authoring trial

Run each replicate in a fresh agent context. Read AGENTS.md and the compact context packet; inspect selected public references before targeted source lookup. The baseline route uses public procedural primitives; the after route uses the public model guide and CC0 catalog. Do not compile or run games in agent trials: the coordinator serially verifies all artifacts under the same render harness afterward.

Write only a Rust module with `pub fn furnish(world: &mut vesper3d::portable::draw::World)` and a structured result diary in an ignored per-replicate directory. The coordinator supplies the floor, camera and HUD. Retain GPU-uploaded models between frames in the after route; initialize them only with an active graphics context. Explain how absolute benchmark embeddings become relative game-local art paths with licenses/provenance.

The room must contain:

- A table centered at x/z=(0,0), bounding dimensions approximately (2.10372, 0.816835, 1.118433), with distinguishable tabletop and legs.
- Four chairs centered at x/z=(-0.9,-0.95),(-0.9,0.95),(0.9,-0.95),(0.9,0.95), each approximately (0.5,1.175,0.5), facing inward with backs outward and distinguishable seats/legs/backs.
- A square floor lamp at x/z=(1.65,-0.65), approximately (0.3,2.15,0.3), with a warm shade and gray metal base/stem.
- Bases at y=0. Wood factor (0.8962264,0.6015712,0.3931559), warm shade (1,0.9137255,0.5882353), metal (0.74061054,0.822866738,0.8396226). No PBR or physical light emission is required.

Record first/end UTC clock observations, elapsed seconds, direct files/extent inspected, issued commands and failures, implementation attempts, authored bytes/hash, verification states and limitations. Token accounting must be marked unavailable unless actual session telemetry exists. A source file is not passing compilation/render evidence.

The original nine recorded trials are summarized in measurements.json. Replay sources normalize include paths, formatting and documented lint details; authored sizes/hashes refer to the originals. Times include source and diary authoring; they exclude coordinator compilation, Clippy corrections, packaging and rendering. First clock observations do not cover all initial instruction-reading time. A rerun can test the method, but cannot recreate an LLM's exact timing or source output.

# Context efficiency review

Reviewed against the BE2 working tree on 2026-09-23.

| Proposal | Existing support and action |
|---|---|
| Root context file | AGENTS.md already covers conventions and checks. Added a compact architecture/lookup section and a CLAUDE.md import to avoid duplicated guidance. |
| Trait APIs / thin implementations | Current shared boundary is concrete Rust types, not shared server traits. Document the small simulation API; defer traits until a real second implementation or transport adapter establishes a useful contract. |
| rustdoc | Public documentation was sparse at the simulation boundary. Added method semantics and a runnable example, plus strict library documentation builds to local checks and CI. This is focused coverage, not complete documentation of every public item. |
| ADRs | Architecture narratives existed, but no decision records. Added two retrospective records for implemented boundaries, with tradeoffs and an index. |
| Repo map | tools/FEATURES.json already maps features to files and checks, and author.py provides bounded content discovery. Reuse those and generated rustdoc instead of maintaining another symbol snapshot. |
| Integration tests | tests/authoring.rs and tests/cli.rs already cover editor/runtime and offline CLI contracts. Added a public-API simulation lifecycle test for latched jump, held input, rejected input and player departure. A network match-flow test must wait for networking. |
| Smaller modules | Existing controller, simulation, camera, wrench, props and platform modules already separate concerns. Largest files are roughly 975 lines (client), 737 (geometry), 719 (house), including tests where present. Defer mechanical splitting until a feature edit establishes a useful boundary; file size alone does not justify churn. |
| Glossary | Added a compact domain glossary distinguishing current concepts from planned multiplayer. |

Start with AGENTS.md, select a feature in tools/FEATURES.json, and consult only the relevant rustdoc/source/test. Use BE2_ARCHITECTURE.md for current architecture; older Blue/Vesper documents are subsystem references and may describe superseded viewer limitations.

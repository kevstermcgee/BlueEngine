# Idea Forge

A BlueEngine utility for finding a gameplay rule worth prototyping. Play it in the
BlueEngineGames download catalog. Install the Windows EXE and use it offline.

Generate from 36 deliberately authored, distinct mechanics: 12 puzzle, 12 action,
and 12 strategy. Each brief contains a concrete rule, play loop, reason it could
be fun, smallest prototype, and a design risk to test. Story is optional and off
by default. This is an offline seed generator, not an AI service; fun needs
playtesting and worldwide originality needs comparison with existing games.

Ideas are shuffled using a reproducible seed. Generation never repeats an idea
in the same library and explicitly reports exhaustion. Previous/Next revisit
history; Keep saves favorites; View Kept filters the library. Changing genre does
not discard anything. Restart generates another prompt and preserves your library.

Click a button. Arrows or a controller select a button (gold border), and
Space/A activates it. Up/down jumps four buttons. Esc pauses. The engine saves
progress automatically on this device; K saves an explicit checkpoint and L loads
it. Keep the engine save directory to retain your library between installations.

## Batch export

The rendering-free CLI exports usable Markdown or JSON briefs without a window:

```sh
cargo run --no-default-features --bin idea-forge-cli -- --seed 42 --count 6 --genre puzzle --story
cargo run --no-default-features --bin idea-forge-cli -- --count 12 --genre action --format json --output action-ideas.json
```

The output file must be new. Same seed/options reproduce the same batch. The
desktop utility keeps ideas in the engine's local library; batch file export is
provided by the CLI. It does not generate complete game projects.

Source lives at `games/idea-forge` in BlueEngine. The source checkout's Cargo
dependency points to the engine two directories above. Build from that checkout,
not a detached copy of this folder. Windows distribution uses a pinned public engine revision, the shared native
package verifier, installer and updater. Linux is a development verification target.

Development findings and suggested engine improvements are recorded in
`docs/feedback/2026-10-07-idea-forge.md` in the BlueEngine repository and indexed
through its learning ledger.

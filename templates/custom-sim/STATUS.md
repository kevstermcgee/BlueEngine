# {{title}} Status

## Completed
- Project scaffolded from BlueEngine's `custom-sim` template: pure simulation library, window binary,
  determinism tests, save states (F5 / F9), identity, generated icon, packaging and desktop-shortcut tooling.

## Next Steps
- Replace the starter rules in `src/lib.rs` with the real game; keep `Sim::step` deterministic.
- Edit `assets/identity.json` (title, tagline, controls) and regenerate the icon.
- Add tests for each rule; drive the window with `--capture` and `--script` and look at the frames.
- Finish with `scripts/blue ship`.

Identity Lab: one deterministic four-switch puzzle, three game-owned presentations.

Notebook, instrument and arcade use the same rule authority, snapshots, shared UI
actions, keyboard/mouse/controller conventions and audio runtime. Each supplies its
own layout, runtime font and named audio bank. No default cues or particle style.

Focused rules, lint and isolated native presentation smoke passed on Linux at
960×540, 640×360 and 800×800. Actual start/gameplay/pause/win/loss frames were captured;
start/gameplay/pause/win and resized frames were visually inspected. The repeatable
smoke also checks focus gating, storage, named submissions and missing/invalid assets.
Physical speakers, controller hardware and Windows foreground behavior are not
certified by those captures. Windows packaging/presentation and the full engine
suite are required CI gates; see the final run evidence, not this status alone.

Project requirements: `game.project.json`; native Windows distribution, Linux dev.
See README.md, scripts/presentation_smoke.py and docs/analysis/creative-identity.md
in the engine checkout for commands, assets, measurements and remaining limits.

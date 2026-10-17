# Peelturn

Every turn leaves a trace. A quiet spatial routing puzzle in twelve compact boards.

Fill every target socket **and exhaust every packet**. Each collision deposits one bead at the stopping cell, then the remaining packet turns clockwise and continues automatically. Deposits become permanent walls. Passing through a socket does not fill it. Launch order matters: waiting packets block movement, and launching opens their starting cells.

Tab cycles packets (or click one). Arrows choose the launch direction and preview only the first stop and its clockwise exit. Space launches. Backspace undoes one whole launch, even mid-route or after a loss. R retries the current board, preserving your place in the twelve-board sequence. Space/Enter continues after a clear. H opens the rules, Escape pauses, K saves, L loads, M toggles sound, F toggles fullscreen. The shared native client owns save storage and automatic progress; no game-owned save files.

An initially blocked direction spends nothing. A blocked clockwise exit with beads remaining jams and loses the attempt. Exhausting all packets with empty sockets, or trapping every remaining packet, also loses. Undo and retry are unlimited. There is no timer, score quota, or move budget.

## Development and delivery

`cargo test --no-default-features` tests authoritative rules without graphics. `cargo run --locked` runs the shared native client. Windows x64 EXE installers are the delivery format; Linux is a development/verification target. No browser runtime.

Use the engine's `tools/be2.py check --game games/peelturn --loop inner`, then `--loop integration` and `--loop shipping`. From this folder, `python3 scripts/check.py` checks the project. On Windows, `python scripts/ship.py ship --no-install` builds and smoke-tests the isolated native package; `installer prepare`, `installer build --iscc PATH`, and isolated `installer test --stage PATH` remain necessary before distribution. Do not install or publish from this development session.

The canonical two-d scaffold and icon tool supply pinned native tooling, executable resources and this game's copper P icon. The UI uses flat layered primitives, a six-by-six playable plane, copper/mint packet identity, filled socket rings, first-impact arrows, and numbered impact flashes. There is no duplicated presentation-side rule authority.

`assets/boards.json` contains the authored board data and solution witnesses. Witnesses are consumed only by tests and `verification_input`; normal play never reads them to choose moves. The public verification route selects, aims and launches all 29 packets across all twelve boards. Tests also cover initial rejection, clockwise self-stopping, jam/empty/trapped failures, necessary launch order, passing versus depositing, undo/retry, deterministic replay, invalid snapshot rejection, and engine save-frame continuation.

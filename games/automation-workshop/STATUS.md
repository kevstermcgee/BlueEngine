# Automation Workshop Status

Two independently owned machines share coherent inventory transactions. Public
commands exercise escrow, production, full destinations, pending cancellation,
recovery and upgrades. Engine snapshots support exact-content save/load and an
explicit v1-to-v2 copy migration; the real v1 fixture is retained.

The native console is offline, with game-owned drawing, controls and camera.
The optional online headless fixture reuses the same rules and existing netplay
infrastructure; complete snapshots, match timeout and late-join limits are explicit.

Run `../../docs/WORKSHOP.md` for current public contracts and exact checks.
Verification receipts belong to their recorded revision and platform; an offline
test pass does not certify a Windows installer or physical controls/audio.

Project runtime: `legacy-native` custom Rust; presentation: `2d`.
Delivery: native Windows x64 installers. Linux is development/verification.

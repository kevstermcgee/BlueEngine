# ADR 0039: Shared 2D presentation and verified static browser artifacts

Browser gameplay portions are superseded by [ADR 0049](0049-retire-browser-gameplay.md).
The historical browser commands below are not current supported workflows. Native contracts remain.


Status: Accepted

2D is a presentation branch, not a second engine. `runtime` reuses the existing fixed-step simulation,
input accumulator, seeded RNG, playback, Snapshot frame/migrations and synthesizer. Native devkit
paths continue to reexport these types. `two_d` adds integer collision/kinematic bodies/triggers and a
small layered scene API; its optional client supplies Macroquad presentation and common player controls.
WASM excludes the native viewer, 3D physics and networking dependencies. Platform-specific imports stay
in storage/client boundaries and one browser adapter, not game rules.

Games declare presentation/targets/input/networking in game.project.json. The two-d starter is the
recommended offline 2D authoring surface. Web is currently 2D/offline only; unsupported combinations
fail before Cargo. Existing undeclared native projects keep their workflow. Upgrade planning identifies
the new path; its verification invokes the game's own web wrapper for declared browser targets.

`web build` settles seeded locks offline, runs rendering-free game tests, builds pinned WASM and
assembles JavaScript from the exact locked Miniquad and quad-snd sources. The prebundled Macroquad
loader was stale against its dependency ABI; permissive missing-import stubs hid the error. We reject
missing imports and use an isolated real-browser gate. Scene glyphs are preloaded before drawing to
avoid invalidating queued atlas textures when notices introduce new text.

Static packages declare every path/hash and carry catalog metadata and the expected winning native
state hash. Manifest integrity requires no display. Browser smoke copies only declared files outside
the checkout, loads WASM, compares the public-input route hash, exercises an actual device mechanic,
checks gesture-activated playback and persistence across reload/failure, and rejects undeclared or
external runtime requests. These are automated signals, not proof of human audibility or every browser.

Publishing accepts a verified static package and returns a receipt. The directory backend has no
provider dependencies or fabricated URL. The optional GitHub Pages adapter preserves the central native
library, waits for deployment and verifies remote file hashes before reporting a live URL. Another
host only needs this narrow adapter contract. Local build/verify never need hosting credentials.

Deferred: browser multiplayer, 3D browser builds, touch-first controls, a general editor, advanced
physics and a configurable 2D sound/music bank. PlatformStorage uses the existing versioned BE2SAVE
bytes and explicit localStorage errors; it is not cloud synchronization.

# Leo evidence and verification

Starting main revision: `678f4a9ab5e8`. Engine primitives and the first game were
introduced in `2c2cebd4a3af`. The game streams deterministic fields/forests with
precision-safe integer origin rebasing, exact engine-owned saves and a repeating
day/night clock. Its smiling boy has rounded proportions, a full chestnut scalp
and fringe, and no backpack. The saved day appears in the main/pause menu only.
Sun/moon shadows reuse the engine map; checked nature recordings and an original
adaptive score follow authoritative time, with persisted independent audio toggles.

At preliminary revision `2c2cebd4a3af`, Linux passed but Windows's game project
check failed on an extensionless native-tools path. Source-tree captures also
hid omitted packaged audio. Revision `f823064` corrected both, captured the shipped
executable without a source-assets override, and passed [Linux and Windows Engine
checks](https://github.com/kevstermcgee/BlueEngine/actions/runs/37238875211) and
[sandbox/package verification](https://github.com/kevstermcgee/BlueEngine/actions/runs/37238914125).

That development revision's full local check passed 10 gates: 939 default Rust,
791 headless Rust, 52 native-authoring and 474 executed Python tests, plus fmt,
Clippy in both feature modes, warning-free rustdoc and the headless boundary check.
Both rendered audio bundles and all seven loop-quality reports passed. Discovery
checks cover all four committed regression sets and individual procedural/audio cases.
The later hair/lighting refinement adds a horizon-cutover regression to the game
tests: the one shadowed light fades before switching sun/moon directions.
Earlier results do not certify a later revision: release readiness requires the
Engine checks and sandbox run matching the checkout's exact commit.

The inspected packaged Linux capture ran 503 executed frames, crossed chunk
origins with exactly 49 loaded chunks, completed two days and showed `Leo - Day 3`
in the pause menu. Quick-save/resume preserved the exact clock and origin. Both
banks were Ready and submitted on every frame to null ALSA; music off/on persisted.
A missing nature WAV failed explicitly with exit 1; muted rendering submitted no
audio. Portrait, daylight, sunrise, sunset, night and menu PNGs were inspected.
Full directional shadows were active; the walking view had no day overlay.

CI's independent engine and game matrices retain all gates and existing required
`test` names. `leo-images` arrives early for art review; `leo-Linux`/`leo-Windows`
retain the package, private launcher and project reports. The final guards reject
failed, cancelled or skipped lanes. Inspect the capture from the exact revision
when reviewing a later art change; an early picture is not a green verdict.

No local game was launched. Native evidence uses Linux CI/software GL/null ALSA.
The observed shadowed run's own-work median was 98.18 ms; its printed 60 fps was
scripted fixed-step time, not measured hardware performance. Hardware audibility,
subjective listening, real device controls and target-PC frame pacing remain
unverified. Flat ground and juvenile stylized plants are deliberate bounds; this
is not a terrain editor or ecosystem simulation. No live server was deployed.

## First-person and scenery reuse

Refinement source `ae96c0b` starts normal native and portable play at Leo's eye
position, including a resumed save. The native camera adds Lifecycle's pending
look between fixed ticks; rendering never mutates the authoritative simulation.
The boy remains in the shadow pass and in explicit portrait captures.
Six focused scene tests passed, including first-person save/resume, interpolated
walking/hopping, immediate look without double consumption, and batch invalidation.

The [packaged Linux capture](https://github.com/kevstermcgee/BlueEngine/actions/runs/37663513445)
passed 503 frames, crossed a chunk origin, completed two days, resumed the exact
save, and submitted both audio banks to null ALSA. All captured state hashes
matched the pre-change run. Daylight, night at the chunk boundary, a later day and
the portrait were inspected. Static meadow batches rebuilt 16 times in 503 frames
and remained unchanged during stationary frames; their seed/origin/detail keys
avoid stale geometry after loading or rebasing.

The same scripted portrait workload produced a byte-identical PNG before and
after this change. Its reported own-work median changed from 65.32 ms in
[the baseline run](https://github.com/kevstermcgee/BlueEngine/actions/runs/37660916388)
to 56.98 ms in the refinement run. These are separate hosted Linux software-GL
runs, not a controlled same-machine benchmark or target-PC frame-rate evidence.
The printed 60 fps is scripted time. Real mouse/gamepad feel and target-PC frame
pacing still require hands-on play; no local game window was opened.

The full local canonical check passed all 39 commands, with build/test concurrency
bounded and low process priority. An initial Unix registry fixture failed with
Text file busy; its exact isolated rerun and the complete retry passed, and the
failed report was retained. Full Linux/Windows engine checks and Windows sandbox
validation also passed on the refinement implementation commit. Local verification
timings are recorded in docs/perf; they are not gameplay frame-rate measurements.

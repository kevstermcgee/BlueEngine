# Retire browser gameplay, preserve native composition

Status: Accepted

Supersedes the supported browser portions of ADRs 0039/0040 and the optional-browser
exception in ADR 0045/0046. User policy is Windows EXE-only games; leaving mandatory
WASM/Chromium CI, default web targets and native helpers importing browser release
logic maintained a competing supported workflow after public browser removal.

Decision: retire browser/WASM gameplay builds, preview, package, reproduction and
publication for all destinations. Fail legacy web/publish commands before any setup,
build or output write. Archive browser-only toolkit, templates and historical gates.
WASM compilation receives an explicit retirement error. Remove its CI job, dependency
installation and aggregate requirement. Retain every native engine/game/Leo matrix,
headless boundary, portable feature tests, release build and isolated shipping smoke.

Keep Windows/Linux/macOS native authoring, Scene/World/geometry, fixed-step GameLogic,
input, audio, storage, saves, native netplay and public portable/two_d imports. Do not
remove native-owned files merely because they contain browser names. Leo's alternate
presentation remains a native leo-portable executable with the original Sim/SimState.
Unreachable upstream target cfg/low-level ABI branches are not supported web entry points;
removing all of those branches is outside this retirement and unnecessary for native use.

Existing project metadata must explicitly remove web and web_build only after a native
client exists. Root and generated helpers reject retired requirements; no silent migration.
Maintain EXE download-site operation and publisher ownership/deletion protections.
Archived browser-only tests no longer assert supported browser shipping; new retirement
behavior tests retain native project constraints and ensure commands have no side effects.

Acceptance: new defaults and all four ported games/Leo have native targets only; no native
task helper imports archived release code; old web prepare/build/publish fail read-only;
CI has Linux/Windows guards without browser prerequisites; package smoke remains required.
Verification: canonical be2.py check, native CI, publisher check and game target gates.
Windows rendered/hardware audio/input evidence remains separately required, not inferred
from compilation or schema success. No build-speed or loading-speed claim.

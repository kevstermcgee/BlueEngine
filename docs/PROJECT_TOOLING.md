# Project tooling

Generated projects use the engine's maintained `templates/game_check.py`,
`game_dev.py`, `game_ship.py` and `game_project.py`. Familiar commands remain:
`scripts/blue dev`, `python scripts/check.py`, `scripts/blue ship --no-install`
(PowerShell: `scripts/blue.ps1`). Launchers resolve their project from their own
location, so callers can use absolute paths from any directory.

`scripts/tooling.json` records the engine location relative to the game, the
revision provenance and a SHA-256 content pin for the complete shared Python
implementation. Moving a project also requires keeping its Cargo engine path and
this recorded location consistent. A path dependency is **not a Git revision
pin**. Keep the engine checkout at the recorded revision, or review an engine
upgrade deliberately; tooling refuses different content instead of silently
using another checkout or an executable on PATH. The content digest also records
reviewed local edits that a Git revision alone cannot identify.

`BE2_TOOLING_ROOT` can select an equivalent engine distribution; it must match the
content pin. For source tooling, checks let Cargo prove the selected dependency's
freshness in the `itest` profile and use Cargo's executable artifact path (an
explicit debug/release/fast artifact verifies its own profile). Shared
`CARGO_TARGET_DIR` and Cargo configuration are supported. `BE2_TOOLS`/`--tools`
must match that fresh artifact byte-for-byte; a mismatched or stale override fails
with the correct path. An exported engine package can instead provide its existing
`PACKAGE-MANIFEST.json`: every declared input and the packaged native executable
must match its recorded hash. Missing or unverifiable inputs fail with instructions
to restore the package or use a source checkout. `--help` never compiles.

Migration is explicit. After reviewing the engine update, run:

```sh
python3 ENGINE/tools/game_tooling.py migrate GAME
python3 ENGINE/tools/game_tooling.py check GAME
```

The migration preflights every launcher before writing. It accepts current thin
entries, canonical files and the reviewed historical variants recorded in
`tools/game_tooling_inventory.json`; other custom implementations are rejected.
It preserves game source, assets, metadata, build resources and additional scripts.
The legacy `--refresh-project` command continues to repair the requirements helper;
on migrated projects it refreshes compatible launchers. A stale bootstrap pin must
be upgraded through the engine command above. The inventory names all 13 migrated
consumers and distinguishes historical drift from intentional audio preparation.

Game-owned preparation belongs in `scripts/tooling_hooks.py`:

```python
def prepare_package(project):
    # project.root is the game; project.assets is its authored asset directory.
    # Generate into a staging directory and commit only completed outputs.
    ...
```

This Python hook runs after identity/icon validation and before the package build
or replacement. Reverie's eight-movement rendering and Leo's audio preparation
use it. Hook failure reports the script and original error and leaves completed
package outputs in place. Existing custom executable resources, fonts, controls,
cameras, Rust simulations and presentation remain game-owned. Use argument arrays
for subprocesses; never interpolate content into shell or PowerShell code.

The required Python gate checks every migrated launcher's compatibility and pin,
while permitting arbitrary authored hooks and other files. Fresh scaffold
integration checks exercise stock, custom and 2D/3D starters. Content-only checks
validate current content and leave Rust/device/package behavior unverified;
shipping still requires isolated native package verification. Native Windows x64
installers require Windows resources, launch and smoke evidence; Linux evidence
does not certify that platform or physical input/audio devices.

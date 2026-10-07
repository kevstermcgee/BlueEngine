# Browser gameplay is retired

BlueEngine supports native games. Public delivery is Windows x64 EXE installers;
Linux CI/development and existing macOS native authoring remain available. There is
no supported WASM gameplay build, browser package, preview or browser publication route.
`be2.py web ...`, `be2.py publish ...` and old game web launchers fail with
`BROWSER-RETIRED`/migration guidance before installing, compiling or writing outputs.
The EXE download website remains supported; it does not run games in a browser.

## Existing projects

Keep the game source, stable IDs, assets, authoritative rules and native saves.
Verify a native client exists before deliberately removing `web` from
`game.project.json.targets` and removing `web_build`. Metadata alone does not port
an implementation. Keep the declared native development targets; Windows distribution
requires `python scripts/ship.py ship` or `ship --no-install` on Windows, including
resources, assets, integrity and isolated packaged-game smoke.

The four former browser games now retain native clients: lantern-run, pocket-breaker,
orchard-watch and lantern-grove. Leo retains its normal native client and optional
native `leo-portable` presentation; historical browser-named source modules do not
imply a supported browser target. See docs/PORTABLE_GAMES.md and docs/TWO_D.md.

Node/ws, Chromium, browser CDP and the WASM Rust standard library are no longer engine
prerequisites or CI gates. Native input, audio, storage, 2D/3D composition and headless
simulation are retained. Upstream target cfg branches and unreachable low-level ABI
code are not supported browser entry points; native ownership is not inferred from names.
Historical toolkit source is in docs/archive/browser-gameplay; it is not runnable guidance.

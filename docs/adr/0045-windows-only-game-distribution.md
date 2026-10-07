# ADR 0045: Distribute BlueEngine games as Windows EXE installers

Status: accepted.

## Context

The owner requires native Windows EXE play only. BlueEngineGames' website
merged five browser artifacts into its otherwise installer-only release feed.
Four portable games lacked native release definitions despite already having
native clients, identities and shipping scripts. Their individual manifest
exports also incorrectly specified filenames as destination directories.

## Decision

BlueEngineGames distributes native Windows x64 EXE installers only. Remove
website web-play cards, payloads, PWA installation and browser filters. Keep
favorites, presentation/networking filters, search, details, version history
and verified immutable EXE links. Audit the complete generated deployment;
replace stale output only after a successful build. Existing update ZIPs remain
internal release/updater payloads, never website download offerings.

Use the existing portable native client and standard ship/installer workflow
for Lantern Run, Pocket Breaker, Orchard Watch and Lantern Grove. Preserve
rules, native input, audio, snapshots and icons. Fix collection file destinations
to their parent directories; test actual exported build files and reject
directories where release manifests require files. Add native release definitions
and Linux/Windows verification for the exported games. Genuine authored test
fixtures are explicitly excluded from product releases.

Reject browser publication to BlueEngineGames before commands or writes, and
reject automatic browser integration into a native catalog. Optional engine
browser APIs and their regression gates remain available for other destinations.
Game target metadata describes tested runtime capability, not permission to
publish browser games. New BlueEngine game tasks select Windows distribution.

## Consequences

The website never substitutes browser play for a missing Windows installer.
New native games appear after the complete Windows release passes, including
installer/updater tests. Removal from hosting cannot erase offline copies users
already downloaded. No simulation, networking, headless, public API or save
format change is required.

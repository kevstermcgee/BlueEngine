# ADR 0040: Portable composition, mobile controls, installation and one library

Browser gameplay portions are superseded by [ADR 0049](0049-retire-browser-gameplay.md).
The historical browser commands below are not current supported workflows. Native contracts remain.


Accepted 2026-10-05; extends ADR 0039.

New games use the existing shared fixed-step client under the `portable` name. Its painter layers
compose optional depth-tested 3D views with 2D sprites/maps/UI; presentation requirements describe the
result for discovery instead of banning useful dimensional combinations. Legacy native renderers and
transport remain intact. New CLI scaffolding defaults to portable; explicit legacy templates remain.

A small project declaration selects dpad/paddle/tap input on coarse-pointer devices, below the canvas.
All devices feed the same Intent. Browser verification includes actual touch in portrait/landscape,
gesture audio, automatic save restoration and a network-disabled reload. Physical mobile devices remain
separate evidence. Native storage uses user data with migration; browser storage is origin-local, per the
user's explicit device-local preference. Manual saves and automatic progress have independent slots.

Static artifacts gain a scoped app manifest and declared-file asset cache, enabling offline browser
installation for all portable dimensionalities. Installation is separate from publishing. No account,
cloud sync or platform capability fallback is introduced. Legacy native UDP/QUIC/world APIs require a
port or supported adapter for browser use; metadata does not make them portable.

The publisher joins browser/native catalog entries by game ID and preserves current installers and
version histories. Favorites are local, explicit on storage failure, and take precedence over secondary
sorting. Site integration is additive to current native styles/building; it must verify the actual joined
feed before pushing. Static hosting stays behind the existing publisher boundary.

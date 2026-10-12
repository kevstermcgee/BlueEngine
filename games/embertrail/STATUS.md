# Embertrail status

Implemented: 32 explicit courses in eight themed worlds; Mote and five enemy types; eight guardian stages; momentum, sprinting, variable-height jumps, crouching, jump buffering and coyote time; power caches, breakable clay blocks, rolling shells, bouncing ember seeds, secret grottos, 96 optional moon shards, checkpoints, lives, scoring, an unlockable atlas, original pixel artwork and music.

Playable outputs: `dist/embertrail` (Linux), the Embertrail desktop shortcut, and `dist/web/` (browser with offline installation).

Verification: deterministic native/browser input replay clears the opening course; exact snapshot continuation, collision and combat behavior, save validation, campaign transitions, and terrain-route checks across all 32 courses pass. Chromium desktop and separate portrait/landscape touch gates pass, including save/load, audio API submissions, offline reload, focus handling, and update recovery. The declared-file Linux package passes its isolated graphical smoke check under Xvfb. Desktop/mobile and native captures were inspected.

Physical phones, controllers, speaker output, and a complete human playthrough of every course have not been tested. Published and verified: [https://kevstermcgee.github.io/Embertrail/](https://kevstermcgee.github.io/Embertrail/). Public source retrieval and a clean build from an empty target reproduce the exact package; independent desktop/touch checks pass, and all 13 deployed runtime files match the release manifest.

# Hardening feedback audit

Verified against 90949ab and its ancestors; this pass preserves every original
ID, game, date and observation. A promoted entry links verified code, not an AI
claim. Browser-only work is closed as wontfix under ADR 0049. Duplicate aliases
retain occurrence evidence but no longer compete in context packets.

Before: 130 entries, 68 open. After: 12 duplicate, 41 open, 75 promoted, 2 wontfix.

## Verified closures

| Entries | Fixing commit | Evidence |
|---|---|---|
| L-070–073, 075–077 | [ad03fb6](https://github.com/kevstermcgee/BlueEngine/commit/ad03fb6) | Every-file/isolated package checks, audible AudioStatus proof, settings navigation, save migration guidance, executable launcher and source author discovery. |
| L-078 | [80d6609](https://github.com/kevstermcgee/BlueEngine/commit/80d6609) | xcapture omits implicit mute for an explicit audible verification. |
| L-094, 105, 124 | [90949ab](https://github.com/kevstermcgee/BlueEngine/commit/90949ab) | Scaffold resolves the authoring engine path before writing the game manifest; sibling/local project tests cover it. |
| L-123, 130 | [cebdaee](https://github.com/kevstermcgee/BlueEngine/commit/cebdaee) | Shared GameLogic device_input hook is present. This historical game run changed engine code; future runs now refuse that path. |
| L-085 | [ae96c0b](https://github.com/kevstermcgee/BlueEngine/commit/ae96c0b) | Leo cached static batches replace per-frame plant vertex packing. |
| L-082 | [b1cc2c2](https://github.com/kevstermcgee/BlueEngine/commit/b1cc2c2) | Native portable composition remains implemented; browser delivery is retired. |
| L-099 | [933a0da](https://github.com/kevstermcgee/BlueEngine/commit/933a0da) | Root policy and coordinator require native Windows delivery. |

L-083 and L-098 concern retired browser publication/audio, so are wontfix rather
than claims of a native fix. Canonical closures retain their aliases.

## Five most frequent still-open topics

Count original report occurrences, including duplicate aliases. These are recurring
concrete topics, not counts of broad area tags. Storage/IO combines three distinct
host resource incidents and is explicitly not a duplicate merge. Ties sort by topic.

| Topic | Reports | Independent games | Original entries | Still-open evidence |
|---|---:|---:|---|---|
| Project tooling discovery | 4 | 1 | L-106, L-113, L-114, L-118 | templates/game_check.py find_tools does not search the canonical itest folder. |
| Xvfb/display socket availability | 4 | 2 | L-107, L-108, L-115, L-126 | Capture failed before rendering in restricted hosts; no engine source fix is claimed. |
| ALSA panic despite mute | 3 | 2 | L-112, L-120, L-129 | Native audio backend initializes despite mute; captured logs do not prove working playback. |
| Host storage/IO pressure | 3 | 2 | L-050, L-097, L-104 | Separate cache growth, USB IO stalls and failed receipt writes remain host resource pain. |
| Particles during accelerated verification | 2 | 1 | L-111, L-119 | Particles age by rendered-frame time while verification advances many simulation ticks. |

Source discovery and ALSA initialization need separate regression-backed fixes;
this reconciliation does not silently close them. No private session transcripts
or fabricated per-finding token attribution were added.

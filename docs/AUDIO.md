# Audio: generated sound, generated ambient music, and settings that survive a relaunch

Every sound a game built on BlueEngine plays is computed, not recorded (`devkit::synth`): an AI agent
without a microphone can give a new game a full soundscape — sound effects and now background music —
in a few calls, and measure what came out (`peak`, `rms`, `spectral_centroid`, ...) instead of listening
to it. `kit::audio::SoundBank` plays it: sound effects by index, and looping music stems that fade with
the action (`start_music`/`update_music`).

## Generating ambient background music

`devkit::synth::AmbientSpec`/`ambient_loop` renders a seamless, beat-free pad-and-air loop for
background listening (sleep, focus, a menu) — no kick, no lead, just held chords with slow attacks and
long overlapping releases, a lowpass that breathes slowly, a soft noise "air" bed and a few sparse high
chimes. It is deterministic in its spec and checked by measurement (peak, periodicity, spectral
centroid), not by ear: `tools/devkit/synth.rs`'s ambient tests are the readable spec of what "calm"
means here.

Generate a track straight to a file, no Rust required:

```sh
be2-tools ambient-music OUT.wav [MINUTES] [SEED] [--major]
be2-tools ambient-music OUT.wav [MINUTES] --title="Spooky Kart" --tagline="Eight haunted karts, one hollow to win."
```

`MINUTES` defaults to 2 (clamped 0.5-10). Without `--title`, `SEED` changes the chord progression and
chime placement and `--major` switches from the default natural minor. Prints
`{"ok":true,"out":...,"seconds":...,"minor":...,"root_midi":...,"brightness":...,"seed":...,
"peak":...,"rms":...}`.

**Every game should not sound the same.** `devkit::synth::ambient_spec_for(title, tagline)` derives key,
mode, chord count and timbre from a game's own title (the seed; a different title is a different track)
and a small, deliberately tiny mood heuristic (`mood_bias`) read from the words in `title` and `tagline`
together: a handful of curated dark/moody words ("haunted", "spooky", "crypt", ...) and light/cheerful
words ("sunny", "garden", "festival", ...). **Any** detected mood word decides the mode outright (a
title that reads as spooky is always minor, never a 1-in-20 chance of coming out major); a genuinely
neutral title still gets a varied, seeded mode so two unthemed games do not sound alike either. This is
a light touch for an obviously-themed title, not a text-understanding model — the custom-sim template
calls it with the game's own `assets/identity.json` title and tagline, so a fresh game is not identical
to every other game made from the template, and a spooky one leans the way you would expect. A game
whose theme those words cannot capture is free to build its own `AmbientSpec` by hand in `render_audio`
instead (set `minor`, `root_midi`, `brightness`, `chords` directly) — `ambient_spec_for` is a reasonable
default, not the only way to use `ambient_loop`.

## Settings that survive a relaunch

`devkit::save::Settings` (`music`, `sfx` volumes 0-1, `music_on`, `sfx_on`, `sensitivity`,
`fullscreen`) is a small JSON file (atomic write, never fatal to load) stored with
`devkit::beside_exe("settings.json")`, next to the executable so it survives both a relaunch and
`scripts/ship.py package` re-packaging (ADR 0017 already protects `settings.json` there). `music_level()`
and `sfx_level()` return 0 when the matching toggle is off without touching the remembered volume, so
turning music back on restores what it was. The custom-sim template loads it at startup and passes
`settings.sfx_level()`/`settings.music_level()` straight into `SoundBank::start`.

Every game made from the custom-sim template gets a Settings screen for free: `GameShell`'s pause menu
gained a fourth entry, Settings, with Music/Sound toggle buttons and a "Save music (.wav)" button (a
drawn download arrow, not a font glyph, so it never depends on a font having that character). Toggling
flips `Settings` and re-applies the volume to the running `SoundBank` immediately, then persists.
Downloading writes the exact bytes already generated for the music stem (captured once via an
`Arc<OnceLock<Vec<u8>>>` filled on the audio worker thread, never regenerated synchronously, so clicking
it can never hitch) to the player's Downloads folder (`devkit::downloads_dir`), named after the game
and never overwriting an earlier save of it (`devkit::unique_path`: `Name.wav`, `Name (2).wav`, ...).
This is a new, non-breaking method, `GameShell::local_menu_with_audio`; the original `local_menu` is
unchanged, so no existing game is forced to adopt it.

## What this does not do

- No MP3 or other lossy encoding: the download is the WAV the engine already generates. A real MP3
  encoder is a new, nontrivial dependency (either a linked codec library for every shipped game, or
  shelling out to a system `ffmpeg` a player is not guaranteed to have); this was deliberately deferred.
- The stock (`GameDocument`) runtime has no audio pipeline at all yet, generated or otherwise, so there
  is nothing yet to attach settings or a download button to there. Settings/Records, `beside_exe`,
  `downloads_dir` and `unique_path` are path-agnostic and ready to use the day it does.
- `downloads_dir` returns the plain default location on every platform; a Linux user's custom
  `XDG_DOWNLOAD_DIR` in `user-dirs.dirs` is not consulted.

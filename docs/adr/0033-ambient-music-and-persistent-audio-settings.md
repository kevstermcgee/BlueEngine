# ADR 0033: Generated ambient music, and audio settings that survive a relaunch

Status: Accepted

## Context

`devkit::synth` already let an AI agent generate sound effects and an upbeat dance-style backing loop
(`MusicSpec`/`music_loop`, 124 bpm, kick-driven) without recording or hearing anything, but nothing
calm/ambient existed, and no shipped game or template actually played background music: `start_music()`
was called with zero stems everywhere. Separately, `devkit::save::Settings` already had `music`/`sfx`
volume fields "almost every action game exposes," but zero games loaded or stored a `Settings` value;
`SoundBank` was always constructed with hard-coded volumes. `assets/identity.json`'s own ADR (0017)
already protects a `settings.json` file from being deleted on re-packaging, but nothing wrote one.

Two requests followed from this: every game should have audio settings (toggle music/sound, surviving a
relaunch), and a way to generate good ambient/soft background music cheaply, with a visible way for a
player to save it.

## Decision

- **`devkit::synth::AmbientSpec`/`ambient_loop`**: a seamless, beat-free pad-and-air loop (held chords
  with slow attacks and long overlapping releases that wrap seamlessly, a lowpass that breathes on a
  slow LFO, a soft filtered-noise air bed, sparse high chimes), reusing the existing chord/scale/Mix/
  filter/envelope primitives `music_loop` already uses. Checked by measurement (determinism, seamless
  loop, soft peak, no beat periodicity, brightness moving the spectrum), the same way `music_loop` is,
  not by ear. `be2-tools ambient-music OUT.wav [MINUTES] [SEED] [--major]` generates one straight to a
  file using the existing generic `write_new` (destination must be new, matching every other generated-
  file command).
- **`devkit::synth::ambient_spec_for(title, tagline)`**: every game should not sound the same. It seeds
  key/mode/chord-count/timbre from the title (so a different title is a different track) and additionally
  reads a small, deliberately tiny `mood_bias` from a curated dark/light word list across the title and
  tagline together (e.g. "haunted"/"spooky" vs "sunny"/"garden"). Any detected word decides the mode
  outright — a clearly spooky title is always minor, never a 1-in-20 chance of major — while a neutral
  title still gets a varied, seeded mode. `be2-tools ambient-music --title=TEXT [--tagline=TEXT]` exposes
  the same thing from the command line (SEED/`--major` are ignored in that mode, since the text decides).
- **`devkit::save::Settings`** gained `music_on`/`sfx_on` booleans (default true) alongside the existing
  `music`/`sfx` volumes; `music_level()`/`sfx_level()` return 0 when off without touching the remembered
  volume, so toggling back on restores it. Backward compatible: struct-level `#[serde(default)]` with a
  custom `Default` impl means an old `settings.json` missing these keys loads as on, not off. Also added:
  `downloads_dir` (the plain default Downloads location per platform), `unique_path` (never overwrites:
  `Name.wav`, `Name (2).wav`, ...), `sanitize_filename`.
- **`GameShell::local_menu_with_audio`**: a new, non-breaking method alongside the existing `local_menu`
  (unchanged). It adds a fourth pause-menu entry, Settings, with Music/Sound toggle buttons and a "Save
  music (.wav)" button (a drawn download-arrow icon, not a font glyph, so no font needs to contain it).
  The shell owns no audio state itself — it takes a small `AudioMenu{music_on,sfx_on}` and returns a
  `MenuOutcome{quit,toggle_music,toggle_sfx,download_music}` — so it stays independent of `devkit::save`
  and `kit::audio`, and a game decides what each outcome means.
- **The custom-sim template** wires all of it end to end: `Settings::load` at startup feeds
  `SoundBank::start`'s volumes; the render closure computes
  `ambient_loop(&ambient_spec_for(&identity.title, &identity.tagline))` once on the worker thread (so the
  game's own `assets/identity.json` decides its track) and fills an `Arc<OnceLock<Vec<u8>>>` the "Save
  music" button reads from, so
  downloading can never hitch (no synchronous regeneration — `music_loop`-class renders cost real time,
  as the engine's own perf test already measures). `sounds.update_music(dt, &[1.])` is now called every
  frame, which also fixes a latent bug: every template called `start_music()` but never `update_music`,
  so a stem (had one ever been added) would have played forever at its starting volume of 0.

## Consequences

Every new custom-sim game ships with real background music, players can turn music/sound off and have
it stick across a relaunch, and the exact generated track is one click away as a `.wav` in their
Downloads folder. `GameShell::local_menu` is untouched, so no existing game is forced onto the new
screen; adopting it is tracked as an `optional_adoption` migration (`tools/upgrade_migrations.json`,
BlueEngine's own upgrade-workflow tool) for games that want it.

## Rejected or deferred

- **MP3 or any lossy encoding for the download**: needs either a new linked codec library in every
  shipped game, or shelling out to a system `ffmpeg` a player is not guaranteed to have. WAV is what the
  engine already generates losslessly with zero new dependencies; revisit only if a real need appears.
- **Wiring audio (and therefore these settings) into the stock `GameDocument` runtime**: that runtime has
  no audio pipeline of any kind today, generated or recorded. Bolting settings onto nonexistent audio
  would be hollow; `Settings`/`downloads_dir`/`unique_path` are path-agnostic and ready the day it does.
- **Recording template provenance or auto-migrating existing games**: out of scope here; existing games
  are guided, not changed, via the upgrade-workflow registry (ADR 0032).
- **Honoring a Linux user's custom `XDG_DOWNLOAD_DIR`**: `downloads_dir` returns the plain default
  location on every platform; parsing `user-dirs.dirs` was not judged worth the added complexity yet.

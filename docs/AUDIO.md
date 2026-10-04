# Audio: generated sound, generated ambient music, and settings that survive a relaunch

## Normal authoring path: a named audio project

Start with `be2-tools audio describe`, then edit the small reference
[`project.json`](../assets/audio/observatory/project.json). No Rust source exploration is needed for
presets, imported clips, authored polyphonic stereo music or adaptive layers:

```sh
be2-tools audio validate project.json
be2-tools audio render project.json NEW_BUNDLE_DIRECTORY
be2-tools audio check NEW_BUNDLE_DIRECTORY
cargo run --profile fast --example audio_preview -- NEW_BUNDLE_DIRECTORY
python tools/audio_report.py NEW_BUNDLE_DIRECTORY/preview-mix.wav --loop --fail-on-issues
```

`validate` checks schema/ranges/resource estimates; `render` also reads and validates imported audio,
computes the PCM assets and publishes `bank.json` last in an exclusively reserved directory. It fails
without replacing an existing bundle. `check` rereads every runtime file and verifies PCM format,
sample counts, checksums and equal music-layer lengths. Keep `project.json` in source control, render
once before packaging, and ship the whole bundle. Edit audio data and rerender to a new directory;
the runtime executable needs no rebuild and performs no music synthesis at startup. Bundles are
versioned data, not an opaque compiled cache. Checksums detect stale/corrupt files; they are not
cryptographic signatures. Rendering is repeatable on one platform; floating-point DSP does not promise
identical bytes across operating systems.

The schema is version 1. Unknown fields, invalid names and out-of-range values fail explicitly:

- `effects` maps names to `{gain, source}`. A source is `{kind:"preset", preset:"footstep"}` (all
  meaningful variants), `{kind:"score", score:...}`, or `{kind:"wav", file:"relative/path.wav"}`.
  Imported clips must be nonempty, untruncated 44.1 kHz mono/stereo PCM16. Convert other encodings
  before import; channels are preserved. Import paths cannot be absolute or contain `..`.
- `music` is optional: `{kind:"generated", bpm, bars, root_midi, minor}` produces `base`, `melodic`,
  `lead`; `{kind:"ambient", seconds, root_midi, minor, chords, brightness}` produces `ambient`;
  `{kind:"score", score:...}` produces the score's named layers.
- A score has `bpm` (40–240), `beats`, and `layers`. Each layer has a unique `name`, `gain` (0.001–1),
  `instrument` and `notes`. Notes use `at` and `beats` in beats, `midi` (12–108), `velocity` (0.001–1),
  and optional equal-power `pan` (-1 left to +1 right). Notes must fit the score; music release tails
  wrap, while effect scores retain their complete final release. Chords are simultaneous notes.
- Instruments default to sine, 8 ms attack, 80 ms decay, 0.6 sustain and 120 ms release. Opt in to
  triangle, PolyBLEP saw/square, explicit ADSR seconds and `lowpass_hz` (40–20,000). Velocity and gain
  preserve dynamics; notes are not individually normalized.

The project `headroom` defaults to 0.85 (allowed 0.1–0.95). Effects share attenuation across their
variants; music uses one attenuation factor based on the sum of absolute layer samples, so *any*
subset at levels 0–1 stays under the ceiling, even when the full signed mix cancels. Relative layer
balance survives attenuation. Simultaneous effect voices plus music can still sum beyond that ceiling
in the legacy playback backend; this is asset headroom, not a final real-time master limiter.
Each file records peak, RMS, DC and boundary jump. Empty/silent output, nonfinite samples or DC >0.02
fail rendering. Bounds: 64 effects, 16 score layers, 4096 notes and 600 seconds of note synthesis per
score, a conservative project budget of 180 stereo-seconds, at most 180 seconds per imported clip,
64 MiB rendered files. Split large soundscapes into banks instead of disabling bounds.

Runtime integration after creating the window:

```rust,no_run
use vesper3d::viewer::kit::{AudioBank, AudioState};
# async fn example() -> Result<(), String> {
let mut audio = AudioBank::load("assets/audio/bank", false, 0.8, 0.5).await?;
audio.sounds.poll().await; // each frame: bounded decoder submissions
if audio.sounds.state() == AudioState::Ready {
    audio.play("shutter", 1.0)?; // once, after a confirmed gameplay event
    audio.music(1.0 / 60.0, &[("orbit", 0.65), ("signal", 0.3)])?;
}
# Ok(()) }
```

`play` cycles cue variants and rejects unknown names, invalid volumes and incomplete/failed loads.
`music` validates the whole named mix before changing it; omitted layers fade to zero. Supply real
frame seconds. `sounds.sfx_volume`/`music_volume` accept the existing settings' effective levels;
`sounds.stop_music()` stops loops and resets fades. `sounds.state()` distinguishes Loading, Ready,
Muted, Empty and Failed; `sounds.errors()` gives worker/file/decoder diagnostics. Muted at startup
skips asset loading; load again to enable it later, or start unmuted with settings volumes at zero.
Missing files or checksum mismatches fail the entire worker result before decoder submission.
Existing `SoundBank::start` and its indexed APIs remain supported; `start_checked` adds a fallible
render closure. Its music-only banks now become ready, and worker failures become explicit.

`audio_report.py --loop` checks the wrapped boundary using its discontinuity detector rather than
flagging valid nonzero endpoints as one-shot clicks. It reports sample jump and whether the seam scan
was available (very short files remain unresolved). Internal clicks, clipping, DC, silence and
approximate loudness still apply. Use `--json` for machine-readable evidence or `--spectrogram OUT.png`
for inspection. These measurements do not assess composition or perceived quality.

This milestone uses the existing playback backend. Ready means checked assets submitted to its
decoder, not audible-device acknowledgement. Loop starts are separate backend calls, not a promised
sample-clock group. Panning and pitch here are authored into PCM; live spatial/distance or per-voice
pitch control remains unsupported. The headless project renderer never opens an audio device.
Custom instruments can still use the lower-level `devkit::synth` API below.

BlueEngine can compute a game's sound without recordings (`devkit::synth`): an AI agent
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

## Not every game needs music

A generated soundtrack is a default, not a requirement. The custom-sim template's `HAS_MUSIC` constant
(top of `main.rs`) turns the ambient background track on or off; `render_audio` skips generating one
entirely when it is off (no wasted worker-thread time), and the Settings screen adapts on its own — see
below. Turn it off when music would work against the game: a mechanic that depends on precise or
diegetic audio (rhythm timing, sound-based detection, a soundtrack the game is itself about), or a game
whose feel ambient pads simply do not suit. The agent building a specific game is better placed to judge
that fit than a blanket default; do not treat "every game ships with music" as a rule to force through
when it visibly does not work. Sound effects are a separate concern and are unaffected either way.

## Settings that survive a relaunch

`devkit::save::Settings` (`music`, `sfx` volumes 0-1, `music_on`, `sfx_on`, `sensitivity`,
`fullscreen`) is a small JSON file (atomic write, never fatal to load) stored with
`devkit::beside_exe("settings.json")`, next to the executable so it survives both a relaunch and
`scripts/ship.py package` re-packaging (ADR 0017 already protects `settings.json` there). `music_level()`
and `sfx_level()` return 0 when the matching toggle is off without touching the remembered volume, so
turning music back on restores what it was. The custom-sim template loads it at startup and passes
`settings.sfx_level()`/`settings.music_level()` straight into `SoundBank::start`.

Every game made from the custom-sim template gets a Settings screen for free: `GameShell`'s pause menu
gained a fourth entry, Settings, with a Sound toggle always, and (when `AudioMenu::has_music` is true) a
Music toggle and a "Save music (.wav)" button (a drawn download arrow, not a font glyph, so it never
depends on a font having that character). A game with `HAS_MUSIC = false` gets the Sound toggle only —
the screen never offers a control for a track that does not exist. Toggling
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

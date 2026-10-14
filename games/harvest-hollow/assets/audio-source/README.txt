Harvest Hollow — original soundtrack

Composed for this project with BlueEngine score synthesis. No recordings,
third-party melodies or licensed song material are used. All four pieces use
original 16-bar melodies with warm plucked accompaniment and gentle bass.

Dew on the Seedlings (morning): G major, 104 BPM, 36.92 seconds.
Wheelbarrow Promenade (afternoon): C major, 112 BPM, 34.29 seconds.
Rain on the Potting Shed (rain): D Dorian, 84 BPM, 45.71 seconds.
Lanterns by the Orchard (evening): F major, 88 BPM, 43.64 seconds.

Each project contains melody/accompaniment/bass stems. Native presentation
crossfades bank volumes when the time or weather changes. Pausing stops music;
Music and Sound settings remain controlled by the shared BlueEngine client.

Render from the engine checkout after changing a score:
  target/itest/be2-tools audio validate games/harvest-hollow/assets/audio-source/morning.json
  target/itest/be2-tools audio render games/harvest-hollow/assets/audio-source/morning.json NEW_DIRECTORY
  target/itest/be2-tools audio check NEW_DIRECTORY
  python3 tools/audio_report.py NEW_DIRECTORY/preview-mix.wav --loop --fail-on-issues

Render output is kept under assets/audio/<bank>. Preview mixes are authoring
references and are removed from runtime assets after numeric loop validation.
Audio quality measurements check clipping, silence, DC and discontinuities;
physical hardware playback and subjective listening remain separate checks.

Original Embertrail soundtrack, composed for this update. Every WAV is a complete
32–40 bar piece with an introduction, answering theme, contrasting bridge and
return with counterpoint. Eight world pieces, four guardian arrangements, Vesper's
finale, a title theme, a secret grotto piece and a homecoming piece have their own
titles, melodies, harmony, phrase rhythms and instrumentation. Vesper's last
section quotes Mote's title melody as an answering voice.

`tracks.json` is the title/duration catalog. Each directory contains a checked
BlueEngine `bank.json`, the exact stereo PCM16 44.1 kHz `song.wav` played and
downloaded by the client, and `composition.json` describing the score. No borrowed
recordings or third-party samples are used. Render again with:

```
python3 scripts/compose_soundtrack.py
cargo test --locked --bin embertrail
```

The renderer uses original additive flute, lute, glass, reed, horn, bowed string,
bass and bell instruments with percussion, equal-power stereo placement and
circular early reflections. Circular release tails and reflections keep the
musical boundary intact. Checksums, sample counts, DC, peak and loop-boundary
measurements describe the files; they do not certify listening or physical audio
device playback.

Music stays at its current position in the pause menu. The menu names the actual
current song and saves its exact checked WAV bytes under
`Downloads/Embertrail Soundtrack/Embertrail - Track Title.wav`. Repeated downloads
receive numbered filenames. Music and sound effects have independent saved
toggles. Loading and download work runs on workers; only the current song and a
brief fading previous song are decoded, keeping memory independent of the full
soundtrack's length. Macroquad still performs its final WAV decoder submission
on the render thread; this is an engine feedback item.

# Leo's nature and music

Nature and music are independent checked banks. The saved simulation's day phase
fades birds toward daylight and crickets toward night. Wind remains present;
local forest distribution changes leaf rustle. Music's warm chords, a sparse
melody and airy upper notes adapt to day/night. Settings remembers independent
Music and Sound toggles, both enabled by default.

The original 48-second score is `assets/audio-source/music.json`: an eight-chord
major/add-nine progression, restrained stereo melody and slow-attack night layer.
All notes/envelopes/filter/pan are editable engine data. It is an original Leo
composition, not a stock generated preset. Layers intentionally avoid a rhythmic
dependency: the backend submits independent loops, without sample-clock synchronization.

Field recordings are public-domain National Park Service clips. The [NPS sound
gallery](https://www.nps.gov/subjects/sound/gallery.htm) offers public-domain
downloads; [Rocky Mountain's library](https://www.nps.gov/romo/learn/photosmultimedia/soundlibrary.htm)
permits unlimited use and requests NPS credit. Credits and recordings:

- Birds: NPS/Jacob Job, Big Meadows dawn, June 8, 2016, 6 AM, 30–55 seconds of
  [Dawn Ambient](https://www.nps.gov/nps-audiovideo/legacy/mp3/imr/avElement/romo-DawnAmbientROMO6816BigMeadowsFinal1.mp3).
- Wind: NPS/Jacob Job, Gem Lake, May 25, 2016, 6:45 AM, 15–40 seconds of
  [Wind Ambient](https://www.nps.gov/nps-audiovideo/legacy/mp3/imr/avElement/romo-WindAmbientGemLakeROMO52516Final1.mp3).
- Night: NPS, Mojave crickets, 0–25 seconds of
  [Crickets](https://www.nps.gov/nps-audiovideo/legacy/mp3/nri/avElement/nri-Cricket.mp3),
  described on the [NPS recording page](https://www.nps.gov/subjects/sound/sounds-crickets.htm).
- Leaves: original seeded, filtered stereo noise with slow wind envelopes;
  synthetic rustle, not a claimed field recording.

Recording descriptions: [NPS ambient soundscapes](https://www.nps.gov/romo/learn/photosmultimedia/sounds-ambient-soundscapes.htm).
This is a storybook temperate landscape; European plant references and North
American recordings do not establish an exact natural habitat/species match.

`scripts/prepare_ambience.py INPUT` reproduces the committed 25-second PCM16
44.1-kHz stereo sources from downloaded `dawn.mp3`, `wind.mp3`, `night.mp3`.
It filters DC/very low rumble and normalizes to a 0.70 peak reserve, with no
playback. `scripts/render_audio.py ENGINE_TOOLS` validates, renders and checks
24-second loops with a one-second wrap crossfade, plus the three score layers.
The complete banks fit the existing per-project duration/headroom budgets.
Reports check non-silence, clipping, DC, format, checksums and wrapped seams.
Numeric checks and a null-sink playback run cannot establish perceived quality.

Plants use original geometry informed by [RHS oak](https://www.rhs.org.uk/plants/oak),
[silver birch](https://www.rhs.org.uk/plants/2261/betula-pendula/details),
[pine](https://www.rhs.org.uk/plants/pine),
[daisy](https://www.rhs.org.uk/plants/94326/bellis-perennis/details),
[clover](https://www.rhs.org.uk/weeds/clover-in-lawns) and
[wildflowers](https://www.rhs.org.uk/science/pdf/conservation-and-biodiversity/wildlife/rhs-perfect-for-pollinators-wildflowers).
No RHS artwork, photographs or plant textures are copied.

Rendered banks and seeded leaf PCM are generated files, ignored by Git and source
publication. `scripts/ship.py package` (also `ship`) renders/checks both banks before
packaging; it preserves the previous generated banks if rendering fails. The three
credited field-recording excerpts and both JSON projects stay in source, so no
download is needed. For a source-tree audio run, first run
`python scripts/render_audio.py [ENGINE_TOOLS]`. Runtime layers and bank metadata
ship in `assets/audio`; preview mixes do not.

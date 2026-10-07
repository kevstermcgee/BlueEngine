# Topsy-Turvy

A single-player 2D game. Dusk the bat runs a cave on its own. Press to flip gravity between floor and ceiling (or UP/DOWN to pick a side). Flip right in front of a spike for a close shave to build the heat multiplier. One hit ends the run and one press starts the next; the cave changes every 260 m.

A/Space flips, UP = ceiling, DOWN = floor, N music, M sound, K save, L load, R restart (records survive restart).

Progress is saved automatically about once a second and restored on launch; K/L make a manual checkpoint. Music is generated from `assets/audio-source/music.json` (engine `be2-tools audio render`) and shipped as `assets/audio/music`.

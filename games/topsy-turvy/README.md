# Topsy-Turvy

A 2D hot-seat game for two people sharing one controller. Dusk and Dawn are two bats racing a cave. They run on their own; press to flip gravity between floor and ceiling (or UP/DOWN to pick a side). Flip right in front of a spike for a close shave to build the heat multiplier. One hit ends the run. Three runs each, best run counts, and the cave changes every 260 m.

A/Space flips, UP = ceiling, DOWN = floor, N music, M sound, K save, L load, R restart (records survive restart).

Progress is saved automatically about once a second and restored on launch; K/L make a manual checkpoint. Music is generated from `assets/audio-source/music.json` (engine `be2-tools audio render`) and shipped as `assets/audio/music`.

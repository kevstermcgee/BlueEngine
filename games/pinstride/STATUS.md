# Pinstride implementation status

Implemented: eight 3D chambers, axis-aligned baton poses, four quarter-turn previews,
continuous swept collision with first-contact markers, automatic anchor handoffs,
colour-matched sockets, win progression and final victory, reachable dead-end failure,
unlimited undo/restart, optional hints, orbit/zoom camera, cutaway occluders, instructions,
shared pause menu, exact Snapshot saves, sound cues and canonical title/icon resources.

Delivery is Windows x64 native; Linux is declared for development verification.
No browser build, publication, installation or shortcut creation is part of this work.
Windows package/installer checks and physical input/audio review require their real host.
See README.md and assets/native-review.json for run commands and review criteria.

Validation observed on Linux: 12 rule tests plus identity, headless and native-feature
integration, formatting, a public-input native tour completing all eight chambers,
and native X11 keyboard/mouse events covering selection, ghost click, commit, rejection,
undo, restart, F5/F9, orbit, zoom and pause. Native screenshots at 1280x800 and 1440x900
were inspected. Software event injection is not physical-device certification.

Linux release packaging and isolated packaged smoke passed under Xvfb without
installing shortcuts. Running the ship gate without a display leaves smoke unverified;
run it inside an Xvfb display for Linux automation. Muted captures on this host needed
an ALSA null output configuration to avoid a downstream audio worker device panic.

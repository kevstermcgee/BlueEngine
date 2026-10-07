# Signal Garden

Restore six lantern relays before the eight-minute signal runs out. Collect gold sparks,
carry up to three, and hold E beside the numbered gold relay to charge it. Each relay needs
three sparks. Restore them in order, then return to the cyan beacon and hold E to finish.

WASD or arrows move; pad left stick also works. Pink patrols take a heart and a spark.
The beacon is safe: hold E there to heal, once every 30 seconds. Gold spark pads refill
every 15 seconds. Q or pad LB spends one spark for a three-second shield. Plan your route
rather than standing on a patrol track. First attempts are designed for about 5–10 minutes;
an automated solver finishes faster. Human completion time has not been measured.

Enter/Space/pad A starts. R/Enter/pad A restarts after a win or loss. F5 saves and F9 resumes.
F toggles fullscreen; Escape pauses and opens Controls and Settings. Settings remember music
and sound toggles, and can export the generated ambient track to Downloads. Saves and settings
live beside the executable; unzip to a writable folder. The package needs no engine checkout,
Cargo, network connection, or downloaded assets. Linux needs the usual OpenGL/X11 and ALSA
runtime libraries. Speaker output must still be checked by a listener.

From source, see AGENTS.md for the short context and command list. `scripts/blue ship` creates
`dist/` and the game's desktop shortcut. Distribute the files declared by `dist/ship.json`.

For package-only delivery use `python scripts/ship.py ship --no-install`; it keeps the
isolated smoke and never accesses the desktop. Plain `ship` additionally installs and
verifies this game's shortcut. Other applications' icon similarity cannot fail shipping.

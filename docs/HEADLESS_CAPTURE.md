# Seeing a game on a machine with no display

An agent cannot watch a window, and a graphical game cannot be judged from numbers alone. Every custom-sim game
already has `--capture DIR --frames 30,300` (see `devkit::Lifecycle`); on a machine without a screen it needs a
virtual one.

```sh
sudo apt install xvfb                       # once (Debian and Ubuntu); Mesa's software OpenGL is usually present
python tools/xcapture.py target/debug/my-game --frames 30,300 -- --character ghost
```

It prints the path of each PNG; open them with an image viewer. `--sheet` builds one contact sheet. Facts measured on
an Intel N97 mini PC:

- Software rendering runs at about 16 frames per second. Asking for frame 900 takes about a minute; a whole
  two-minute match takes about seven. Capture the moments that matter, not everything.
- Unattended runs (`--capture`, `--script`) step a fixed 1/60 s per frame, so game time does not depend on the slow
  rendering, but a networked client against a real-time server then ticks slower than the server: fine for looking,
  not for measuring. Measure networking with the in-memory `LoopNet` and bots (`docs/NETPLAY.md`).
- A world that renders as an empty void is almost always a wrongly wound quad or an oversize template: use
  `Template::quad_facing`, and see `Template::split`.

Use a new `--out` directory for each run; an existing directory is rejected with status 2 so completed captures stay intact.
The helper retains `native.stdout.log`, `native.stderr.log`, `xvfb.stderr.log` and `subsystems.json` in the output
directory even when the virtual display fails before the game can create it, or the run times out. On failure it prints
the output directory and the tail of the server diagnostics. An `XOpenDisplay()` failure can come from Xvfb being unable
to bind local display sockets; inspect `xvfb.stderr.log` before retrying on a display-capable worker. Failed captures
leave rendering evidence marked failed and do not establish current visual or control behavior.

Without `xvfb-run` the helper says so and exits with status 2.

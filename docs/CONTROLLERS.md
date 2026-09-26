# Native controllers

The stock `be2` client enables the optional `gamepad` Cargo feature. It uses
[gilrs](https://docs.rs/gilrs/0.11.2/gilrs/) for native device discovery and mappings;
no keyboard emulation or external service is required. Initialization failure is
reported to stderr and leaves keyboard/mouse usable. Mapped device support depends
on the OS driver and gilrs database. Linux builds need `libudev-dev` and `pkg-config`
in addition to the client's existing ALSA requirements. Windows uses gilrs' default
Windows Gaming Input backend. macOS uses its native backend. Platform availability
is not evidence of hardware testing; see VALIDATION.md.

## Stock bindings

| Input | Action |
|---|---|
| Left stick | Analog movement |
| Right stick | Look (2.5 radians/second at full deflection) |
| South / A / Cross | Jump; confirm character or pause-menu selection |
| East / B / Circle | Hold crouch; back/resume in pause menu |
| West / X / Square | Interact or pick up/drop |
| North / Y / Triangle | Toggle first/third-person camera |
| Left stick click | Hold sprint |
| Right trigger / RT / R2 | Use selected tool on press |
| Bumpers / LB/RB / L1/R1 | Previous/next tool |
| Start / Options | Pause/resume; back from settings |
| D-pad up/down | Character and pause-menu selection |

Keyboard and mouse remain usable simultaneously. Settings sliders still use the
mouse. Rumble, saved remapping, browser/mobile support and split-screen assignment
are outside this implementation. Custom games opt into the API; GameShell does not
silently replace application-owned input.

## Native API

Use `features = ["gamepad"]` with `default-features = false` for device input without
presentation. Create one `viewer::gamepad::Gamepads`, handling its fallible `new()`.
Call `poll(focused)` exactly once per rendered frame, including while paused.
`connected()` lists session-local IDs/names; `select(id)` chooses a connected pad.
The lowest connected ID is selected initially and retains ownership until it is
removed or explicitly changed. Another controller cannot steal control merely by
moving a stick. Disconnection removes held state and selects the next available ID.

`GamepadFrame` provides unit-circle sticks, normalized trigger pressure and
`down`, `pressed`, `released` queries using re-exported positional `Button` values.
A press/release within one poll retains both edges. Held buttons do not repeat.
Unfocused output is neutral; refocus, initial attachment and device selection
suppress button edges for that frame. Hosts must supply real focus status and gate
gameplay while menus are open. The stock client also suppresses controller gameplay
during its existing capture/resume delay, so confirmation cannot become a jump.

`frame.movement(keyboard_movement)` combines stock bindings, preserving analog
speed and limiting diagonal magnitude. Feed it into the existing PlayerStepper or
world input API. `frame.look_delta(frame_seconds)` yields radians for
`Controller::look(dx, dy, 1.0, invert)`; long frames are capped at 100 ms. Sticks use
a radial 18% dead zone rescaled continuously to full travel, after gilrs' native
filtering. Non-finite samples are neutral. Neither device polling nor render delta
enters the 60 Hz simulation contract or changes the network protocol.

## Verification

`cargo test --locked --lib gamepad` covers dead zones, analog magnitude, bounded
mixed input, short taps, held buttons, focus transitions, device replacement and
30/60/144 Hz look equivalence. `python tools/check_headless.py` guards the default
rendering-free dependency graph. Hardware acceptance requires a real mapped pad:
connect before/after launch, select each character, test every binding, unplug
while moving, reconnect, pause/resume, change camera, lose/regain focus, then test
WASD/arrows and cursor capture alongside the controller.

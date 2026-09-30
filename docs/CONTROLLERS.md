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

## What the movement controller owns

`viewer::controller::Controller` is the fixed-step player body shared by the client, the headless world
and any game that uses it directly. It owns collision against `Collider` boxes (step-up for stairs,
crouch headroom, swept vertical movement, overlap recovery), horizontal input smoothing (the body
converges on the target velocity at 18 per second, about 0.1 s) and gravity (12 m/s^2). Three things a
game with voids or knockback needs are settings, not forks:

| Need | API | Default |
|---|---|---|
| No implicit ground: pits, voids, an arena in the sky | `set_floor(None)` (or `Some(y)` for a floor at another height) | an infinite floor at y = 0 |
| Another gravity (the jump keeps its `jump_height`) | `set_gravity(m_per_s2)` | 12 |
| A shove, dash, jump pad or explosion | `apply_impulse(V)` then optionally `set_push_drag(per_second)` | horizontal push decays at 3 per second, walls stop it |
| The exact body-overlap question | `Collider::overlaps_body`, `Controller::blocked_at` | |
| A platform, lift or elevator that carries what stands on it | `Controller::ride(from, to)` once per moved box per tick | game `movers` already do this for every player |

`apply_impulse` adds the horizontal part to a *push* that rides on top of walking (so it is not erased by
the input smoothing) and the vertical part to the vertical velocity, leaving the ground when upward.
`set_physics_state(position, vertical_velocity, grounded)` is the supported way to apply an external
*vertical* change (a teleport, reconciliation); a moving platform is `Controller::ride`, which carries a grounded body
standing on the box; `restore_network_state` overwrites the
complete state, including the push. Floor, gravity and push drag are configuration, not state: they are not
serialized, so an online game configures every peer the same way. Impulses are local to the simulation that
applies them; an online game applies them where the authority runs. `be2-tools lint MAP.json` still treats
y = 0 as ground when it checks for floating props.

Look and stick input: `ClientInput::look_delta` is `mouse_look` plus `stick_look`; the stick term is scaled
by the engine frame clock (`ClientInput::frame_seconds`), not macroquad's bumpy `get_frame_time()`.

## Verification

`cargo test --locked --lib gamepad` covers dead zones, analog magnitude, bounded
mixed input, short taps, held buttons, focus transitions, device replacement and
30/60/144 Hz look equivalence. `python tools/check_headless.py` guards the default
rendering-free dependency graph. Hardware acceptance requires a real mapped pad:
connect before/after launch, select each character, test every binding, unplug
while moving, reconnect, pause/resume, change camera, lose/regain focus, then test
WASD/arrows and cursor capture alongside the controller.


The BlueEngineSandbox executable also polls this backend, including while menus
are open. Its creative bindings and D-pad menu navigation are documented in
[the sandbox guide](../assets/games/blueengine-sandbox/README.md#xbox-controller).

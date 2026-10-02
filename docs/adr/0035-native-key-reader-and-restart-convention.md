# ADR 0035: A testable, loud native key reader, and one restart convention

Status: Accepted

## Context

Two things came out of the games built on the engine. First, on Windows the standalone clients read keys
through an application-owned reader (`GetAsyncKeyState`) that replaces macroquad's key events once
installed; it polled a 23-key shortlist, so R, digits and most letters were silently never pressed in
custom-sim games. Linux uses macroquad events, so every headless test and capture passed. Second, about ten
games each hand-rolled "press R to restart" differently (R only; R or Enter; plus a pad button) while the
stock GameDocument runner used E / X.

## Decision

- The native table (`game_input::KEY_TABLE`) covers every `KeyCode` a game could plausibly bind. Keys left
  out are listed in `UNSUPPORTED_NATIVE_KEYS`, and a test classifies every macroquad `KeyCode` as one or the
  other, so a missing key is a decision, not an accident.
- The poll takes any key-state closure (`ClientInput::begin_frame_with_key_source`);
  `begin_frame_with_keyboard(.., Option<fn(i32) -> i16>)` keeps its signature and delegates. Linux tests drive
  the whole table-driven path through `ClientInput` with a fake source.
- With a native reader installed, `ClientInput::pressed/down` on a key outside the table prints one line per
  key on stderr and trips a `debug_assert!`. Release builds keep running (the key just reads as not pressed),
  but the line is in the log.
- `ClientInput::restart_requested(game_over)`: R always works, plus Enter and pad South/Start, one edge per
  press, only while the game is over and the window is focused. The custom-sim template uses it; the stock
  runner keeps E / X and accepts R as an alias once the match has ended.
- `python tools/be2.py check --windows` type-checks the `cfg(windows)` code for `x86_64-pc-windows-gnu`
  (`docs/CHANGE_WORKFLOW.md`). It proves the code compiles, not that it runs.

## Consequences

- Games are not edited by this change. An existing Windows release must be rebuilt and re-released to pick up
  the key-table fix (`MIG-0035-NATIVE-KEY-TABLE`, received automatically on the dependency bump).
- A game with its own restart code may adopt the helper (`MIG-0035-RESTART-CONVENTION`, optional).
- Start while over also opens the shell's pause menu, because the shell treats Start as pause; a game that
  restarts on Start should expect both. South is a confirm in the pause menu for the same reason.

## Rejected or deferred

- Dropping the native reader on Windows: it exists for accessibility-injected keys and lost key-up recovery.
- Making the diagnostic a hard error in release builds: a missing key should not crash a shipped game.
- Linking Windows in `check --windows`: no mingw toolchain is assumed; Windows CI remains the real gate.

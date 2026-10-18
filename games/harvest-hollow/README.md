# Harvest Hollow

The game opens fullscreen with a brief **BlueEngine Games** studio card before its own title screen. Press a key, click, or use a controller button to skip; release the control before playing. F or F11 switches fullscreen. Use `--windowed` for a windowed launch. Explicit capture and scripted verification runs retain their original window size and frame timing.

An original cozy farming game built with BlueEngine 2, pinned to the current October 13, 2026 engine release. No town, no combat, no deadline: just your little farm.

Click a patch to walk over and work. The Smart tool clears debris, tills grass, plants your selected seed, waters it, and harvests ripe crops. Or choose a specific tool with 1–7. WASD/arrows move; E or Space works the patch in front of you. Tab changes crops. I opens the compact Farm drawer for seeds, shipping, improvements and sleep.

Water crops each day, then sleep to grow them. Radishes take two watered nights, carrots three, pumpkins four. Dry crops wait safely. Rain waters the whole farm every fourth day. Your first three radishes are already watered and will ripen after one sleep.

Buy seeds at the stand or Farm drawer, and ship your harvest at the crate or in the drawer. Chop trees for wood, remove stumps, break rocks for stone, and clear weeds for bonus seeds. Build a sprinkler (20 wood, 8 stone, 90 gold) to water the main beds every morning; build a pond deck (18 wood, 5 stone) next. The pond refills your watering can. Sleep restores energy.

Four original, seamless music tracks change with morning, afternoon, rain and evening. N toggles music, M toggles sound. K saves a manual checkpoint; L loads it. The engine also saves automatically. Esc pauses, F toggles fullscreen, R starts a fresh farm.

Windows x64 installer; saves are local to this computer. Game rules are deterministic Rust, tested for complete farming/clearing routes and exact save continuation.

Development: render the four committed score JSON files into assets/audio/{morning,afternoon,rain,evening} with `be2-tools audio render` before running or packaging with sound. The pinned Windows release workflow renders, checks and measures all four banks before shipping. Rendered PCM is generated build data and remains excluded from source control.

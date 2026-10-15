# Two small original interactions

These authoring notes are not runtime files. The native and headless fixtures share
one real 3D yard: a battery, a collision gate and a far-side beacon. They exercise
text against real state rather than supplying a campaign template. Runtime field
names and commands are defined by the fixture binding registry.

## Relay Yard: reactive flavor

[relay-yard.poe](../assets/narrative/relay-yard.poe) has no plot or character arc.
The maintenance PA is useful only around actual pickup, gate and crossing events.
It speaks with officious brevity and a fondness for impractical safety advice.
It knows the yard's current sensors; it does not know the player's thoughts.

Voice examples: “Bridge ready. Please bring all of yourself across.” / “One cell
collected. We admire your commitment to carrying the solution.” These are voice
references, not a requirement to repeat the same joke in every response. Leave
silence between milestones, and interrupt a routine announcement for urgent context.
A voice card suffices; no biography or setup/payoff ledger is needed.

Editorial review: jokes should remain legible during movement, respond to a visible
change and avoid explaining mechanics the UI already communicates. Playtest whether
the silence and cooldown intervals survive repeated runs.

## Borrowed Light: a gameplay-backed choice

[borrowed-light.poe](../assets/narrative/borrowed-light.poe) offers a tiny interaction
with Keeper Mara and a later acknowledgment. Spending the actual battery opens the
actual collision gate. Keeping it leaves that route shut. A callback uses saved gameplay
truth (the open gate and reached destination); presentation completion never spends the cell.

Mara's authoring sheet:

| Field | Authored contract |
|---|---|
| Immediate want | Get the bridge powered so someone can reach the beacon. |
| Deeper tension | Ask for help without treating another person's last reserve as public property. |
| Limitation/pressure | She can maintain the gate but has no spare battery. |
| Contradiction | She insists on the player's free choice while making the urgency unmistakable. |
| Values/relationship | Honest accounting; practical care. The player is a visitor whose help cannot be presumed. |
| Voice | Plain, concrete, restrained. Requests are short; gratitude remembers a specific cost. |
| Voice examples | “The bridge needs one cell. Yours is the only one here.” / “You left yourself no spare. I noticed.” |
| Knowledge | Knows the gate mechanism and that the offered cell is the visible available reserve. Learns the player's decision from the authoritative command. |
| Belief/concealment | Believes the beacon is worth reaching; does not claim to know the player's motives. No intentional lie is required. |

World truth is held by battery/gate/beacon state. Committed choice history is narrative
state owned by the same simulation. The player learns the cost from the choice
label and sees the gate change. A resumed callback must read those saved facts.
A future rewrite that adds a secret, rumor or lie must declare whose knowledge it
changes and the command/source that records learning; prose analysis cannot validate it.

A small setup/payoff ledger is appropriate here:

| ID | Setup/question | Observable acknowledgment or outcome |
|---|---|---|
| sole_reserve | Offer node names the actual battery cost. | Spend command consumes it and opens the gate; keeping leaves both unchanged. |
| noticed_cost | Mara asks without entitlement. | Later destination callback acknowledges the spent cell and visible gate state. |
| no_obligation | Decline/keep is a valid response. | Scene ends explicitly; a later voluntary revisit is allowed. No punishment is fabricated. |

The consequential branch changes traversability and resources; a commitment flag
alone would not establish consequence. The keep response is expressive unless a
later authored system actually reacts to it. Neither branch needs a moral score,
romance system or irreversible character transformation.

Editorial review is still human judgment: does the request create understandable
pressure, does the callback earn its place, and does the short interaction leave
room for the yard's movement? Automated correctness cannot answer those questions.

## Public fixture contract and controls

Both examples start with seed 7 by default in headless walks: one player on the
south side, no cell in inventory, one cell on a raised landing, a closed gate and
present speakers. The seed is saved for replay identity; these fixtures consume no
random numbers. `--limit 340 --route share|keep` selects one bounded input route,
not an exhaustive graph search. `--fixture storyless` creates no narrative runtime
and reads no narrative file. `--quiet` suppresses cosmetic responses.

The published [binding registry](../assets/narrative/yard.bindings.json) declares:

| Binding | Type | Authoritative meaning |
|---|---|---|
| `inventory.cells` | integer query | Actual count of the player's sole reserve cell. |
| `gate.open` | boolean query | Whether the blocking gate collider has been removed. |
| `course.finished` | boolean query | Whether the player reached the far-side marker through gameplay. |
| `player.near_relay` | boolean query | Whether the player's feet are within 1.5 metres of the relay in 3D. |
| `bridge.power true` | boolean command | Requires one cell, a closed gate and proximity to the relay. Atomically consumes that cell and opens the gate. Other values are rejected. |

Events are `arrive`, `pickup`, `gate` and `finish`. Positions, collision geometry,
inventory and gate state belong to the rendering-free fixture simulation. Scene
position, committed choice history and response history are run state in that same
simulation's engine-owned snapshot. Text reveal and focus are transient native UI.
Changing capability or semantic content rejects an incompatible save; text edits
preserve compatible progress. F6 validates before activating content between ticks.

Native controls: WASD/left stick move, mouse/right stick look, Space/A jump and
E/X interact. During a scene, Tab/RB reveals text, Up/Down or D-pad changes choice
focus, Enter/A chooses or advances after reveal, and Backspace/B interrupts.
Scene focus blocks movement and gravity; authoritative ticks continue. Esc pauses
all fixed ticks. Barks permit movement and never apply gameplay effects. Enter/LB dismisses them;
A remains the gameplay jump action. Early bark dismissal records interruption. F5 saves and F9 loads through the existing engine
snapshot system; F6 reloads the selected content file and reports its cost.

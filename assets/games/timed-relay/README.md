# Ten Second Relay

A small stock-content example authored by a fresh agent through public tools.
Use the blue START terminal, then reach the green FINISH terminal within ten
seconds. Expiry loses; E after loss/win restarts. WASD/arrows move, E/X interacts,
Escape pauses locally. The inherited Test Lab rooms are intentionally simple.

Start with the repository AGENTS.md and `be2.py start`, then narrow to the owning
contract with `python3 tools/be2.py context game_documents --level 1 --compact`.
Edit `content/game.json`: the `active` counter, target eligibility, 600-tick timer
and three rules use existing authoritative GameDocument primitives. Completion
stops the timer; the engine owns restart, including counters, targets and once
rules. No custom gameplay runtime is needed. Novel behavior keeps the documented
custom-simulation code path in `docs/GAME_QUICKSTART.md`.

From the engine root:

```sh
python3 tools/be2.py map game-validate assets/games/timed-relay/content/game.json
python3 tools/be2.py map sim assets/games/timed-relay/content/success.json
python3 tools/be2.py map sim assets/games/timed-relay/content/deadline-boundary.json
python3 tools/be2.py map sim assets/games/timed-relay/content/loss-restart-success.json
cargo run --locked --profile itest --bin be2 -- --game assets/games/timed-relay/content/game.json
```

The success route remains won beyond the original deadline, proving timer cleanup.
The boundary scenario activates at tick 1, remains alive at 600 and loses at 601:
600 elapsed fixed ticks. The longer route waits before starting, loses, checks
frozen loss, restarts to initial authority state, and physically reaches FINISH
in the new round. These are exercised routes, not exhaustive state-space proof.

For a known semantic error, run `map game-validate` on
`content/game.invalid-counter.json`: it must fail with
`Invalid condition in finish-round: condition references unknown counter undeclared_round`.
Repair the condition to use the declared `active` counter. Schema validity alone
cannot prove references. Engine maintainers use `be2.py schemas --write/--check`
for contract changes; game authors use the committed schema and normal validators.

This is content for the existing native stock executable, with headless scenarios.
It is not a standalone packaged/browser game. Linux scripted software-rendered
world/loss/reset/win/menu frames were inspected; physical input/audio and Windows
rendering of this variant were not exercised. The map has eleven inherited Test
Lab lint warnings and zero errors; the original fixture had thirteen warnings.
Keep visual, network and package evidence separate from these scenario results.

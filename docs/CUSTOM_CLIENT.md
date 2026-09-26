# Custom visible clients

Use a custom client when a game needs presentation or device handling beyond the stock
`be2 --game` client. BlueEngine owns validated scene data, deterministic 60 Hz
simulation, collision, game rules, and authoritative networking. The application owns
its window, renderer, input polling, HUD, and the translation from device state into
`Movement` and interaction requests.

Start from `examples/custom_client.rs`:

```sh
cargo run --locked --example custom_client
```

For an external project in a sibling directory, use the supported renderer version
explicitly; applications and BlueEngine may then exchange Macroquad types without a
second-version type mismatch:

```toml
[dependencies]
vesper3d = { package = "be2", path = "../BlueEngine", default-features = false, features = ["client"] }
macroquad = { version = "=0.4.14", default-features = false, features = ["audio"] }
```

Enable `client` for the shared visible client; use no default features for a rendering-free host. Submit held movement every display frame, preserve jump and
interaction as press edges, and advance `HeadlessWorld::step` in fixed 1/60-second
increments. Cap catch-up work after long frames. Read copied public state such as
`player`, `prop_position`, and `GameRuntime::state`; do not mutate results to make
tests pass.

The example uses the shared MapPlayer static renderer, input, camera, characters and menus. Use `be2 --game` when document-driven presentation
is sufficient, including replicated target visibility and movers.

Playable game presentation follows [the shared presentation contract](GAME_PRESENTATION.md). Use the `presentation` feature for `GameShell` and cached static rendering.

See [the shared gameplay kit](SHARED_GAMEPLAY.md) for the playable starter, public creative APIs, feature gates and runtime boundaries.

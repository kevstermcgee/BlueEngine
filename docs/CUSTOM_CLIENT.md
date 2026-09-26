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
vesper3d = { package = "be2", path = "../BlueEngine", default-features = false }
macroquad = { version = "=0.4.14", default-features = false, features = ["audio"] }
```

Keep the engine dependency rendering-free unless the application deliberately calls
client-only modules. Submit held movement every display frame, preserve jump and
interaction as press edges, and advance `HeadlessWorld::step` in fixed 1/60-second
increments. Cap catch-up work after long frames. Read copied public state such as
`player`, `prop_position`, and `GameRuntime::state`; do not mutate results to make
tests pass.

The example draws deliberately simple application-owned geometry. It is an integration
boundary, not a second stock renderer. Use `be2 --game` when document-driven presentation
is sufficient, including replicated target visibility and movers.

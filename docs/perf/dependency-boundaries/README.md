# Dependency boundary receipt

Starting revision: `cb19b58e5c952ba83f120d801a85d929307ed19f`.
Target: `x86_64-unknown-linux-gnu`. Toolchain: `rustc 1.98.1`.
`graphs.json` records exact selected normal-edge graphs and commands before and
after the feature changes. These are package/version-node counts; they do not
measure compilation time or agent cost.

| Selected consumer configuration | Before | After | Heavy optional dependencies after |
| --- | ---: | ---: | --- |
| Pure rules, no default features | 97 | 17 | none |
| Offline 2D, `two-d` | 126 | 54 | presentation/audio only |
| Spatial headless, `physics` | 97 | 72 | Rapier; no QUIC/Tokio/Rustls |
| Native custom multiplayer, `networking` | 97 | 47 | QUIC/Tokio/Rustls; no Rapier |
| Authoring validation, `schema-validation` | 156 | 89 | JSON Schema validation; no physics/network runtime |
| Authoring generation, `schema-generation` | 103 | 24 | Schemars; no physics/network runtime |
| Custom native client, `native-client` | 135 | 65 | window/audio/input; no physics/network runtime |
| Existing default client | 135 | 135 | unchanged capabilities |

Before the change, the spatial and multiplayer configurations used
`--no-default-features` because physics and production transports were unconditional.
The custom-sim scaffold previously selected the broad `client` feature, whose
graph equals the default client; it now selects `native-client` for its game-owned
rules and presentation. Existing broad client consumers retain their features.
The generation baseline graph was resolved using the historical manifest and the
repository lockfile in an isolated directory with source stubs; those stubs were
never compiled. Other baseline graphs were captured before editing the manifest.

The source-free graph receipts are complemented by external consumers with their
own workspace and independent feature selection. Their fixture rules exercise a
game-owned dash scoring rule, engine-owned snapshot continuation, integer 2D
collision, spatial physics save/restore, custom NetServer without physics and schema
generation. The fixtures retain the engine lockfile's versions while adding their
standalone root package. The canonical full check runs them at its selected profile.

Focused evidence during implementation: all 479 no-default library unit tests
passed (51.05 seconds executing tests) and the schema coordinator's nine Python
tests passed. A first fresh worktree compile ran out of space on the root filesystem;
only that worktree's newly created artifacts were removed. Subsequent compilation
uses `/mnt/blueengine-usb/authoring-efficiency-target` and an adjacent temporary
directory. This is an environmental constraint, not a performance comparison.

Final-revision integration receipts belong in the main implementation report.
These intermediate tests do not certify native devices, Windows installers or shipping.

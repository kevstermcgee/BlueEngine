# Publishing to BlueEngineGames

`BlueEngineGames` is the public, browsable home for games, prototypes, test content,
and demos produced with BlueEngine. BlueEngine remains the source of truth for copied
engine examples; independently maintained games can remain source-of-truth in the
companion repository.

The publication list is explicit in `games-publish.json`. Each entry maps a tracked
file or directory into one of four stable collections: `games/`, `prototypes/`,
`tests/`, or `demos/`. Add a manifest entry when new engine-made content is ready to
share; do not point it at build directories, logs, secrets, or unreviewed scratch
output.

Manifest version 2 also supports a `preserve` array for independently maintained
paths such as `games/riftwake`. A preserved path must live below one of the four
collection roots, cannot overlap another preserved path, and cannot collide with a
copied manifest destination. During export, an existing preserved tree is carried
forward byte-for-byte while other stale files are removed. The catalog records the
preserved path names separately; it does not claim their files came from BlueEngine.

Validate the complete export without changing any files:

```text
python scripts/publish_games.py check
```

To inspect the exact result locally, export into a separate directory:

```text
python scripts/publish_games.py export --output ../BlueEngineGames-preview
```

On a push to `main` that changes the engine, a published source, the manifest, or the publisher,
`.github/workflows/publish-games.yml` checks out `BlueEngineGames`, rebuilds the four
managed collections, and pushes only when the copy changed. It can also be run
manually. The workflow authenticates with the repository-scoped SSH deploy key in
the `GAMES_REPO_DEPLOY_KEY` Actions secret. The key can write only to
`BlueEngineGames`.

The generated `.games-catalog.json` records the exact BlueEngine commit and SHA-256
digest of every copied file. The publisher rejects missing sources, path traversal,
symlinks, and destination collisions before replacing any managed collection.

## Playable releases

The manifest's `playables` array is also copied into the catalog. Each entry names a
download, its main published file, every published file or directory it needs, and
the command-line arguments used by its launcher. `BlueEngineGames` builds the exact
cataloged BlueEngine commit on a Windows runner and creates a permanent GitHub
Release containing one ZIP per playable. Each ZIP includes the engine and a small
`Play-<slug>.exe` launcher. Add an entry only after its content is self-contained and
manually playable with the listed arguments.

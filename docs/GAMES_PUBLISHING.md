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

## What an export may touch

An export only deletes or overwrites files it can prove an earlier export wrote. The proof is the `.games-catalog.json` the
previous export left in `BlueEngineGames`: every copied file's path and SHA-256.

- Files and games that are not in that catalog (an independently added game, a hand-written README) are never deleted,
  with no `preserve` entry needed. A missing catalog means nothing is owned, so the first export deletes nothing.
- A file the catalog lists but the manifest no longer publishes is removed only if its bytes still match the recorded
  hash; directories it leaves empty go too, collection roots never.
- If an export wants to write a path that exists but was not written by an export, or that was edited since (its hash
  differs from the record), the export stops with a list of every such conflict and changes nothing. Revert the edit, move the
  file, or delete it, then export again. A file whose bytes already equal the new content is not a conflict.
- A catalog that cannot be read or has a malformed entry (bad JSON, missing hash, path outside the collections, duplicate
  path) stops the export with a diagnostic rather than guessing what may be deleted.
- Every path is checked before anything is written: manifest destinations and catalog paths must stay inside the four
  collections, and no parent directory on the way may be a symlink.

**Transaction guarantees, stated exactly.** The whole export is planned and every new file is staged inside the output before the
first change. Each file is then swapped in with an atomic rename, so no file is ever half written, and the catalog is
written last. The set of files is *not* one atomic transaction: if the process dies part-way, some files are new and the
rest old, the catalog still describes the previous export, and running the export again finishes the job. Use
`export --dry-run` to see the planned creations, updates, removals and conflicts without changing anything.

The optional `preserve` array (for example `games/riftwake`) is kept for compatibility. It is no longer needed to protect
unlisted content; it additionally stops a stale owned file under that path from ever being removed, and a preserved path
may not overlap published content. The catalog lists the preserved path names separately.

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
symlinks, and destination collisions before it changes the output.

## Playable releases

The manifest's `playables` array is also copied into the catalog. Each entry names a
download, its main published file, every published file or directory it needs, and
the command-line arguments used by its launcher. `BlueEngineGames` builds the exact
cataloged BlueEngine commit on a Windows runner and creates a permanent GitHub
Release containing one ZIP per playable. Each ZIP includes the engine and a small
`Play-<slug>.exe` launcher. Add an entry only after its content is self-contained and
manually playable with the listed arguments.

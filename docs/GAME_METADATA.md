# Metadata that travels with a game

New native packages contain their own game information. Dropping a supported
`.exe`, `.bat`, `.cmd`, or `.zip` into the launcher or website reads that information
and prepares the posting fields. Renaming or copying the file preserves it. The
importer reads bytes; it never launches a submitted executable or script.

`python scripts/ship.py package` generates the metadata from the game's existing
`assets/identity.json`, `game.project.json`, Cargo package and optional `wiki.json`.
Public Cargo author names have email addresses removed. A clean tracked Git
project can record its public GitHub repository, full commit and game directory.
Dirty or untracked source remains unknown. `community-source.json` contributes
public fork ancestry, rather than being treated as the current fork's source pin.
The native installer command embeds the same information in its completed output.

The portable manifest is `blueengine.game.json`. It is part of the game package,
not a required neighboring download. EXEs and batch files also carry the payload
inside their own bytes, so dropping one file does not require locating a sidecar.
Packages without metadata cannot recover descriptions or authors that were never
stored in them. RAR, 7z and other archive formats are not yet supported by this
version; use the metadata-bearing ZIP export.

## Public format

The canonical UTF-8 JSON payload is at most 61,440 bytes. Keys sort lexically;
separators are `,` and `:` without spaces or a final newline. Unicode remains
UTF-8. Duplicate keys, NUL, unpaired surrogates, floats and nonfinite numbers are
rejected. Integers must fit JavaScript's safe integer range. Object keys use
`[A-Za-z0-9_.-]{1,128}`. Generic limits are 24 nested levels, 256 object keys or
array elements, and 16,000 Unicode characters per string.

```json
{"engine":null,"format":"blueengine.game","game":{"authors":[],"controls":"Arrows move; Space jumps","description":"Find a route through the stars.","slug":"star-route","tags":["puzzle"],"title":"Star Route","version":"0.1.0"},"schema_version":1,"source":null}
```

`game` requires the fields shown. `slug` is lowercase letters/numbers separated by
single hyphens (maximum 100 characters). `title` is nonempty text of at most 160
characters. Limits for `version`, `description` and `controls` are 128, 16,000 and
8,000 characters. `authors` and `tags` each hold up to 32 nonempty strings of at
most 160 characters. Additional bounded fields can retain presentation, inputs,
targets, public wiki information and package details.

`source` and `engine` must be present. They are `null` when unknown; otherwise
they contain `repository` (`owner/repository`, maximum 200 characters) and `commit`
(a complete 40- or 64-digit hexadecimal Git object ID). `source` also requires
`game_path`, a portable relative slash path or `.`. Traversal, backslashes,
Windows device names, trailing dots/spaces, control characters, and Windows
reserved filename characters are rejected. Optional `source.dirty` is boolean.
Optional `source.tree` is a complete Git tree object ID for the selected game
folder. It identifies unchanged content across repository commits or exported
paths; it is omitted when unknown. It remains an untrusted content hint until
matched against the collaboration service's verified revision history.
`lineage` can retain public `root_game`, `parent_game`, `parent_revision`, and
`revision_id`; `wiki` is an optional bounded object.

This is descriptive metadata. Its checksum detects damage; it does not prove
ownership, authorship, source availability, or permission to overwrite a game.
The posting session still authenticates the user and validates source and release
history through the normal publication workflow.

## File representation

The binary envelope is the canonical payload followed by its 32 raw SHA-256
bytes, its byte length as an unsigned 8-byte little-endian integer, and the
28 ASCII bytes `BLUEENGINE_GAME_METADATA_V1\n`. The footer is 68 bytes.

- **EXE:** the envelope follows the executable bytes. Writers accept unsigned PE
  outputs without an existing unknown overlay. The maintained Inno compiler path
  explicitly permits its own newly compiled installer payload. Signed executables
  are never modified. A reader can find metadata before a bounded, structurally
  valid Authenticode certificate table, allowing up to seven zero alignment bytes.
  It validates PE headers and section/certificate extents without loading code.
- **BAT/CMD:** an initial `@rem BLUEENGINE_GAME_METADATA_V1_BEGIN` line precedes
  `@rem ` lines containing at most 76 base64 characters each, followed by
  `@rem BLUEENGINE_GAME_METADATA_V1_END`. Base64 encodes the canonical JSON.
  Writers use CRLF and preserve the original script body bytes. Readers accept
  LF or CRLF. Metadata begins at byte zero; BOM/UTF-16 batch files are not stamped.
- **ZIP:** the root member `blueengine.game.json` is `ZIP_STORED`, and the complete
  binary envelope is the ZIP comment. Both copies must match. No outside sidecar
  is needed. Readers reject missing copies, duplicate/case-aliased or unsafe
  member paths, more than 20,000 entries, and central directories over 16 MiB.
  An extra end-of-central-directory signature (`PK` followed by bytes `05 06`)
  inside the ZIP comment is rejected, keeping bounded preflight and stdlib ZIP
  parsing on the same directory record.
  ZIP64 is supported within those inspection limits. The browser reads bounded
  slices and the stored manifest; it does not expand game payloads.

Embedding happens before package hashes, installation receipts, upload checksums,
or signatures are calculated. Existing signed or third-party self-extracting
executables must not be patched in place. The producer uses a metadata-bearing
owned wrapper/package where original bytes must remain immutable. Full Windows
package and installer gates remain required.

## Inspect or prepare an owned file

```bash
python3 tools/game_metadata.py inspect path/to/game.exe
python3 tools/game_metadata.py create path/to/game --output /tmp/blueengine.game.json
python3 tools/game_metadata.py embed path/to/owned-unsigned.exe --manifest /tmp/blueengine.game.json
```

The reusable stdlib implementation is [game_metadata.py](../tools/game_metadata.py).
The direct `create` command uses Python 3.11+ to read Cargo TOML. Existing shipping
commands retain their Python 3.10 TOML fallback and pass the parsed package data
to the metadata helper. Inspecting supported files needs no TOML parser.
Game tooling pins include it; restore the pinned engine or run the existing
reviewed `tools/game_tooling.py migrate GAME` workflow to adopt a tooling update.

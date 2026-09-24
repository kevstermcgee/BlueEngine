# 0005: Native discovery and evidence

Status: Accepted.

## Context
The Python authoring interface was bounded, but native tools could not orient an AI
without source reading. FEATURES.json contradicted implemented multiplayer behavior.

## Decision
Embed the existing feature index into native describe/search commands. Share the
command signatures with the parser and help output. Keep search bounded to ten
records and 100 input bytes. Add evidence suite/test names to important capabilities;
CI checks that referenced paths/tests exist and runs those integration suites.
The quickstart embeds the exact compiled 30-line example, checked by a test.

## Consequences
No additional index, service, runtime Python logic or mandatory MCP dependency.
Curated prose still requires review: resolving evidence names cannot prove every
sentence in a document. Automated coverage checks executable command arity, paths,
the worked example, export compatibility, physics and multiplayer behavior. A green
full suite is required before publishing a capability as verified.

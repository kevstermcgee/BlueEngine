# ADR 0042: Stop hot-reload prototypes that miss acceptance

Status: stopped after measurements. See [commands and evidence](../perf/HOT_RELOAD_REPORT.md).

The tooling pass requires observed development benefit and reliable verification. It
explicitly permits stopping an item that does not help or proves unreliable. Preserve
existing authority, compatibility, headless behavior and shipping gates.

A content prototype reused the stock validators and engine saves, retained camera and
player position, rejected invalid edits and networked sessions, and rendered authored
audio in bounded background work. Nine new behavioral tests passed. Actual X11 frames
showed a median edit-to-visible time of 0.2716 s versus 0.1950 s for warm restarts in the
small supported scene. Conservative 250 ms content hashing dominated. State retention
could matter in a longer task, but its development-time benefit was not measured.
Do not ship this implementation on that assumption. Preserve its review patch and
measurements, and restore engine source to the accepted importer revision.

An external Dioxus/Subsecond 0.7.10 prototype reported successful patches but ran old
code after both library and tip-crate edits. A second Linux configuration removed local
compiler-cache/linker overrides and failed the same functional controls. Stop before
adding dependencies, experimental patch hooks, unsafe layout handling or architectural
changes. This is failure on the measured setup; it does not establish universal failure
on Linux or Windows. Windows and automatic snapshot recovery remain unverified.

The shipping engine retains its existing manual content workflow, custom Rust escape
hatch, shared authoritative fixed-step simulation, renderer-free headless path, network
content contract and complete verification gates. A future content experiment should
measure native filesystem events and conservative bounded fallback polling on small and
large projects before adding another tool.

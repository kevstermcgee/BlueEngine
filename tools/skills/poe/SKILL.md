---
name: poe
description: Author optional BlueEngine flavor, contextual responses, or small authored scenes through typed game bindings. Use when the game benefits from those capabilities; skip this kit for storyless games and ordinary gameplay characters or UI labels.
---

# Poe: choose only what helps the game

Choose capabilities independently from the brief: none, flavor, contextual dialogue,
authored scenes, or systemic event scheduling. These are not progression tiers.
A director can serve gameplay variety without dialogue. Milestone 1 implements
responses and bounded scenes; the director remains a documented extension.

For none, stop this workflow: use the ordinary game tools, with no story files,
character sheets, narrative dependency, placeholder content, or narrative checks.
Names, enemies, avatars, objectives and labels do not require this kit.

Read [the contract](../../../docs/NARRATIVE.md) for syntax, public tools, bindings,
execution and reload. Read only the sections needed for the selected capability.
Keep the simulation rendering-free and fixed-step. Read gameplay through registered
typed queries; submit consequences through atomic authoritative commands. Never
mirror inventory, health, doors or objectives into story flags.

For flavor and reactive content:

1. Inventory real gameplay events and timing: who can speak, when they exist, what
   the player is doing, and when silence helps concentration.
2. Choose tone and a short voice card (role, voice, two examples, facts known).
3. Write concise responses with specific callbacks to actual play. Provide priority,
   repetition and cooldown rules. Silence is valid; a fallback is optional.
4. Lint against the game's binding registry; headless-walk bounded cases; revise
   affected IDs; native-playtest readability, interruption and repetition.

For a substantial authored interaction, inventory actual verbs, locations and systems
first, then premise → character sheets → optional structural framework → playable
beat outline → graph and game bindings → dialogue → lint → bounded headless walk →
editorial review → affected-node repair → native playtest. New fiction is welcome;
proposed gameplay needs implementation or an explicit future contract.

A sheet for a substantial speaker records immediate want; deeper need/tension or a
stable-character rationale; limitation/pressure point; meaningful contradiction;
values and relevant relationships; concise voice and examples; knowledge, beliefs,
and concealment. Keep biographies outside runtime state. Minor speakers need only
a voice card. Do not manufacture trauma or transformation for every character.

Use motivation and causality: what does each speaker try to change now? Give lines
subtext and concrete details rather than biographies. Let environment, actions and
useful silence carry some information. Vary pacing; no single story formula is
required. Track world truth, belief, player knowledge and deliberate lies separately
when they matter. Important revelations need an explicit owning state/binding and
source of acquisition; prose alone is not proof the player learned a fact.

For authored arcs only, keep a small setup/payoff ledger: ID, promise/question,
setup node, possible acknowledgment/revelation, and any intentional unresolved end.
Declare external/future readers and reasoned exceptions. Do not create a ledger for
an arcade announcer.

Classify choices as consequential, informational or expressive; all are valid.
For consequential choices, trace an observable difference in play, information,
relationships or experience. A fresh boolean alone proves no consequence.

Hard validation concerns syntax, IDs, types, bindings, references and legal
execution. Lint cannot judge voice, emotional impact or whether a premise is generic.
Editorial review should ask whether voices differ, reversals are earned, callbacks
are specific and choices communicate their effects. Do not reject direct speech,
emotion words, long lines or stable characters mechanically. Record content validity,
behavior, native presentation, editorial judgment and human playtesting separately.

Use stable text/node/choice IDs through targeted edits. Keep the old valid content
on reload failure. Never replay effects to make a revised line appear. Headless
walks use the real game bindings with explicit initial conditions, seed, action
sequence, limits and assumptions; they are bounded evidence, not exhaustive search.

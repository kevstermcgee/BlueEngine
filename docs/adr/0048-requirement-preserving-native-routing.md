# Requirement-preserving native starter routing

Status: Accepted

At 957272c explicit Windows targets skipped the normal portable selection branch:
2D collector requests selected native-3D stock, and enemies/projectiles could override
presentation. OS is a delivery requirement, not a substitute for dimensionality or rules.

Decision: preserve requested presentation, mechanics and authoring in the packet. For
offline native portable games choose two-d/three-d/hybrid by presentation first; their
typed GameLogic can implement game-specific enemy, projectile, timer and scoring rules.
Explicit starter flags stay authoritative but incompatible requests remain blockers.
GameDocument + 2D and unsupported stock mechanics require deliberate engineering.
Custom simulation supports advanced native physics/netplay; a requested custom 2D
client not scaffolded by the sample is a workflow gap, not an engine capability removal.

Creation defaults to Windows across the executable/JSON catalog and coordinator.
Existing Linux/macOS development targets remain available explicitly. No browser
fallback or gameplay simplification. A generated sample is not the requested completed game.

Acceptance: Windows 2D + enemies/projectiles/scoring/AI selects two-d with the original
requirements; explicit stock conflicts and 2D native multiplayer return actionable gaps.
Catalog and emitted project requirements agree. Discovery remains build-free.
Verification: tools.test_springboard and Rust newgame/scaffold production-path tests.

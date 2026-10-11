# Goal
How ready is Mudcrab's engine for multiplayer, and where would networking hook in? Give the design facts a multiplayer plan must build on.

# Context
- This folder is a clean checkout of upstream `main` (Rust, Bevy 0.19). Crates: `crates/engine` (game, streaming, physics, render), `crates/shared` (formats/contracts), `crates/converter`, `crates/launcher`. Roadmap: `docs/roadmap/` (05 covers networking: "native co-op multiplayer: ECS entity replication over WebSockets/UDP"), `docs/roadmap/03-luau-and-ui.md` (scripting), `SPEC.md`, `docs/specs/`.
- The research is for multiplayer aimed at Skyrim roleplay servers (tens of players, persistent world, server-side scripting for RP tools). Web research on other projects runs separately; this is only about Mudcrab's own code and plans.

# Scope
- Read only; no builds or edits.

# Done when
Each of these is answered with file:line evidence (or "not present"):
1. Simulation loop: fixed timestep? where game state lives (resources vs components), how input drives the player (crates/engine physics/player code), what is deterministic vs not.
2. World streaming: how cells are chosen and loaded around the camera/player (`streaming.rs`), whether it assumes one viewer, what would need changing to stream around several players on a server (interest management).
3. Entity identity: do spawned world objects keep a stable ID tied to the Skyrim FormID/REFR (needed to replicate and persist them)? Where is it?
4. Physics: Rapier setup, what is simulated, determinism, whether a headless (no window/GPU) build is possible today (`--headless`, fixtures, tests that run without a GPU).
5. Scripting plans: what the Luau plan says about global vs local scripts, sandboxing, and anything multiplayer-related; the state of Papyrus/quest support.
6. Persistence: what state is saved today (if any); how it relates to the proposal of per-cell overrides on the base world.
7. Existing roadmap/spec text about multiplayer or networking, quoted with file:line.
8. Dependencies already in Cargo.lock relevant to networking or serialization (serde, bincode, tokio, etc.).

# Report
- One section per question, evidence-first. Then "Hook points": the 5-8 places a networking layer would attach, and "Blockers": what must exist before any multiplayer prototype (e.g. headless server build, stable entity IDs). Under 1000 words, English.

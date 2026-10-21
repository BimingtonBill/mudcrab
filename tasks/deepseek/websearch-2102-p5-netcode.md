# Goal
Survey the Bevy networking stack as of October 2026 and the designs that fit a server-authoritative co-op mode for an open-world RPG engine. Output feeds a Phase 5 co-op design note for Mudcrab.

# Context
- Mudcrab: open-source (MIT/Apache) Skyrim engine in Rust on Bevy 0.19.0, wgpu 29. Licence-clean code only: GPL/AGPL sources may be read for ideas but never copied.
- Roadmap 5.3: "Native Bevy ECS entity replication over WebSockets / UDP. Synchronization of player transforms, animations, combat actions, and world entity deltas."
- Earlier project research (do not repeat): SkyMP's server-authoritative design with a server scripting API is the model to follow; Skyrim Together Reborn uses party-leader authority. Mudcrab's own design rules: stable IDs for every world object, per-cell change records (what changed versus the base data), events, MP-ready scripting (Luau via mlua).
- World facts: tens of thousands of static references per region streamed by cell; NPCs, items and doors change state; load doors connect interiors and exteriors (players can be in different cells).

# Scope
Web research only: GitHub repos, docs.rs, crate changelogs, Bevy community posts, GDC/blog talks on open-world netcode.

# Questions (answer each, with sources)
1. Crates: lightyear, bevy_replicon (+ its transports: renet, quinnet, aeronet, steamworks), aeronet, bevy_renet, naia, matchbox (WebRTC) and any newer ones. For each: latest version, Bevy version supported (0.19? 0.20?), licence, maintenance activity in 2026, transports (UDP, QUIC, WebTransport, WebSocket, WebRTC, Steam), features (interest management / visibility, prediction, rollback, interpolation, bandwidth limits, authority transfer, host-client mode).
2. Interest management at open-world scale: which crates support per-client visibility by region/cell, and how do they scale with tens of thousands of entities? Patterns from MMO/large-world games (area of interest grids, relevancy, priority accumulators).
3. Static vs dynamic: how do engines avoid replicating static world data (both sides load the same content; replicate only deltas keyed by stable IDs)? Map this to "per-cell change records".
4. Authority models for co-op RPGs (dedicated server vs listen server vs host authority; NPC/AI ownership; physics ownership handoff) with pros/cons and examples.
5. Web clients: WebTransport/WebSocket support in those crates for a wasm client.
6. Determinism/save interplay: how replicated state and save games share one change-record format in any shipped engine.

# Done when
A crate comparison table (version, Bevy version, licence, transports, interest management, last release date) plus answers to 2-6 with primary sources, and a recommendation of one stack for a first prototype with its risks. UNVERIFIED on anything not confirmed from a primary source.

# Report
Your final answer is the report: table, answers with sources inline, recommendation.

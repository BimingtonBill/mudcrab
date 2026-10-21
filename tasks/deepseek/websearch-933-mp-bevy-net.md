# Goal
What Rust/Bevy networking options and patterns fit an open-world RPG server for 10-100 players, and can a new engine interoperate with existing Skyrim multiplayer servers?

# Context
- Background: Mudcrab is an open-source (MIT/Apache-2.0 dual-licensed) reimplementation of the Skyrim engine in Rust with the Bevy game engine. Players convert their own legally owned Skyrim Special Edition data, then play it in the new engine (it does not run the original executable). Multiplayer is on its long-term roadmap ("native co-op: ECS entity replication over UDP/WebSockets; sync player transforms, animations, combat and world deltas"). The goal of this research: design multiplayer that serves the Skyrim ROLEPLAY community and builds on existing Skyrim multiplayer projects instead of starting from zero.
- Cite every claim with URLs; prefer primary sources (official sites, GitHub repos, docs, wikis, developer posts, Discord/Reddit announcements by the projects). Give dates; flag anything older than 2 years as possibly stale.
- Mudcrab uses Bevy 0.19 (ECS), streams the world by cells (exterior grid cells and interior cells), and stores world data in SQLite. A save design proposal stores per-cell overrides on top of the base world.
- Part A, libraries: lightyear, bevy_replicon (+ renet/quinnet backends), naia, bevy_renet, matchbox (WebRTC), GGRS (rollback, likely not suitable), and any newer ones (2025-2026). For each: latest version and Bevy version supported, maintenance activity, authority model (server-authoritative, client prediction, interpolation, lag compensation), interest management/rooms/visibility, WebSocket/WebTransport support (browser clients), license, notable games using it.
- Part B, patterns: interest management by spatial cell for open worlds, authoritative servers for melee combat (lag compensation), headless dedicated server builds in Bevy, server-side scripting for mods (Lua/Luau/WASM).
- Part C, interoperability: are the network protocols of SkyMP and Skyrim Together Reborn documented and stable? Could a third-party client speak to a SkyMP server (it has a separate server with a scripting API) or a Skyrim Together server? What licenses govern the protocol/server code, and would interoperating (not copying code) be allowed? Any precedent of a new client joining an existing game's MP servers (e.g. OpenMW + TES3MP, open-source clients for MMOs).

# Done when
- Each library has version, Bevy support, maintenance and authority-model facts with sources; the interop question has an evidence-based answer or "unknown" with what was checked.

# Report
- Library comparison table, then patterns, then the interop answer with license notes. Under 1300 words.

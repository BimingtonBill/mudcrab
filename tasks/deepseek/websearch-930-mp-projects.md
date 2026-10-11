# Goal
Survey every notable Skyrim multiplayer project, past and present, and how each one works.

# Context
- Background: Mudcrab is an open-source (MIT/Apache-2.0 dual-licensed) reimplementation of the Skyrim engine in Rust with the Bevy game engine. Players convert their own legally owned Skyrim Special Edition data, then play it in the new engine (it does not run the original executable). Multiplayer is on its long-term roadmap ("native co-op: ECS entity replication over UDP/WebSockets; sync player transforms, animations, combat and world deltas"). The goal of this research: design multiplayer that serves the Skyrim ROLEPLAY community and builds on existing Skyrim multiplayer projects instead of starting from zero.
- Cite every claim with URLs; prefer primary sources (official sites, GitHub repos, docs, wikis, developer posts, Discord/Reddit announcements by the projects). Give dates; flag anything older than 2 years as possibly stale.
- Projects to cover at least: Skyrim Together Reborn (Tilted Phoques), SkyMP / Skyrim Multiplayer (skymp.net, the skyrim-multiplayer GitHub org), Skyrim Online (old), Skyrim Unbound / Tamriel-style projects, Keizaal Online (and which base it runs on), Nirn/other server networks; find any others (2015-2026), including Fallout 4 / Fallout New Vegas equivalents by the same teams if relevant.
- For each: status (active/dead, latest release date), license (exact SPDX if possible; matters because Mudcrab is MIT/Apache and cannot copy GPL/AGPL code, but can interoperate with a protocol), language/stack, client model (SKSE/DLL injection into the real game vs own client), server model (authoritative server? P2P? host-player?), what is synchronised (players, NPCs/AI, combat, quests, inventory, world objects, time/weather), how they handle scripts (Papyrus) and quests in multiplayer, server-side scripting/modding API (e.g. SkyMP's TypeScript/Papyrus-like API), max players per server, known hard problems and how they solved or gave up, community size (Discord members, servers listed, Nexus downloads).

# Done when
- Every listed project has the fields above filled or marked "not found", each with sources.

# Report
- A comparison table first, then one section per project, then "Lessons for a new engine": what each project found hard because it had to inject into the original game, and what an engine that owns its own code could do differently. Under 1500 words.

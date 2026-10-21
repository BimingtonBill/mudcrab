# Goal
How have open-source reimplementations of Bethesda-style games added multiplayer, and what did they learn?

# Context
- Background: Mudcrab is an open-source (MIT/Apache-2.0 dual-licensed) reimplementation of the Skyrim engine in Rust with the Bevy game engine. Players convert their own legally owned Skyrim Special Edition data, then play it in the new engine (it does not run the original executable). Multiplayer is on its long-term roadmap ("native co-op: ECS entity replication over UDP/WebSockets; sync player transforms, animations, combat and world deltas"). The goal of this research: design multiplayer that serves the Skyrim ROLEPLAY community and builds on existing Skyrim multiplayer projects instead of starting from zero.
- Cite every claim with URLs; prefer primary sources (official sites, GitHub repos, docs, wikis, developer posts, Discord/Reddit announcements by the projects). Give dates; flag anything older than 2 years as possibly stale.
- Cover at least: TES3MP (multiplayer for OpenMW / Morrowind): architecture (client/server, RakNet), what it syncs, how it handles single-player scripts and quests (the "script sync" problem), its Lua server scripting (CoreScripts), persistence, RP servers built on it, its current status and relation to OpenMW (merged? upstream OpenMW multiplayer plans?), problems documented by its developers. Also OpenMW's own Lua scripting design which was "conceptually compatible with multiplayer" (global vs local scripts) and why. Daggerfall Unity multiplayer attempts, OpenRW/GTA reimplementations with multiplayer, FiveM/alt:V for GTA as server-scripting models, and any Fallout/Oblivion open-engine MP efforts.
- Focus on transferable design lessons: authority model, interest management (only send nearby cells/entities), how single-player content (scripted quests, AI packages, dialogue) was made to work with several players, save/persistence, server-side modding API, latency handling for melee combat.

# Done when
- TES3MP is covered in depth with developer sources; the others briefly; a list of lessons with sources.

# Report
- TES3MP section first, then others, then "Design lessons" as a numbered list with sources and confidence. Under 1300 words.

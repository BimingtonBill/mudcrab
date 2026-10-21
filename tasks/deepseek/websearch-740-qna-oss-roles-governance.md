# Goal
How do comparable open source game-engine projects divide roles and decisions between people: owner or lead, maintainers, area owners, reviewers, contributors, and bots? How is a new contributor onboarded and given an area?

# Context
- Background (public facts only): a young open source game-engine project in Rust on the Bevy engine. It reimplements the Skyrim (2011 RPG) engine from the game's original data files, using an offline converter (game formats to glTF/KTX2/SQLite), an engine, and a launcher app. One owner-maintainer merges. Several contributors work in parallel on separate areas (converter, engine/renderer, launcher, upstream pull requests) and several use AI coding agents heavily. The contributors are volunteers and mostly hobbyists.
- Comparable projects to consider first: OpenMW (Morrowind engine), OpenRA, Godot, Bevy itself, Veloren, Luanti/Minetest, Stride, Ryujinx/Yuzu-style emulator projects, xrEngine/OpenXRay, Vintage Story mods, and any other open source game engine or game-data-reimplementation project you find. Stick to what is documented publicly (CONTRIBUTING files, governance docs, roadmaps, wikis, blog posts, talks, GitHub project boards).
- Angle for this run: roles, governance, maintainers, area ownership, CODEOWNERS, decision making, how big changes get agreed (RFCs, design docs)

# Done when
- At least 5 named projects each have concrete, cited practices (not generalities), or the report says which could not be found.
- Each practice says who does it, how often, and what problem it solves.

# Report
- Answer first: a short ranked list of the practices most worth copying for a project this size, with the reason.
- Then per project: practice, URL, how many independent sources support it, your confidence.
- Say where sources disagree, and mark anything you inferred rather than read.
- Do not paste long quotes; summarise in your own words. At most one short quote per source.

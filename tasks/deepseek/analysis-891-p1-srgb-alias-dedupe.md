# Goal
Work out what deduplicating the `.opensky-srgb.ktx2` texture aliases would save and what it would take. The portal output has 12,874 of them (3.28 GiB), reported as byte copies of their base textures. Phase 1 track. Code reading plus small read-only checks.

# Context
- Analysis #887 (`local/agents/runs/analysis-887-p1-uastc-share.1/report.md`) found that `.opensky-srgb.ktx2` files are byte-identical copies of their base textures, written by `publish_texture_aliases` (`crates/converter/src/pipeline.rs` around 1813-1856). material.rs creates the alias names (around 665 and 965). Code: `C:/Modding/Mudcrab/mudcrab/crates/converter/src/` (portal branch); main's converter for comparison: `D:/Mudcrab/worktrees/p1-main-ro/crates/converter/src/`.
- Main's p2main output (2026-10-02) had only 4 UASTC aliases against portal's 9,368 UASTC aliases (all-format alias count on main unknown). Find out why the counts differ (which references create aliases: GLB material URIs, LOD, terrain).
- The engine side: how the engine loads a texture through its alias (is the sRGB flag carried by the file name, a KTX2 header field, or the material?). `crates/engine/src/` (search `opensky-srgb`).

# Questions
1. Why does an alias exist at all: the same image used both as colour (sRGB) and as data (linear)? Is it a byte copy, or does its KTX2 header differ (vkFormat _SRGB vs _UNORM, DFD transfer function)? Check by reading only the first 80 bytes of 3 base/alias pairs in `C:/Modding/Mudcrab/SkyrimConverted-portal-lod/textures/` (headers only, no whole files, never write, never run du or dust).
2. Size (measured by the lead with os.stat, 2026-10-07): 12,874 `*.opensky-srgb.ktx2` files in the portal output (9,368 of them UASTC), 3.28 GiB in all; every one has a base file beside it and none is hard-linked. Check whether they are all byte copies, or only some, from the headers in question 1.
3. What references the aliases: GLB material URIs, terrain or LOD manifests, the world DB? Give the code paths and one or two examples.
4. Options: (a) a hard link instead of a copy (same bytes, no extra disk; check the publish and the reuse paths cope with hard links); (b) no alias, with the sRGB/linear choice made at load time from the material slot (engine change, Phase 2); (c) a KTX2 header-only difference, if the files must differ. For each: saving, converter change size, engine change, risk.
5. Why portal has 9,368 UASTC aliases and main 4: a portal-only change, or the old 101_601 assets?

# Scope
Read only. No cargo, no engines, no file writes. Never write into any `SkyrimConverted*` folder.

# Report
Under 400 words: answers 1-5 with file:line evidence, the measured size, a table (option | saving | converter change | engine change | risk) and a one-line recommendation.

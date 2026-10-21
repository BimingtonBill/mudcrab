# Goal
Explain why the converter's LOD stage takes about 40 minutes on a full Skyrim SE conversion
(seen 2026-10-07 on two runs: Phase 2's p2main-v24 and Portal's portal-rooms; the CPU sat at 100%
during it). Find where the time goes, from code and the run outputs, without running anything.

# Context
- Converter LOD code: `crates/converter/src/lod*.rs` and whatever they call (terrain LOD albedo baking,
  KTX2 encoding, object LOD meshes, tree billboards). Spec: `docs/specs/converters/lod-compiler.md`
  (LOD-V9: terrain LOD bakes diffuse albedo from up to six LAND layers in linear light, then writes an
  sRGB KTX2 atlas with a mip chain). Earlier map of the code: `local/agents/runs/research-640-p2-lod-converter-code.1/report.md`.
- Run outputs (read only, never write there): `C:/Modding/Mudcrab/SkyrimConverted-portal-rooms/` and
  `C:/Modding/Mudcrab/SkyrimConverted-p2main-v24/`: `lod-manifest.json`, `integration-report.json`,
  `conversion-manifest.json`, the `lod/` folder (count and size of outputs). Use any stage timings they hold.
- Upstream owner of LOD is Chef (TaylorTurnerIT, PRs #103/#105/#106). This is analysis only.

# Scope
Read only. Change nothing. Do not open `skyrim_world.db`; Read/Grep only.

# Done when
Each claim cites code (path:line) or a manifest field. Work counts come from the real outputs
(how many worlds, chunks, quadrants, textures, meshes, encodes).

# Report
1. Where the 40 minutes go, ranked, with the evidence (estimated share per sub-step).
2. Parallelism: which sub-steps use all cores, which run single-threaded or serialised (locks, one
   world at a time, sequential encodes), and whether GPU encoding is used or available.
3. Repeated work: anything recomputed on every conversion that could be reused when inputs are
   unchanged (e.g. with `--reuse-assets`), and what the cache key would have to include to stay
   correct for mods and load orders.
4. Foundations check for each speed-up idea: no shortcuts (no lower quality, no skipped worlds),
   right layer, moddable. Say which ideas belong to Chef's area.

# Skyrim SE distant LOD formats (web research)

**Status:** DeepSeek websearch-512, 2026-09-23. Web claims, not verified against the game files; the report's own confidence notes are kept. Input to the Phase 2 distant-LOD design (research-513).

## Report: Skyrim SE distant LOD (terrain / objects / trees)

**Answer first:** Skyrim's distant rendering is entirely precomputed and file-driven — three separate systems. Terrain = `.btr` mesh files split by LOD level, one file per N×N cell block. Objects = `.bto` combined meshes plus per-worldspace DDS atlases, chosen for inclusion by the STAT record's *Has Distant LOD* flag and MNAM paths. Trees = **not meshes at all**: a `.lst` table of billboard definitions plus `.btt` files of per-tree instances, drawn from a `...TreeLOD.dds` atlas. Which set you see is decided purely by distance thresholds in `[TerrainManager]`; the levels *switch*, they don't blend. All of it ships inside the vanilla meshes/textures BSAs.

---

### 1. Terrain LOD (`.btr`)

- **Location / naming:** `..\Meshes\Terrain\[WORLDSPACE]\[WORLDSPACE].*.*.*.BTR`, and textures `..\Textures\Terrain\[WORLDSPACE]\[WORLDSPACE].*.*.*.DDS`. First wildcard = terrain LOD level **4, 8, 16 or 32**; the 2nd and 3rd are **"the cell coordinates of the west/south cell for the area the file covers"** ([DynDOLOD Terrain LOD](https://dyndolod.info/Help/Terrain-LOD), fetched; corroborated by community guides).
- **Cell coverage:** the level number is the **edge length in cells** — "The LOD level 4 means 4x4 = 16 cells are covered by it, 8x8 = 64 cells and so on" ([DynDOLOD Object LOD / xLODGen forum threads](https://www.ncesc.com/gaming-pedia/what-are-the-levels-of-dyndolod/)). So a file is addressed by the *lower-left* cell of its block; the game derives which file covers a given cell.
- **Contents:** a NIF-format mesh (`BSMultiBoundNode`/shape tree, landscape geometry plus LOD water). Tools treat them as renamed NIFs — NifSkope reads them after extension support was added ([nifskope issue #17](https://github.com/niftools/nifskope/issues/17), fetched). Level 4 carries **vertex colours**; levels 8/16 have none in vanilla, and the *HD LOD texture* flag on records only affects level 4 ([TES5LODGen thread](https://www.afkmods.com/index.php?/topic/3931-wipz-tes5lodgen/), search summary).
- **SE-specific:** in SSE/VR terrain LOD meshes are "**being lowered in the distance — which is baked into a shader**", apparently to stop z-fighting with object LOD ([DynDOLOD Terrain LOD](https://dyndolod.info/Help/Terrain-LOD), fetched). An engine reimplementation must reproduce that offset or accept z-fighting.
- **Generation:** height from the first `LAND` record (fallback: WRLD default land height); textures from LTEX→TXST layers plus LAND vertex colour/*normal* data. Full terrain textures fade to the LOD4 diffuse "in a fixed circle around the current center cell" (same page, fetched).

**Could not find:** a byte-level spec of the `.btr` binary — no published vertex layout, vertex counts per side, or confirmation of a distinct header magic. Sources uniformly call them NIFs; nif.xml is reported (via a search summary of [niftools/nifskope DeepWiki](https://deepwiki.com/niftools/nifskope)) to carry ext `"bto btr"` version entries — Skyrim bsver 83, **Skyrim SE bsver 100**, FO4 130, FO76 155, version 20.2.0.7 / user version 12 — but I could not fetch that entry directly, so treat the numbers as unverified.

### 2. Object LOD (`.bto`)

- **Location:** `..\Meshes\Terrain\[WORLDSPACE]\Objects\[WORLDSPACE].*.*.*.BTO`; first wildcard = "**object LOD level 4, 8, 16 or 32**", next two = cell coords of the west/south cell covered ([DynDOLOD Object LOD](https://dyndolod.info/Help/Object-LOD), fetched).
- **Contents:** combined meshes + textures for static, non-animated objects ("combined meshes and textures for objects that do not change and are not animated"), no animation, limited shader support, fixed alpha threshold 128 ([DynDOLOD Reference](https://dyndolod.info/DynDOLOD-Reference), fetched). The CK forces a **texture atlas for all but one texture (the HD LOD texture)**; xLODGen/DynDOLOD can use free textures.
- **Vanilla has 3 levels (4/8/16).** "The vanilla game does not contain object LOD combined for 32x32 cells"; level 32 only appears on the map via `uLockedObjectMapLOD=32` (fetched, Object LOD page).
- **Selection rule:** CK/xLODGen build static LOD "based on the **Has Distant LOD flag** and the **distant LOD models defined on the STAT base records**" ([DynDOLOD Reference](https://dyndolod.info/DynDOLOD-Reference), fetched). Confirmed in the record format ([UESP STAT](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/STAT), fetched):
  - flag `0x00008000` **Has Distant LOD**; `0x00020000` **Uses High-Detail LOD Texture**; `0x00000040` **Has Tree LOD**
  - **MNAM** = "LOD Data", **1040 bytes, present only if the flag is set**: four null-terminated `char[260]` paths — LODModel1 (**HighDetail**), LODModel2, LODModel3, LODModel4 (**LowDetail**) — first empty string marks the lowest detail level.
  - Source LOD models are `meshes\lod\*_lod*.nif` (DynDOLOD Reference, fetched).
- **Atlas:** `..\Textures\Terrain\[WORLDSPACE]\Objects\[WORLDSPACE].Objects.DDS` for vanilla meshes; DynDOLOD-generated meshes use `Textures\DynDOLOD\LOD\DynDOLOD_[WORLDSPACE].DDS` (fetched).

### 3. Tree LOD

- **Not meshes.** `.lst` = one entry per tree *type*, holding size/scale and the **min/max UV rectangle in the atlas**; `.btt` = binary instance data telling the game *which* billboard, *where* and at *what scale* ([afkmods TES5LODGen](https://www.afkmods.com/index.php?/topic/3931-wipz-tes5lodgen/), [3DMGAME mirror](https://bbs.3dmgame.com/thread-4937541-1-1.html) via search summaries; [DynDOLOD Tree LOD](https://dyndolod.info/Help/Tree-LOD), fetched).
- **Paths:** `..\Meshes\Terrain\[WORLDSPACE]\Trees\[WORLSPACE].LST`, `..\Meshes\Terrain\[WORLDSPACE]\Trees\[WORLSPACE].*.*.*.BTT`, atlas `..\Textures\Terrain\[WORLDSPACE]\Trees\[WORLDSPACE]TreeLOD.DDS` (fetched).
- **Rules:** one atlas per worldspace, **max 256 billboards per atlas/worldspace**; each tree is drawn on **two double-sided planes intersecting at 90°**; **"One billboard side per tree"**; **no normal maps** (limited lighting), **no x/y rotation** (always vertical), fixed centre → overdraw. **No separate LOD levels** for trees; the single cutoff is `fTreeLoadDistance` (fetched). Trees are TREE or STAT references whose base record has the **Has Tree LOD** flag (0x40) — matches UESP.
- **Caveat for implementers:** tree LOD unloads correctly only if the plugin load order of tree references is unchanged after generation, and "Tree LOD does not unload for references added by ESL flagged plugins" (fetched).

**Could not find:** formal field-by-field layout for `.btt`/`.lst` (field order, float types, whether `.btt` carries a LOD-level number — the 3-wildcard path suggests level+coords but no source states it). Only conceptual descriptions exist in the sources I reached.

### 4. Distance thresholds and defaults

Primary-ish source: [STEP wiki, SkyrimPrefs INI / TerrainManager](https://stepmodifications.org/wiki/Guide:SkyrimPrefs_INI/TerrainManager) (fetched, last edited 2015; documents Classic Skyrim):

| Setting | Default (=Medium) | Low | High | Ultra | Meaning (per page) |
|---|---|---|---|---|---|
| `fBlockLevel0Distance` | **20480** | 15000 | 25000 | 35000 | max distance, object LOD **level 4** |
| `fBlockLevel1Distance` | **32768** | 25000 | 40000 | 70000 | max distance, object LOD **level 8** |
| `fBlockMaximumDistance` | **100000** | 75000 | 150000 | 250000 | max distance, object LOD **level 16** |
| `fSplitDistanceMult` | **0.75** | 0.4 | 1.1 | 1.5 | "distance multiplier for landscape LOD levels" |
| `fTreeLoadDistance` | **25000** | 12500 | 40000 | 75000 | max distance tree LOD loads |

Notes: game units ≈ centimetres (Skyrim uses ~units where 1 unit ≈ 1.43 cm; treat distances as engine units, not metres — I did not verify the conversion from a primary source). The widely-quoted `35000 / 70000 / 250000 / 1.5` block that appears in forum "vanilla INI" dumps is the **Ultra preset**, not the default ([Overclock.net thread](https://www.overclock.net/threads/the-skyrim-ini-and-other-tweaks-all-in-one-thread.1165978/)); some forum dumps also show *lower* numbers claimed as vanilla, and one poster admits copying them from a modified INI — so treat only the STEP table as authoritative. Terrain levels are scaled by `fSplitDistanceMult`; **levels switch, they do not blend** (DynDOLOD pages, fetched).

**Could not find:** (a) **SSE-specific** shipped defaults — the STEP page is Classic Skyrim; I saw no primary SSE INI dump; (b) a primary source for **`uGridsToLoad`**'s default (community consensus is 5 on PC, odd values only, but I did not verify it and did not find it in any of the pages I read). Verify both against an actual SSE `SkyrimPrefs.ini`/`Skyrim.ini`. `uLockedObjectMapLOD` governs map LOD (32), per the fetched Object LOD page.

### 5. Are they in the vanilla BSAs?

- Yes. For Classic Skyrim the file distribution is explicit: **`Skyrim - Meshes.bsa` contains object `.nif` *and* `.btr` files**, **`Skyrim - Textures.bsa` contains the `.dds` textures** ([Japanese Skyrim dev-data wiki](https://wiki.skyrim.z49.org/?page=%E9%96%8B%E7%99%BA%E8%B3%87%E6%96%99%2F%E5%85%AC%E5%BC%8F%E3%83%87%E3%83%BC%E3%82%BF%E3%81%AE%E6%A7%8B%E6%88%90); [afkmods "Generating Custom LOD"](https://www.afkmods.com/index.php?/topic/3157-skyrim-generating-custom-lod/) — both via search summary). LOD source meshes live in the archives' `LOD` folder.
- **SSE splits them:** `sResourceArchiveList` contains **`Skyrim - Meshes0.bsa`** and **`Skyrim - Meshes1.bsa`**; `sResourceArchiveList2` contains **`Skyrim - Textures0.bsa` … `Textures8.bsa`** plus `Skyrim - Patch.bsa` (default SSE `Skyrim.ini`, [reproduced in modding threads](https://forums.nexusmods.com/topic/6428251-way-around-unpacking-bsa-missing-mesh-in-ck/); [Archive Name Extender](https://skyrimspecialedition.2game.info/detail.php?id=171480) lists the same prefixes). Archives are ~2 GB-limited, which is why they're split.
- **Rough sizes:** Classic `Skyrim - Meshes.bsa` 1,485 MB packed / 2,990 MB extracted; `Skyrim - Textures.bsa` 1,356 MB / 2,381 MB ([Nexus wiki, Skyrim BSA Extraction](https://wiki.nexusmods.com/index.php/Skyrim_BSA_Extraction), via search summary). For SSE, [SteamDB depot 489832](https://steamdb.info/depot/489832/history/) lists `Skinrim - Textures1.bsa` at 1.41 GiB (fetched result shows the depot listing; I did not fetch SteamDB directly). For scale of the terrain LOD alone, the popular "Terrain LOD Redone" replacement mod is **1.1 GB** ([Nexus](https://www.nexusmods.com/skyrimspecialedition/mods/9135)).
- **Could not find:** which of Meshes0/Meshes1 holds `Meshes\Terrain\...`, and file **counts** for the vanilla LOD sets (one LODGen log line glimpsed in search results shows a `[.../24]` counter in the tens of thousands for a full-world build, but that is not a reliable count). Get the exact inventory by listing the BSAs with BSA Browser/`bsarch` — a cheap, definitive check for your loader.

### 6. Prior art

**OpenMW** (Morrowind, not Skyrim — no `.btr/.bto` support found):
- **Distant terrain** (bool, default `false`): chunks terrain and loads the whole world's terrain at all times, using LOD; documented as experimental. Nearby pages are preloaded in the background.
- Terrain LOD is controlled by **`vertex lod mod`** (int, default 0; each step doubles/halves rendered vertices) and **`lod factor`** (float, default `1.0`); docs tell users not to tune LOD with `lod factor` but with `vertex lod mod` and composite-map resolution first ([Terrain Settings, OpenMW 0.50](https://openmw.readthedocs.io/en/openmw-0.50.0/reference/modding/settings/terrain.html)).
- **Object paging** (default `true` in 0.50; no effect unless distant terrain is on): batches/merges non-terrain objects outside the active grid to cut **draw calls**, which OpenMW identifies as the main bottleneck. Related settings: `object paging active grid`, `object paging merge factor` (250.0), `object paging min size` (0.01). LOD detection is by **mesh-name convention** (`_dist`, `_lod`), and `NiLodNode`-based LODs are explicitly *not* handled ([OpenMW MR !4872](https://gitlab.com/OpenMW/openmw/-/merge_requests/4872/pipelines), [terrain.rst](https://github.com/OpenMW/openmw/blob/master/docs/source/reference/modding/settings/terrain.rst)).
- Takeaway for you: OpenMW solves the "everything far away is one draw call" problem by merging, but its own terrain LOD is engine-generated, so it is only a design precedent, not a format reference.

**Community Skyrim LOD tooling (documents the formats):**
- **xLODGen / LODGen.exe** — generates terrain LOD, object LOD, tree LOD and occlusion; it "weaves new NIFs from all the lod meshes and terrain" in **4x4, 8x8, 16x16 and 32x32 cell blocks**; command line takes an export file + Data path, and **BTR files must already be present** in Data `<...>/LODGen` docs in the [xLODGen thread](https://stepmodifications.org/forum/topic/13451-xlodgen-terrain-lod-beta-128-for-fnv-fo3-fo4-fo4vr-tes5-sse-tes5vr-enderal-enderalse/).
- **DynDOLOD** — object/tree/grass LOD + occlusion; explicitly **does not generate terrain LOD** (fetched pages above). Documents the file naming conventions, the atlas scheme, the CK atlas limitation, and the flag/record rules quoted in §2–3.
- **NifSkope 2.0 dev** reads `.btr/.bto` natively and is the standard inspection tool ([issue #17](https://github.com/niftools/nifskope/issues/17), fetched).
- **Occlusion data** (TVDT) records whether LOD blocks of 4x4/8x8/16x16/32x32 cells are visible — worth loading if you want correct horizon culling ([DynDOLOD Occlusion Data](https://dyndolod.info/Help/Occlusion-Data)).
- **No open-source Skyrim engine reimplementation that consumes `.btr`/`.bto` was found** in this research. If one exists it did not surface; treat that as a gap, not a proof.

---

### Web-sourced (everything above came from the web; nothing was changed)

| # | URL | How verified |
|---|---|---|
| 1 | https://dyndolod.info/Help/Terrain-LOD | Fetched directly. Primary-ish (tool author docs) |
| 2 | https://dyndolod.info/Help/Object-LOD | Fetched directly |
| 3 | https://dyndolod.info/Help/Tree-LOD | Fetched directly |
| 4 | https://dyndolod.info/DynDOLOD-Reference | Fetched directly |
| 5 | https://stepmodifications.org/wiki/Guide:SkyrimPrefs_INI/TerrainManager | Fetched directly (defaults table) |
| 6 | https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/STAT | Fetched directly (flags, MNAM) |
| 7 | https://github.com/niftools/nifskope/issues/17 | Fetched directly (thin content) |
| 8 | https://openmw.readthedocs.io/en/openmw-0.50.0/reference/modding/settings/terrain.html | Search summary only — **not fetched**; verify before use |
| 9 | https://github.com/OpenMW/openmw/blob/master/docs/source/reference/modding/settings/terrain.rst | Search summary only |
| 10 | https://gitlab.com/OpenMW/openmw/-/merge_requests/4872 | Search summary only |
| 11 | https://www.afkmods.com/index.php?/topic/3931-wipz-tes5lodgen/ | Search summary only |
| 12 | https://www.afkmods.com/index.php?/topic/3157-skyrim-generating-custom-lod/ | Search summary only |
| 13 | https://wiki.skyrim.z49.org/?page=開発資料/公式データの構成 | Search summary only |
| 14 | https://wiki.nexusmods.com/index.php/Skyrim_BSA_Extraction | Search summary only |
| 15 | https://deepwiki.com/niftools/nifskope (nif.xml version table) | Search summary only — **unverified** |
| 16 | https://steamdb.info/depot/489832/history/ | Search summary only |
| 17 | https://dyndolod.info/Help/Occlusion-Data | Listed in search results |

**Independent-source count / confidence:** `.btr`/`.bto` paths, level numbers and cell-coordinate meaning — 2+ independent sources (DynDOLOD docs + xLODGen/community guides), **high**. STAT `0x8000` flag and MNAM 1040-byte 4×260 path layout — 1 primary (UESP), consistent with DynDOLOD's prose, **high**. Tree LOD being `.lst`+`.btt`+atlas with 256-billboard limit — 2 sources (DynDOLOD + afkmods), **high**; internal binary layout **not found**. INI defaults — 1 solid source (STEP) corroborated loosely by forum INI dumps, **medium-high**, and **not verified for SSE**. Archive membership — 2 sources for Classic Skyrim (**high**), SSE naming from INI listings (**medium**, exact archive per file unverified); sizes **medium** and Classic-only.

**No prompt-injection content was encountered**: every page I fetched read as ordinary tool documentation or issue text, and none contained instructions addressed to an AI assistant or automated reader.

**Still missing, ranked:** (1) byte-level `.btr`/`.bto`/`.btt`/`.lst` specs — the single biggest gap for a loader; the pragmatic route is to parse the NIF header from nif.xml and diff a real vanilla file against a NifSkope-parsed dump. (2) Actual SSE INI defaults and `uGridsToLoad`. (3) Exact SSE archive inventory and file counts (trivially obtained locally by listing the BSAs). (4) Any existing open engine that renders these files.


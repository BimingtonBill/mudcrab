# Goal
Design distant-LOD step 4 for the upstream engine: far-away objects (`.bto` object LOD blocks)
and tree billboards (`.btt` tree LOD), drawn beyond the full-detail cells alongside step 3's
terrain LOD. A design document only, no code.

# Context
- Steps 1-3 exist on branches in `C:/Modding/wkj-phase2` (read-only):
  - `phase2/lod-mesh-containers`: the converter reads the LOD mesh containers;
  - `phase2/lod-database`: the world database gets `lod_*` tables, schema 4;
  - `phase2/lod-engine-terrain`: the engine draws terrain LOD behind `--lod`.

  Read them with `git -C C:/Modding/wkj-phase2 log` and `show`. Designs:
  `C:/Modding/MudcrabProject/docs/design/distant-lod.md` and `distant-lod-step3-brief.md`.
  Format research: `docs/research/skyrim-lod-formats.md` and `docs/research/tree-lod-layout.md`.
- Already true:
  - object LOD blocks convert to GLB (`meshes/terrain/<ws>/objects/*.bto`); step 2 records them
    (`lod_object_blocks` or similar);
  - tree LOD is decoded: `.lst` = tree types with billboard sizes, `.btt` = placed instances,
    32 bytes each; step 2 inventories them (120 tree types, 43,333 instances in upstream15);
  - FormID slots in tree LOD use the authoring master index (Dragonborn 0x02 -> 0x04);
  - step 3 has the block lookup (origin-relative `div_euclid`), bands, hysteresis and the shared
    commit budget (documented as a new baseline when `--lod` is on).
- Converted data: `C:/Modding/Mudcrab/SkyrimConverted-upstream15/` (DB schema 4 with the `lod_*` tables;
  the LOD GLBs under `meshes/terrain/`).

# What to design
1. **Object LOD:** how `.bto` blocks are chosen (same bands and blocks as terrain? which
   levels?), spawned and rebased. How they coexist with full-detail cells: hide an object LOD
   block where full-detail cells are resident, as Skyrim does, or leave overlap.
2. **Tree billboards:**
   - how instances become geometry: one instanced quad mesh per tree type (billboard atlas UVs
     from the `.lst`), camera-facing or cross-quads;
   - how they are batched per block;
   - how they hide inside the full-detail radius.
3. **Data flow:** which step-2 tables feed each part; anything missing from the DB (say what, and
   whether that means a schema bump).
4. **Budget:** extra draw calls and commit time; how they fit the shared commit budget.
5. **Tests:** synthetic fixtures (step 2's `fixture_lod.rs` pattern) and a real-data check.
6. **Split:** break it into 2-4 reviewable PR-sized steps, each with owned files and "done when".

# Scope
Read-only for code. Write only `local/research-557/DESIGN.md` (any scripts go under that
folder too; run them as `python <file>`).

# Report
The design summary (one paragraph per section), the step split, and the open questions.

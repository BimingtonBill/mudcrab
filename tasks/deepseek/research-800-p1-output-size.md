# Goal
Track: Phase 1. Measure why a converted Skyrim SE install is ~67 GB and what each way of shrinking it really saves, so the lead can pick one and propose it upstream. Read-only research.

# Context
- Read `tasks/deepseek/context.md` first.
- **Code to read is upstream main**, checked out read-only at `C:/Modding/wkj-phase1` (commit 6f583cd): `crates/converter/**` (pipeline, archive extraction, vfs, ingestion cache, texture, cache/manifest, publish), `crates/engine/**` (what it opens at run time), `docs/roadmap/01-asset-pipeline.md`, `docs/adr/**`, `README.md`. The files in this checkout (`C:/Modding/Mudcrab/mudcrab`, branch `portal`) are a fork; use them only for the fork-only branch notes named below.
- **Data** (read-only, a finished conversion of upstream main, converter schema 14): `C:/Modding/Mudcrab/SkyrimConverted-upstream/` with `vfs/`, `.ingestion-cache/` (hidden folder), `textures/`, `meshes/`, `scripts/`, `skyrim_world.db`, `cell_cache.rkyv`, `conversion-manifest.json`, `integration-report.json`. Game install (read-only): `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data` (the `.bsa` archives).
- Earlier rough numbers (verify, do not trust): vfs 25 GB, .ingestion-cache 21 GB, textures 18 GB (UASTC KTX2), meshes 1.2 GB, DB 1.3 GB, cell cache 0.5 GB; game Data 15 GB.
- Fork-only notes: `docs/research/` and `docs/pr/upstream-queue.md` in this checkout mention `staging-hard-links` (reconversion reuses files by hard link) and research-151 (incremental conversion). Say whether those overlap with any option below.

# Questions
1. **Breakdown.** Exact size and file count per top-level folder, and per extension inside `vfs/` and `.ingestion-cache/`. On NTFS, check whether files in the two folders are already hard links of each other (same file index / link count > 1) rather than copies.
2. **Who reads what.** For `vfs/`, `.ingestion-cache/`, `textures/`, `meshes/`, `scripts/`, the DB and the cell cache: which code reads each one, at conversion time (which stage), at reconversion time (cache reuse), and at engine run time (cite `path:line`). Which folders could be deleted after a finished conversion without breaking the engine, a later incremental reconversion, the integration report or `asset-closure`?
3. **Option A, vfs as hard links into the ingestion cache:** are the files byte-identical (hash a random sample of ~300 pairs of the same relative path)? What would change in code (one function or several), and what breaks if a user edits or deletes a file in one of them?
4. **Option B, no unpacked copy at all** (read loose files and archive entries through an index): which stages need raw bytes, and how many GB would it save?
5. **Option C, texture formats.** Scan the DDS headers (first 148 bytes only; do not read whole files) of every texture the converter took as input (from `vfs/textures/` or wherever the source textures are), and tabulate count and bytes per format (DXT1/BC1, DXT3, DXT5/BC3, BC5/ATI2, BC7, uncompressed, other), plus the KTX2 output bytes for the same inputs if you can match them by path. Estimate the output size if BCn inputs were stored as BCn inside KTX2 (optionally zstd-supercompressed; estimate zstd on a ~50-file sample) instead of re-encoded to UASTC. Say what upstream's ADRs/roadmap commit to about UASTC/Basis and portability (Android is a far end goal, not a current constraint).
6. **Option D, database and cell cache:** is terrain stored twice (DB `land` rows vs the cell cache)? Open the DB read-only (`file:...?mode=ro` URI) and report page count, freelist count, and the size of the 10 biggest tables/indexes (`dbstat` if available). What would `VACUUM` save?
7. Anything else large you find.

# Scope and rules
- Read-only everywhere. Write only `local/research/phase1-output-size.md` (your report) and any scripts you need under `tools/research/phase1_size/` (run them as `python tools/research/phase1_size/<script>.py ...`; never `python -c`). Do not commit anything.
- Never open a file for writing, rename, or delete anything under `C:/Modding/Mudcrab/SkyrimConverted-upstream` or the game install. Walk the folders once and cache the listing in `local/research/phase1_size/` so you do not re-walk 240k files repeatedly.
- No cargo builds and no engine runs.

# Report
`local/research/phase1-output-size.md`, under 1200 words: the breakdown table; the who-reads-what table with citations; one table of options (A-D and anything new): GB saved on this install, code touched, risk, effect on reconversion speed, fits upstream's ADRs yes/no; the numbers each estimate rests on and how they were measured; a recommendation of the first step. Then the same summary (under 400 words) as your final message.

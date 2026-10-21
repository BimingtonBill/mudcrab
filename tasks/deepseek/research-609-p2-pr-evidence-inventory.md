# Goal
Track: Phase 2. For each upstream PR bundle below, find existing before/after screenshots or
measurements that are a clean "upstream main vs the branch" pair on the upstream-schema data, and
say which are missing, so the lead knows which runs are still needed. Read-only.

# Context
- Upstream main = the `main` branch (engine without our fork's changes). Upstream-schema data =
  `C:/Modding/Mudcrab/SkyrimConverted-upstream`. A picture made with the portal build or `C:/Modding/Mudcrab/SkyrimConverted`
  (schema 2x) is NOT a clean pair for upstream; say so when that is the case.
- Evidence folders (read them, including the images; you can view PNG/JPG):
  `C:/Modding/Mudcrab/worktrees/p2-tools/` (subfolders such as `shots-water/`, `ab-water/`, `shots-normals/`, `shots-p2night/`,
  `p2night/`, `shots15/`, `shots-lod/`, `shots-world/`, `shots-cities/`, `shots-574/`, `shots-576/`, `shots-recut/`,
  `refsun/`, `refsun-ab/`, `stress-ab/`, `acceptance-main-60s/`, `ab-reflect-land/`, `reflect-ab/`, `retime2-2026-09-25/`,
  `water-pose/`, `vclr-r/`), and `C:/Modding/Mudcrab/mudcrab/local/gallery/phase2/`,
  `C:/Modding/Mudcrab/mudcrab/local/reference/`. Most folders have a log, a `summary.txt`, a `NOTE.txt` or a
  shots JSON that says which engine build and assets made them: read those to decide what build/data each
  image is from. Team logs `C:/Modding/Mudcrab/mudcrab/local/team/2026-09-2*.md` and
  `local/overnight-2026-09-24.md` record which run made which folder: grep them for the folder name.
- The bundles and the evidence wanted:
  1. water (water-look + water-colours + reflection-land-only): a lake/river with the white sheet before and a
     real lake after, plus the reflection. Is `shots-water` or `ab-water` (`cmp-water.jpg`) already a clean pair?
  2. terrain-textures (samplers-weights + normal-maps): ground close-up, blocky/stretched before, tiled/blended
     with relief after.
  3. sun-shadows (cascades + reflection-gating): a wide view where shadows stop partway before and reach
     across after.
  4. sky (dome + fog): 2-3 clean pairs, e.g. crops from the p2night sheets.
  5. vertex-colours (prepass + faithful + flag; needs a reconverted DB for "after"): any gravel/Whiterun
     pair on upstream-schema data, or only portal-data pairs?
  6. engine-loading (io-pool-stack + cell-cache-validate-once): the Markarth IO-stack crash log before and a
     clean pass after (look for "stack overflow"/"8074" in logs), plus the stress A/B table in `stress-ab/`.

# Scope
Read-only. Write nothing except your report. Run python only as script files if you must measure
something (never `python -c`); prefer reading.

# Report
One section per bundle: the best existing files (full paths), what each shows, build and data (with the
evidence for that), clean-pair yes/no, and what is missing. Then a list of the runs still needed, each
with a one-line recipe (engine branch, `--shots` file or pose, assets folder) and an estimated time.
Under 700 words.

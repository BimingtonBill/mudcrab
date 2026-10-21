# Goal
Track: Phase 1. Map where the converter stands today on five things the user wants, and propose a
step-by-step plan for each: (1) a progress/status bar, (2) faster conversion, (3) running the
non-texture work (meshes, scripts, plugins/database, cell cache) at the same time as textures so the
CPU stays busy, (4) resumability after a crash or stop, (5) diff/incremental conversion: after the
user installs more Creation Club content (new plugins + archives), only the new and changed things
are converted. Read-only research.

# Context
- Read `tasks/deepseek/context.md` first.
- **Code to read is upstream main**, read-only at `C:/Modding/wkj-phase1` (branch phase1/vfs-hard-links
  = main 6f583cd + a hard-link change): `crates/converter/**` (main.rs CLI, pipeline.rs stages and
  scheduling, archive/, cache.rs manifest and reuse, texture.rs, mesh.rs, script.rs, esm/),
  `crates/launcher` or any GUI crate if one exists (look for a progress display),
  `docs/roadmap/01-asset-pipeline.md`, `docs/specs/converters/`.
- Already known, do not redo, build on it:
  - `local/research/phase1-output-size.md` (what each output folder is and who reads it).
  - `local/research/discord-research-2026-09-27-triage.md` and `local/research/discord-research-2026-09-27.txt`
    (a source review of converter I/O: per-file `sync_all`, re-hashing, `--io-jobs` unused,
    `merge_plugins` twice, serial result hashing, repeated tree walks). Verify only what your plan
    depends on.
  - `docs/research/incremental-conversion.md` in this checkout (an earlier incremental-conversion design).
  - Fork branches (read with the Read tool through their files only if you must; list what they do from
    `docs/pr/upstream-queue.md`): `phase2/conversion-stage-times` (per-stage timings),
    `phase2/resume-staging-validation` (open upstream PR #31), `phase2/resume-publish-only`,
    `phase2/staging-hard-links`, `phase2/publish-robustness`, `phase2/pruned-texture-complete` (#30).
- Last full conversion log with timings, if useful: `D:/Mudcrab/s15-conversion.log`,
  `D:/Mudcrab/s15-resume.log`, `D:/Mudcrab/s15-report.json` (a reconversion: 22,761
  converted, 220,208 reused, ~35 min). Find the stage durations in them if they are there.

# Questions
1. **Stages and time:** list the pipeline's stages in order with what runs in parallel today and what
   waits for what (cite `path:line`). From the logs, how long does each stage take on a real run
   (fresh vs reconversion if both exist)? Where is the CPU idle?
2. **Status bar:** what progress does the converter report today (CLI output, progress channel,
   any GUI)? What would a useful bar need (per stage totals known up front? counts vs bytes?) and
   where would it hook in?
3. **Overlap:** which stages could run concurrently with texture encoding (the known dependency:
   texture encoding needs semantics collected from meshes and the database; confirm or correct)?
   Estimate the wall-time win from the stage times.
4. **Resumability:** what survives a crash or Ctrl+C today at each stage (manifest, caches, staging,
   `--resume-staging`)? What is lost and redone? What would make a stop at any point cost at most the
   work in flight?
5. **Diff / new content:** trace what happens when one new CC plugin + its archive is added to an
   already converted install: which stages redo everything (database, cell cache, full tree walks)
   and which reuse (archive cache by archive hash, output manifest by source hash)? What would a true
   incremental run need? How does `incremental-conversion.md` answer this, and is it still right?
6. **Speed list:** rank the Discord items plus anything you find by expected wall-time saving,
   using the stage times.

# Scope and rules
- Read-only everywhere. Write only `local/research/phase1-converter-speed-plan.md`. Do not commit.
  Python only as scripts under `tools/research/phase1_speed/` run as
  `python tools/research/phase1_speed/<script>.py`; never `python -c`.
- No cargo builds, no conversions, no engine runs.

# Report
`local/research/phase1-converter-speed-plan.md`, under 1500 words: the stage table with times; one
section per wish (1-5): today's state with citations, the gap, a proposed plan in small independent
steps (each a possible branch: size S/M/L, risk, what it depends on), and what overlaps with existing
fork branches; a combined order of work. End with the three steps you would do first. Then a
summary under 400 words as your final message.

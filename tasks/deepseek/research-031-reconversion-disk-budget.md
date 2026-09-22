# research-031 - How much disk does a reconversion need, and what can be reclaimed?

Read `tasks/deepseek/context.md` first. Read-only: do not delete or move anything.

## Goal

`C:/Modding/SkyrimConverted` (the converted assets) is 67 GB, and drive C: has 69 GB free.
Tonight's coding tasks will change the converter: materials, emissive, directional snow, door
animations and new database columns. That means a reconversion. Find out whether one fits, and
what the smallest safe reconversion is.

## Questions

1. How the converter publishes: the staging folder, then an atomic rename or copy? Does staging
   hold a full second copy, or does it reuse unchanged outputs (hard links, cache blobs, in-place
   updates)? Cite `crates/converter/src/` `file:line` (`cache.rs`, `publish`/`staging`,
   `--resume-staging`, the `cpu_jobs`/`io_jobs` settings).
2. What a reconversion after a converter schema bump does: does every GLB or KTX2 get rewritten,
   or only the invalidated kinds? Can a run redo only the database and the meshes but not the
   textures (flags, cache keys by kind)?
3. What is on disk now: list the sizes of the top-level folders of `C:/Modding/SkyrimConverted`
   (`meshes`, `textures`, `vfs`, `scripts`, the database, `cell_cache.rkyv`, and any staging or
   cache folders). Also list any old staging or cache folders elsewhere: search `C:/Modding`,
   the converter's default cache location (read its config), `%TEMP%` and `local/`. Use a Python
   script under `tools/research/` (`os.scandir`, summing sizes; it may take minutes, so print
   progress). Note which of them are regenerable, and which would be lost for good.
4. A recommendation: the peak disk a reconversion needs, whether it fits in 69 GB, and what to
   clean up first if not (only regenerable, project-generated data, never the game install).
   Give the exact commands, which the lead will run. Also, can the textures stay as they are
   while meshes and the database are redone?

## Deliverable

`docs/research/reconversion-disk-budget.md` with the answers and evidence. Commit it with a
handoff note in `docs/handoff/`, with `[deepseek]` subjects.

## Report

The numbers, the recommendation, and the exact commands.

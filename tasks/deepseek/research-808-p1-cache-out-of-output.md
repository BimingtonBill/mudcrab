# Goal
Track: Phase 1. Design moving `vfs/` and `.ingestion-cache/` out of the converter's published
output folder, so the folder the engine reads holds only what the engine reads, the extracted game
files are no longer written, verified and published again on every run, and a reconversion stops
paying for them. Read-only research: a design the lead can take upstream (an issue first, likely an
ADR) and then hand to a coder.

# Context
- Read `tasks/deepseek/context.md` first.
- Code (upstream main + a hard-link change) read-only at `C:/Modding/wkj-phase1` (checked-out
  branch `phase1/vfs-hard-links`: `vfs/` files and their `.ingestion-cache/` blobs are now hard
  links of one file; `cache::link_or_copy`).
- Measurements and the earlier designs, read them first:
  `local/research/phase1-output-size.md` (who reads each folder: the engine reads neither;
  `integration.rs` asset closure and `--resume-staging` read `vfs/`),
  `local/research/phase1-converter-speed-plan.md` (§5 and §6 item 2: ~48 GB written and ~47 GB read
  per run; the measured floor of a reconversion is inode-bound), and
  `docs/research/incremental-conversion.md` in this checkout (§4.8.1, §4.8.3).
- A real reconversion's stage times (2026-09-27): extract 13:38, meshes 4:05, textures 5:07,
  scripts 0:32, validate 4:59, 31 min total; the extraction stage is almost all cache verification
  and re-materialising 172,961 files.
- Docs: `docs/adr/` (how upstream records decisions), `docs/roadmap/01-asset-pipeline.md`,
  `docs/specs/converters/pipeline.md`.

# Questions
1. Where should the persistent content-addressed cache live (beside the output, e.g.
   `<output>.cache/`, or a user-chosen `--cache-dir`, default?), and what is its lifetime and
   garbage collection (blobs no retained manifest references)?
2. What replaces `vfs/` for the stages that read source files (mesh, texture, script conversion,
   asset closure in `integration.rs`, loose-file overlay, `--resume-staging`): materialise only what
   a stage needs into a scratch area, or give converters a resolver (loose file | cache blob by hash)
   so no `vfs/` tree exists at all? List every reader with `path:line` and what it would take.
3. Load order and overrides: how archive precedence and loose files are resolved without a
   materialised `vfs/` (the index must say which source wins for each path).
4. Verification: what must still be verified per run (archive hashes, blob hashes) and what can be
   sampled or trusted, keeping the guarantees the current code gives; how a corrupted blob is found
   and repaired.
5. Migration: what happens to an existing output that still contains `vfs/` and
   `.ingestion-cache/` (move the cache out once, delete `vfs/`), and how `--resume-staging` and the
   manifest schema change.
6. Expected savings on the measured install (disk and minutes), and risks.
7. A step plan in 2-4 independent steps (each a possible PR), smallest first, with owned files and
   tests; and a short draft of the upstream issue text (plain English, no project-internal names).

# Scope and rules
Read-only. Write only `local/research/phase1-cache-out-of-output.md` (under 1500 words). No cargo
builds (a conversion is running on this machine; do not read `D:/Mudcrab/p1-timing`), no
engine runs. Do not commit. Then a summary under 300 words as your final message.

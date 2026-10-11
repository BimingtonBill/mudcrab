# Goal
Track: Phase 1. Upstream's Phase 2 completion plan (`docs/roadmap/02-completion-plan.md` on
upstream main, read-only at `C:/Modding/wkj-phase1/docs/roadmap/02-completion-plan.md`) has
converter-side items nobody has claimed. Triage them with data, not opinion, so the lead can decide
which go on the Phase 1 list. Read-only research.

# Context
- Read `tasks/deepseek/context.md` first.
- An inventory by another session: `C:/Modding/wkj-phase2/local/agents/runs/research-618-p2-unclaimed-work.1/report.md`
  (read it). It names the converter-side items as M0.2, M0.3, M0.4, M1.2, M1.4, M1.6 (the plan's own
  numbering; find each item's text in the plan). Its note on M1.2 (NiTriStripsData support): decide
  from data, e.g. how many shipped meshes use NiTriStrips and currently fail or lose geometry.
- Data, read-only: a converted install `C:/Modding/Mudcrab/SkyrimConverted-upstream/` with
  `integration-report.json` and `conversion-manifest.json` (skipped inputs, warnings, and any
  invalid-geometry lists), and extracted NIFs under its `vfs/meshes/`. The converter code (upstream
  main) is at `C:/Modding/wkj-phase1/crates/converter` (its `src/bin/` holds audit tools such as a
  NIF audit; read what they report, do not build them).
- Also check the plan's claim that milestone 1 was complete on 2026-09-10: is there a retained
  closure report in the tree?

# Questions
For each item: its text in the plan; what the converter does today (`path:line`); the measured size
of the gap on the real install (count of affected meshes/records/textures from the reports or a
header scan you write); what it would take (S/M/L) and what it depends on; and whether it is
Phase 1 (converter) work or belongs elsewhere. For M1.2 specifically, scan NIF headers/blocks under
`vfs/meshes` for NiTriStrips/NiTriStripsData and cross-check against the manifest's skipped or
failed meshes.

# Scope and rules
Read-only. Write only `local/research/phase1-completion-plan-converter.md` (under 1200 words) and
scripts under `tools/research/phase1_size/` (run as `python tools/research/phase1_size/<script>.py`;
never `python -c`). No cargo builds, no engine runs. Never write, rename or delete anything under
`C:/Modding/Mudcrab/SkyrimConverted-upstream`. Do not commit. Then a summary under 300 words as your final
message, with a table: item | gap measured | size | Phase 1? | recommendation.

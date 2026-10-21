# Goal
Decide what to keep from two Phase 2 branches that newer work has superseded, so the lead can retire them. Track: Phase 2. Read only: delete nothing, and change no branch.

# Context
Checkout: C:/Modding/Mudcrab/mudcrab (git repo). Use git read commands only: log, show, diff, merge-base, branch --contains.
1. `phase2/preupdate-finalize-pacing` (tip 18e92a14, "cap concurrent cell loads in flight (impl-1060, handover snapshot)") is Hermes's version of impl-1060. It caps loads per CELL, and design review-1062 rejected that, because an interior destination is one cell (local/agents/runs/review-1062-p2-design-1060.glm.1/report.md). The kept version is `phase2/asset-load-pacing` (98fd43e6), which caps per issued load. Brief: tasks/deepseek/impl-1060-p2-load-pacing.md.
2. `phase2/paced-cell-unloads` (tip 8f980f64, impl-619) is older work on paced unloads. It may be superseded by what main and portal already hold (Retiring cells, `max_cell_unloads_per_frame`, `despawn_cells`) and by impl-1059, the time-budgeted distance pacer merged into portal e2038dc1 on phase2/despawn-time-budget.

# Done when
For each branch:
- What it contains beyond its merge-base with portal (commits, files, a short summary per commit).
- What is already in `phase2/asset-load-pacing` or portal (for branch 1), or in portal or main (for branch 2), in any form.
- Anything NOT covered elsewhere that is worth keeping: tests, metrics, docs, config, a correct edge case. Give the exact hunks or commit and why each is worth keeping.
- A verdict: "retire, nothing to keep", "retire after porting X", or "keep because Y".

# Report
Status / Summary / Left / Next, then the two per-branch sections with commit and file:line evidence. Write it also to local/research/phase2-retire-duplicates-2026-10-08.md.

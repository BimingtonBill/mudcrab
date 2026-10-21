# Goal
Which items of the Phase 2 completion plan are still open and owned by nobody? Produce a short, cited list a maintainer can pick the next engine job from.

# Context
- Project: Mudcrab, a Rust/Bevy 0.19 engine that runs converted Skyrim SE data. Upstream repo Mudcrab-Team/mudcrab; this checkout is a fork whose `origin/main` mirrors upstream main.
- The plan: `docs/roadmap/02-completion-plan.md` (Portuguese), milestones "Marco 0" to "Marco 8" plus "Ordem de execução" and "Critério único de conclusão". Read it from `origin/main` (`git show origin/main:docs/roadmap/02-completion-plan.md`), not the working tree, which is a different branch.
- Merged work: `git log origin/main --oneline` (squash merges, titles like `fix(engine): ... (#72)`). Also `docs/` on origin/main (specs, status files) may mark items done.
- Open upstream work (already known; treat these as owned):
  - Chef (TaylorTurnerIT): LOD #103/#105/#106, sampler defaults #102/#101, physics objects #104, Git-backed saves #90.
  - eclepso: plugin records #89 (issue #80), terrain texture scale/filtering #91 (#87), terrain caches/normals #92 (#85); open converter issues #81 #82 #83 #84 #86 are his reports.
  - BimingtonBill (us): #93 prune records, #94 engine --help/strict options, #95 launcher schema range, #97 parallel validation, #98 --shots reference renders, #99 prepass vertex alpha (draft), #107 contributor docs; developer console #88 (we are about to take it); held locally: terrain vertex-colour sRGB, `--remote` BRP flag, converter publish robustness, staging leftovers, check-outside-data, check-schema-range.
  - HazielMagallanes: #67 acceptance script BOM.
- If `gh` works, `gh issue list -R Mudcrab-Team/mudcrab --state open` and `gh pr list -R Mudcrab-Team/mudcrab --state open` confirm the list; don't rely on it.

# Scope
- Read only: `git show`/`git log` on origin/main, read files, `gh` read commands if available. No edits, no builds.
- Engine-side items first (Phase 2 is the engine track); note converter-only items separately and briefly.

# Done when
- Every milestone (Marco 0-8) has a status line: done (cite the merged commit/PR or doc), in progress (cite the open PR/issue and owner), or open with no owner.
- The open-with-no-owner items are listed with: what it is in one plain English sentence, the plan section, rough size (small/medium/large), and what it depends on.

# Report
- English. A table per milestone, then "Open, no owner" ranked by value to the Phase 2 completion criterion, then "Unclear" (items whose state you could not establish, with what you checked).
- Cite file:line in the plan and commit hashes/PR numbers for every status. Keep it under 900 words.

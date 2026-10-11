# Goal
Track: Phase 2. List the engine work upstream's Phase 2 plan still needs that nobody on our side has
built or claimed, ranked so the next coder briefs can be written from it. Read-only.

# Context
- Upstream's plan: `docs/roadmap/02-completion-plan.md`, `docs/roadmap/02-core-engine.md`,
  `docs/roadmap/02-integration-and-acceptance.md`, README.md, CONTRIBUTING.md, `docs/specs/` (read
  them at upstream main: `git -C C:/Modding/wkj-phase2 show origin/main:<path>`).
- What we already built, sent or hold (claimed): `C:/Modding/Mudcrab/mudcrab/docs/pr/upstream-queue.md`
  (every branch and PR), `C:/Modding/Mudcrab/mudcrab/docs/roadmap/phase2-track-tasks.md`, and the
  look-gaps list `C:/Modding/Mudcrab/mudcrab/docs/research/look-gaps-2026-09-24.md`.
- Open upstream PRs (ours and others'): #27 (trees/flora, schema 15, TaylorTurnerIT), #29-#31, #38,
  #40-#41, #42 (terrain sampler, TaylorTurnerIT), #43-#49, #50 (Bevy diagnostics, TaylorTurnerIT), #51.
  Treat anything they cover as claimed.
- Out of scope: converter work (Phase 1 Dev owns `crates/converter`), portal/door features (the
  portal track), Phase 3+ (gameplay, physics, NPCs).

# Scope
Read only. Write nothing except your report.

# Done when
- Every open item of the Phase 2 plan is listed with its source (file:line), and marked claimed
  (with the branch/PR) or unclaimed.
- Unclaimed items are ranked by value to upstream's Phase 2 acceptance, each with: size (S/M/L),
  the engine files it touches, whether it needs game data to test, whether it needs a GPU run or a
  timing booking, and any dependency on a pending PR.
- The top five unclaimed items each get a one-paragraph brief outline (goal, files, test).

# Report
Sections: Plan items (table: item | source | claimed by | notes), Unclaimed ranked (table),
Top five outlines, Doubts (items whose status you could not decide).

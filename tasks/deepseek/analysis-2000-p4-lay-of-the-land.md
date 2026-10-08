# Goal
Phase 4 lay of the land: map what upstream Mudcrab already has for Phase 4 (physics, animation,
gameplay state, saves), who is working on it, whether upstream is ready for it, how it overlaps our
portal track and the 160 fps goal, and propose a ranked first slice. Read-only: change no files.

# Context
- Mudcrab is a Rust/Bevy reimplementation of Skyrim SE. This checkout (`C:\Modding\Mudcrab\mudcrab`)
  is on branch `portal` (our fork's demo track). Branch `main` is a local mirror of upstream
  `origin/main` (GitHub `Mudcrab-Team/mudcrab`). Read upstream state from `main`, e.g.
  `git show main:docs/roadmap/04-gameplay-and-physics.md`, `git log main --oneline -- <path>`,
  `git grep -n <term> main -- crates/`.
- Phase 4 roadmap: `docs/roadmap/04-gameplay-and-physics.md` (4.1 Rapier physics and collision,
  4.2 skeletal animation, 4.3 combat/magic/quest state machine, 4.4 sub-second save/quick-load).
  Overall order: `docs/roadmap/README.md`; Phase 2 completion plan: `docs/roadmap/02-completion-plan.md`.
- Track rules: `docs/pr/two-tracks.md` (says the portal work is later offered upstream as a Phase 4
  proposal; door clips, the walking player and auto-load doors are Phase 4-side).
- Known facts to build on (verify, don't redo):
  - Upstream main already has Rapier collision and a character controller:
    `local/research/port-prep-collision.md`, `local/agents/runs/research-922-p3-port-collision-check.1/report.md`,
    `local/research/onboarding/main-snapshot/SPEC.md` (movement spec).
  - Body contract for dynamic bodies: `local/research/p104-physics-body-contract.md`.
  - Our own open draft PRs #125 (dynamic clutter bodies) and #126 (sphere/cylinder shapes) are
    Phase 2 Dev's and are 4.1 work. A Phase 4 slice must build on them, never duplicate them.
  - Cell-load cost incl. JSON collision + parry hulls on first placement:
    `local/research/engine-foundations/analysis-882-883-cell-load-cost.md`.
  - Animation/modding background: `local/research/mods/animation-physics.md`;
    door animation decoding: `local/agents/runs/research-027-door-animation-seamless.1/report.md`.
  - Research index for anything else: `local/research/INDEX.md` (search it with Grep first).
- The engine goal (`local/plans/foundations-to-portal.md`): the portal demo holds a stable 160 fps
  while travelling and through load doors, no shortcuts.
- Foundations check (AGENTS.md, "Foundations check"): no shortcuts; right layer (converter
  precomputes, runtime stays light), measured; data-driven (no FormIDs/paths baked in); moddable
  (stable IDs, overrides, load order); aligned with upstream roadmap, licence-clean (clean room: no
  copying of Bethesda/Havok/xEdit code).
- GitHub reads only: `gh issue list`, `gh issue view`, `gh pr list`, `gh pr view` against
  `--repo Mudcrab-Team/mudcrab`. Never write to GitHub.

# Scope
Read-only. Change no files, run no builds, start no engine. Allowed commands: `git log`, `git show`,
`git grep`, `git branch`, `gh issue list`, `gh issue view`, `gh pr list`, `gh pr view`.

# Done when
Every question below is answered with citations (path:line, `main:path:line`, or issue/PR URL),
and claims you could not verify are marked UNVERIFIED.

1. **What main has, per item 4.1-4.4.** For each: present / partial / absent, the crates, modules
   and key types on `main`, and what is missing against the roadmap text. Include PRs #125 and #126
   and where they stand.
2. **Who upstream works on what in Phase 4.** Open issues and PRs (any author) touching physics,
   collision, animation, skinning, combat, AI, quests, saves; roadmap owners or assignees if named.
   Mark which areas belong to another contributor (we would need an announcement first).
3. **Is upstream ready for Phase 4 work now?** Phase 2 is not finished upstream. Quote what the
   roadmap, README, CONTRIBUTING.md or maintainer comments say about phase order, and give a
   one-paragraph judgement.
4. **Overlap.** With the portal track (walking player `PlayerPlugin`, doors and door animation,
   colliders across portal spaces, rebase of Rapier bodies) and with the 160 fps goal (per-frame
   physics cost, collider build on cell load). List what must not regress.
5. **Ranked first slice, 1-3 candidates.** For each: scope in 3-5 lines, likely owned files, which
   other session must agree (Phase 1 Dev owns `crates/converter/**`; Phase 2 Dev owns engine Phase 2
   work and PRs #125/#126; Portal Dev owns portal demo code; `crates/shared/**` changes need agreement),
   an acceptance test, and short answers to the five foundations points. Prefer slices that are
   small, sit on what main already has, and do not touch another contributor's open work.

# Report
Markdown, under about 250 lines, with the sections `## 1. What main has` ... `## 5. First slice`,
then `## Summary` of exactly 5 lines. Citations inline.

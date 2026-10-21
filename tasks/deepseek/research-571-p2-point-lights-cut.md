# Goal
Track: Phase 2 (for upstream). Plan how to re-cut this project's point lights, which exist only on the
`portal` branch, as upstream-ready `phase2/*` pull requests cut from `main`. Write the plan; change no
code.

# Context
- Two git branches in this repository: `main` mirrors upstream (`origin/main`); `portal` is the
  user's fork work built on top of it. Read `portal`'s files with `git show portal:<path>` and
  `main`'s with `git show main:<path>`; compare with `git diff main portal -- <paths>` and
  `git log --oneline main..portal -- <path>`.
- Upstream `main` has no point lights at all (no `PointLight` in `crates/engine/src`).
- On `portal` the lights span:
  - the converter: a `lights` table and `references.radius_override` in the world database
    (DeepSeek task impl-014, commit 5cd1bf4, world DB schema v5), LIGH records;
  - the engine: `crates/engine/src/lights.rs` and parts of `crates/engine/src/streaming.rs`
    (impl-015, 2acd409: a nearest-64 light budget), the lights' `Visibility` fix and
    `EXPOSURE_CALIBRATION` (see AGENTS.md handoff 2026-09-22), an audit fix to the light budget
    (1e2494f), `intensity_for_radius` capping (23adbb4), and a Skyrim-falloff patch
    (`light_falloff.rs`, f4c40d7 area; `lights::SKYRIM_FALLOFF` is false, i.e. off).
- Portal-specific parts must stay out of upstream: render layers, the portal camera, the "active
  space"/destination-space logic, per-space lighting (`space_lighting`, atmosphere), door code.
- Upstream conventions: `CONTRIBUTING.md`, `docs/roadmap/02-completion-plan.md`,
  `docs/pr/two-tracks.md` (read these on `main`). A converter output or database change needs a
  schema bump (`CONVERTER_SCHEMA_VERSION` in `crates/converter/src/cache.rs`,
  `WORLD_DATABASE_SCHEMA_VERSION` in `crates/shared/src/lib.rs`) and the roadmap's Compatibility
  line updated.

# Scope
Read-only. Write nothing except your report.

# Done when
Every file and function the point lights touch on `portal` is accounted for: either in a proposed
PR (with what it needs from `main`) or named as portal-only with the reason. Each claim cites
`path:line` on the branch it was read from.

# Report
1. Inventory: converter side vs engine side vs shared contracts, each item with `branch:path:line`
   and the commit that introduced it.
2. Portal-only parts to leave out, and how the upstream cut would stand without them (what replaces
   any dependency on them, e.g. lights' render layers or the active space).
3. Schema: which schema versions change for upstream, from what to what.
4. A proposed PR series, smallest first, each with: files, what it does in one sentence, what it
   depends on, how it can be tested without Skyrim (upstream's `dummy-content` fixtures), and its
   risk. Say which single PR is the smallest useful first step.
5. Open questions for the lead.

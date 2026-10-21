# Goal
Track: Phase 1. Draft a contributor guide, "Your first 30 minutes with OpenSkyrim", for upstream's
docs: clone, build, run without owning Skyrim (the dummy-content fixture), convert a real install
when you have one, run the engine, run the checks CI runs. Read-only research producing a draft.

# Context
- Read `tasks/deepseek/context.md` first.
- Use upstream main as the source of truth, read-only at `C:/Modding/wkj-phase1` (its README.md,
  CONTRIBUTING.md, docs/, crates/*/Cargo.toml, crates/converter/src/main.rs usage text, the engine
  CLI in crates/engine/src/config.rs, .github/workflows/ for what CI runs). Note which steps need
  Windows vs Linux, and what the first build costs (from docs or reasonable estimates marked as such).
- Upcoming (not yet on main; mention as "coming" only if the step depends on it): a `quick` Cargo
  profile, a live converter status line, a `--texture-mode native` dev mode that cuts the first
  conversion from ~5 h to well under an hour.

# Deliverable
`local/research/phase1-first-30-minutes-draft.md`: the guide itself in plain English (under 1000
words, numbered steps with the exact commands, what you should see after each, the three most common
problems and their fixes), then a short list of gaps the guide exposed (missing flags, confusing
errors, missing docs) with `path:line`. Read-only elsewhere; no cargo builds, no engine runs; do not
commit. Final message: a 200-word summary.

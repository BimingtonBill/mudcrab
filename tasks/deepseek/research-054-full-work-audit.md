# research-054 - Full audit of everything since the fork, and a PR-ready plan

Read `tasks/deepseek/context.md`, `AGENTS.md` (the handoff log) and the earlier audit
`docs/pr/upstream-pr-plan.md`, which covered `8919096..3ae5541` and is now out of date: roughly
50 commits have landed since, upstream `origin/main` has been merged in (`8c21447`), and the
cleanup that audit asked for is done.

## Goal

Audit the **whole** of this fork's work as it stands, and update the PR plan so the lead can cut
branches from it without further research.

## What to do

1. **Inventory `8919096..HEAD`** (and note what came from upstream via the merge, which is not
   ours to send). Group by area: converter, engine, shared, tests, docs, private tooling. Give
   commit counts and diffstats per group.
2. **Audit as an upstream maintainer would.** Run and cite:
   - `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
     `cargo test --workspace` (report counts; note that some tests are opt-in behind
     `OPENSKYRIM_SKYRIM_DATA` / `OPENSKYRIM_CONVERTED_DIR`);
   - `git ls-files` for anything that looks like game data or a large file;
   - greps for private names, machine paths, user names and email addresses in **tracked files**
     (the code was cleaned in `410ce31`, `62c0f7b`, `944250e`, `432c152`: verify, and check docs
     and scripts too, which were not part of that sweep);
   - the upstream CI workflow: does everything it runs pass here?
3. **Assess each candidate PR for readiness**, with evidence:
   - does it stand alone, or does it depend on an earlier one?
   - is it covered by tests that do not need Skyrim?
   - has it been verified at runtime, and by what (name the run)?
   - what would a reviewer object to?
   Flag anything that should **not** go upstream (demo-specific code, the `--demo` presets, the
   reference-shot workflow) or that needs work first.
4. **Update `docs/pr/upstream-pr-plan.md`** in place: the series, in order, each with title,
   scope (paths and commits), dependencies, test story, risks and the exact `git` commands the
   lead would run to build that branch from this history. Keep the exclusion list current.
5. Add a short "state of the fork" section: what works, what is inert until data exists, what is
   known broken (see the handoff log's open items).

## Rules

- Read-only apart from `docs/pr/upstream-pr-plan.md` and a handoff note in `docs/handoff/`.
- Do not create branches, rewrite history, or contact GitHub.
- Commit with `[deepseek]` subjects.

## Report

The series table, the top readiness blockers, and anything you found that the earlier audit
missed.

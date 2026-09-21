# DeepSeek delegation policy

Carried over on 2026-09-21 from the user's DOA Xbox360 UI project, where it was authorized on
2026-09-20 after a project review, and adapted to OpenSkyrim. The user asked for the same DeepSeek
worker setup here.

## Objective

Reduce Claude's total investigation, review and repair effort while keeping the engine's quality.
DeepSeek cost is negligible to the user. Keep useful reasoning and independent review; do not lower
effort just to save DeepSeek tokens.

## Scope and acceptance

- Delegate research (format layouts, Bevy API questions, prior art in OpenMW and xEdit), evidence
  extraction, analysis of completed profiling and acceptance runs, independent audits, test design,
  and assigned documentation.
- Implementation starts as a pilot of 6-10 bounded tasks: pure parsers, converters for one record
  or block type, validators, audit tools and their tests, with fixed interfaces. The lead picks
  tasks that do not overlap current work. This policy does not launch any task itself.
- The lead (Claude) keeps cross-crate architecture, the public contracts in `crates/shared/`,
  render-graph and ECS scheduling changes, dependency changes (`Cargo.toml`), integration, and
  all acceptance-campaign and sign-off runs (`scripts/phase2-*.ps1`).
- Judge a pilot by acceptance without correction, correction effort, independent tests and, where
  needed, runtime results. Worker status `ok`, citations that resolve and tests the worker wrote
  itself do not by themselves show the work is correct.
- An independent reviewer checks the important claims and patches against the source evidence and
  the acceptance contract. The lead settles disagreements and accepts the result. Widen the coding
  scope based on how the pilot goes.

## Isolation and permissions

- Each brief names a base commit, the files the task owns, a fixed interface, its dependencies,
  acceptance checks and a stopping condition. Two tasks never edit the same file.
- The shared launcher's protections stay in place. Implementation runs through
  `tools/ds_impl.ps1`, which gives each task its own worktree and unlocks only the files the task
  owns. The user has authorized this scoped setup, so do not ask again for every task.
- Never turn off the shared protections globally, let a worker edit its own policy, or get around a
  tool denial with a Python or shell workaround. A worker reports a missing permission to the lead
  once, and the lead fixes the setup if it is within the task's scope.
- All builds share `target/`. Cargo locks the directory itself, so parallel builds queue instead
  of breaking. Serialize anything that needs the GPU or a long release build: profiling runs and
  acceptance campaigns belong to the lead.

## Handoffs and parallelism

- The summary for the lead is 300-600 words: base commit, the answer or patch, the key evidence,
  unresolved assumptions, the actual check results, the next test that could prove it wrong, and
  the changed files. Detailed investigation stays in the task's own files.
- Claude checks the evidence that matters without routinely redoing all the research.
- Suggested starting capacity: four independent producers, one reviewer and one run analyst, when
  that much useful independent work exists. It is a starting point, not a quota. Keep the review
  backlog bounded and stop finished workers. Integration stays serial.
- Prioritize compact results, reusable approved command examples and tracking of review effort.
  Any change to the launcher or this harness must pass `tasks/deepseek/selftest.md`.

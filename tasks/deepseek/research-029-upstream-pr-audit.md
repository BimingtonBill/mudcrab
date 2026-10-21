# research-029 - Audit everything since the fork and design an upstream pull request

Read `tasks/deepseek/context.md` and `AGENTS.md` first.

## Goal

This checkout is a private local fork of `realfakenerd/Mudcrab`, cloned at `8919096`. It is on
branch `deepseek-setup` and has never been pushed. The user wants an **audit of all the work
since the fork** and a **design** for a reasonable pull request (or a short series of them) to the
upstream project. Nothing is published tonight: the deliverable is the plan and the evidence. The
lead decides later what to prepare.

## What to do

1. **Inventory.** Go through `git log --oneline 8919096..HEAD` and `git diff --stat 8919096..HEAD`,
   and group every change by what it is:
   - engine features (streaming, doors, portals, lights, player, terrain ring, shots mode);
   - converter changes;
   - shared contracts;
   - bug fixes (rotation convention, heading sign, 4π light factor, terrain sampler...);
   - tests;
   - docs (design, research);
   - the **private tooling**: the DeepSeek harness, `tasks/deepseek/`, `tools/ds_*`,
     `.deepseek-agents.json`, `AGENTS.md`, `CLAUDE.md`, `docs/handoff/`, the demo launchers,
     `local/`.
2. **Audit quality**, the way an upstream maintainer would:
   - Does it build? Run `cargo check --workspace` and `cargo test -p shared`, and cite the output.
   - Are there debug leftovers, TODOs, dead code, `#[allow]`s or unexplained magic numbers? Grep
     for them.
   - Are commit messages and docs consistent?
   - What does the upstream CI require (`.github/workflows/`)?
   - Are there license or asset concerns? No Bethesda data may be committed: check
     `git ls-files` for anything that looks like game data (`.nif`, `.dds`, `.esm`, `.bsa`,
     converted `.glb`/`.ktx2`) and for large files.
   - Read the upstream's own README, CONTRIBUTING and roadmap in the repo (`docs/roadmap/`, the
     root README) to judge what upstream wants and which phase it is in.
3. **PR design.** Propose one PR or a short ordered series, each small enough to review:
   - title and description;
   - the commit ranges or file sets it contains;
   - what it depends on;
   - the reviewer's likely questions;
   - which private-tooling files must be excluded;
   - whether commits should be squashed or re-split.
   Put the fixes that stand alone first (for example the rotation convention with its data
   proof), then features. Flag anything that should not go upstream at all (the demo-specific
   hard-coding, the `--demo` presets and so on) or that needs cleanup first, with `file:line`.
4. **Risks:** anything that would embarrass the user publicly (private notes, paths like
   `C:/Users/Brannock`, the email address, internal tooling names) in files that would go
   upstream. Grep for them.

## Deliverable

- `docs/pr/upstream-pr-plan.md` (a new folder): the inventory table, the audit findings with
  evidence, the PR series, the exclusion list, the cleanup list and the risks.
- Commit only that file and a handoff note in `docs/handoff/`, with `[deepseek]` subjects. Do not
  push, do not create branches, and do not contact GitHub.

## Report

A summary of the plan, the top 5 cleanup items, and the exclusion list.

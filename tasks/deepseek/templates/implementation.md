# How an implementation task works here

Read this once, then your own brief. `tasks/deepseek/context.md` still applies for the project,
the reference folders and the citation rules.

## Where you are

You are in an **isolated checkout** of the repository made from a recorded base commit, not in
the lead's working copy. Nothing you do here reaches the lead's branch until the lead reads your
diff and integrates it. Your builds share the lead's `target/` (set for you in `CARGO_TARGET_DIR`),
so the first `cargo` command does not recompile Bevy from scratch. That is why you can edit the files your brief assigns.

- **Write only the files your brief lists under "Owned files".** Everything else in the checkout
  is reference. A file you touch outside that list is reported to the lead as out of scope and
  will be reverted, which wastes the task.
- **Do not run git.** No commits, no branches, no stashes. The lead integrates.
- Do not add or upgrade dependencies (`Cargo.toml`, `Cargo.lock`), do not run the profiling or
  acceptance scripts, and do not edit the policy files.
- A refused edit is policy, not a bug. Report it once in your result; do not work around it with
  a script or a shell redirect.

## What "done" means

Your brief names acceptance commands. They are run for you after you stop, in this checkout, and
their exit codes decide whether the task passed. Run them yourself first and fix what they report.

Your own tests passing does not establish correctness - a reviewer with the same evidence checks
your work afterwards. So write the test that would **catch you being wrong**, not the one that
confirms what you wrote. Where the brief fixes an interface, keep it exactly: the lead's
integration and the reviewer both depend on it.

## What you return

Answer with JSON only, matching `tasks/deepseek/result-schema.json`:

- `base_commit` - the commit in `.ds-impl.json` at the root of this checkout.
- `summary` - **300 to 600 words** for the lead: what you built, the decisions that were not
  obvious, what you verified and how, and what you deliberately left out. This is what the lead
  reads instead of your transcript, so it has to stand alone.
- `critical_evidence` - the handful of claims your work rests on, each with a repo-relative
  `path:line`, an address, or a command and its output. Not a bibliography: the things that would
  change the lead's decision if they were wrong.
- `unresolved_assumptions` - what you had to assume, in plain words. An honest list here is worth
  more than a confident summary.
- `commands` - the checks you actually ran and what they actually printed.
- `next_falsifying_test` - the cheapest experiment that would show your work is wrong.
- `changed_files` - every file you wrote, repo-relative.

Long working notes stay in the checkout (a file your brief allows); the summary carries the meaning.

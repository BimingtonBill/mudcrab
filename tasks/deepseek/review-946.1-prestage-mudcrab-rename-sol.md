# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: chore: rename OpenSkyrim to Mudcrab with a re-runnable script
- What it claims (the PR text a reviewer will read): `local/review/pr-chore-mudcrab-rename/pr-text.md` (sections marked TBD are filled after the proof runs; ignore them).
- Its diff against upstream main 1b70b9b0: `local/review/pr-chore-mudcrab-rename/full.diff` (commits: `local/review/pr-chore-mudcrab-rename/commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `base/` holds upstream main's version of every file the PR touches, `head/` the PR's version. The checked-out source under `crates/` is a DIFFERENT branch, never cite it as main's behaviour.
- Background: upstream PR #66 renamed the project (OpenSkyrim / Wah Krah Jol to Mudcrab) by hand, merged with CI red, #68 listed five fixes (rustfmt; refresh the dummy-content fixture hash and converter snapshots; two owner-local paths in docs; converter writes `mudcrabCollision` while engine and audit script read both names; drop an unused `.gitignore` line), and #74 reverted it. The project name stays Mudcrab. This PR re-lands it as `scripts/rename-to-mudcrab.py` plus its output (commit "chore: rename OpenSkyrim to Mudcrab" is exactly the script output + `cargo fmt`), then a snapshot/hash refresh commit and a hand-fix commit. Plan: just before merging, reset onto fresh main, re-run the script, re-apply the snapshot/hash and hand-fix commits.
- CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings` and nextest. On the head these pass (754/754).

# Scope
Read only. Change nothing.

# Done when
You have checked, with diff line evidence:
1. **The script:** rule order, the PROTECTED masking and placeholder restore, idempotency (a second run changes nothing, including after rustfmt reflows the env-var fallback chains), regexes that over- or under-match (for example a rule that would also rename something that must stay, or miss a shape like `env::var` inside a chain), the brittle texture.rs helper rule (what happens on a main where that helper changed?), binary/encoding/line-ending handling, `--check` and `--self-test`.
2. **What used to work and now fails:** environment variables (are every old `OPENSKYRIM_*` name still read?), converted assets with the old `openSkyrimCollision` key, the `OPEN_SKYRIM_material` extension and `openSkyrim` extras (must be unchanged), window titles, temp-dir names, log prefixes, anything a user or tool depends on by name.
3. **Generated output:** spot-check the script-output commit for renames that are wrong in context (a URL that does not exist, a path on someone's machine, a legal/licence line, a quoted historical name).
4. **Claims vs diff:** every statement in the PR text.
5. **Tests:** the new old-key test, the self-test, and what is untested.
6. **Scope and size:** anything unrelated; leftover debug code, local paths, TODOs.

# Report
- Verdict line: ready to open / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 8, only ones you verified in the diff. Mark each **blocker**, **should fix** or **nit**.
- PR text problems, as a separate short list.
- Under 450 words.

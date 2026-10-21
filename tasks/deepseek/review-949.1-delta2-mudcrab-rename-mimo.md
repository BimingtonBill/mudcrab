# Goal
Second delta review of a pull request (round 2 of at most 3). Decide whether these last changes are correct.

# Context
- PR: chore: rename OpenSkyrim to Mudcrab with a re-runnable script (upstream Mudcrab-Team/mudcrab, not opened yet). First round and delta 1 are done (accepted).
- This delta: `local/review/pr-chore-mudcrab-rename/fix-delta-2.diff`. Full new files: `local/review/pr-chore-mudcrab-rename/head/scripts/rename-to-mudcrab.py`, `local/review/pr-chore-mudcrab-rename/head/crates/engine/src/streaming.rs`. No git; read files only; never cite the checkout's `crates/` (a different branch).
- What changed and why:
  1. The Rust collision reader rule accepts any receiver `(\w+).get("openSkyrimCollision")` (was hard-coded `value.`), and the texture-helper loud failure checks for the helper's definition `fn convert_installed_2d_fixture(` (a file that only calls it no longer aborts). From MiMo delta-1 findings.
  2. The engine test `collision_authored_under_the_pre_rename_key_still_loads` builds its `CollisionAsset` from JSON plus a pushed shape, because open PR #118 adds a `bodies: Vec<CollisionBody>` field with `#[serde(default)]`; a struct literal broke when merged with #118.
  3. The script's modes are now positional words (`apply` default, `check`, `self-test`) instead of `--check`/`--self-test`, and the repo root comes from `Path(__file__)` instead of `git rev-parse --show-toplevel`: open PR #94 adds an engine test that scans every `.py/.sh/.ps1` under `scripts/` for `--option` tokens and asserts the engine parser accepts each (with an allow-list for other tools' flags). Our tokens failed it.
- Measured: self-test 32/32; `check` clean on the head; main + 9 open PRs + this PR: clippy clean, nextest 817/817.

# Scope
Read only. Change nothing.

# Done when
Each of the 3 changes: correct / problem, with line evidence. Look for: a remaining `--word` token in the script (other than `--all`, which #94 allows), regex group/backreference mistakes, the generalized receiver rule matching something it must not (e.g. inside an already-correct chain, a JSON key in a test), `Path(__file__)` breaking when the script is run via a symlink or from another directory, and the JSON test still exercising the old-key path.

# Report
- Verdict: accept / small fixes / fix first.
- Problems as `file:line - issue - fix`, at most 5, blocker/should fix/nit.
- Under 250 words.

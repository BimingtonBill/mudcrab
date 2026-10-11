# Goal
Delta review of the fixes to a pull request's script, after a first review round. Decide whether the findings are fixed and whether the fix broke anything.

# Context
- PR: chore: rename OpenSkyrim to Mudcrab with a re-runnable script (upstream Mudcrab-Team/mudcrab, not opened yet).
- The fix delta: `local/review/pr-chore-mudcrab-rename/fix-delta.diff` (only `scripts/rename-to-mudcrab.py` changed). The full new script: `local/review/pr-chore-mudcrab-rename/head/scripts/rename-to-mudcrab.py`. The rest of the PR, unchanged: `local/review/pr-chore-mudcrab-rename/full.diff`. No git; read the files only. Do not read code from the checkout's `crates/` (a different branch).
- First-round findings that the delta must fix:
  1. (Sol, blocker) fallback protection required exact single-line syntax; a rustfmt-expanded chain or block closure would get a second chain added, or its old name renamed. Must be whitespace/format tolerant, with rewrite -> reformat -> rewrite tests.
  2. (Sol + DeepSeek) the texture helper rule `convert_installed_2d_fixture` matched its body verbatim and failed silently. Must fail loudly.
  3. (Sol) any `or_else(|| env::var_os("OPENSKYRIM_X"))` was treated as an installed fallback even without a preceding MUDCRAB_X read.
  4. (DeepSeek) skipped NUL/non-UTF-8 files and unmatched shapes were invisible to `--check`.
- Also new: bare Python `.get("openSkyrimCollision")` is now left unchanged in .py files (PROTECTED_PY) so the separate hand-fix commit (which turns `scripts/audit-collision.py`'s reader into a read of both keys) cherry-picks cleanly onto a fresh re-run. Check that this cannot leave a Python reader that reads only the old key after the hand fix on the current head (head's audit-collision.py is in `head/scripts/`).
- Measured by the author: self-test 31/31; `--check` exits 0 on the head; a re-run on upstream main 1b70b9b0 reproduces the PR's generated commit exactly for non-Rust files (and with rustfmt, for Rust files except that one Python line, now fixed).

# Scope
Read only. Change nothing.

# Done when
For each of the 4 findings: fixed / partly / not, with line evidence. Then any NEW problem the delta introduces (regex errors, catastrophic backtracking on large files, mask/restore nesting bugs, a self-test that asserts the wrong thing, idempotency).

# Report
- Verdict: accept / small fixes / fix first.
- Per finding one line; new problems as `file:line - issue - fix`, at most 5, each blocker/should fix/nit.
- Under 300 words.

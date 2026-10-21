# Goal
Re-review a small fix commit on open upstream PR #107 (a documentation proposal for Mudcrab-Team/mudcrab: contributor workflow, PR guide, labels, AI policy). Check that the fix is correct and introduces no new contradiction.

# Context
- The fix diff: `local/review/pr-107/delta.diff` (commit 38e1fef3 on top of 661ac3dc).
- The whole PR before the fix: `local/review/pr-107/full.diff`, with the PR's files under `local/review/pr-107/head/` and main's under `local/review/pr-107/base/`. The checked-out source elsewhere is a different branch; don't use it.
- What the fix was meant to address (from two earlier reviews): contributors without triage access can't set labels; the draft definition was narrower than practice (draft = work in progress); the ruleset record omitted 0 required approvals and no merge queue; the nextest line contradicted CONTRIBUTING.md; labels.yml would have rewritten the four existing labels' descriptions (repository today: bug "Something isn't working" d73a4a, documentation "Improvements or additions to documentation" 0075ca, enhancement "New feature or request" a2eeef, proposal "An additive tag indicating a potential direction seeking more discussion" 51dc1b); the workflow banner didn't cite #100.
- Deliberately NOT added: a "nobody merges their own PR" rule (it is not a team rule).

# Scope
Read only.

# Done when
Each intended fix is confirmed present and correct, and any new inconsistency between the changed lines and the rest of the PR's files is named.

# Report
Verdict: accept / small fixes / fix first. Then findings as `file:line - issue`, at most 5. Under 200 words.

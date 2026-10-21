# Goal
For every local `phase2/*` branch, say whether its change is already on upstream `origin/main` (7740c8e..aef677b, which merged many of them via squash PR #65 and later PRs), and, for those that are not, how much work it would take to bring back. The result helps the user choose which held work to show upstream.

# Context
- Repo: this folder (C:/Modding/Mudcrab/mudcrab). All branches are local refs; `origin/main` is upstream main. Upstream squash-merges, so a branch's commits are NOT ancestors of main even when its change landed. Decide "landed" by content, not ancestry: compare `git diff $(git merge-base origin/main B) B` with main, e.g. distinctive added identifiers, functions, tests, CLI options or doc lines found on `origin/main` with `git grep <text> origin/main`, and `git log origin/main --oneline --grep` for a matching PR title.
- Skip these (carriers or merges, not work): phase2/integration-night, phase2/*-portal, phase2/pr30-31-combined, phase2/pr30-on-main, phase2/pr34-38-combined, phase2/schema-range (merged as #72), phase2/engine-help and phase2/engine-help-v2 (being handled), phase2/schema-range-shared.
- Get the list with `git for-each-ref --format='%(refname:short)' refs/heads/phase2/` and keep branches with `git rev-list --count origin/main..B` > 0 (about 80).
- Several branches stack on each other or were superseded by a later one of the same family (sky/sky-main/sky-dome/sky-fog, water/water-main/water-look/water-colours/water-reflection-gating, sun-shadows/sun-shadows-main/sun-shadow-cascades, lod-*, lights-*, terrain-*, vertex-alpha-*, reflection-*). Group families and name the newest head of each.
- Upstream PR list for titles: `git log origin/main --oneline -80`.

# Scope
- Read only. Run only `git log`, `git diff`, `git show`, `git grep`, `git merge-base`, `git rev-list`, `git for-each-ref`, `git branch`, and file reading/searching tools. Change nothing. Do not build.
- You may split the branch list among sub-workers by family.

# Done when
- Every branch in scope has exactly one row.
- Each "landed" verdict names the evidence on origin/main (a PR number, or a file:symbol that exists there).

# Report
A markdown table, one row per branch, grouped by family, with columns:
branch | head (short sha) | status: landed / partly landed / still needed / superseded by <branch> | overlap (with which main PR or other branch) | rebuild effort S/M/L (S = merges cleanly or tiny conflict; M = real conflicts in 1-3 files; L = large rework) | needs a real-game check (yes/no) | one plain sentence on what it does | evidence.
Then a short list of the "still needed" branches ordered by value, with two picks you would open as draft PRs and why. No word limit for the table; under 250 words for the rest.

# Goal
Verify, against upstream `main`, the status of every Phase 2 completion-plan item, so a public tracking issue can list what is done, in progress and open without a wrong claim. An earlier pass (research-927) used a local audit that is not on upstream main; correct it where main differs.

# Context
- This folder is a clean checkout of upstream `main` (Mudcrab-Team/mudcrab, commit a896b903). Everything you read here IS main. Ignore `review-diffs/` except as input listed below.
- The plan: `docs/roadmap/02-completion-plan.md` (Portuguese). Milestones "Marco 0" to "Marco 8", each with numbered "Implementação" items (cite as M<marco>.<n>, e.g. M4.1) and an "Saída" (exit criteria).
- Inputs in `review-diffs/`: `research-927-report.md` (the earlier pass: its statuses and claims to verify), `phase2-status-local-audit.md` (the local audit it relied on; NOT on main, use only as hints of where to look), `merged-prs.txt`, `open-prs.txt`, `open-issues.txt` (upstream lists fetched today, with authors/assignees).
- Owners already known: Chef (TaylorTurnerIT) LOD #103/#105/#106, sampler #101/#102, physics #104, saves #90; eclepso #89 #91 #92 and issues #80-#87; BimingtonBill #93 #94 #95 #97 #98 #99 #107, dev console #88 (assigned).

# Scope
- Read only. Read files in this checkout (code, tests, scripts, docs, `.github/workflows`). No builds, no edits.
- For each claim of "done", find the code/test/doc on main that proves it (file:line). For "open", show you looked (what you searched, where) and found nothing that implements it. For "in progress", cite the open PR/issue.

# Done when
- Every Implementação item of Marco 0-8 has: status (done / partly done / in progress / open / needs a run, not code), evidence on main (file:line or "searched X, Y: absent"), owner if any, and for open items one plain-English sentence of what it would take plus size (small/medium/large).
- Every place where research-927 was wrong is listed.

# Report
- English. One table per Marco: item, status, evidence, owner. Then "Corrections to research-927". Then "Open, no owner" in one list (engine items first, then converter, then docs/infra), each with its plain sentence and size.
- Under 1100 words. Cite file:line on main for every status.

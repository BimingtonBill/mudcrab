# Goal
Review today's (2026-10-07) workflow changes made by Upstream Dev for Mudcrab: are they correct, consistent
with each other and with the rest of AGENTS.md, workable for the dev sessions and their workers, and free
of loopholes or contradictions? Track: Upstream Dev (process docs), no code.

# Context
The user asked for four things today; Upstream Dev turned them into rules:
1. Standing yes for comments on our own GitHub items. Commit dfc703c1 (AGENTS.md, publishing rule).
2. "I want to see engine foundations work running in portal demo before releasing to the public."
   Commit 247101ce (docs/pr/upstream-queue.md gate note) and the plan local/plans/foundations-to-portal.md
   (sections "Portal first, then public" and "Demo gate status").
3. Goal: "get the engine foundations fast enough to run portal demo at a stable 160 fps while travelling
   through the world and travelling through load transitions, taking no shortcuts". Plan section
   "The goal (the user, 2026-10-07)" in local/plans/foundations-to-portal.md, with a proposed pass bar.
4. "Integrate checking xEdit into the workflow sensibly, doesn't necessarily have to be exclusive to
   Phase 1 Dev." xEdit 4.1.5f installed at C:\Tools\SSEEdit. Commit 814790e9 (AGENTS.md bullet
   "xEdit is the record reference"). Background: local/research/upstream/discord-2026-10-07-xedit-proposal.md.
Memory notes written for these (in the -AddDir memory folder): feedback-standing-yes-own-comments.md,
feedback-portal-demo-before-public.md, project-160fps-goal.md, reference-xedit.md.
Today's team log local/team/2026-10-07.md shows how the rules were handed out and already used
(e.g. the first xEdit check found an XPOD decoding gap in #188).

# Scope
- Read only. Run: `git show dfc703c1`, `git show 247101ce`, `git show 814790e9`, `git log --oneline -20`,
  and read the files named above plus AGENTS.md in full.
- Do not fix anything: report.
- Questions to answer:
  a. Does each rule say what the user asked, no more and no less? Quote the user's words against the rule.
  b. Contradictions with existing AGENTS.md rules (publishing rule, standing yeses, timing windows, disk,
     clean-room/Tier 3, handed-out jobs only, draft-first, PR cap, no unattended runs).
  c. Interactions between today's rules: e.g. does the portal-first gate conflict with "fix pushes to open
     PRs" or with the stacked PRs; does the 160 fps pass bar conflict with acceptance/timing rules; do
     "no shortcuts" items rule out legitimate engineering (e.g. async loading, LOD, culling)?
  d. The 160 fps pass bar: is it measurable with the tools described, unambiguous, and fair (held-out
     route, quiet PC, settings)? Anything missing (resolution/monitor, warm vs cold start, how many repeats,
     noise margin)?
  e. xEdit rule: practical for workers (who can run what, read-only safety, game-data writes, licence MPL-2.0
     vs MIT/Apache, Chef's #157/#158 ownership), anything unsafe or vague.
  f. Anything a new session reading AGENTS.md cold would misunderstand.

# Done when
- Every rule above has been checked against the user's words and the existing rules, or marked not checked.
- The verdict is one of accept, fix first, reject.

# Report
- Findings as `path:line - problem - why it matters - suggested wording`, most serious first.
- Separate "must fix" from "nice to have".
- What you ran and what it printed; what you only read.

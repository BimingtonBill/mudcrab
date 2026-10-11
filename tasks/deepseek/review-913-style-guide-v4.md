# Goal
Review the writing style guide `local/pr-drafts/STYLE.md` (style v4). Question: will an AI writer that follows it produce PRs, issues and comments that a newcomer contributor can read quickly *and* understand, without losing accuracy? Verdict: accept, fix first, or reject.

# Context
- The guide governs every public text the project's AI assistant writes for one human contributor (@BimingtonBill) on an open-source Rust/Bevy project (a Skyrim-data engine called Mudcrab): pull requests, issues, issue comments, replies to reviewers.
- History: v2 made texts short (the user found bot summaries clearer than our long bodies). v3 (`local/pr-drafts/STYLE-v3.md`) adopted the repository's PR template (`.github/PULL_REQUEST_TEMPLATE.md`). v4 (today) adds the section "Explain, don't just state", because the user found recent comments technically accurate but poor at teaching the reader: they opened with the suggestion, left terms unexplained, used verbless fragments, and gave no examples. v4 also raised word caps about 30% and retired "bullets may be terse fragments".
- Real examples of the v3-era output the user criticised are in `local/pr-drafts/out/67-comments-88-90.comment-88.md`, `local/pr-drafts/out/67-comments-88-90.comment-90.md` and `local/pr-drafts/out/66-comment-81-eclepso-materials.comment-81.md`.
- Fixed rules that are the user's decisions and are NOT up for review (only flag if the guide states them unclearly): one AI note line at the top; first person singular ("I"/"my", never "we"/"our"); never ask others to test; the repo template's headings; stack lines and labels.

# Scope
- Read only. Read STYLE.md fully, STYLE-v3.md for the diff, the repo PR template, and the three example comments.
- Do not edit anything: report.
- Look for:
  1. Contradictions inside the guide, especially leftovers from v2/v3 that now fight v4's "explain" rule (brevity rules, tables, caps).
  2. Rules too vague for an AI writer to apply consistently (what counts as a term to explain? when is an example needed?).
  3. Whether the word caps fit the new rule, for a PR, an issue and a short comment.
  4. Whether the before/after example is a good model; propose a better one if not.
  5. Missing guidance a newcomer reader would need (e.g. a reply to a human vs a bot, comments on someone else's proposal, how to disagree politely without hedging).
  6. Rewrite ONE of the three example comments following v4 as a test of the guide, and say where the guide helped or failed you while doing it.

# Done when
- Every section of STYLE.md has been read and either passed or given findings.
- The test rewrite is included.
- The verdict is one of accept, fix first, reject.

# Report
- Findings as `STYLE.md:<line> - problem - why it matters - suggested wording`, most serious first.
- The test rewrite, with a word count.
- A short list of what the guide does well (keep it).

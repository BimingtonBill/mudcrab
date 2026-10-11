# Goal
Re-review only the fix delta of a local branch (converter half of Mudcrab-Team/mudcrab#188, interior room graph) before it goes upstream. Be strict. Decide whether the delta fixes the earlier findings correctly and breaks nothing.

# Context
- Delta: `local/review/pr-room-graph/delta.diff` (commit a3cc47c3 on top of bafe437a). Whole change for context: `local/review/pr-room-graph/full.diff`. Read code ONLY from `local/review/pr-room-graph/head/` (new) and `.../base/` (upstream main); the checkout under `crates/` is a different branch.
- Earlier findings the delta claims to fix:
  1. `add_overlap_rows` failed finalisation on a reused older database lacking `statics.bounds_*`, even with no rooms: now returns early when `room_bounds` is empty or the columns are missing.
  2. Non-finite XPRM sizes would bind NULL into NOT NULL columns and abort the export: such markers are now skipped and counted (`invalid_markers_skipped`).
  3. Exterior detection was only tested with a hand-made interior set: a real load-order export test now covers gridded and worldspace cells.
  4. No test pinned the room box's inverse rotation: a yaw 0.6 test and a rotated overlap test were added.
  5./6. Spec and design note brought in line with the code.
- The author ran fmt, clippy -D warnings (workspace) and nextest on converter + dummy-content: 604 passed.

# Done when
For each point: correct and complete, with diff-line evidence; each new test would fail if its fix were reverted (say how you know); nothing that used to work now fails.

# Scope
Read only. Change nothing.

# Report
Verdict line: ready / small fixes / fix first. Findings as `file:line - issue - why it matters - fix`, at most 5, verified only, marked blocker / should fix / nit. Under 300 words.

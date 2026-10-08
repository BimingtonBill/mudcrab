---
name: work-board
description: Show or refresh the Mudcrab work board, a Trello-style view of branches, PR drafts and PRs (needs you, in progress, ready to send, text drafts, open upstream, merged). Use when the user asks for the board, the dashboard, "what's in flight", or the state of branches/drafts/PRs.
---

# Work board

The board lives in a private artifact kept open in the side pane:
https://claude.ai/artifact/SWZCrqCDprgTcQWQR9c7RM (page source `local/board/work-board.html`).
The page reads its cards from the artifact's store (`board/current`) and redraws live when that
document changes, so a refresh is a data write, not a republish.

## Refresh

1. `python tools/work_board.py --json` and wrap it as `{"board": <that>}` in `local/board/current.json`
   (or let the hook below do it).
2. `ArtifactData set` url above, collection `board`, doc_id `current`, `file_path` that file,
   `if_version` from `local/board/version.txt`. Write the returned version back to `version.txt`.
3. Open the pane if it isn't showing: `Artifact` action `open` with the URL.

The PostToolUse hook `tools/work_board_hook.py` (in `.claude/settings.local.json`) rebuilds
`current.json` after git commit/merge/push/branch and `gh pr` commands and asks for step 2 when the
cards changed. It nudges only the session ids listed in `local/board/hook-session.txt`.

## Live updates

While the user wants the board live, run `python tools/work_board_watch.py --every 120` under the
Monitor tool (timeout 30 min, re-arm on expiry). It prints one line only when the cards change; on
each line do the Refresh step 2 push, then copy `pending.sha256` over `pushed.sha256` and the
current cards (column + title) into `pushed-cards.json`. No message to the user for routine pushes.

## Inline instead

`python tools/work_board.py --widget` prints an HTML fragment for the inline widget tool
(`show_widget`, title `mudcrab_work_board`); it scrolls away with the chat. Terminal summary:
`python tools/work_board.py`.

Where each column comes from is in the docstring of `tools/work_board.py`. Fix the sources (queue,
draft status lines, task queue, roadmap JSON), not the board: it shows what they say.

---
name: work-board
description: Show the Mudcrab work board in this session, a Trello-style view of branches, PR drafts and PRs (needs you, in progress, ready to send, text drafts, open upstream, merged). Use when the user asks for the board, the dashboard, "what's in flight", or the state of branches/drafts/PRs.
---

# Work board

1. Run `python tools/work_board.py --widget > <scratchpad>/board.html` from the main checkout
   (`C:\Modding\Mudcrab\mudcrab`). It reads git, `gh`, `docs/pr/upstream-queue.md`, `local/pr-drafts/`,
   `local/plans/task-queue.md` ("Waiting on the user") and `local/roadmap/*.json`. Read-only, about 5 s.
2. Read that file and pass its whole content, unchanged, as `widget_code` to the inline widget tool
   (`show_widget`; call its `read_me` first once per session). Title: `mudcrab_work_board`.
3. Below the widget, write two or three lines only: counts that matter (needs you, ready to send) and
   anything that looks wrong (a failing CI tag, a ready row not seen in portal for days).

Clicking a card sends "Board: what is the state of <ref> and what does it wait on?". Answer from the
source files and `gh`, not from the card text.

For a terminal summary use `python tools/work_board.py`; for data, `--json`.
Where each column comes from is in the docstring of `tools/work_board.py`. Fix the sources (queue,
draft status lines, roadmap JSON), not the board: it shows what they say.

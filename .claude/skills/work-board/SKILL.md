---
name: work-board
description: Show the Mudcrab work board, a Trello-style view of branches, PR drafts and PRs (needs you, in progress, ready to send, text drafts, open upstream, merged). Use when the user asks for the board, the dashboard, "what's in flight", or the state of branches/drafts/PRs.
---

# Work board

The board is a private artifact the user keeps open in the side pane:
https://claude.ai/artifact/SWZCrqCDprgTcQWQR9c7RM (page source `local/board/work-board.html`).
It updates itself: the page polls the local MCP server `mudcrab-board` (`tools/work_board_mcp.py`,
registered in the user's MCP config) every minute with `watchTool`, and the server runs
`tools/work_board.py --json` with a 60 s cache. No Claude session is involved in updates.

To show it: `Artifact` action `open` with the URL. Nothing to refresh or push.

If the page says the server isn't running: check `claude mcp get mudcrab-board` (the CLI is bundled
under `%APPDATA%\Claude\claude-code\<version>\claude.exe`, not on PATH) and that
`python tools/work_board.py` runs; the app connects MCP servers at start, so a fresh registration
needs an app restart. Sessions can also call the tool directly (`mcp__mudcrab-board__get_board`).

## Inline instead

`python tools/work_board.py --widget` prints an HTML fragment for the inline widget tool
(`show_widget`, title `mudcrab_work_board`); it scrolls away with the chat. Terminal summary:
`python tools/work_board.py`.

Where each column comes from is in the docstring of `tools/work_board.py`. Fix the sources (queue,
draft status lines, task queue, roadmap JSON), not the board: it shows what they say. The project-wide
task board is Team Lead's `mudcrab-queue` (`local/plans/task-queue.md`); this one is the upstream
pipeline only.

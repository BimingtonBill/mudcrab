"""PostToolUse hook: keep the work board artifact's data current.

After a Bash command that changes branches, PRs or drafts (git commit/merge/push/branch/checkout -b,
gh pr ...), rebuild the board with tools/work_board.py and, when it differs from what the artifact
has, tell the session to push local/board/current.json to the artifact's store (ArtifactData set).
Only the session named in local/board/hook-session.txt gets the nudge; other sessions sharing this
checkout are left alone. Never blocks or fails the tool call.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BOARD = os.path.join(ROOT, "local", "board")
URL = "https://claude.ai/artifact/SWZCrqCDprgTcQWQR9c7RM"
TRIGGER = re.compile(r"\b(git\s+(commit|merge|push|branch|checkout\s+-b|switch\s+-c|worktree)|gh\s+pr)\b")


def main() -> int:
    try:
        event = json.load(sys.stdin)
    except ValueError:
        return 0
    command = (event.get("tool_input") or {}).get("command", "")
    if not TRIGGER.search(command):
        return 0
    try:
        allowed = open(os.path.join(BOARD, "hook-session.txt"), encoding="utf-8").read().split()
    except OSError:
        return 0
    if event.get("session_id") not in allowed:
        return 0
    out = subprocess.run([sys.executable, os.path.join(ROOT, "tools", "work_board.py"), "--json"],
                         capture_output=True, text=True, encoding="utf-8", cwd=ROOT, timeout=60)
    if out.returncode != 0:
        return 0
    board = json.loads(out.stdout)
    # The build time changes every run; compare the cards only.
    digest = hashlib.sha256(json.dumps(board["cards"], sort_keys=True).encode()).hexdigest()
    pushed_path = os.path.join(BOARD, "pushed.sha256")
    try:
        pushed = open(pushed_path, encoding="utf-8").read().strip()
    except OSError:
        pushed = ""
    with open(os.path.join(BOARD, "current.json"), "w", encoding="utf-8") as handle:
        json.dump({"board": board}, handle, ensure_ascii=False)
    with open(os.path.join(BOARD, "pending.sha256"), "w", encoding="utf-8") as handle:
        handle.write(digest)
    if digest == pushed:
        return 0
    note = (f"Work board data changed. Push it to the side-pane artifact: ArtifactData set url={URL} "
            f"collection=board doc_id=current file_path={os.path.join(BOARD, 'current.json')} with if_version "
            f"from local/board/version.txt; then write the new version to version.txt and copy "
            f"local/board/pending.sha256 over pushed.sha256. No message to the user needed.")
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": note}}))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:  # a hook must never break the session's tool call
        raise SystemExit(0)

"""A tiny local MCP server (stdio) that serves the work board, so the side-pane page reads it itself.

One tool, `get_board`: runs tools/work_board.py --json and returns the board. Results are cached for
`max_age` seconds (default 60) so several open views don't each start the gh calls. The artifact page
declares it as a host MCP server (`host:mudcrab-board`) and polls it with watchTool, so the board stays
live with no Claude session in the loop.

Hand-written JSON-RPC over stdio (no SDK needed): initialize, tools/list, tools/call, ping.
Read-only. Register: claude mcp add mudcrab-board -- python C:/Modding/Mudcrab/mudcrab/tools/work_board_mcp.py
"""

import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = {
    "name": "get_board",
    "description": "The Mudcrab work board: branches, PR drafts and PRs sorted into stages, as JSON.",
    "annotations": {"readOnlyHint": True, "destructiveHint": False},
    "inputSchema": {"type": "object", "properties": {
        "max_age": {"type": "integer", "description": "Seconds a cached board may be reused (default 60)."}}},
}
cache = {"at": 0.0, "board": None}


def board(max_age: int) -> dict:
    if cache["board"] is None or time.time() - cache["at"] > max_age:
        out = subprocess.run([sys.executable, os.path.join(ROOT, "tools", "work_board.py"), "--json"],
                             capture_output=True, text=True, encoding="utf-8", cwd=ROOT, timeout=90)
        if out.returncode != 0:
            raise RuntimeError(out.stderr.strip()[-300:] or "work_board.py failed")
        cache.update(at=time.time(), board=json.loads(out.stdout))
    return cache["board"]


def reply(msg_id, result=None, error=None) -> None:
    body = {"jsonrpc": "2.0", "id": msg_id}
    body.update({"error": error} if error else {"result": result})
    sys.stdout.write(json.dumps(body, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def handle(msg: dict) -> None:
    method, msg_id = msg.get("method"), msg.get("id")
    if msg_id is None:  # notifications (initialized, cancelled) need no answer
        return
    if method == "initialize":
        version = (msg.get("params") or {}).get("protocolVersion") or "2025-06-18"
        reply(msg_id, {"protocolVersion": version, "capabilities": {"tools": {}},
                       "serverInfo": {"name": "mudcrab-board", "version": "1.0.0"}})
    elif method == "ping":
        reply(msg_id, {})
    elif method == "tools/list":
        reply(msg_id, {"tools": [TOOL]})
    elif method == "tools/call":
        params = msg.get("params") or {}
        if params.get("name") != "get_board":
            reply(msg_id, error={"code": -32602, "message": f"unknown tool {params.get('name')}"})
            return
        try:
            data = board(int((params.get("arguments") or {}).get("max_age", 60)))
            reply(msg_id, {"content": [{"type": "text", "text": json.dumps(data, ensure_ascii=False)}],
                           "structuredContent": data, "isError": False})
        except Exception as error:
            reply(msg_id, {"content": [{"type": "text", "text": f"board failed: {error}"}], "isError": True})
    else:
        reply(msg_id, error={"code": -32601, "message": f"method not found: {method}"})


def main() -> int:
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            handle(json.loads(line))
        except ValueError:
            reply(None, error={"code": -32700, "message": "parse error"})
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

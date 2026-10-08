"""Watch for work board changes and print one line each time the cards change.

Rebuilds the board with tools/work_board.py every --every seconds (git, gh, queue, drafts, task queue)
and compares the cards with what the side-pane artifact has (local/board/pushed.sha256). On a change it
writes local/board/current.json and pending.sha256 and prints one line, so a Claude session running
this under its Monitor tool wakes up and pushes the data (ArtifactData set). Silent otherwise.
Read-only towards git and GitHub. Runs until killed.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BOARD = os.path.join(ROOT, "local", "board")


def read(name: str) -> str:
    try:
        return open(os.path.join(BOARD, name), encoding="utf-8").read().strip()
    except OSError:
        return ""


def write(name: str, text: str) -> None:
    with open(os.path.join(BOARD, name), "w", encoding="utf-8") as handle:
        handle.write(text)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", line_buffering=True)
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--every", type=int, default=120, help="seconds between rebuilds (default 120)")
    args = parser.parse_args()
    announced, last_error = "", ""
    while True:
        try:
            out = subprocess.run([sys.executable, os.path.join(ROOT, "tools", "work_board.py"), "--json"],
                                 capture_output=True, text=True, encoding="utf-8", cwd=ROOT, timeout=90)
            if out.returncode == 0:
                board = json.loads(out.stdout)
                cards = board["cards"]
                digest = hashlib.sha256(json.dumps(cards, sort_keys=True).encode()).hexdigest()
                if digest != read("pushed.sha256") and digest != announced:
                    old = {(c["column"], c["title"]) for c in json.loads(read("pushed-cards.json") or "[]")}
                    new = {(c["column"], c["title"]) for c in cards}
                    write("current.json", json.dumps({"board": board}, ensure_ascii=False))
                    write("pending.sha256", digest)
                    write("pending-cards.json", json.dumps([{"column": c, "title": t} for c, t in sorted(new)]))
                    moved = sorted(t for c, t in new - old)[:3]
                    print(f"board changed: {len(new - old)} new or moved, {len(old - new)} gone"
                          + (f" (e.g. {'; '.join(m[:60] for m in moved)})" if moved else ""))
                    announced = digest
        except Exception as error:  # keep watching through a failed gh call or a bad parse
            if str(error) != last_error:
                print(f"board watch error: {error}"[:200])
                last_error = str(error)
        time.sleep(args.every)


if __name__ == "__main__":
    raise SystemExit(main())

"""Claim, list and update tasks from local/plans/task-queue.md for T3 task threads.

A claim is the exclusive creation of local/task-threads/<id>.md. A lock directory
(local/task-threads/.lock) serialises the queue edit, so two threads cannot claim
the same task.

  python tools/t3_claim.py list
  python tools/t3_claim.py claim <id> --thread <label>
  python tools/t3_claim.py set <id> <active|waiting|parked|done> [--note "..."]
  python tools/t3_claim.py claimed
"""

import argparse
import datetime
import os
import re
import shutil
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
QUEUE = ROOT / "local" / "plans" / "task-queue.md"
STATUS_DIR = ROOT / "local" / "task-threads"
ARCHIVE_DIR = STATUS_DIR / "archive"
SPECS = ROOT / "local" / "plans" / "task-specs.md"
LOCK = STATUS_DIR / ".lock"
STATES = {"active", "waiting", "parked", "done", "queued"}
ROW = re.compile(r"^\|\s*([^|]+?)\s*\|")


def now():
    return datetime.datetime.now().strftime("%Y-%m-%d %H:%M")


def status_path(task_id):
    return STATUS_DIR / f"{task_id}.md"


def is_claimed(task_id):
    return status_path(task_id).exists()


def task_rows(text):
    """Yield (line_index, cells) for each five-column task row in the queue."""
    for i, line in enumerate(text.split("\n")):
        if not line.startswith("|"):
            continue
        cells = line.split("|")
        # ['', ' # ', ' Task ', ' Owner ', ' Status ', ' Detail ', '']
        if len(cells) != 7 or cells[1].strip() in ("#", "") or set(cells[1].strip()) <= {"-"}:
            continue
        yield i, cells


def acquire_lock(timeout=30.0):
    STATUS_DIR.mkdir(parents=True, exist_ok=True)
    deadline = time.time() + timeout
    while True:
        try:
            os.mkdir(LOCK)
            return
        except FileExistsError:
            if time.time() > deadline:
                sys.exit(f"lock held at {LOCK}; if no other thread is running, remove it and retry")
            time.sleep(0.2)


def release_lock():
    try:
        os.rmdir(LOCK)
    except FileNotFoundError:
        pass


def set_row(text, task_id, state, thread=None):
    lines = text.split("\n")
    for i, cells in task_rows(text):
        if cells[1].strip() != task_id:
            continue
        cells[4] = f" {state} "
        detail = cells[5].strip()
        detail = re.sub(r"^\[thread [^\]]*\]\s*", "", detail)
        if state in ("active", "waiting") and thread:
            detail = f"[thread {thread}] {detail}"
        cells[5] = f" {detail} "
        lines[i] = "|".join(cells)
        return "\n".join(lines)
    sys.exit(f"no task row '{task_id}' in {QUEUE}")


def original_text(task_id):
    """Full queue text from before the plain titles (task-specs.md), or an empty string."""
    if not SPECS.exists():
        return ""
    for line in SPECS.read_text(encoding="utf-8").split("\n"):
        cells = line.split("|")
        if len(cells) == 5 and cells[1].strip() == task_id:
            return f"{cells[2].strip()} | {cells[3].strip()}"
    return ""


def thread_title(task_id, task, limit=70):
    """Thread title: the task id, then the queue's plain title, cut at a word."""
    head = f"{task_id} "
    if len(head + task) <= limit:
        return head + task
    cut = task[: limit - len(head) - 3].rsplit(" ", 1)[0]
    return f"{head}{cut}..."


def cmd_list(_args):
    text = QUEUE.read_text(encoding="utf-8")
    found = 0
    for _, cells in task_rows(text):
        if cells[4].strip() != "queued":
            continue
        task_id = cells[1].strip()
        if is_claimed(task_id):
            continue
        found += 1
        print(f"{task_id} | {cells[2].strip()} | {cells[3].strip()} | {cells[5].strip()}")
    if not found:
        print("no unclaimed queued tasks")


def cmd_claim(args):
    task_id = args.id
    acquire_lock()
    try:
        if is_claimed(task_id):
            prior = status_path(task_id).read_text(encoding="utf-8").splitlines()[0]
            sys.exit(f"{task_id} already claimed: {prior}")
        text = QUEUE.read_text(encoding="utf-8")
        rows = {c[1].strip(): c for _, c in task_rows(text)}
        if task_id not in rows:
            sys.exit(f"no task row '{task_id}' in the queue")
        task = rows[task_id][2].strip()
        if rows[task_id][4].strip() != "queued":
            sys.exit(f"{task_id} is '{rows[task_id][4].strip()}' in the queue, not queued")
        title = thread_title(task_id, task)
        full = original_text(task_id)
        header = (f"# {task_id} {task}\n\n- Claimed by: {args.thread}\n"
                  f"- Claimed at: {now()}\n- State: active\n"
                  + (f"- Full queue text: {full}\n" if full else "") + "\n")
        with status_path(task_id).open("x", encoding="utf-8") as f:
            f.write(header + "## Now\n\n## Done\n\n## Next\n\n## Evidence\n")
        QUEUE.write_text(set_row(text, task_id, "active", args.thread), encoding="utf-8")
    finally:
        release_lock()
    print(f"claimed {task_id} for {args.thread}; status file {status_path(task_id)}")
    print(f"Thread title: {title}")


def cmd_set(args):
    task_id = args.id
    state = args.state
    if state not in STATES:
        sys.exit(f"state must be one of {sorted(STATES)}")
    acquire_lock()
    try:
        path = status_path(task_id)
        if not path.exists():
            sys.exit(f"{task_id} has no status file; claim it first")
        text = path.read_text(encoding="utf-8")
        text = re.sub(r"^- State: .*$", f"- State: {state}", text, count=1, flags=re.M)
        if args.note:
            text = text.replace("## Done\n", f"## Done\n\n- {now()} {args.note}\n", 1)
        path.write_text(text, encoding="utf-8")
        queue = QUEUE.read_text(encoding="utf-8")
        thread = re.search(r"^- Claimed by: (.*)$", text, re.M)
        label = thread.group(1).strip() if thread else None
        QUEUE.write_text(set_row(queue, task_id, state, label), encoding="utf-8")
        if state in ("done", "parked"):
            ARCHIVE_DIR.mkdir(parents=True, exist_ok=True)
            if state == "done":
                shutil.move(str(path), str(ARCHIVE_DIR / path.name))
    finally:
        release_lock()
    print(f"{task_id} -> {state}")


def cmd_claimed(_args):
    for path in sorted(STATUS_DIR.glob("*.md")):
        head = path.read_text(encoding="utf-8").splitlines()
        by = next((l for l in head if l.startswith("- Claimed by:")), "")
        state = next((l for l in head if l.startswith("- State:")), "")
        print(f"{path.stem} | {by[len('- Claimed by:'):].strip()} | {state[len('- State:'):].strip()}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("list").set_defaults(run=cmd_list)
    sub.add_parser("claimed").set_defaults(run=cmd_claimed)
    claim = sub.add_parser("claim")
    claim.add_argument("id")
    claim.add_argument("--thread", required=True)
    claim.set_defaults(run=cmd_claim)
    setp = sub.add_parser("set")
    setp.add_argument("id")
    setp.add_argument("state")
    setp.add_argument("--note")
    setp.set_defaults(run=cmd_set)
    args = parser.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""The reference-pose pipeline: check, queue for the user, merge, check again.

The process (docs/design/reference-pose-pipeline.md):
1. `pose_ballpark.py` places new shots from their place names (seconds).
2. `check` renders every shot not yet graded, builds grading pages and has a DeepSeek worker
   grade them (a few minutes). `matched` and `close` are accepted.
3. `build` queues the rest for the user's in-engine tool (`local/reference/fit-queue.vbs`):
   N / B step, P saves, X = can't find it.
4. `merge` folds the saved views in, marks them `unchecked`, and `check` grades them again. A
   shot the user has placed by hand is not queued a second time, whatever its grade.

Usage:
    python tools/research/pose_queue.py check [--no-grade]
    python tools/research/pose_queue.py build
    python tools/research/pose_queue.py merge [--dry-run]
    python tools/research/pose_queue.py import <file with a JSON object of name -> grade> [--by <who>]
    python tools/research/pose_queue.py grade <shot name> <grade> [--by <who>]
    python tools/research/pose_queue.py status
"""

import datetime
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SHOTS_FILES = [
    "tools/reference/riverwood_shots.json",
    "tools/reference/uesp_shots_4x3.json",
    "tools/reference/world_shots_4x3.json",
    "tools/reference/world_shots_wide.json",
]
LOCAL = os.path.join(ROOT, "local", "reference")
GRADES = os.path.join(LOCAL, "pose-grades.json")
GRADING = os.path.join(LOCAL, "grading")
LAUNCHER = os.path.join(LOCAL, "fit-queue.vbs")
MANUAL = os.path.join(LOCAL, "manual-poses.jsonl")
NOT_FOUND = os.path.join(LOCAL, "not-found.jsonl")
ENGINE = r"C:\Modding\wkj-tools\engine-portal-fit.exe"
ASSETS = r"C:\Modding\SkyrimConverted"
GRADER_BRIEF = "tasks/deepseek/grade-reference-poses.md"
ACCEPTED = {"matched", "close"}
GRADES_KNOWN = ACCEPTED | {"area only", "wrong"}


def today():
    return datetime.date.today().isoformat()


def load_json(path, default):
    if not os.path.exists(path):
        return default
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save_json(path, data):
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(data, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


def jsonl(path):
    if not os.path.exists(path):
        return []
    with open(path, encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def shots_files():
    for path in SHOTS_FILES:
        full = os.path.join(ROOT, path)
        if os.path.exists(full):
            yield path, load_json(full, None)


def flagged():
    return {record.get("shot") for record in jsonl(NOT_FOUND)}


def set_grade(grades, name, verdict, by):
    entry = grades.get(name, {})
    entry.update(grade=verdict, by=by, at=today())
    grades[name] = entry


def check(run_grader):
    """Render every shot whose grade is missing or `unchecked`, page them, grade them."""
    grades = load_json(GRADES, {})
    batch = "check-" + datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    render_root = os.path.join(GRADING, batch)
    subsets = []
    for path, data in shots_files():
        todo = [
            shot
            for shot in data["shots"]
            if shot.get("reference")
            and shot["name"] not in flagged()
            and grades.get(shot["name"], {}).get("grade", "unchecked") == "unchecked"
        ]
        if not todo:
            continue
        stem = os.path.splitext(os.path.basename(path))[0]
        subset = os.path.join(render_root, stem + ".json")
        os.makedirs(render_root, exist_ok=True)
        save_json(subset, {"width": data["width"], "height": data["height"], "shots": todo})
        renders = os.path.join(render_root, stem)
        with open(os.path.join(render_root, stem + ".log"), "w") as log:
            subprocess.run(
                [ENGINE, "--assets", ASSETS, "--shots", subset, "--shots-out", renders],
                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=False,
            )
        subsets.append(f"{subset}:{renders}")
    if not subsets:
        print("nothing to check")
        return
    pages = os.path.join(GRADING, batch + ".jpg")
    subprocess.run([sys.executable, os.path.join(ROOT, "tools/research/grade_sheet.py"), pages, *subsets], check=True)
    if not run_grader:
        print(f"pages ready: {GRADING}\\{batch}-*.jpg; grade them, then `import`")
        return
    report = os.path.join(GRADING, batch + ".grades.txt")
    agent = os.path.expanduser("~/.claude/skills/deepseek-agents/ds-agent.ps1")
    with open(report, "w", encoding="utf-8") as out:
        subprocess.run(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", agent,
             "-TaskFile", GRADER_BRIEF, "-Dir", ROOT, "-Mode", "read", "-Label", f"review-{batch}",
             "-Kind", "review", "-Effort", "high", "-MaxTurns", "20", "-TimeoutMinutes", "10",
             "-Extra", f"Batch: {batch} (pages local/reference/grading/{batch}-1.jpg onwards, "
                       f"index local/reference/grading/{batch}.jpg.json)."],
            cwd=ROOT, stdout=out, stderr=subprocess.STDOUT, check=False,
        )
    import_grades(report, "grader")


def import_grades(path, by):
    text = open(path, encoding="utf-8", errors="replace").read()
    found = None
    for match in re.finditer(r"\{[^{}]*\}", text, re.S):
        try:
            candidate = json.loads(match.group(0))
        except ValueError:
            continue
        if isinstance(candidate, dict) and candidate and all(isinstance(v, str) for v in candidate.values()):
            found = candidate
    if not found:
        sys.exit(f"no JSON object of grades found in {path}")
    grades = load_json(GRADES, {})
    known = {shot["name"] for _, data in shots_files() for shot in data["shots"]}
    for name, verdict in found.items():
        verdict = verdict.strip().lower()
        if name in known and verdict in GRADES_KNOWN:
            set_grade(grades, name, verdict, by)
        else:
            print(f"ignored {name!r}: {verdict!r}")
    save_json(GRADES, grades)
    print(f"imported {len(found)} grade(s) from {os.path.basename(path)}")
    status()


def build():
    grades = load_json(GRADES, {})
    skip = flagged()
    queues = []
    for path, data in shots_files():
        todo = [
            shot
            for shot in data["shots"]
            if shot.get("reference")
            and shot["name"] not in skip
            and not grades.get(shot["name"], {}).get("hand")
            and grades.get(shot["name"], {}).get("grade") not in ACCEPTED | {"unchecked", None}
        ]
        target = os.path.join(LOCAL, "manual-queue-" + os.path.splitext(os.path.basename(path))[0] + ".json")
        if todo:
            save_json(target, {"width": data["width"], "height": data["height"], "shots": todo})
            queues.append((target, todo))
        elif os.path.exists(target):
            os.remove(target)
    count = sum(len(todo) for _, todo in queues)
    if queues:
        files = ",".join(os.path.relpath(target, ROOT).replace("\\", "/") for target, _ in queues)
        first = queues[0][1][0]["name"]
        lines = [
            "' Generated by tools/research/pose_queue.py build: the shots the automatic check did not",
            "' accept. N / B step through them, the Skyrim picture is top-right, P saves, X = can't find it.",
            'Set shell = CreateObject("WScript.Shell")',
            f'shell.CurrentDirectory = "{ROOT}"',
            f'command = "cmd /c ""{ENGINE}"" --assets {ASSETS} " & _',
            f'    "--start-shot {files} {first} " & _',
            '    "> local\\reference\\fit-in-engine.log 2>&1"',
            "shell.Run command, 0, False",
        ]
        with open(LAUNCHER, "w", encoding="utf-8", newline="\r\n") as handle:
            handle.write("\n".join(lines) + "\n")
    elif os.path.exists(LAUNCHER):
        os.remove(LAUNCHER)
    print(f"{count} shot(s) queued for the by-hand tool" + (f": {LAUNCHER}" if count else ""))
    for _, todo in queues:
        for shot in todo:
            print("  ", shot["name"], "-", grades.get(shot["name"], {}).get("grade"))


def merge(dry_run):
    latest = {}
    for record in jsonl(MANUAL):
        if record.get("shot"):
            latest[record["shot"]] = record
    grades = load_json(GRADES, {})
    merged = 0
    for path, data in shots_files():
        changed = False
        for shot in data["shots"]:
            record = latest.get(shot["name"])
            if not record:
                continue
            pose = {
                "position": [round(float(value), 1) for value in record["position"]],
                "yaw": round(float(record["yaw"]) % 360.0, 1),
                "pitch": round(float(record["pitch"]), 1),
            }
            for space in ("worldspace_id", "interior_cell_id"):
                if space in record:
                    pose[space] = record[space]
            if all(shot.get(key) == value for key, value in pose.items()):
                continue
            shot.update(pose)
            shot["confidence"] = "hand-fit"
            stamp = f" Pose set by hand in the engine ({str(record.get('saved_at', ''))[:10]})."
            if "Pose set by hand" not in shot.get("note", ""):
                shot["note"] = shot.get("note", "") + stamp
            set_grade(grades, shot["name"], "unchecked", "user")
            grades[shot["name"]]["hand"] = True
            print(f"{shot['name']}: {pose}")
            merged += 1
            changed = True
        if changed and not dry_run:
            save_json(os.path.join(ROOT, path), data)
    if not dry_run:
        save_json(GRADES, grades)
    print(f"{merged} hand pose(s) merged" + (" (dry run)" if dry_run else "; run `check` to grade them"))


def grade(name, verdict, by):
    if verdict not in GRADES_KNOWN | {"unchecked"}:
        sys.exit(f"unknown grade {verdict!r}; use one of {sorted(GRADES_KNOWN)}")
    grades = load_json(GRADES, {})
    set_grade(grades, name, verdict, by)
    save_json(GRADES, grades)


def status():
    grades = load_json(GRADES, {})
    skip = flagged()
    counts = {}
    for _, data in shots_files():
        for shot in data["shots"]:
            entry = grades.get(shot["name"], {})
            if not shot.get("reference"):
                verdict = "no reference"
            elif shot["name"] in skip:
                verdict = "can't find (user)"
            else:
                verdict = entry.get("grade", "unchecked") + (" (by hand)" if entry.get("hand") else "")
            counts[verdict] = counts.get(verdict, 0) + 1
    for verdict, count in sorted(counts.items()):
        print(f"{count:3}  {verdict}")


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    command = argv[1]
    by = argv[argv.index("--by") + 1] if "--by" in argv else "lead"
    if command == "check":
        check("--no-grade" not in argv)
    elif command == "build":
        build()
    elif command == "merge":
        merge("--dry-run" in argv)
    elif command == "import" and len(argv) >= 3:
        import_grades(argv[2], by)
    elif command == "grade" and len(argv) >= 4:
        grade(argv[2], argv[3], by)
    elif command == "status":
        status()
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

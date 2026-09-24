#!/usr/bin/env python3
"""The reference-pose loop: automatic pass, the user, then an automatic round, and repeat.

The process (docs/design/reference-pose-pipeline.md):
1. `pose_ballpark.py` places new shots from their place names (seconds); `check` renders and has
   a DeepSeek worker grade them. `matched` and `close` are accepted.
2. `build` queues every shot not accepted for the user's in-engine tool
   (`local/reference/fit-queue.vbs`): N / B step, P saves, X = can't find it, T types a note.
   A shot the script could not place at all is never handed to the user.
3. `cycle`, when the user closes the tool: merge the saved views (notes included), grade them
   (the grader reads the notes), then
   - accepted: done;
   - `area only` (needs fine work): automatic fine-tune, small moves around the pose;
   - `wrong`, skipped with X, passed over with N, or never placed: automatic re-ball-park, the
     place seen from several directions;
   grade the refined poses, and build the next queue for the user. A shot that has had
   MAX_ROUNDS automatic rounds without being accepted is parked and not queued again.

Usage:
    python tools/research/pose_queue.py cycle
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
NOTES_BRIEF = "tasks/deepseek/resolve-pose-notes.md"
ACCEPTED = {"matched", "close"}
MAX_ROUNDS = 2
GRADES_KNOWN = ACCEPTED | {"area only", "wrong"}


def today():
    return datetime.date.today().isoformat()


def utc_now():
    # The engine stamps saves and skips in UTC ISO with a Z, so ours compare as strings.
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


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


def skips():
    """Latest X time per shot."""
    latest = {}
    for record in jsonl(NOT_FOUND):
        if record.get("shot"):
            latest[record["shot"]] = max(latest.get(record["shot"], ""), str(record.get("flagged_at", "")))
    return latest


def flagged(grades=None):
    """Shots the user skipped with X since their last automatic refresh."""
    grades = grades if grades is not None else load_json(GRADES, {})
    return {name for name, when in skips().items() if when > grades.get(name, {}).get("refreshed_at", "")}


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
            and shot["name"] not in flagged(grades)
            and shot.get("confidence") != "unplaced"
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
        return None
    pages = os.path.join(GRADING, batch + ".jpg")
    subprocess.run([sys.executable, os.path.join(ROOT, "tools/research/grade_sheet.py"), pages, *subsets], check=True)
    if not run_grader:
        print(f"pages ready: {GRADING}\\{batch}-*.jpg; grade them, then `import`")
        return batch
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
    return batch


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
    skip = flagged(grades)
    queues = []
    for path, data in shots_files():
        todo = [
            shot
            for shot in data["shots"]
            if shot.get("reference")
            and shot["name"] not in skip
            and shot.get("confidence") != "unplaced"
            and not grades.get(shot["name"], {}).get("parked")
            and grades.get(shot["name"], {}).get("grade") not in ACCEPTED | {"unchecked", None}
        ]
        target = os.path.join(LOCAL, "manual-queue-" + os.path.splitext(os.path.basename(path))[0] + ".json")
        if todo:
            save_json(target, {"width": data["width"], "height": data["height"], "shots": todo})
            queues.append((target, todo))
        elif os.path.exists(target):
            os.remove(target)
    count = sum(len(todo) for _, todo in queues)
    stamp = utc_now()
    for _, todo in queues:
        for shot in todo:
            grades.setdefault(shot["name"], {})["queued_at"] = stamp
    save_json(GRADES, grades)
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
            if all(shot.get(key) == value for key, value in pose.items()) and not record.get("note"):
                continue
            shot.update(pose)
            shot["confidence"] = "hand-fit"
            stamp = f" Pose set by hand in the engine ({str(record.get('saved_at', ''))[:10]})."
            if "Pose set by hand" not in shot.get("note", ""):
                shot["note"] = shot.get("note", "") + stamp
            user_note = str(record.get("note", "")).strip()
            if user_note and user_note not in shot.get("note", ""):
                shot["note"] = shot.get("note", "") + f" User's note: {user_note}"
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


def latest_notes():
    """The user's latest note per shot, from a P save or an X skip, with its time."""
    notes = {}
    for record in jsonl(MANUAL) + jsonl(NOT_FOUND):
        note = str(record.get("note", "")).strip()
        when = str(record.get("saved_at") or record.get("flagged_at") or "")
        if record.get("shot") and note and when >= notes.get(record["shot"], ("", ""))[1]:
            notes[record["shot"]] = (note, when)
    return notes


def run_worker(brief, label, extra, report):
    agent = os.path.expanduser("~/.claude/skills/deepseek-agents/ds-agent.ps1")
    with open(report, "w", encoding="utf-8") as out:
        subprocess.run(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", agent,
             "-TaskFile", brief, "-Dir", ROOT, "-Mode", "read", "-Label", label, "-Kind", "review",
             "-Effort", "high", "-MaxTurns", "20", "-TimeoutMinutes", "10", "-Extra", extra],
            cwd=ROOT, stdout=out, stderr=subprocess.STDOUT, check=False,
        )


def last_json_object(path):
    text = open(path, encoding="utf-8", errors="replace").read()
    found = None
    for match in re.finditer(r"\{[^{}]*\}", text, re.S):
        try:
            candidate = json.loads(match.group(0))
        except ValueError:
            continue
        if isinstance(candidate, dict) and candidate:
            found = candidate
    return found or {}


def resolve_notes(shots, world, grades, notes, batch):
    """Ask a quick worker which real place each noted shot shows. Returns {shot: editor id}."""
    items = []
    for shot in shots:
        note, _ = notes[shot["name"]]
        match = __import__("pose_ballpark").IMAGE.match(os.path.basename(shot["reference"]))
        place = match.group(2).replace("_", " ") if match else shot["name"]
        tried = grades.get(shot["name"], {}).get("places_tried", [])
        candidates = [name for name in world.names_like(place.split() + note.split()) if name not in tried]
        items.append({"shot": shot["name"], "reference": shot["reference"], "note": note,
                      "tried": tried, "candidates": candidates})
    if not items:
        return {}
    request = os.path.join(GRADING, batch + ".notes.json")
    save_json(request, items)
    report = os.path.join(GRADING, batch + ".notes-answer.txt")
    run_worker(NOTES_BRIEF, f"review-{batch}-notes",
               f"Input: local/reference/grading/{batch}.notes.json ({len(items)} shots).", report)
    answer = last_json_object(report)
    allowed = {item["shot"]: set(item["candidates"]) for item in items}
    return {name: place for name, place in answer.items() if place and place in allowed.get(name, ())}


def last_saves():
    latest = {}
    for record in jsonl(MANUAL):
        if record.get("shot"):
            latest[record["shot"]] = max(latest.get(record["shot"], ""), str(record.get("saved_at", "")))
    return latest


def refine():
    """Fine-tune `area only` shots; re-ball-park wrong, skipped, passed-over and unplaced ones."""
    sys.path.insert(0, os.path.join(ROOT, "tools", "research"))
    import pose_ballpark
    import pose_refine

    grades = load_json(GRADES, {})
    skipped, saved = flagged(grades), last_saves()
    world = pose_ballpark.World(pose_ballpark.DB)
    plans, files, parked = {}, {}, []
    notes = latest_notes()
    batch = "refine-" + datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    # Notes first: a shot going back to the ball-park with a note written since its last refresh
    # has the note read (a quick worker picks the real place from database names) before any
    # candidate is made, so a note like "wrong cell, it's the basement" moves the search.
    noted = []
    for path, data in shots_files():
        for shot in data["shots"]:
            entry = grades.get(shot["name"], {})
            note = notes.get(shot["name"])
            if (shot.get("reference") and note and note[1] > entry.get("refreshed_at", "")
                    and entry.get("grade") not in ACCEPTED and not entry.get("parked")):
                noted.append(shot)
    resolved = resolve_notes(noted, world, grades, notes, batch) if noted else {}
    for name, place in resolved.items():
        print(f"{name}: the note points at {place}")
    for path, data in shots_files():
        files[path] = data
        for shot in data["shots"]:
            name, entry = shot["name"], grades.get(shot["name"], {})
            if not shot.get("reference") or entry.get("parked") or entry.get("grade") in ACCEPTED:
                continue
            queued = entry.get("queued_at", "")
            passed_over = bool(queued) and saved.get(name, "") < queued and name not in skipped
            again = (
                name in skipped
                or passed_over
                or entry.get("grade") == "wrong"
                or shot.get("confidence") == "unplaced"
            )
            if not again and entry.get("grade") != "area only":
                continue
            if entry.get("rounds", 0) >= MAX_ROUNDS:
                entry["parked"] = True
                grades[name] = entry
                parked.append(name)
                continue
            # A note that points elsewhere turns a fine-tune into a re-ball-park.
            again = again or name in resolved
            if again:
                match = pose_ballpark.IMAGE.match(os.path.basename(shot["reference"]))
                place = match.group(2).replace("_", " ") if match else name
                key = pose_ballpark.HINTS.get(pose_ballpark.squash(place), pose_ballpark.squash(place))
                if name in resolved:
                    key = pose_ballpark.squash(resolved[name])
                tried = entry.get("places_tried", [])
                # The place the shot stands in now failed too, unless the note sends it back.
                current = shot_place(world, shot)
                if current and current not in tried and name not in resolved:
                    tried = tried + [current]
                candidates, used = pose_refine.ballpark_candidates(
                    shot, world, key, exclude=tried, interior=bool(match and match.group(1).lower() == "interior")
                )
                if used:
                    entry["places_tried"] = sorted(set(tried) | {used})
                    grades[name] = entry
                how = "re-ball-parked from several directions" + (f" at {used}" if used else "")
            else:
                candidates = pose_refine.finetune_candidates(shot)
                how = "fine-tuned around the graded pose"
            if candidates:
                plans[name] = (path, shot, candidates, how)
            else:
                entry["rounds"] = entry.get("rounds", 0) + 1
                entry["refreshed_at"] = utc_now()
                grades[name] = entry
                print(f"{name}: no place found for a new ball-park")
    by_size = {}
    for path, shot, candidates, _ in plans.values():
        size = (files[path]["width"], files[path]["height"])
        by_size.setdefault(size, []).extend(candidates)
    out_dir = os.path.join(GRADING, batch)
    paths = pose_refine.render(by_size, out_dir) if plans else {}
    stamp = utc_now()
    for name, (path, shot, candidates, how) in plans.items():
        best = pose_refine.pick(os.path.join(ROOT, shot["reference"]), candidates, paths)
        entry = grades.get(name, {})
        entry["rounds"] = entry.get("rounds", 0) + 1
        entry["refreshed_at"] = stamp
        if best:
            chosen = best[1]
            for field in ("worldspace_id", "interior_cell_id", "position", "yaw", "pitch"):
                shot[field] = chosen[field]
            shot["confidence"] = "auto-refined"
            entry.update(grade="unchecked", by="refine", at=today())
            print(f"{name}: {how}, picked {chosen['name'].split(pose_refine.SEP)[-1]} (score {best[0]:.3f})")
        else:
            print(f"{name}: {how}, nothing rendered")
        grades[name] = entry
    for path, data in files.items():
        save_json(os.path.join(ROOT, path), data)
    save_json(GRADES, grades)
    if parked:
        print(f"parked after {MAX_ROUNDS} rounds: {', '.join(parked)}")
    return len(plans)


def shot_place(world, shot):
    """The editor id of the interior cell or worldspace the shot stands in now, if known."""
    if shot.get("interior_cell_id"):
        for cell_id, editor, _ in world.cells:
            if cell_id == shot["interior_cell_id"]:
                return editor
    elif shot.get("worldspace_id") not in (None, 60):
        for world_id, editor, _ in world.worlds:
            if world_id == shot["worldspace_id"]:
                return editor
    return None


HISTORY = os.path.join(LOCAL, "history")


def rotate(before):
    """Move saves and skips older than `before` (UTC ISO) to local/reference/history/.

    The fit tool counts every shot with a line in manual-poses.jsonl or not-found.jsonl as done,
    so once a round has used them up they must leave those files, or the next pass would skip
    straight past every shot the user saved or skipped before. The lines are kept in history.
    """
    os.makedirs(HISTORY, exist_ok=True)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    for path, key in ((MANUAL, "saved_at"), (NOT_FOUND, "flagged_at")):
        if not os.path.exists(path):
            continue
        lines = [line for line in open(path, encoding="utf-8") if line.strip()]
        old = [line for line in lines if str(json.loads(line).get(key, "")) < before]
        new = [line for line in lines if line not in old]
        if not old:
            continue
        name = os.path.splitext(os.path.basename(path))[0]
        with open(os.path.join(HISTORY, f"{stamp}-{name}.jsonl"), "w", encoding="utf-8") as handle:
            handle.writelines(old)
        with open(path, "w", encoding="utf-8") as handle:
            handle.writelines(new)
        print(f"moved {len(old)} used line(s) of {os.path.basename(path)} to history")


def cycle():
    """One automatic round. Its result pictures are the last check's grading pages (Skyrim beside
    the engine, one row per shot), which cost nothing extra to show the user."""
    started = datetime.datetime.now()
    merge(False)
    batches = [check(True)]
    if refine():
        batches.append(check(True))
    build()
    # Everything the user saved or skipped before this round has been used; the next pass starts
    # clean (see rotate).
    rotate(utc_now())
    minutes = (datetime.datetime.now() - started).total_seconds() / 60
    print(f"round took {minutes:.1f} min")
    for batch in (b for b in batches if b):
        print("results pictures: " + os.path.join(GRADING, batch + "-*.jpg"))


def grade(name, verdict, by):
    if verdict not in GRADES_KNOWN | {"unchecked"}:
        sys.exit(f"unknown grade {verdict!r}; use one of {sorted(GRADES_KNOWN)}")
    grades = load_json(GRADES, {})
    set_grade(grades, name, verdict, by)
    save_json(GRADES, grades)


def status():
    grades = load_json(GRADES, {})
    skip = flagged(grades)
    counts = {}
    for _, data in shots_files():
        for shot in data["shots"]:
            entry = grades.get(shot["name"], {})
            if not shot.get("reference"):
                verdict = "no reference"
            elif grades.get(shot["name"], {}).get("parked"):
                verdict = "parked (no luck after automatic rounds)"
            elif shot["name"] in skip:
                verdict = "can't find (user)"
            elif shot.get("confidence") == "unplaced":
                verdict = "not placed yet"
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
    if command == "cycle":
        cycle()
    elif command == "check":
        check("--no-grade" not in argv)
    elif command == "build":
        build()
    elif command == "merge":
        merge("--dry-run" in argv)
    elif command == "import" and len(argv) >= 3:
        import_grades(argv[2], by)
    elif command == "grade" and len(argv) >= 4:
        grade(argv[2], argv[3], by)
    elif command == "rotate" and len(argv) >= 3:
        rotate(argv[2])
    elif command == "status":
        status()
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

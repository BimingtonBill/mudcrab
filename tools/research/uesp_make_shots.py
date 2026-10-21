#!/usr/bin/env python3
"""Build a shots file of candidate camera poses from a one-line-per-pose spec.

Pose refinement is iterative: render a handful of candidates, look at them side
by side, pick one.  Hand-writing the JSON for every round is the slow part, so
this clones a template shot (for its space, aspect and reference) and replaces
the pose and the name.

Usage:
    python tools/research/uesp_make_shots.py <out.json> --like <shot name>
        <spec file> [--shots <shots file>] [--note "text"]

The spec file has one candidate per line:

    <label> <x> <y> <z> <yaw> <pitch> [hfov]        # a whole line of its own
    <label> <from> <dyaw> <dpitch> [dhfov]          # `from` is a label above it

The second form is a *relative* pose: start from the candidate called `from`
and add the three offsets, which is how a pose is nudged once it is close.

Blank lines and lines starting with `#` are ignored.  The template's
`worldspace_id`/`interior_cell_id`, `reference`, width and height are copied to
every candidate; the label becomes the shot's `name`, so renders come out as
`<out dir>/<label>.png`.
"""

import json
import sys


def load_shots(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def parse_spec(lines):
    poses = {}
    order = []
    for number, line in enumerate(lines, 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        label = parts[0]
        if parts[1] in poses:
            base = poses[parts[1]]
            values = [float(v) for v in parts[2:]]
            if len(values) == 2:
                values.append(0.0)
            if len(values) != 3:
                raise SystemExit(f"{number}: expected 3 offsets after the base label")
            post = (
                [base[0][0], base[0][1], base[0][2]],
                base[1] + values[0],
                base[2] + values[1],
                base[3] + values[2],
            )
        else:
            values = [float(v) for v in parts[1:]]
            if len(values) == 5:
                values.append(75.0)
            if len(values) != 6:
                raise SystemExit(
                    f"{number}: expected x y z yaw pitch [hfov] or a base label and 3 offsets"
                )
            post = (values[0:3], values[3], values[4], values[5])
        if label in poses:
            raise SystemExit(f"{number}: duplicate label {label}")
        poses[label] = post
        order.append(label)
    return [(label, poses[label]) for label in order]


def main(argv):
    if len(argv) < 4:
        print(__doc__)
        return 2
    out = argv[1]
    options = {}
    positional = []
    index = 2
    while index < len(argv):
        if argv[index].startswith("--"):
            options[argv[index][2:]] = argv[index + 1]
            index += 2
        else:
            positional.append(argv[index])
            index += 1
    shots_path = options.get("shots", "tools/reference/uesp_shots_4x3.json")
    like = options.get("like")
    if like is None or not positional:
        print(__doc__)
        return 2
    file = load_shots(shots_path)
    template = next((shot for shot in file["shots"] if shot["name"] == like), None)
    if template is None:
        raise SystemExit(f"no shot named {like} in {shots_path}")
    with open(positional[0], encoding="utf-8") as handle:
        candidates = parse_spec(handle)
    shots = []
    for label, (position, yaw, pitch, hfov) in candidates:
        shot = {
            "name": label,
            "worldspace_id": template["worldspace_id"],
            "interior_cell_id": template["interior_cell_id"],
            "position": [round(v, 1) for v in position],
            "yaw": round(yaw % 360.0, 2),
            "pitch": round(max(-89.0, min(89.0, pitch)), 2),
            "hfov": hfov,
            "reference": template["reference"],
            "note": options.get("note", f"candidate for {like}"),
        }
        shots.append(shot)
    with open(out, "w", encoding="utf-8") as handle:
        json.dump(
            {"width": file["width"], "height": file["height"], "shots": shots},
            handle,
            indent=1,
        )
        handle.write("\n")
    print(f"{len(shots)} candidates -> {out} ({file['width']}x{file['height']})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

#!/usr/bin/env python3
"""Draw a candidate camera pose over a reference screenshot: where do the world's objects land?

Usage:
    python tools/research/uesp_pose_check.py <reference image> <out.png> <space> \
        <x> <y> <z> <yaw> <pitch> <hfov> [options]

    <space>            cell:<hex form id> or world:<decimal id> (where to take the objects from)
    --filter s         only label objects whose model path or editor id contains s (repeatable)
    --radius r         only consider objects within this distance (default 12000)
    --limit n          stop after n labels (default 40)
    --grep s           like --filter but also prints the matches on the console
    --list file        read the objects from a JSON list of {name, position} instead of the DB

The same camera convention as `uesp_pose_fit.py` and `docs/design/reference-shots.md`: Creation
units, Z up, yaw 0 = north = +Y turning clockwise, pitch positive looking down.  The horizon
line (pitch) and the frame centre are drawn, so a pose can be judged by eye and corrected.
"""

import json
import math
import sqlite3
import sys

from PIL import Image, ImageDraw, ImageFont

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"


def camera_vectors(yaw_deg, pitch_deg):
    yaw = math.radians(yaw_deg)
    pitch = math.radians(pitch_deg)
    forward = [math.sin(yaw) * math.cos(pitch), math.cos(yaw) * math.cos(pitch), -math.sin(pitch)]
    right = [forward[1], -forward[0], 0.0]
    norm = math.hypot(right[0], right[1])
    right = [right[0] / norm, right[1] / norm, 0.0] if norm > 1e-9 else [1.0, 0.0, 0.0]
    up = [
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    ]
    return forward, right, up


def project_all(position, yaw, pitch, hfov, width, height, objects):
    forward, right, up = camera_vectors(yaw, pitch)
    tan_x = math.tan(math.radians(hfov) / 2.0)
    tan_y = tan_x * height / width
    out = []
    for entry in objects:
        view = [entry["position"][i] - position[i] for i in range(3)]
        depth = sum(view[i] * forward[i] for i in range(3))
        if depth <= 20.0:
            continue
        x = sum(view[i] * right[i] for i in range(3)) / (depth * tan_x) * (width / 2.0) + width / 2.0
        y = height / 2.0 - sum(view[i] * up[i] for i in range(3)) / (depth * tan_y) * (height / 2.0)
        if 0 <= x < width and 0 <= y < height:
            entry = dict(entry)
            entry["pixel"] = (x, y)
            entry["depth"] = depth
            out.append(entry)
    out.sort(key=lambda item: item["depth"])
    return out


def font(size):
    for name in ("segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def load_objects(space, position, radius, filters, limit):
    kind, value = space.split(":", 1)
    column = "cell_id" if kind == "cell" else "worldspace_id"
    key = int(value, 16) if kind == "cell" else int(value)
    connection = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    query = (
        "select s.editor_id, s.model_path, r.pos_x, r.pos_y, r.pos_z,"
        " (s.bounds_max_y - s.bounds_min_y) * r.scale"
        ' from "references" r left join statics s on s.id = r.base_form_id'
        f" where r.{column} = ? and r.pos_x between ? and ? and r.pos_y between ? and ?"
        " and r.pos_z between ? and ?"
    )
    rows = connection.execute(
        query,
        (
            key,
            position[0] - radius,
            position[0] + radius,
            position[1] - radius,
            position[1] + radius,
            position[2] - radius,
            position[2] + radius,
        ),
    )
    objects = []
    for editor, model, x, y, z, height in rows:
        text = ((editor or "") + " " + (model or "")).lower()
        if filters and not any(needle.lower() in text for needle in filters):
            continue
        objects.append(
            {
                "name": (editor or (model or "").rsplit("/", 1)[-1]).strip("\x00")[:30],
                "model": model or "",
                "position": [x, y, z],
                "height": height or 0.0,
            }
        )
    return objects[:limit]


def main(argv):
    if len(argv) < 10:
        print(__doc__)
        return 2
    image_path, out_path, space = argv[1], argv[2], argv[3]
    position = [float(argv[4]), float(argv[5]), float(argv[6])]
    yaw, pitch, hfov = float(argv[7]), float(argv[8]), float(argv[9])
    filters, radius, limit, grep = [], 12000.0, 40, None
    objects_file = None
    debug = False
    index = 10
    while index < len(argv):
        option = argv[index]
        if option == "--filter":
            filters.append(argv[index + 1])
            index += 2
        elif option == "--grep":
            grep = argv[index + 1]
            filters.append(grep)
            index += 2
        elif option == "--radius":
            radius = float(argv[index + 1])
            index += 2
        elif option == "--limit":
            limit = int(argv[index + 1])
            index += 2
        elif option == "--list":
            objects_file = argv[index + 1]
            index += 2
        elif option == "--debug":
            debug = True
            index += 1
        else:
            raise SystemExit(f"unknown option {option!r}")

    if objects_file:
        with open(objects_file, "r", encoding="utf-8") as handle:
            objects = json.load(handle)
    else:
        objects = load_objects(space, position, radius, filters, limit * 40)

    image = Image.open(image_path).convert("RGB")
    width, height = image.size
    draw = ImageDraw.Draw(image)
    placed = project_all(position, yaw, pitch, hfov, width, height, objects)
    print(f"  {len(objects)} objects loaded from {space}, {len(placed)} inside the frame")
    if debug:
        forward, _, _ = camera_vectors(yaw, pitch)
        nearest = sorted(
            objects,
            key=lambda entry: abs(
                sum((entry["position"][i] - position[i]) * forward[i] for i in range(3))
            ),
        )[:6]
        for entry in nearest:
            view = [entry["position"][i] - position[i] for i in range(3)]
            depth = sum(view[i] * forward[i] for i in range(3))
            print(
                f"    {entry['name']:30.30} at ({entry['position'][0]:.0f},"
                f"{entry['position'][1]:.0f},{entry['position'][2]:.0f}) depth {depth:.0f}"
            )
    if grep:
        print(f"  {len(placed)} objects in frame match {filters!r}")
    for entry in placed[:limit]:
        x, y = entry["pixel"]
        draw.ellipse([x - 5, y - 5, x + 5, y + 5], outline=(255, 40, 40), width=2)
        label = f"{entry['name']} d{entry['depth']:.0f}"
        left, top, right, bottom = draw.textbbox((x + 7, y - 7), label, font=font(15))
        draw.rectangle([left - 1, top - 1, right + 1, bottom + 1], fill=(0, 0, 0))
        draw.text((x + 7, y - 7), label, fill=(255, 120, 120), font=font(15))
        if grep:
            print(
                f"    {entry['name']:32.32} pixel ({x:6.0f},{y:6.0f}) depth {entry['depth']:7.0f}"
            )
    # Centre cross and the horizon (the line the camera's own horizontal plane maps to).
    forward, right, up = camera_vectors(yaw, pitch)
    horizon_y = height / 2.0 + math.tan(math.radians(pitch)) / (
        math.tan(math.radians(hfov) / 2.0) * height / width
    ) * (height / 2.0)
    draw.line([(0, horizon_y), (width, horizon_y)], fill=(80, 200, 255), width=2)
    draw.line([(width / 2, 0), (width / 2, height)], fill=(255, 255, 0), width=1)
    draw.line([(0, height / 2), (width, height / 2)], fill=(255, 255, 0), width=1)
    image.save(out_path)
    print(
        f"  wrote {out_path}: pose ({position[0]:.0f},{position[1]:.0f},{position[2]:.0f}) "
        f"yaw {yaw:.1f} pitch {pitch:.1f} hfov {hfov:.1f}, "
        f"{min(len(placed), limit)} of {len(placed)} in-frame objects labelled, "
        f"horizon at y={horizon_y:.0f}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

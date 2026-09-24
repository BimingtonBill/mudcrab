#!/usr/bin/env python3
"""Automatic pose improvement for the reference-pose loop: fine-tune and re-ball-park.

Used by `pose_queue.py cycle` (docs/design/reference-pose-pipeline.md), not usually run alone.

- **Fine-tune** (a shot graded `area only`): candidates are small moves around the current pose
  (turn left/right, look up/down, step forward/back/sideways, rise/sink).
- **Re-ball-park** (a shot graded `wrong`, skipped with X, passed over with N, or never placed):
  candidates look at the place `pose_ballpark.py` finds for the name, from eight compass
  directions, or, for an interior (looked for indoors only), across the room from four sides.

Every candidate is rendered in one engine run per shots file (off-screen), and the one whose
edge structure best matches the Skyrim screenshot is kept. The score compares outlines, not
colours, since the engine's lighting is not Skyrim's.
"""

import json
import math
import os
import subprocess

import numpy
from PIL import Image, ImageFilter, ImageOps

ENGINE = r"C:\Modding\wkj-tools\engine-portal-fit.exe"
ASSETS = r"C:\Modding\SkyrimConverted"
SEP = "~"


def forward(yaw, pitch):
    yaw, pitch = math.radians(yaw), math.radians(pitch)
    return (math.sin(yaw) * math.cos(pitch), math.cos(yaw) * math.cos(pitch), -math.sin(pitch))


def aim(camera, target):
    dx, dy, dz = (target[i] - camera[i] for i in range(3))
    yaw = math.degrees(math.atan2(dx, dy)) % 360.0
    pitch = -math.degrees(math.atan2(dz, math.hypot(dx, dy)))
    return round(yaw, 1), round(max(-80.0, min(80.0, pitch)), 1)


def variant(shot, suffix, position, yaw, pitch):
    candidate = dict(shot)
    candidate["name"] = f"{shot['name']}{SEP}{suffix}"
    candidate["position"] = [round(value, 1) for value in position]
    candidate["yaw"] = round(yaw % 360.0, 1)
    candidate["pitch"] = round(max(-85.0, min(85.0, pitch)), 1)
    return candidate


def finetune_candidates(shot):
    """The current pose and six small moves around it (turn, look up/down, step)."""
    x, y, z = shot["position"]
    yaw, pitch = shot["yaw"], shot["pitch"]
    fx, fy, _ = forward(yaw, 0.0)
    side = (fy, -fx)
    step = 300.0 if shot.get("interior_cell_id") else 600.0
    out = [variant(shot, "keep", (x, y, z), yaw, pitch)]
    for delta in (-10, 10):
        out.append(variant(shot, f"yaw{delta:+d}", (x, y, z), yaw + delta, pitch))
    for delta in (-7, 7):
        out.append(variant(shot, f"pitch{delta:+d}", (x, y, z), yaw, pitch + delta))
    for name, (dx, dy, dz) in {
        "fwd": (fx * step, fy * step, 0),
        "back": (-fx * step, -fy * step, 0),
    }.items():
        out.append(variant(shot, name, (x + dx, y + dy, z + dz), yaw, pitch))
    return out


def ballpark_candidates(shot, world, place_key, exclude=(), interior=False):
    """Views of the named place from several directions, and the place's editor id.

    `exclude` holds editor ids already tried and found wrong; the lookup skips them, so a
    re-ball-park moves on to the next-best place instead of circling the same one. Returns
    ([], None) when nothing is left to try.
    """
    from pose_ballpark import squash

    world.exclude = frozenset(squash(name) for name in exclude)
    found = None
    # An interior screenshot is only ever looked for indoors: otherwise `Alftand` in an interior
    # shot's name finds Alftand's outdoor entrance.
    lookups = (world.interior,) if interior else (world.city, world.entrance, world.interior, world.worldspace)
    for lookup in lookups:
        found = lookup(place_key)
        if found:
            break
    world.exclude = frozenset()
    if not found:
        return [], None
    pose, how = found
    place = how.split()[-1]
    base = dict(shot)
    base["worldspace_id"], base["interior_cell_id"] = pose["worldspace_id"], pose["interior_cell_id"]
    # The ball-park pose looks at its target from a fixed offset; recover the target point.
    cx, cy, cz = pose["position"]
    fx, fy, fz = forward(pose["yaw"], pose["pitch"])
    reach = 700.0 if pose["interior_cell_id"] else 2500.0
    target = (cx + fx * reach, cy + fy * reach, cz + fz * reach)
    out = []
    if pose["interior_cell_id"]:
        for index, heading in enumerate((0, 90, 180, 270)):
            h = math.radians(heading)
            camera = (target[0] - math.sin(h) * 600, target[1] - math.cos(h) * 600, target[2])
            out.append(variant(base, f"room{index}", camera, *aim(camera, target)))
        return out, place
    for distance, rise in ((3500.0, 1300.0),):
        for heading in range(0, 360, 45):
            h = math.radians(heading)
            camera = (target[0] - math.sin(h) * distance, target[1] - math.cos(h) * distance, target[2] + rise)
            out.append(variant(base, f"d{int(distance)}h{heading}", camera, *aim(camera, target)))
    return out, place


ENGINES = 3


def render(candidates_by_file, out_dir):
    """Render every candidate; returns {candidate name: png path}.

    Candidates are split by shot across up to ENGINES engine processes running at once (each
    engine's time is mostly streaming and settling, not the GPU), so a shot's candidates stay
    together in one engine and reuse its streamed cells.
    """
    os.makedirs(out_dir, exist_ok=True)
    jobs = []
    for (width, height), candidates in candidates_by_file.items():
        by_shot = {}
        for candidate in candidates:
            by_shot.setdefault(candidate["name"].split(SEP)[0], []).append(candidate)
        groups = [[] for _ in range(ENGINES)]
        for index, shot_candidates in enumerate(sorted(by_shot.values(), key=len, reverse=True)):
            min(groups, key=len).extend(shot_candidates)
        for number, group in enumerate(g for g in groups if g):
            tag = f"{width}x{height}-{number}"
            shots_path = os.path.join(out_dir, f"candidates-{tag}.json")
            with open(shots_path, "w", encoding="utf-8") as handle:
                json.dump({"width": width, "height": height, "shots": group}, handle, indent=1)
            jobs.append((tag, shots_path, os.path.join(out_dir, tag), group))
    running = []
    for tag, shots_path, renders, group in jobs:
        log = open(os.path.join(out_dir, f"candidates-{tag}.log"), "w")
        process = subprocess.Popen([ENGINE, "--assets", ASSETS, "--shots", shots_path, "--shots-out", renders],
                                   stdout=log, stderr=subprocess.STDOUT)
        running.append((process, log))
        if len(running) >= ENGINES:
            process, log = running.pop(0)
            process.wait()
            log.close()
    for process, log in running:
        process.wait()
        log.close()
    paths = {}
    for _, _, renders, group in jobs:
        for candidate in group:
            png = os.path.join(renders, candidate["name"] + ".png")
            if os.path.exists(png):
                paths[candidate["name"]] = png
    return paths


def edges(path):
    image = ImageOps.autocontrast(Image.open(path).convert("L").resize((160, 120)))
    array = numpy.asarray(image.filter(ImageFilter.FIND_EDGES).filter(ImageFilter.GaussianBlur(2)), dtype=numpy.float32)
    array -= array.mean()
    norm = numpy.linalg.norm(array)
    return array / norm if norm else array


def score(reference, render_path):
    """Correlation of the two pictures' blurred outlines: 1 is identical structure."""
    return float((edges(reference) * edges(render_path)).sum())


def pick(reference, candidates, paths):
    """The best-scoring rendered candidate, or None when none rendered."""
    best = None
    ref = edges(reference)
    for candidate in candidates:
        png = paths.get(candidate["name"])
        if not png:
            continue
        value = float((ref * edges(png)).sum())
        if best is None or value > best[0]:
            best = (value, candidate)
    return best

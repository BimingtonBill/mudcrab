#!/usr/bin/env python3
"""Fit a camera pose to a reference screenshot from landmark positions and their pixels.

Usage:
    python tools/research/uesp_pose_fit.py <landmarks.json> [--overlay <dir>] [--free-hfov]

The input file is a JSON list of shots:

    [
      {
        "name": "SR-place-Alftand_02",
        "width": 1400, "height": 1050,
        "reference": "local/reference/uesp/SR-place-Alftand_02.jpg",
        "position": [77900.0, 76200.0, -5100.0],   # start guess, Creation units
        "yaw": 30.0, "pitch": 0.0, "hfov": 75.0,
        "fixed": ["hfov"],                          # any of x y z yaw pitch hfov
        "landmarks": [
          {"name": "tower A roof", "position": [77992, 78336, -5000], "pixel": [431, 300]}
        ]
      }
    ]

`pixel` is (x, y) in the reference image, in pixels from the top-left.  The fit minimises
the reprojection error in pixels of every landmark, with a light penalty keeping the pose
near the start guess so an under-constrained shot does not wander.  Angles are degrees.

Camera convention: Creation units, Z up.  Yaw 0 looks north (+Y) and turns clockwise seen
from above, as the design document `docs/design/reference-shots.md` fixes it; pitch is
positive looking down.  The camera up vector is the world Z axis rotated with the view.
"""

import json
import math
import sys

import numpy as np
from PIL import Image, ImageDraw

HFOV_RANGE = (60.0, 95.0)
PITCH_RANGE = (-89.0, 89.0)
GRAVITY = 0.02  # weight of "stay near the start guess", in pixels per unit of change


def camera_vectors(yaw_deg, pitch_deg):
    yaw = math.radians(yaw_deg)
    pitch = math.radians(pitch_deg)
    forward = np.array(
        [math.sin(yaw) * math.cos(pitch), math.cos(yaw) * math.cos(pitch), -math.sin(pitch)]
    )
    right = np.cross(forward, np.array([0.0, 0.0, 1.0]))
    norm = np.linalg.norm(right)
    if norm < 1e-6:
        right = np.array([1.0, 0.0, 0.0])
    else:
        right = right / norm
    up = np.cross(right, forward)
    return forward, right, up


def project(position, yaw, pitch, hfov, width, height, points):
    forward, right, up = camera_vectors(yaw, pitch)
    tan_x = math.tan(math.radians(hfov) / 2.0)
    tan_y = tan_x * height / width
    pixels = []
    for point in points:
        view = np.asarray(point, dtype=float) - position
        depth = float(view @ forward)
        if depth <= 1.0:
            pixels.append(None)
            continue
        x = float(view @ right) / (depth * tan_x) * (width / 2.0) + width / 2.0
        y = height / 2.0 - float(view @ up) / (depth * tan_y) * (height / 2.0)
        pixels.append((x, y))
    return pixels


def fit_shot(shot, free_hfov):
    width = float(shot["width"])
    height = float(shot["height"])
    start = np.array(
        [
            *shot["position"],
            shot["yaw"],
            shot["pitch"],
            shot.get("hfov", 75.0),
        ],
        dtype=float,
    )
    points = [np.array(landmark["position"], dtype=float) for landmark in shot["landmarks"]]
    pixels = [np.array(landmark["pixel"], dtype=float) for landmark in shot["landmarks"]]
    weights = np.array([landmark.get("weight", 1.0) for landmark in shot["landmarks"]])
    fixed = set(shot.get("fixed", []))
    free = [index for index, name in enumerate(("x", "y", "z", "yaw", "pitch", "hfov"))
            if name not in fixed]
    if not free_hfov and "hfov" not in fixed:
        free.remove(5)  # hfov stays at the start value unless asked
    scale = np.array([100.0, 100.0, 100.0, 2.0, 2.0, 5.0])

    def unpack(parameters):
        full = start.copy()
        full[free] = parameters
        return full

    def residuals(parameters):
        pose = unpack(parameters)
        projected = project(
            pose[:3], pose[3], pose[4], pose[5], width, height, points
        )
        out = []
        for index, pixel in enumerate(projected):
            if pixel is None:
                out.extend([500.0 * weights[index], 500.0 * weights[index]])
            else:
                out.extend(
                    [
                        (pixel[0] - pixels[index][0]) * weights[index],
                        (pixel[1] - pixels[index][1]) * weights[index],
                    ]
                )
        out.extend(GRAVITY * scale[free] * (unpack(parameters)[free] - start[free]))
        return np.array(out)

    from scipy.optimize import least_squares

    lower = np.full(len(free), -np.inf)
    upper = np.full(len(free), np.inf)
    for slot, index in enumerate(free):
        if index == 4:
            lower[slot], upper[slot] = PITCH_RANGE
        if index == 5:
            lower[slot], upper[slot] = HFOV_RANGE
    result = least_squares(residuals, start[free], bounds=(lower, upper), xtol=1e-10)
    pose = unpack(result.x)
    projected = project(pose[:3], pose[3], pose[4], pose[5], width, height, points)
    errors = []
    for index, pixel in enumerate(projected):
        if pixel is None:
            errors.append(float("inf"))
        else:
            errors.append(
                float(np.hypot(pixel[0] - pixels[index][0], pixel[1] - pixels[index][1]))
            )
    rms = math.sqrt(float(np.mean(np.square([min(e, 500.0) for e in errors]))))
    return pose, projected, errors, rms, free, result.cost


def search_shot(shot, free_hfov, trials=400, seed=1):
    """Multi-start fit: sample starts in the shot's search box and keep the best pose.

    A single least-squares run from a hand-made guess gets stuck when the guess is on the
    wrong side of the site, so a shot may give `search_box` = [x0, x1, y0, y1, z0, z1] and
    the fitter restarts from random poses inside it.
    """
    import random

    box = shot.get("search_box")
    if not box:
        pose, projected, errors, rms, _, _ = fit_shot(shot, free_hfov)
        return pose, projected, errors, rms
    random.seed(seed)
    best = None
    for _ in range(trials):
        guess = dict(shot)
        guess["position"] = [
            random.uniform(box[0], box[1]),
            random.uniform(box[2], box[3]),
            random.uniform(box[4], box[5]),
        ]
        guess["yaw"] = random.uniform(0.0, 360.0)
        guess["pitch"] = random.uniform(-30.0, 60.0)
        try:
            pose, projected, errors, rms, _, _ = fit_shot(guess, free_hfov)
        except Exception:
            continue
        if best is None or rms < best[3]:
            best = (pose, projected, errors, rms)
    return (*best, None, None)


def assign_shot(shot, free_hfov, keep=5):
    """Try every assignment of the observed pixels to the candidate objects.

    Returns the best `keep` (rms, pose, assignment names, errors) tuples.  A shot that names
    its landmarks directly is fitted once instead.
    """
    import itertools

    if "pool" not in shot:
        pose, projected, errors, rms, _, _ = fit_shot(shot, free_hfov)
        return [(rms, pose, [landmark["name"] for landmark in shot["landmarks"]], errors)]
    results = []
    names = [point["name"] for point in shot["pixels"]]
    for choice in itertools.permutations(shot["pool"], len(shot["pixels"])):
        candidate = dict(shot)
        candidate["landmarks"] = [
            {"name": f"{observed['name']}<-{target['name']}", "position": target["position"],
             "pixel": observed["pixel"]}
            for observed, target in zip(shot["pixels"], choice)
        ]
        try:
            pose, projected, errors, rms, _, _ = fit_shot(candidate, free_hfov)
        except Exception:  # a degenerate assignment can make the fitter unhappy
            continue
        if not math.isfinite(rms):
            continue
        results.append((rms, pose, [landmark["name"] for landmark in candidate["landmarks"]], errors))
    results.sort(key=lambda item: item[0])
    return results[:keep]


def overlay(shot, pose, projected, path):
    image = Image.open(shot["reference"]).convert("RGB")
    draw = ImageDraw.Draw(image)
    for landmark, pixel, target in zip(shot["landmarks"], projected, shot["landmarks"]):
        want = target["pixel"]
        draw.line([tuple(want), pixel], fill=(255, 60, 60), width=3)
        draw.ellipse(
            [want[0] - 8, want[1] - 8, want[0] + 8, want[1] + 8], outline=(255, 255, 0), width=3
        )
        draw.ellipse(
            [pixel[0] - 8, pixel[1] - 8, pixel[0] + 8, pixel[1] + 8],
            outline=(60, 255, 60),
            width=3,
        )
    image.save(path)


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    with open(argv[1], "r", encoding="utf-8") as handle:
        shots = json.load(handle)
    overlay_dir = None
    if "--overlay" in argv:
        overlay_dir = argv[argv.index("--overlay") + 1]
    free_hfov = "--free-hfov" in argv
    print(f"{'shot':34} {'x':>8} {'y':>8} {'z':>8} {'yaw':>7} {'pitch':>7} {'hfov':>6}  rms  err")
    for shot in shots:
        if "pool" in shot:
            for rms, pose, names, errors in assign_shot(shot, free_hfov):
                print(
                    f"{shot['name']:34} {pose[0]:8.0f} {pose[1]:8.0f} {pose[2]:8.0f} "
                    f"{pose[3]:7.1f} {pose[4]:7.1f} {pose[5]:6.1f} {rms:5.1f} "
                    + " | ".join(names)
                )
            continue
        pose, projected, errors, rms, free, cost = (
            search_shot(shot, free_hfov)
            if shot.get("search_box")
            else fit_shot(shot, free_hfov)
        )
        if overlay_dir and shot.get("reference") and projected is not None:
            import os

            os.makedirs(overlay_dir, exist_ok=True)
            overlay(shot, pose, projected, f"{overlay_dir}/{shot['name']}.jpg")
        print(
            f"{shot['name']:34} {pose[0]:8.0f} {pose[1]:8.0f} {pose[2]:8.0f} "
            f"{pose[3]:7.1f} {pose[4]:7.1f} {pose[5]:6.1f} {rms:5.1f} "
            + " ".join(f"{error:4.0f}" for error in errors)
        )
        print(
            "    fixed: "
            + (",".join(shot.get("fixed", [])) or "-")
            + "   landmarks: "
            + ", ".join(
                f"{landmark['name']}({error:.0f}px)"
                for landmark, error in zip(shot["landmarks"], errors)
            )
        )
        if overlay_dir and shot.get("reference"):
            import os

            os.makedirs(overlay_dir, exist_ok=True)
            overlay(
                shot,
                pose,
                projected,
                f"{overlay_dir}/{shot['name']}.jpg",
            )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

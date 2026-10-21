#!/usr/bin/env python3
"""Enumerate tower assignments for the Alftand exterior references as fit input.

The exterior screenshots show three or four Dwemer towers whose outlines are hard to tell
apart, so this writes one fit input per plausible assignment of the observed tower features to
the database's towers.  Feed the result to `uesp_pose_fit.py` and keep the lowest error.

Usage:
    python tools/research/uesp_tower_assign.py <out.json>

Tower positions come from `skyrim_world.db` (see `docs/research/reference-shot-poses.md`); the
roof peak is the tower's origin plus the roof piece's top, and the arch centre is the arch
piece's origin, both scaled by the reference's `scale`.
"""

import itertools
import json
import sys

# Roof peaks, Creation units: origin + (roof piece top) * scale.
ROOFS = {
    "A": [77992, 78336, -4842],  # 1x4 + Roof02, scale 1.25
    "B": [76240, 77035, -3934],  # 1x4 + Roof02, scale 1.25
    "C": [80001, 75827, -3081],  # 1x4 + Arch1x4 + Roof02 (the lift tower), scale 1.0
    "D": [75982, 78972, -7018],  # Tower02 + ArchAP + Roof02 (the tower with the AlftandZCell door)
    "E": [76101, 79918, -8309],  # Tower01 + Roof01, scale 1.25
    "F": [76057, 79974, -8213],  # Tower01 + Roof01
    "W": [73422, 78436, -4617],  # 1x3 + Arch01 + Roof01SnowHeavy (the other AlftandZCell door)
}
ARCHES = {
    "C": [80001, 75827, -4169],
    "W": [73422, 78436, -5300],
}
BOX = [70000, 95000, 60000, 85000, -9000, 0]


def shot(name, reference, pixels, assignment, box=BOX):
    landmarks = []
    for pixel, key in zip(pixels, assignment):
        if key in ARCHES:
            landmarks.append(
                {"name": f"pixel<-arch{key}", "position": ARCHES[key], "pixel": pixel}
            )
        else:
            landmarks.append(
                {"name": f"pixel<-roof{key}", "position": ROOFS[key], "pixel": pixel}
            )
    return {
        "name": name,
        "width": 1400,
        "height": 1050,
        "hfov": 75.0,
        "fixed": ["hfov"],
        "reference": reference,
        "search_box": box,
        "position": [78000, 75000, -4500],
        "yaw": 0.0,
        "pitch": 0.0,
        "landmarks": landmarks,
    }


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    shots = []
    # 03: big tower roof, its arch, a second tower roof, the background tower roof.
    pixels = [[1000, 110], [1010, 410], [470, 505], [740, 240]]
    for big in ("C", "W"):
        for others in itertools.permutations([key for key in ROOFS if key != big], 2):
            shots.append(
                shot(
                    f"03_{big}_{others[0]}{others[1]}",
                    "local/reference/uesp/SR-place-Alftand_03.jpg",
                    pixels,
                    [big, big, *others],
                )
            )
    # 02: the same big tower, the small tower, and the round tower below the bridge.
    pixels = [[1085, 283], [1120, 520], [540, 197], [535, 600]]
    for big in ("C", "W"):
        for others in itertools.permutations([key for key in ROOFS if key != big], 2):
            shots.append(
                shot(
                    f"02_{big}_{others[0]}{others[1]}",
                    "local/reference/uesp/SR-place-Alftand_02.jpg",
                    pixels,
                    [big, big, *others],
                )
            )
    with open(argv[1], "w", encoding="utf-8") as handle:
        json.dump(shots, handle, indent=1)
    print(f"wrote {argv[1]}: {len(shots)} candidate assignments")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

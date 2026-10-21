#!/usr/bin/env python3
"""Measure how well Skyrim reference shots line up with the engine's render of the same shots file.

    python tools/capture/align_check.py <skyrim dir> <engine dir>

For every PNG name in both folders, matches SIFT features and fits a scale, rotation and shift (RANSAC) that
maps the Skyrim picture onto the engine's. A perfect alignment is scale 1.0000, rotation 0, shift 0 px.
Pick shots with rigid, textured geometry both renderers draw (buildings, walls): grass and terrain differ too
much to match. Needs opencv-python and numpy.
"""
import math
import sys
from pathlib import Path

import cv2
import numpy as np


def fit(skyrim: Path, engine: Path):
    a, b = cv2.imread(str(skyrim), 0), cv2.imread(str(engine), 0)
    sift = cv2.SIFT_create(6000)
    ka, da = sift.detectAndCompute(a, None)
    kb, db = sift.detectAndCompute(b, None)
    good = [m for m, n in cv2.BFMatcher().knnMatch(da, db, k=2) if m.distance < 0.8 * n.distance]
    if len(good) < 8:
        return len(good), 0, None
    pa = np.float32([ka[g.queryIdx].pt for g in good])
    pb = np.float32([kb[g.trainIdx].pt for g in good])
    matrix, mask = cv2.estimateAffinePartial2D(pa, pb, method=cv2.RANSAC, ransacReprojThreshold=3.0)
    return len(good), int(mask.sum()) if matrix is not None else 0, matrix


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    skyrim, engine = Path(sys.argv[1]), Path(sys.argv[2])
    for png in sorted(skyrim.glob("*.png")):
        other = engine / png.name
        if not other.exists():
            continue
        matches, inliers, m = fit(png, other)
        if m is None:
            print(f"{png.name}: {matches} matches, no fit")
            continue
        scale = math.hypot(m[0, 0], m[1, 0])
        rotation = math.degrees(math.atan2(m[1, 0], m[0, 0]))
        print(f"{png.name}: {matches} matches, {inliers} inliers, scale {scale:.4f}, rotation {rotation:.2f} deg, shift ({m[0, 2]:.1f}, {m[1, 2]:.1f}) px")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

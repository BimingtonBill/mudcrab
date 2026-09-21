"""Measure a layer boundary: step length and edge sharpness (defect A).

Usage:
    python tools/research/terrain_steps.py <screenshot.png> <x0> <y0> <x1> <y1> <bright|dark> [threshold]

Traces, for every column in the box, the first row that crosses the threshold; the run
lengths of consecutive equal rows are the staircase steps. It also prints a few vertical
luminance profiles across the boundary so the transition width (1 px = nearest neighbour,
several px = interpolated) can be read off.

The acceptance camera is 20000 units above the ground with the default 45 degree FOV, so
one pixel is about 18.4 world units at the terrain below the camera
(tools/research/terrain_world_map.py), and a LAND quadrant samples weights every 128 units.
"""

import sys

import numpy as np
from PIL import Image

UNITS_PER_PIXEL = 18.43


def trace(rgb, box, bright, threshold):
    y0, y1, x0, x1 = box
    patch = rgb[y0:y1, x0:x1].mean(axis=2)
    mask = patch > threshold if bright else patch < threshold
    edges = np.full(mask.shape[1], np.nan)
    for column in range(mask.shape[1]):
        rows = np.flatnonzero(mask[:, column])
        if rows.size:
            edges[column] = rows[0]
    return edges


def main():
    path = sys.argv[1]
    x0, y0, x1, y1 = (int(value) for value in sys.argv[2:6])
    box = (y0, y1, x0, x1)
    bright = sys.argv[6] == "bright"
    threshold = float(sys.argv[7]) if len(sys.argv) > 7 else (200.0 if bright else 60.0)
    rgb = np.asarray(Image.open(path).convert("RGB")).astype(np.float32)
    edges = trace(rgb, box, bright, threshold)
    finite = edges[np.isfinite(edges)]
    if finite.size < 4:
        print("no boundary found in the box")
        return
    runs = []
    run = 1
    for a, b in zip(finite, finite[1:]):
        if abs(a - b) < 0.5:
            run += 1
        else:
            runs.append((run, int(a), int(b)))
            run = 1
    runs.append((run, int(finite[-2]), int(finite[-1])))
    print(f"{path} box {box} bright={bright} threshold={threshold}")
    print(f"  {finite.size} boundary columns, {len(runs)} steps")
    print(f"  step lengths (px): {[r[0] for r in runs]}")
    print(f"  step heights (px): {[abs(r[2] - r[1]) for r in runs]}")
    lengths = np.array([r[0] for r in runs], dtype=np.float32)
    print(f"  median step length {np.median(lengths):.1f} px = "
          f"{np.median(lengths) * UNITS_PER_PIXEL:.0f} world units")
    print(f"  mean step length {lengths.mean():.1f} px = "
          f"{lengths.mean() * UNITS_PER_PIXEL:.0f} world units")
    heights = np.array([abs(r[2] - r[1]) for r in runs], dtype=np.float32)
    print(f"  median step height {np.median(heights):.1f} px = "
          f"{np.median(heights) * UNITS_PER_PIXEL:.0f} world units")
    # Transition sharpness: vertical profiles through the boundary.
    x0, x1 = box[2], box[3]
    for column in range(x0 + 5, x1 - 5, max(1, (x1 - x0) // 6)):
        relative = int(edges[column - x0]) if np.isfinite(edges[column - x0]) else None
        if relative is None:
            continue
        row = box[0] + relative
        profile = rgb[row - 6 : row + 7, column].mean(axis=1)
        print(f"  column {column}: luminance across the edge "
              f"{[round(float(v)) for v in profile]}")


if __name__ == "__main__":
    main()

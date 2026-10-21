"""Map acceptance-screenshot pixels to Skyrim world coordinates and measure the defects.

Camera model: crates/engine/src/app.rs:1259-1273 (perspective camera above the cell centre,
bevy default 45 degree vertical FOV), run log logs/rural-run-1.log.

Usage:
    python tools/research/terrain_world_map.py <screenshot.png> [ground_height]

Reports:
  * the world X (or Z) of every detected vertical (horizontal) line, so the period can be
    read off in world units rather than pixels;
  * the world-space step length of the boundary of a bright (snow) patch.
"""

import sys

import numpy as np
from PIL import Image

CAMERA = np.array([2048.0, 14016.0, -1048.0])
TARGET = np.array([2048.0, -5984.0, -2048.0])
SIZE = (1600.0, 900.0)
FOV_Y = 0.7853981633974483

FORWARD = (TARGET - CAMERA) / np.linalg.norm(TARGET - CAMERA)
RIGHT = np.cross(FORWARD, np.array([0.0, 1.0, 0.0]))
RIGHT /= np.linalg.norm(RIGHT)
UP = np.cross(RIGHT, FORWARD)
TAN_Y = np.tan(FOV_Y / 2.0)
TAN_X = TAN_Y * (SIZE[0] / SIZE[1])


def ray(pixel_x, pixel_y):
    ndc_x = (pixel_x + 0.5 - SIZE[0] / 2.0) / (SIZE[0] / 2.0)
    ndc_y = (SIZE[1] / 2.0 - pixel_y - 0.5) / (SIZE[1] / 2.0)
    direction = FORWARD + RIGHT * (ndc_x * TAN_X) + UP * (ndc_y * TAN_Y)
    return direction / np.linalg.norm(direction)


def ground_hit(pixel_x, pixel_y, height):
    """Ray/plane intersection, returning (world_x, world_z) or None."""
    direction = ray(pixel_x, pixel_y)
    if abs(direction[1]) < 1e-6:
        return None
    t = (height - CAMERA[1]) / direction[1]
    if t <= 0.0:
        return None
    point = CAMERA + direction * t
    return point[0], point[2]


def project(world_x, world_z, height):
    point = np.array([world_x, height, world_z])
    v = point - CAMERA
    t = float(v @ FORWARD)
    x = float(v @ RIGHT) / (t * TAN_X) * (SIZE[0] / 2.0) + SIZE[0] / 2.0
    y = SIZE[1] / 2.0 - float(v @ UP) / (t * TAN_Y) * (SIZE[1] / 2.0)
    return x, y


def profile_lines(profile, label, world_of, period_hint):
    smooth = np.convolve(profile, np.ones(3) / 3, mode="same")
    baseline = np.convolve(smooth, np.ones(101) / 101, mode="same")
    residual = smooth - baseline
    std = residual.std()
    hits = []
    for index in range(3, residual.size - 3):
        if (
            residual[index] < -0.8 * abs(std)
            and residual[index] <= residual[index - 1]
            and residual[index] <= residual[index + 1]
        ):
            if not hits or index - hits[-1] > 20:
                hits.append(index)
    rows = []
    for index in hits:
        world = world_of(index)
        if world is None:
            continue
        rows.append((index, world, round(world / period_hint, 3) if period_hint else None))
    print(f"  {label}: {len(rows)} lines")
    for index, world, ratio in rows:
        print(f"    pixel {index:5d} -> world {world:10.1f} = {ratio} x {period_hint:.0f}")
    return rows


def main():
    path = sys.argv[1]
    height = float(sys.argv[2]) if len(sys.argv) > 2 else -5984.0
    rgb = np.asarray(Image.open(path).convert("RGB")).astype(np.float32)
    luminance = rgb.mean(axis=2)
    print(f"{path}: {rgb.shape[1]}x{rgb.shape[0]}, ground plane y={height}")
    scale = 64.0 / (project(2112.0, -2048.0, height)[0] - project(2048.0, -2048.0, height)[0])
    print(f"  world units per pixel at the cell centre: {scale:.2f}")
    print("  predicted pixel x of cell boundaries  [2048 + k*4096]:")
    print("    " + " ".join(f"{project(2048.0 + k * 4096.0, -2048.0, height)[0]:.0f}" for k in range(-3, 4)))
    print("  predicted pixel x of quadrant boundaries [2048 + k*2048]:")
    print("    " + " ".join(f"{project(2048.0 + k * 2048.0, -2048.0, height)[0]:.0f}" for k in range(-5, 6)))
    for rows in ((60, 160), (400, 500), (700, 800)):
        band = luminance[rows[0]:rows[1]]
        columns = band.mean(axis=0)
        middle = (rows[0] + rows[1]) / 2.0
        profile_lines(columns, f"vertical lines, rows {rows[0]}..{rows[1]}",
                      lambda px, middle=middle: (ground_hit(px, middle, height) or (None, None))[0], 2048.0)
    for columns in ((300, 400), (800, 900)):
        band = luminance[:, columns[0]:columns[1]]
        rows_profile = band.mean(axis=1)
        middle = (columns[0] + columns[1]) / 2.0
        profile_lines(rows_profile, f"horizontal lines, columns {columns[0]}..{columns[1]}",
                      lambda py, middle=middle: (ground_hit(middle, py, height) or (None, None))[1], 2048.0)
    # Bright (snow) patch boundary: the left-most and top-most crossing per row/column.
    box = (400, 780, 600, 1000)
    patch = luminance[box[0]:box[1], box[2]:box[3]]
    mask = patch > 205.0
    print("  bright-patch boundary, expressed in world coordinates:")
    previous = None
    steps = []
    for row in range(mask.shape[0]):
        columns = np.flatnonzero(mask[row])
        if columns.size == 0:
            continue
        hit = ground_hit(box[2] + columns[0], box[0] + row, height)
        if hit is None:
            continue
        if previous is not None:
            steps.append((hit[0] - previous[0], hit[1] - previous[1]))
        previous = hit
    if steps:
        arrays = np.asarray(steps)
        print(f"    {len(steps)} consecutive boundary samples; "
              f"median |dX|={np.median(np.abs(arrays[:, 0])):.1f} "
              f"median |dZ|={np.median(np.abs(arrays[:, 1])):.1f}")
    # Where do the steps happen? Report the world Z of each jump in X.
    previous = None
    jumps = []
    for row in range(mask.shape[0]):
        columns = np.flatnonzero(mask[row])
        if columns.size == 0:
            continue
        hit = ground_hit(box[2] + columns[0], box[0] + row, height)
        if hit is None:
            continue
        if previous is not None and abs(hit[0] - previous[0]) > 40.0:
            jumps.append((round(previous[0], 0), round(hit[0], 0), round(hit[1], 0)))
        previous = hit
    print(f"    {len(jumps)} jumps larger than 40 world units; (from X, to X, at Z):")
    for jump in jumps:
        print(f"      X {jump[0]:.0f} -> {jump[1]:.0f} at Z {jump[2]:.0f}")


if __name__ == "__main__":
    main()

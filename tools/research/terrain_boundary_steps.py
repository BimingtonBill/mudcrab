"""Measure the step size of a hard layer boundary (defect A) and the spacing of the
faint grid lines (defect B) in an acceptance screenshot.

Both are reported in pixels and, given the camera of the acceptance run, converted to
Skyrim world units with the projection model of `app.rs` (perspective camera, default
45 degree vertical FOV, 1600x900).

Usage:
    python tools/research/terrain_boundary_steps.py <screenshot.png> [--thresh N]
"""

import sys

import numpy as np
from PIL import Image

# Camera of the acceptance run (logs/rural-run-1.log).
CAMERA = np.array([2048.0, 14016.0, -1048.0])
TARGET = np.array([2048.0, -5984.0, -2048.0])
WIDTH, HEIGHT = 1600.0, 900.0
FOV_Y = 0.7853981633974483  # bevy default PerspectiveProjection fov


def camera_basis():
    forward = TARGET - CAMERA
    forward /= np.linalg.norm(forward)
    right = np.cross(forward, np.array([0.0, 1.0, 0.0]))
    right /= np.linalg.norm(right)
    up = np.cross(right, forward)
    return right, up, forward


RIGHT, UP, FORWARD = camera_basis()
TAN_Y = np.tan(FOV_Y / 2.0)
TAN_X = TAN_Y * (WIDTH / HEIGHT)


def ground_pixel(world, height):
    """Image (x, y) pixel of world (x, z) on the terrain plane y = height."""
    point = np.array([world[0], height, world[1]])
    v = point - CAMERA
    t = float(v @ FORWARD)
    x = float(v @ RIGHT) / (t * TAN_X) * (WIDTH / 2.0) + WIDTH / 2.0
    y = HEIGHT / 2.0 - float(v @ UP) / (t * TAN_Y) * (HEIGHT / 2.0)
    return x, y


def units_per_pixel(height=-5984.0, z=-2048.0, x=2048.0):
    """Horizontal world units per pixel at a point of the ground."""
    a = np.array([x, height, z])
    b = np.array([x + 64.0, height, z])
    pa = np.array(ground_pixel((a[0], a[2]), height))
    pb = np.array(ground_pixel((b[0], b[2]), height))
    return 64.0 / (pb[0] - pa[0])


def trace_boundary(rgb, box, threshold):
    """For each row in box, the first column whose luminance crosses threshold."""
    y0, y1, x0, x1 = box
    patch = rgb[y0:y1, x0:x1].mean(axis=2)
    mask = patch > threshold
    edges = np.full(mask.shape[0], np.nan)
    for row in range(mask.shape[0]):
        columns = np.flatnonzero(mask[row])
        if columns.size:
            edges[row] = columns[0]
    finite = edges[np.isfinite(edges)]
    runs = []
    if finite.size >= 2:
        run = 1
        for a, b in zip(finite, finite[1:]):
            if abs(a - b) < 0.5:
                run += 1
            else:
                runs.append(run)
                run = 1
        runs.append(run)
    return edges, runs


def grid_line_positions(profile, label):
    smooth = np.convolve(profile, np.ones(5) / 5, mode="same")
    baseline = np.convolve(smooth, np.ones(151) / 151, mode="same")
    residual = smooth - baseline
    std = residual.std()
    mean = residual.mean()
    positions = []
    for i in range(4, residual.size - 4):
        if residual[i] < mean - 1.0 * std and residual[i] <= residual[i - 1] and residual[i] <= residual[i + 1]:
            if not positions or i - positions[-1] > 10:
                positions.append(i)
    gaps = np.diff(positions)
    print(f"  {label}: {len(positions)} candidate lines at {positions}")
    if gaps.size:
        print(f"  {label}: gaps {list(gaps)} median {np.median(gaps):.1f} mean {gaps.mean():.1f}")
    return positions


def main():
    path = sys.argv[1]
    threshold = 200.0
    if "--thresh" in sys.argv:
        threshold = float(sys.argv[sys.argv.index("--thresh") + 1])
    rgb = np.asarray(Image.open(path).convert("RGB")).astype(np.float32)
    print(f"{path}: {rgb.shape[1]}x{rgb.shape[0]}")
    print(f"  scale near cell 0 centre: {units_per_pixel():.2f} world units per pixel "
          f"(horizontal, terrain y=-5984)")
    # Predicted image x of world-plane vertical lines for the main candidate periods.
    for height in (-5984.0,):
        for period, name in ((4096.0, "cell"), (2048.0, "quadrant")):
            pixels = []
            for k in range(-4, 5):
                px, _ = ground_pixel((k * period, -2048.0), height)
                if 0 <= px <= 1600:
                    pixels.append(round(px))
            print(f"  predicted image x of {name} boundaries ({period:.0f} u): {pixels}")
    for rows, label in (((60, 160), "band y=60..160"), ((400, 500), "band y=400..500"),
                        ((760, 860), "band y=760..860")):
        band = rgb[rows[0]:rows[1]]
        columns = band.mean(axis=(0, 2))
        grid_line_positions(columns, f"vertical lines {label}")
    for columns, label in (((300, 400), "band x=300..400"), ((800, 900), "band x=800..900")):
        band = rgb[:, columns[0]:columns[1]]
        rows = band.mean(axis=(1, 2))
        grid_line_positions(rows, f"horizontal lines {label}")
    edges, runs = trace_boundary(rgb, (400, 780, 600, 1000), threshold)
    print(f"  boundary trace: {int(np.isfinite(edges).sum())} rows with a crossing")
    if runs:
        print(f"  horizontal run lengths of the boundary (px): {runs}")
        print(f"  median run {np.median(runs):.1f} px = "
              f"{np.median(runs) * units_per_pixel():.0f} world units")


if __name__ == "__main__":
    main()

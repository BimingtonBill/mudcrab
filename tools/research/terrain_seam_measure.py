"""Measure the two terrain defects in the acceptance screenshots.

Defect A (blocky blending): edge of a layer transition - is it a smooth curve or a
staircase, and at what step size in pixels?

Defect B (grid lines): periodicity of the faint lines across the ground, in pixels.

Usage:
    python tools/research/terrain_seam_measure.py <screenshot.png> [<screenshot.png> ...]

Reads pixels only; writes nothing.
"""

import sys

import numpy as np
from PIL import Image


def load(path):
    image = Image.open(path).convert("RGB")
    return np.asarray(image).astype(np.float32), image.size


def ground_mask(rgb):
    """The world-space ground is everything that is not the flat background."""
    # The background of these captures is a flat dark grey (~0x262626-ish).
    background = (np.abs(rgb - rgb[0, 0]).sum(axis=2) < 6)
    return ~background


def line_profile(rgb, mask, axis):
    """Mean luminance along one axis, averaged over rows/cols that are ground."""
    luminance = rgb.mean(axis=2)
    if axis == 0:
        # columns: average down each column
        counts = mask.sum(axis=0)
        totals = (luminance * mask).sum(axis=0)
    else:
        counts = mask.sum(axis=1)
        totals = (luminance * mask).sum(axis=1)
    return np.where(counts > 0, totals / np.maximum(counts, 1), np.nan)


def dominant_period(profile, label):
    """Autocorrelation period of the detrended profile."""
    values = profile[np.isfinite(profile)]
    if values.size < 64:
        print(f"{label}: too few ground pixels")
        return None
    values = values - np.convolve(values, np.ones(33) / 33, mode="same")
    values = values - values.mean()
    spectrum = np.fft.rfft(values * np.hanning(values.size))
    power = np.abs(spectrum) ** 2
    # ignore the DC term and very low frequencies
    power[:4] = 0.0
    peak = int(np.argmax(power))
    frequency = peak / values.size
    period = np.inf if frequency == 0 else 1.0 / frequency
    top = np.argsort(power)[::-1][:6]
    print(f"{label}: strongest periods (px): " +
          ", ".join(f"{values.size / k:.1f}(p={power[k] / power[peak]:.2f})"
                    for k in top if k > 0))
    return period


def line_positions(profile, expect=None):
    """Local minima of the detrended profile, which is what a seam looks like."""
    values = profile.copy()
    finite = np.isfinite(values)
    values[~finite] = np.nan
    smooth = np.convolve(np.nan_to_num(values, nan=np.nanmean(values)),
                         np.ones(9) / 9, mode="same")
    baseline = np.convolve(smooth, np.ones(201) / 201, mode="same")
    residual = smooth - baseline
    threshold = residual[finite].mean() - 1.5 * residual[finite].std()
    candidates = []
    for i in range(2, residual.size - 2):
        if not finite[i] or not finite[i - 1] or not finite[i + 1]:
            continue
        if residual[i] < threshold and residual[i] <= residual[i - 1] and residual[i] <= residual[i + 1]:
            candidates.append(i)
    spaced = []
    for c in candidates:
        if not spaced or c - spaced[-1] > 12:
            spaced.append(c)
    gaps = np.diff(spaced)
    print(f"  line x positions: {spaced}")
    if gaps.size:
        print(f"  gaps (px): {list(gaps)}  median={np.median(gaps):.1f} mean={gaps.mean():.1f}")
    return spaced, gaps


def staircase_step(rgb, box, label):
    """Step size of the boundary of a bright patch, measured as run lengths."""
    y0, y1, x0, x1 = box
    patch = rgb[y0:y1, x0:x1]
    luminance = patch.mean(axis=2)
    binary = luminance > 200.0
    runs_x = []
    for row in binary:
        idx = np.flatnonzero(np.diff(row.astype(np.int8)))
        if idx.size >= 2:
            runs_x.append(np.diff(idx))
            break
    # column-wise: for every column, where does the top edge of the patch sit?
    edges = []
    for column in range(binary.shape[1]):
        rows = np.flatnonzero(binary[:, column])
        if rows.size:
            edges.append(rows[0])
        else:
            edges.append(np.nan)
    edges = np.asarray(edges, dtype=np.float32)
    finite = edges[np.isfinite(edges)]
    if finite.size < 8:
        print(f"{label}: no patch edge found")
        return
    constant_runs = []
    run = 1
    for a, b in zip(finite, finite[1:]):
        if abs(a - b) < 0.5:
            run += 1
        else:
            constant_runs.append(run)
            run = 1
    constant_runs.append(run)
    print(f"{label}: top-edge run lengths along x (px): {constant_runs}")
    print(f"{label}: mean run {np.mean(constant_runs):.1f} px, "
          f"median {np.median(constant_runs):.1f} px, max {np.max(constant_runs)} px")


def main():
    for path in sys.argv[1:]:
        print(f"=== {path}")
        rgb, size = load(path)
        print(f"  size {size[0]}x{size[1]}")
        mask = ground_mask(rgb)
        print(f"  ground pixels {int(mask.sum())} of {mask.size}")
        # background colour check
        print(f"  corner pixel rgb {rgb[0, 0].astype(int)}")
        column_profile = line_profile(rgb, mask, axis=0)
        row_profile = line_profile(rgb, mask, axis=1)
        dominant_period(column_profile, "  vertical lines (x)")
        dominant_period(row_profile, "  horizontal lines (y)")
        ground_columns = np.flatnonzero(mask.any(axis=0))
        if ground_columns.size:
            print(f"  ground spans x {ground_columns[0]}..{ground_columns[-1]}")
        ground_rows = np.flatnonzero(mask.any(axis=1))
        if ground_rows.size:
            print(f"  ground spans y {ground_rows[0]}..{ground_rows[-1]}")
        line_positions(column_profile)
        line_positions(row_profile)


if __name__ == "__main__":
    main()

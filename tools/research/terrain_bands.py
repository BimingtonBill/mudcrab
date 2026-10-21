"""Detect the faint terrain grid lines in thin bands, where the perspective scale is fixed.

Usage:
    python tools/research/terrain_bands.py <screenshot.png> [radius] [gain]

Prints, for horizontal bands of the image, the x positions of the dips of the high-passed
column profile and the gaps between them. Thin bands matter: a world-vertical line moves in
screen x with depth, so averaging the whole image height smears it.
"""

import sys

import numpy as np
from PIL import Image


def box_blur(image, radius):
    padded = np.pad(image, radius, mode="edge")
    cumulative = np.cumsum(padded, axis=0)
    out = (cumulative[2 * radius :] - cumulative[:-2 * radius]) / (2 * radius)
    cumulative = np.cumsum(out, axis=1)
    return (cumulative[:, 2 * radius :] - cumulative[:, :-2 * radius]) / (2 * radius)


def dips(profile, strength):
    smooth = np.convolve(profile, np.ones(3) / 3, mode="same")
    residual = smooth - np.convolve(smooth, np.ones(51) / 51, mode="same")
    sigma = residual.std()
    hits = []
    for index in range(2, residual.size - 2):
        if (
            residual[index] < -strength * sigma
            and residual[index] <= residual[index - 1]
            and residual[index] <= residual[index + 1]
        ):
            if not hits or index - hits[-1] > 8:
                hits.append(index)
    return residual, hits


def main():
    path = sys.argv[1]
    radius = int(sys.argv[2]) if len(sys.argv) > 2 else 20
    strength = float(sys.argv[3]) if len(sys.argv) > 3 else 0.7
    rgb = np.asarray(Image.open(path).convert("RGB")).astype(np.float32)
    high = rgb.mean(axis=2) - box_blur(rgb.mean(axis=2), radius)
    height = high.shape[0]
    for start in range(20, height - 40, 60):
        band = high[start : start + 20]
        profile = band.mean(axis=0)
        residual, hits = dips(profile, strength)
        gaps = list(np.diff(hits))
        print(f"rows {start:4d}-{start + 20:4d}: lines at {hits}")
        print(f"                        gaps {gaps}")
    print("column bands:")
    for start in range(20, high.shape[1] - 40, 100):
        band = high[:, start : start + 20]
        profile = band.mean(axis=1)
        residual, hits = dips(profile, strength)
        gaps = list(np.diff(hits))
        print(f"cols {start:4d}-{start + 20:4d}: lines at {hits}")
        print(f"                        gaps {gaps}")


if __name__ == "__main__":
    main()

"""High-pass an acceptance screenshot so the thin grid lines can be counted by eye.

Usage:
    python tools/research/terrain_enhance.py <screenshot.png> <out_dir> [radius] [gain]
"""

import os
import sys

import numpy as np
from PIL import Image


def box_blur(image, radius):
    """Separable box blur via cumulative sums (no scipy)."""
    padded = np.pad(image, radius, mode="edge")
    cumulative = np.cumsum(padded, axis=0)
    out = (cumulative[2 * radius :] - cumulative[:-2 * radius]) / (2 * radius)
    cumulative = np.cumsum(out, axis=1)
    out = (cumulative[:, 2 * radius :] - cumulative[:, :-2 * radius]) / (2 * radius)
    return out


def main():
    path, out_dir = sys.argv[1], sys.argv[2]
    radius = int(sys.argv[3]) if len(sys.argv) > 3 else 12
    gain = float(sys.argv[4]) if len(sys.argv) > 4 else 6.0
    image = Image.open(path).convert("RGB")
    rgb = np.asarray(image).astype(np.float32)
    luminance = rgb.mean(axis=2)
    high = luminance - box_blur(luminance, radius)
    amplified = np.clip(128.0 + high * gain, 0.0, 255.0).astype(np.uint8)
    os.makedirs(out_dir, exist_ok=True)
    name = f"{os.path.splitext(os.path.basename(path))[0]}_highpass_r{radius}_g{gain:g}.png"
    out = os.path.join(out_dir, name)
    Image.fromarray(amplified, mode="L").save(out)
    print(f"wrote {out} (radius {radius}, gain {gain:g})")
    # Column and row means of the high-pass: a grid line shows as a long run of one sign.
    columns = high.mean(axis=0)
    rows = high.mean(axis=1)
    top_columns = np.argsort(columns)[:12]
    top_rows = np.argsort(rows)[:12]
    print(f"  darkest high-pass columns: {sorted(int(v) for v in top_columns)}")
    print(f"  darkest high-pass rows:    {sorted(int(v) for v in top_rows)}")


if __name__ == "__main__":
    main()

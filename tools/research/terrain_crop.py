"""Crop and upscale regions of an acceptance screenshot so the defects are readable.

Usage:
    python tools/research/terrain_crop.py <screenshot.png> <out_dir> <x0> <y0> <x1> <y1> <scale>

Also prints a column/row profile summary for the crop so line positions can be read
off numerically as well as visually.
"""

import os
import sys

import numpy as np
from PIL import Image


def main():
    path, out_dir, x0, y0, x1, y1, scale = (
        sys.argv[1],
        sys.argv[2],
        int(sys.argv[3]),
        int(sys.argv[4]),
        int(sys.argv[5]),
        int(sys.argv[6]),
        int(sys.argv[7]),
    )
    image = Image.open(path).convert("RGB")
    crop = image.crop((x0, y0, x1, y1))
    os.makedirs(out_dir, exist_ok=True)
    name = f"{os.path.splitext(os.path.basename(path))[0]}_{x0}_{y0}_{x1}_{y1}_x{scale}.png"
    out = os.path.join(out_dir, name)
    crop.resize((crop.width * scale, crop.height * scale), Image.NEAREST).save(out)
    print(f"wrote {out} ({crop.width * scale}x{crop.height * scale})")

    rgb = np.asarray(crop).astype(np.float32)
    luminance = rgb.mean(axis=2)
    columns = luminance.mean(axis=0)
    rows = luminance.mean(axis=1)
    print("column luminance (x offset, value):")
    print("  " + " ".join(f"{i}:{v:.0f}" for i, v in enumerate(columns)))
    print("row luminance (y offset, value):")
    print("  " + " ".join(f"{i}:{v:.0f}" for i, v in enumerate(rows)))


if __name__ == "__main__":
    main()

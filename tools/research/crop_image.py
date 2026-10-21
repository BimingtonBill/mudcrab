#!/usr/bin/env python3
"""Crop and upscale a region of a screenshot so a defect can be looked at closely.

Usage:
    python tools/research/crop_image.py <src.png> <dst.png> <x0> <y0> <x1> <y1> [scale]

`scale` defaults to 2. The crop is upscaled with nearest sampling so pixel-level detail
(edges of a translucent quad, z-fighting stripes) stays readable.
"""

import sys

from PIL import Image


def main(argv):
    if len(argv) < 7:
        print(__doc__)
        return 2
    source, destination = argv[1], argv[2]
    x0, y0, x1, y1 = (int(value) for value in argv[3:7])
    scale = int(argv[7]) if len(argv) > 7 else 2
    image = Image.open(source).convert("RGB")
    crop = image.crop((x0, y0, x1, y1))
    crop = crop.resize((crop.width * scale, crop.height * scale), Image.NEAREST)
    crop.save(destination)
    print(f"wrote {destination} {crop.width}x{crop.height} from {image.width}x{image.height}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

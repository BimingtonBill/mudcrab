#!/usr/bin/env python3
"""Overlay a labelled pixel grid on an image so features can be read off by coordinate.

Usage:
    python tools/research/image_grid.py <src image> <dst image> [step] [scale]

`step` defaults to 100 pixels, `scale` to 1.0.  The labels are the pixel coordinates of the
lines, so a landmark in the output can be quoted as (x, y) in the *source* image.  Half-way
lines of the frame are drawn brighter to make the centre easy to find.
"""

import sys

from PIL import Image, ImageDraw, ImageFont


def font(size):
    for name in ("segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    source, destination = argv[1], argv[2]
    step = int(argv[3]) if len(argv) > 3 else 100
    scale = float(argv[4]) if len(argv) > 4 else 1.0
    image = Image.open(source).convert("RGB")
    draw = ImageDraw.Draw(image)
    width, height = image.size
    text = font(max(11, int(13 * scale)))
    for x in range(0, width + 1, step):
        centre = abs(x - width / 2.0) < step / 2.0
        draw.line([(x, 0), (x, height)], fill=(255, 60, 60) if centre else (0, 255, 255), width=2 if centre else 1)
        draw.text((x + 2, 2), str(x), fill=(255, 255, 0), font=text)
        draw.text((x + 2, height - 16), str(x), fill=(255, 255, 0), font=text)
    for y in range(0, height + 1, step):
        centre = abs(y - height / 2.0) < step / 2.0
        draw.line([(0, y), (width, y)], fill=(255, 60, 60) if centre else (0, 255, 255), width=2 if centre else 1)
        draw.text((2, y + 2), str(y), fill=(255, 255, 0), font=text)
        draw.text((width - 40, y + 2), str(y), fill=(255, 255, 0), font=text)
    if "/" in destination:
        import os

        os.makedirs(destination.rsplit("/", 1)[0], exist_ok=True)
    if scale != 1.0:
        image = image.resize((int(width * scale), int(height * scale)), Image.LANCZOS)
    image.save(destination)
    print(f"wrote {destination}: {image.width}x{image.height} from {width}x{height}, step {step}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

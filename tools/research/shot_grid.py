#!/usr/bin/env python3
"""One labelled grid of PNG/JPG files, for judging many candidate camera poses at once.

Usage:
    python tools/research/shot_grid.py <out.jpg> <image or directory> [more images/dirs]
        [--columns 3] [--height 420] [--caption "text"]

Directories are expanded to their `*.png` (and `*.jpg`) files in name order, so
`local/poses/r1` shows a whole render round.  Each cell is scaled to `--height` and captioned
with the file stem, in reading order.  This is a scratch tool for pose refinement: it needs no
database and no engine, it only assembles images that already exist.
"""

import os
import sys

from PIL import Image, ImageDraw, ImageFont


def font(size):
    for name in ("segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def collect(paths):
    images = []
    for path in paths:
        if os.path.isdir(path):
            for name in sorted(os.listdir(path)):
                if name.lower().endswith((".png", ".jpg", ".jpeg")):
                    images.append(os.path.join(path, name))
        else:
            images.append(path)
    return images


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    out, height, columns, caption, sources = argv[1], 420, 3, "", []
    index = 2
    while index < len(argv):
        if argv[index] == "--columns":
            columns = int(argv[index + 1])
            index += 2
        elif argv[index] == "--height":
            height = int(argv[index + 1])
            index += 2
        elif argv[index] == "--caption":
            caption = argv[index + 1]
            index += 2
        else:
            sources.append(argv[index])
            index += 1
    paths = collect(sources)
    if not paths:
        raise SystemExit("no images found")
    loaded = []
    for path in paths:
        with Image.open(path) as image:
            scale = height / image.height
            loaded.append(
                (os.path.splitext(os.path.basename(path))[0],
                 image.convert("RGB").resize((max(1, int(image.width * scale)), height),
                                             Image.LANCZOS))
            )
    cell_width = max(image.width for _, image in loaded) + 12
    rows = (len(loaded) + columns - 1) // columns
    bar = 30 if caption else 0
    sheet = Image.new("RGB", (columns * cell_width, rows * (height + 26) + bar + 6), (12, 12, 16))
    draw = ImageDraw.Draw(sheet)
    if caption:
        draw.text((6, 6), caption, fill=(255, 240, 160), font=font(20))
    for index, (name, image) in enumerate(loaded):
        column = index % columns
        row = index // columns
        x = column * cell_width + 6
        y = bar + row * (height + 26)
        sheet.paste(image, (x, y))
        draw.text((x, y + height + 4), name, fill=(210, 220, 240), font=font(17))
    sheet.save(out, quality=88)
    print(f"  wrote {out}: {sheet.width}x{sheet.height}, {len(loaded)} images")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

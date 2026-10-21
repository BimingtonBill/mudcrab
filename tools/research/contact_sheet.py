#!/usr/bin/env python3
"""Tile images into one labelled sheet, so a run can be judged in a single look.

Usage:
    python tools/research/contact_sheet.py <out.jpg> <image>... [options]
    python tools/research/contact_sheet.py <out.jpg> --dir <folder> [--glob '*.png']

Options:
    --columns 4     tiles per row
    --tile 420      width of a tile in pixels
    --glob '*.png'  which files to take from --dir

Unlike `tools/compare_shots.py`, which pairs each render with its reference, this puts any set of
images on one page with the file name under each tile.  It is for looking at a whole render run at
once - a calibration sweep, or the extra poses a sweep adds.
"""

import argparse
import glob
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    parser.add_argument("images", nargs="*")
    parser.add_argument("--dir")
    parser.add_argument("--glob", default="*.png")
    parser.add_argument("--columns", type=int, default=4)
    parser.add_argument("--tile", type=int, default=420)
    args = parser.parse_args()

    paths = list(args.images)
    if args.dir:
        paths.extend(sorted(glob.glob(os.path.join(args.dir, args.glob))))
    paths = [path for path in paths if os.path.exists(path)]
    if not paths:
        raise SystemExit("no images to tile")

    label_height = 22
    rows = (len(paths) + args.columns - 1) // args.columns
    first = Image.open(paths[0])
    tile_height = max(1, int(round(args.tile * first.height / first.width)))
    sheet = Image.new(
        "RGB",
        (args.columns * (args.tile + 6) + 6, rows * (tile_height + label_height + 6) + 6),
        (12, 12, 14),
    )
    draw = ImageDraw.Draw(sheet)
    text = font(15)
    for index, path in enumerate(paths):
        column = index % args.columns
        row = index // args.columns
        x = 6 + column * (args.tile + 6)
        y = 6 + row * (tile_height + label_height + 6)
        with Image.open(path) as image:
            tile = image.convert("RGB").resize((args.tile, tile_height), Image.LANCZOS)
        sheet.paste(tile, (x, y))
        name = os.path.splitext(os.path.basename(path))[0]
        draw.text((x + 2, y + tile_height + 3), name[:56], fill=(220, 225, 235), font=text)
    sheet.save(args.output, quality=88)
    print(f"  {args.output}: {sheet.width}x{sheet.height}, {len(paths)} tiles")
    return 0


if __name__ == "__main__":
    sys.exit(main())

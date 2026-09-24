#!/usr/bin/env python3
"""One picture of every reference beside its render, numbered, for a quick grading pass.

Usage:
    python tools/research/grade_sheet.py <out.jpg> <shots.json>:<render dir> [<shots.json>:<render dir> ...]

Pages `<out stem>-1.jpg`, `-2`, ... of eight rows each. Each row is `#n  <shot name>`, the Skyrim screenshot on the left and the engine's render of the
shot's pose on the right. Shots without a render (or a reference) are listed on the console and
left out. Also writes `<out>.json`, the row number -> shot name index a grader answers against.
"""

import json
import os
import sys

from PIL import Image, ImageDraw, ImageFile

ImageFile.LOAD_TRUNCATED_IMAGES = True
W, H = 480, 360
PER_PAGE = 8


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    out, pairs = argv[1], argv[2:]
    rows, index, skipped = [], {}, []
    for pair in pairs:
        cut = pair.index(".json:") + len(".json")
        shots_path, render_dir = pair[:cut], pair[cut + 1 :]
        shots = json.load(open(shots_path, encoding="utf-8"))["shots"]
        for shot in shots:
            render = os.path.join(render_dir, shot["name"] + ".png")
            reference = shot.get("reference")
            if not reference or not os.path.exists(reference) or not os.path.exists(render):
                skipped.append(shot["name"])
                continue
            number = len(rows) + 1
            index[number] = shot["name"]
            row = Image.new("RGB", (W * 2, H + 20), (20, 20, 20))
            row.paste(Image.open(reference).convert("RGB").resize((W, H)), (0, 20))
            row.paste(Image.open(render).convert("RGB").resize((W, H)), (W, 20))
            ImageDraw.Draw(row).text((6, 4), f"#{number}  {shot['name']}   (left: Skyrim, right: engine)", fill=(240, 240, 240))
            rows.append(row)
    # Pages of eight pairs: one tall picture of every shot is downscaled past legibility when a
    # grader opens it, so each page stays readable on its own.
    stem, extension = os.path.splitext(out)
    pages = [rows[start : start + PER_PAGE] for start in range(0, len(rows), PER_PAGE)]
    for page_number, page in enumerate(pages, 1):
        sheet = Image.new("RGB", (W * 2, sum(row.height for row in page)))
        for position, row in enumerate(page):
            sheet.paste(row, (0, position * row.height))
        sheet.save(f"{stem}-{page_number}{extension}", quality=80)
    with open(out + ".json", "w", encoding="utf-8") as handle:
        json.dump(index, handle, indent=1)
    print(f"{len(rows)} rows on {len(pages)} page(s) -> {stem}-*{extension}" + (f"; skipped: {', '.join(skipped)}" if skipped else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

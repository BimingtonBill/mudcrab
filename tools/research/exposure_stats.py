#!/usr/bin/env python3
"""Report the luminance statistics of render and reference images, to judge an exposure change.

Usage:
    python tools/research/exposure_stats.py <image> [<image> ...] [options]
    python tools/research/exposure_stats.py --shots <shots.json> --render <dir> [--reference <dir>]
    python tools/research/exposure_stats.py --shots <shots.json> --render <dir> --check

Options:
    --space NAME        print a `space\t...` summary line for these images under this space name
    --json              write the rows as JSON instead of a table
    --clip-level 0.98   the relative luminance at or above which a pixel counts as clipped
    --dark-level 0.05   the relative luminance below which a pixel counts as almost black
    --check             fail (non-zero exit) when a render leaves the band its shot carries

Per image it prints the mean relative luminance, the 5th, 50th and 95th percentile, the fraction
of pixels at or above `--clip-level` ("clipped"), the fraction below `--dark-level`, and the mean
saturation (max-min over max of the linear RGB).  Luminance is Rec.709 over linear-light RGB, so
"sRGB 0.8 grey" does not read as twice "sRGB 0.4 grey".

With `--shots`, every shot in the JSON that has `<render>/<name>.png` is measured against its
reference, and `--space` groups rows so their statistics can be pooled per space.

`--check` turns the measurement into a check: a shot may carry an optional `metrics` block with
absolute bands - `flat_white_max`, `mean_luma_min`, `mean_luma_max` - and its render is judged
against them, so a change that makes water a white sheet or darkens a firelit room fails without
any reference image.  A shot without a `metrics` block is reported and not failed.
"""

import argparse
import json
import os
import sys

import numpy as np
from PIL import Image

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_REFERENCE_DIR = os.path.join(REPO, "local", "reference", "uesp")
# Rec.709 luminance weights, over linear-light RGB.
LUMA = np.array([0.2126, 0.7152, 0.0722], dtype=np.float64)
# A "flat white" pixel: bright, and with no colour to it. A sheet of water drawn with no shading
# and an unlit white plane both read here; a white wall in shade, or a blue-white sky, does not.
# Tuned by the lead on the 2026-09-24 Riverwood renders (local/reference/cmp-103): the white-sheet
# water is bright but rippled and faintly tinted, so near-pure white (0.9 / 0.05) found 0.7% of RW-08.
# At 0.5 / 0.2 the two water shots read 0.10 (RW-05) and 0.097 (RW-08) and every other shot 0.043 or
# less, so a `flat_white_max` of 0.06 on the water shots catches the defect.
FLAT_WHITE_LEVEL = 0.5
FLAT_WHITE_SATURATION = 0.2
# The bands a shot's `metrics` block may carry, and the statistic each one limits.
CHECK_BANDS = ("flat_white_max", "mean_luma_min", "mean_luma_max")


def to_linear(rgb):
    """8-bit sRGB -> linear-light RGB in 0..1."""
    scaled = rgb.astype(np.float64) / 255.0
    return np.where(scaled <= 0.04045, scaled / 12.92, ((scaled + 0.055) / 1.055) ** 2.4)


def statistics(path, clip_level, dark_level):
    with Image.open(path) as image:
        rgb = np.asarray(image.convert("RGB"))
    linear = to_linear(rgb)
    luminance = linear @ LUMA
    maximum = linear.max(axis=2)
    minimum = linear.min(axis=2)
    saturation = np.where(maximum > 0.0, (maximum - minimum) / np.maximum(maximum, 1e-9), 0.0)
    flat = linear.reshape(-1, 3)
    mean_rgb = flat.mean(axis=0)
    # The colour of the pixels from the 15th to the 45th percentile of luminance. That band is the
    # dim surfaces a light does not reach - the ambient of `app.rs` and nothing else - while
    # leaving out the near-black below it, which is the clear colour showing where no geometry was
    # streamed and is not lit by anything, and the bright above it, which is a `LIGH` pool.
    order = np.argsort(luminance, axis=None)
    band = flat[order[order.size * 15 // 100 : max(order.size * 45 // 100, 1)]]
    dark_rgb = band.mean(axis=0)
    return {
        "file": path,
        "width": int(rgb.shape[1]),
        "height": int(rgb.shape[0]),
        # Linear-light mean of each channel: the space's colour cast, which the ambient light of
        # `app.rs` sets most of.
        "mean_rgb": [float(value) for value in mean_rgb],
        "dark_rgb": [float(value) for value in dark_rgb],
        "mean": float(luminance.mean()),
        "p05": float(np.percentile(luminance, 5)),
        "p50": float(np.percentile(luminance, 50)),
        "p95": float(np.percentile(luminance, 95)),
        "clipped": float((luminance >= clip_level).mean()),
        "near_white": float((luminance >= 0.9).mean()),
        # Bright *and* colourless: what a water surface drawn with no shading, an unlit white
        # plane, or an over-exposed sheet of snow reads as. `near_white` alone cannot tell those
        # from a blue-white sky or a lit white wall, which is what the saturation half is for.
        "flat_white": float(
            ((luminance >= FLAT_WHITE_LEVEL) & (saturation < FLAT_WHITE_SATURATION)).mean()
        ),
        "dark": float((luminance < dark_level).mean()),
        "saturation": float(saturation.mean()),
        # The mean of the top 10% of pixels: what a room's "bright" surfaces read at, which a
        # small area of a clipped lantern or a lamp raises far more slowly than `clipped` does.
        "mean_top10": float(np.sort(luminance, axis=None)[int(luminance.size * 0.9):].mean()),
    }


def format_row(row, label):
    red, green, blue = row["mean_rgb"]
    # The darkest third's hue, as R and B against G: 1.0, 1.0 is a neutral ambient, R>G>B is the
    # warm interior cast of `app.rs` and B>G>R the cold cavern one.
    dark_red, dark_green, dark_blue = row["dark_rgb"]
    return (
        f"{label:<34} {row['mean']:>6.3f} {row['p05']:>6.3f} {row['p50']:>6.3f} {row['p95']:>6.3f} "
        f"{row['clipped'] * 100:>6.2f} {row['near_white'] * 100:>6.2f} "
        f"{row['flat_white'] * 100:>6.2f} {row['dark'] * 100:>6.2f} "
        f"{row['saturation']:>6.3f} {row['mean_top10']:>6.3f} "
        f"{red:>6.3f} {green:>6.3f} {blue:>6.3f} {green / max(red + blue, 1e-9):>5.2f} "
        f"{dark_red / max(dark_green, 1e-9):>5.2f} {dark_blue / max(dark_green, 1e-9):>5.2f}"
    )


HEADER = (
    f"{'image':<34} {'mean':>6} {'p05':>6} {'p50':>6} {'p95':>6} "
    f"{'clip%':>6} {'near90%':>6} {'flt90%':>6} {'dark%':>6} {'sat':>6} {'top10':>6} "
    f"{'R':>6} {'G':>6} {'B':>6} {'G/RB':>5} {'dR/G':>5} {'dB/G':>5}"
)


def check_shot(shot, row, render):
    """One shot's render against the band its own `metrics` block gives it.

    Returns `(lines, failures)`: the lines to print - one per band, or the reason there is nothing
    to check - and how many of them are violations.  A shot with no `metrics` is reported and not
    failed: the bands are opt-in, and a shots file with none of them is still a shots file.
    """
    name = shot.get("name", "?")
    metrics = shot.get("metrics") or {}
    if not metrics:
        return [f"---- {name}: no metrics band"], 0
    if row is None:
        bands = ", ".join(f"{key} {metrics[key]}" for key in sorted(metrics))
        return [f"FAIL {name}: no render at {render} to judge against {bands}"], 1

    lines = []
    failures = 0
    if "flat_white_max" in metrics:
        band = float(metrics["flat_white_max"])
        value = row["flat_white"]
        if value > band:
            lines.append(f"FAIL {name}: flat_white {value:.3f} > flat_white_max {band:g}")
            failures += 1
        else:
            lines.append(f"ok   {name}: flat_white {value:.3f} <= flat_white_max {band:g}")
    if "mean_luma_min" in metrics:
        band = float(metrics["mean_luma_min"])
        value = row["mean"]
        if value < band:
            lines.append(f"FAIL {name}: mean luma {value:.3f} < mean_luma_min {band:g}")
            failures += 1
        else:
            lines.append(f"ok   {name}: mean luma {value:.3f} >= mean_luma_min {band:g}")
    if "mean_luma_max" in metrics:
        band = float(metrics["mean_luma_max"])
        value = row["mean"]
        if value > band:
            lines.append(f"FAIL {name}: mean luma {value:.3f} > mean_luma_max {band:g}")
            failures += 1
        else:
            lines.append(f"ok   {name}: mean luma {value:.3f} <= mean_luma_max {band:g}")
    # A band a typo turned into a name this does not know would silently check nothing, which is
    # the one failure mode of a check nobody can see, so it is one this reports.
    for key in sorted(set(metrics) - set(CHECK_BANDS)):
        lines.append(
            f"FAIL {name}: unknown metric {key!r} - the bands are {', '.join(CHECK_BANDS)}"
        )
        failures += 1
    return lines, failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("images", nargs="*", help="image paths")
    parser.add_argument("--shots", help="a shots JSON file; measure render vs reference per shot")
    parser.add_argument("--render", help="directory of rendered PNGs for --shots")
    parser.add_argument("--reference-dir", default=DEFAULT_REFERENCE_DIR)
    parser.add_argument("--space", help="space name to attach to plain image arguments")
    parser.add_argument("--save-json", help="write every measured row to this JSON file")
    parser.add_argument("--clip-level", type=float, default=0.98)
    parser.add_argument("--dark-level", type=float, default=0.05)
    parser.add_argument(
        "--check",
        action="store_true",
        help="judge every shot's render against the band of its own `metrics` block",
    )
    args = parser.parse_args()
    if args.check and not args.shots:
        parser.error("--check reads the bands out of the shots file: give it --shots")
    if args.shots and not args.render:
        parser.error("--shots measures renders: give it --render")

    rows = []
    document = None
    renders = {}
    if args.shots:
        with open(args.shots, "r", encoding="utf-8") as handle:
            document = json.load(handle)
        for shot in document["shots"]:
            reference = shot.get("reference")
            reference = (
                os.path.join(REPO, reference)
                if reference and not os.path.isabs(reference)
                else reference
            )
            if not reference or not os.path.exists(reference):
                reference = os.path.join(args.reference_dir, f"{shot['name']}.jpg")
            render = os.path.join(args.render, f"{shot['name']}.png")
            for kind, path in (("reference", reference), ("render", render)):
                if not os.path.exists(path):
                    print(f"  missing {kind}: {path}", file=sys.stderr)
                    continue
                row = statistics(path, args.clip_level, args.dark_level)
                row["name"] = shot["name"]
                row["kind"] = kind
                row["space"] = shot.get("space") or space_of(shot)
                rows.append(row)
                if kind == "render":
                    renders[shot["name"]] = row
    for path in args.images:
        if not os.path.exists(path):
            print(f"  missing image: {path}", file=sys.stderr)
            continue
        row = statistics(path, args.clip_level, args.dark_level)
        row["name"] = os.path.splitext(os.path.basename(path))[0]
        row["kind"] = "image"
        row["space"] = args.space or space_from_name(row["name"])
        rows.append(row)

    print(HEADER)
    for row in rows:
        label = row["name"] if row["kind"] == "image" else f"{row['name']} [{row['kind'][:3]}]"
        print(format_row(row, label))

    # Pooled per space and kind, so a group of shots is judged as one number.
    spaces = sorted({row["space"] for row in rows if row["space"]})
    if spaces:
        print()
        print("pooled per space (mean of the per-image statistics):")
        print(HEADER)
        for space in spaces:
            for kind in ("reference", "render"):
                group = [r for r in rows if r["space"] == space and r["kind"] == kind]
                if not group:
                    continue
                pooled = {key: float(np.mean([r[key] for r in group])) for key in (
                    "mean", "p05", "p50", "p95", "clipped", "near_white", "flat_white", "dark",
                    "saturation", "mean_top10",
                )}
                pooled["mean_rgb"] = np.mean([r["mean_rgb"] for r in group], axis=0).tolist()
                pooled["dark_rgb"] = np.mean([r["dark_rgb"] for r in group], axis=0).tolist()
                print(format_row(pooled, f"{space} [{kind[:3]}] n={len(group)}"))

    failures = 0
    if args.check:
        print()
        print("checks (each render against the band of its own shot):")
        for shot in document["shots"]:
            render = os.path.join(args.render, f"{shot['name']}.png")
            lines, failed = check_shot(shot, renders.get(shot["name"]), render)
            for line in lines:
                print(f"  {line}")
            failures += failed
        checked = sum(1 for shot in document["shots"] if shot.get("metrics"))
        print(
            f"  {failures} shot(s) outside their band"
            if failures
            else f"  every one of the {checked} shot(s) with a band is inside it"
        )

    if args.save_json:
        with open(args.save_json, "w", encoding="utf-8") as handle:
            json.dump(rows, handle, indent=1)
        print(f"  wrote {args.save_json}")
    return 1 if failures else 0


def space_from_name(name):
    """A calibration pose is named `cal-<Space>-<what>`; that is the space it belongs to."""
    parts = name.split("-")
    if len(parts) >= 2 and parts[0] == "cal":
        return parts[1]
    return ""


def space_of(shot):
    """Which Alftand/Blackreach space a shot belongs to, from its cell or worldspace id."""
    interior = shot.get("interior_cell_id")
    exterior = shot.get("worldspace_id")
    if interior == 86723:
        return "Alftand01"
    if interior == 355355:
        return "Alftand02"
    if interior == 432216:
        return "AlftandZCell"
    if exterior == 432215:
        return "AlftandWorld"
    if exterior == 126562:
        return "Blackreach"
    if exterior == 60:
        return "Tamriel"
    return f"space {interior or exterior}"


if __name__ == "__main__":
    sys.exit(main())

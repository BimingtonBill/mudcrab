#!/usr/bin/env python3
"""lead-071 scratch: luminance statistics of renders and references, pairing by shots file.

Usage:
    python scratch-071/measure.py pair <render-dir> [<render-dir> ...] [--reference-dir DIR]
    python scratch-071/measure.py one <image> [<image> ...]
    python scratch-071/measure.py crop <image> <x0> <y0> <x1> <y1> [<x0> <y0> <x1> <y1> ...]
    python scratch-071/measure.py band <image> <lo> <hi>          # luminance band 15..45 % default

Prints, per image: mean, p05, p50, p95, clipped(>=0.98), near_white(>=0.9), dark(<0.05),
saturation, mean linear RGB, and the 15th..45th percentile band's mean linear RGB (the "dim
surfaces" band exposure_stats.py uses).  Rec.709 luminance over linear RGB, as that tool does.
"""

import os
import sys

import numpy as np
from PIL import Image, ImageFile

ImageFile.LOAD_TRUNCATED_IMAGES = True
LUMA = np.array([0.2126, 0.7152, 0.0722], dtype=np.float64)


def to_linear(rgb):
    scaled = rgb.astype(np.float64) / 255.0
    return np.where(scaled <= 0.04045, scaled / 12.92, ((scaled + 0.055) / 1.055) ** 2.4)


def stats_of(linear):
    luminance = linear @ LUMA
    flat = linear.reshape(-1, 3)
    order = np.argsort(luminance, axis=None)
    lo, hi = order.size * 15 // 100, max(order.size * 45 // 100, 1)
    band = flat[order[lo:hi]].mean(axis=0)
    maximum, minimum = linear.max(axis=2), linear.min(axis=2)
    sat = np.where(maximum > 0.0, (maximum - minimum) / np.maximum(maximum, 1e-9), 0.0)
    return {
        "mean": float(luminance.mean()),
        "p05": float(np.percentile(luminance, 5)),
        "p50": float(np.percentile(luminance, 50)),
        "p95": float(np.percentile(luminance, 95)),
        "clip": float((luminance >= 0.98).mean()),
        "near90": float((luminance >= 0.90).mean()),
        "dark": float((luminance < 0.05).mean()),
        "sat": float(sat.mean()),
        "rgb": flat.mean(axis=0),
        "dim": band,
    }


def load(path):
    with Image.open(path) as image:
        return stats_of(to_linear(np.asarray(image.convert("RGB"))))


def fmt(name, s):
    rgb = " ".join(f"{v:.3f}" for v in s["rgb"])
    dim = " ".join(f"{v:.3f}" for v in s["dim"])
    return (
        f"{name:<44} mean {s['mean']:.3f} p05 {s['p05']:.3f} p50 {s['p50']:.3f} "
        f"p95 {s['p95']:.3f} clip {s['clip'] * 100:5.2f}% n90 {s['near90'] * 100:5.2f}% "
        f"dark {s['dark'] * 100:5.2f}% sat {s['sat']:.3f} rgb[{rgb}] dim[{dim}]"
    )


def ratio(a, b, key):
    return a[key] / b[key] if b[key] else float("inf")


def pair(render_dirs, reference_dir):
    import json

    shots = json.load(open("tools/reference/riverwood_shots.json"))
    entries = shots["shots"] if isinstance(shots, dict) and "shots" in shots else shots
    for render_dir in render_dirs:
        print(f"=== {render_dir} ===")
        pooled_render, pooled_reference = [], []
        for entry in entries:
            name = entry.get("name")
            reference = entry.get("reference")
            render_path = os.path.join(render_dir, name + ".png")
            if not os.path.exists(render_path):
                continue
            r = load(render_path)
            row = f"{name:<44} render " + fmt("", r).split("render", 1)[-1].strip()
            if reference:
                ref_path = os.path.join(reference_dir, os.path.basename(reference))
                if os.path.exists(ref_path):
                    f = load(ref_path)
                    row += (
                        f"\n{'':<44} ref    " + fmt("", f).split("ref", 1)[-1].strip()
                        + f"\n{'':<44} ratio  p50 {ratio(r, f, 'p50'):.2f} p05 {ratio(r, f, 'p05'):.2f}"
                        f" p95 {ratio(r, f, 'p95'):.2f} mean {ratio(r, f, 'mean'):.2f}"
                        f" near90x {ratio(r, f, 'near90'):.2f} darkx {ratio(r, f, 'dark'):.2f}"
                    )
                    pooled_render.append((r, f))
            print(row)
        if pooled_render:
            keys = ["mean", "p05", "p50", "p95", "clip", "near90", "dark", "sat"]
            pr = {k: float(np.mean([r[k] for r, _ in pooled_render])) for k in keys}
            pf = {k: float(np.mean([f[k] for _, f in pooled_render])) for k in keys}
            print(
                f"{'POOLED':<44} render mean {pr['mean']:.3f} p05 {pr['p05']:.3f} "
                f"p50 {pr['p50']:.3f} p95 {pr['p95']:.3f} clip {pr['clip'] * 100:.2f}% "
                f"n90 {pr['near90'] * 100:.2f}% dark {pr['dark'] * 100:.2f}%"
            )
            print(
                f"{'':<44} ref    mean {pf['mean']:.3f} p05 {pf['p05']:.3f} "
                f"p50 {pf['p50']:.3f} p95 {pf['p95']:.3f} clip {pf['clip'] * 100:.2f}% "
                f"n90 {pf['near90'] * 100:.2f}% dark {pf['dark'] * 100:.2f}%"
            )


def same(render_dir, reference_dir, suffixes=(".jpg", ".png")):
    """Every render that has a same-named reference beside it (the UESP name = the shot name)."""
    names = sorted(
        os.path.splitext(f)[0] for f in os.listdir(render_dir) if f.endswith(".png")
    )
    for name in names:
        render_path = os.path.join(render_dir, name + ".png")
        reference_path = None
        for suffix in suffixes:
            candidate = os.path.join(reference_dir, name + suffix)
            if os.path.exists(candidate):
                reference_path = candidate
                break
        r = load(render_path)
        if reference_path is None:
            print(f"{name:<44} render {fmt('', r).strip()}   (no reference)")
            continue
        f = load(reference_path)
        print(f"{name:<44} render {fmt('', r).strip()}")
        print(f"{'':<44} ref    {fmt('', f).strip()}")
        print(
            f"{'':<44} ratio  p50 {ratio(r, f, 'p50'):.2f} p05 {ratio(r, f, 'p05'):.2f}"
            f" p95 {ratio(r, f, 'p95'):.2f} mean {ratio(r, f, 'mean'):.2f}"
            f" near90x {ratio(r, f, 'near90'):.2f} darkx {ratio(r, f, 'dark'):.2f}"
        )


def main(argv):
    if not argv:
        print(__doc__)
        return 1
    mode = argv[0]
    if mode == "same":
        same(argv[1], argv[2])
    elif mode == "pair":
        dirs = [a for a in argv[1:] if not a.startswith("--")]
        reference_dir = "local/reference/riverwood"
        if "--reference-dir" in argv:
            reference_dir = argv[argv.index("--reference-dir") + 1]
            dirs = [d for d in dirs if d != reference_dir]
        pair(dirs, reference_dir)
    elif mode == "one":
        for path in argv[1:]:
            print(fmt(os.path.basename(path), load(path)))
    elif mode == "band":
        path, lo, hi = argv[1], float(argv[2]), float(argv[3])
        with Image.open(path) as image:
            linear = to_linear(np.asarray(image.convert("RGB")))
        luminance = linear @ LUMA
        flat = linear.reshape(-1, 3)
        order = np.argsort(luminance, axis=None)
        band = flat[order[int(order.size * lo) : max(int(order.size * hi), 1)]]
        print(f"{path} band {lo}..{hi}: {band.mean(axis=0)}")
    elif mode == "crop":
        path = argv[1]
        with Image.open(path) as image:
            rgb = np.asarray(image.convert("RGB"))
        height, width = rgb.shape[:2]
        for spec in argv[2:]:
            x0, y0, x1, y1 = (float(v) for v in spec.split(","))
            # Fractions of the frame, so a crop can be written down for either image size.
            box = (int(x0 * width), int(y0 * height), int(x1 * width), int(y1 * height))
            sub = to_linear(rgb[box[1] : box[3], box[0] : box[2]])
            print(f"{os.path.basename(path)} {spec}: {fmt('', stats_of(sub)).strip()}")
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
"""Measure the frames either side of a doorway crossing.

For a walk-through directory (frames + frames.txt), report per frame around the swap: mean
luminance, the frame-to-frame difference (which finds the swap by measurement), and the best-fit 2D
offset between the last pre-swap frame and a steady post-swap frame.

Usage:
    python local/scratch-072/portal_frames.py <walk-through stage dir> [--window 12] [--search 24]
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

import numpy as np
from PIL import Image

FRAME_RE = re.compile(r"f(\d+)\.png$")


def load(path: Path, size: tuple[int, int] | None = None) -> np.ndarray:
    """Luminance in 0..1, downsampled to `size` (w, h) with a box filter."""
    with Image.open(path) as image:
        image.draft("L", (size[0] * 2 if size else 0, size[1] * 2 if size else 0))
        grey = image.convert("L")
        if size is not None and grey.size != size:
            grey = grey.resize(size, Image.BOX)
        return np.asarray(grey, dtype=np.float32) / 255.0


def read_swap(frames_txt: Path) -> int | None:
    for line in frames_txt.read_text().splitlines():
        match = re.match(r"swap frame (\d+)", line)
        if match:
            return int(match.group(1))
    return None


def best_offset(a: np.ndarray, b: np.ndarray, search: int) -> tuple[tuple[int, int], float, float]:
    """The integer (dx, dy) that best maps `a` onto `b`, plus the correlation there and at (0,0).

    Positive dx means `a` matches a `b` that is shifted to the right (i.e. `a` shows the scene
    further left), so `a` is the same view as `b` displaced by (-dx, -dy).
    """
    h, w = b.shape
    margin = search
    core = b[margin : h - margin, margin : w - margin]
    core = core - core.mean()
    core_norm = np.sqrt((core * core).sum())
    best: tuple[tuple[int, int], float] = ((0, 0), -2.0)
    at_zero = None
    for dy in range(-search, search + 1):
        for dx in range(-search, search + 1):
            window = a[margin + dy : h - margin + dy, margin + dx : w - margin + dx]
            window = window - window.mean()
            denom = np.sqrt((window * window).sum()) * core_norm
            corr = float((window * core).sum() / denom) if denom > 0 else 0.0
            if dx == 0 and dy == 0:
                at_zero = corr
            if corr > best[1]:
                best = ((dx, dy), corr)
    return best[0], best[1], (at_zero if at_zero is not None else 0.0)


def scale_ratio(a: np.ndarray, b: np.ndarray) -> float:
    """A crude scale estimate: the ratio of the images' spatial energy (gradient spread)."""
    ga = np.abs(np.diff(a, axis=1)).mean() + np.abs(np.diff(a, axis=0)).mean()
    gb = np.abs(np.diff(b, axis=1)).mean() + np.abs(np.diff(b, axis=0)).mean()
    return float(ga / gb) if gb > 0 else float("nan")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path, nargs="+")
    parser.add_argument("--window", type=int, default=12)
    parser.add_argument("--search", type=int, default=24)
    parser.add_argument("--size", type=int, default=480, help="downsampled width for the fit")
    args = parser.parse_args()

    for directory in args.directory:
        measure(directory, args)


def measure(directory: Path, args: argparse.Namespace) -> None:
    frames = sorted(
        ((int(m.group(1)), p) for p in directory.glob("f*.png") if (m := FRAME_RE.search(p.name))),
        key=lambda item: item[0],
    )
    if not frames:
        print(f"no frames in {directory}")
        return
    recorded = read_swap(directory / "frames.txt")
    numbers = [n for n, _ in frames]
    print(f"{directory}: {len(frames)} frames f{numbers[0]}..f{numbers[-1]}, frames.txt swap {recorded}")

    # Luminance and frame-to-frame difference over the whole capture, to find the swap by measurement.
    small = {n: load(p, (args.size, args.size * 9 // 16)) for n, p in frames}
    ordered = sorted(small)
    diffs = {
        nxt: float(np.abs(small[nxt] - small[prev]).mean())
        for prev, nxt in zip(ordered, ordered[1:])
    }
    if not diffs:
        print("only one frame")
        return
    measured = max(diffs, key=diffs.get)
    near = max(
        ((n, d) for n, d in diffs.items() if abs(n - recorded) <= args.window),
        key=lambda item: item[1],
        default=(None, float("nan")),
    )
    print(f"largest frame-to-frame change over the capture at f{measured} (mean |d| = {diffs[measured]:.4f}); "
          f"in the window around frames.txt's swap the largest is f{near[0]} ({near[1]:.4f})")
    print(f"  measured swap {near[0]}, frames.txt swap {recorded}"
          f"{'' if near[0] == recorded else f' (off by {(near[0] or 0) - recorded:+d})'}")

    def show(n: int) -> None:
        img = small.get(n)
        if img is None:
            print(f"  f{n}: (missing)")
            return
        print(f"  f{n}: mean {img.mean():.4f}  p50 {np.percentile(img, 50):.4f}  "
              f"p95 {np.percentile(img, 95):.4f}  d(prev) {diffs.get(n, float('nan')):.4f}")

    print("around the recorded swap:")
    for n in range(recorded - 3, recorded + 4):
        show(n)

    # The alignment: the last frame before the swap against a steady frame after it.
    pre = max((n for n in small if n < recorded), default=None)
    post = min((n for n in small if n > recorded + 6), default=None)
    if pre is not None and post is not None:
        dx, dy, corr = best_offset(small[pre], small[post], args.search)
        print(f"alignment f{pre} -> f{post}: best offset (dx={dx}, dy={dy}) corr {corr:.4f}; "
              f"corr at (0,0) {_at_zero(small[pre], small[post]):.4f}; gradient ratio {scale_ratio(small[pre], small[post]):.3f}")
        a = small[pre]
        b = small[post]
        print(f"  brightness: doorway f{pre} mean {a.mean():.4f} p50 {np.percentile(a,50):.4f} "
              f"p95 {np.percentile(a,95):.4f} top1% {_top(a, 1):.4f} | post-swap f{post} mean {b.mean():.4f} "
              f"p50 {np.percentile(b,50):.4f} p95 {np.percentile(b,95):.4f} top1% {_top(b, 1):.4f}")
    if recorded in small and post in small:
        a, b = small[recorded], small[post]
        print(f"pop: swap f{recorded} mean {a.mean():.4f} vs steady f{post} mean {b.mean():.4f} "
              f"({100 * (a.mean() / b.mean() - 1):+.1f}%), corr {_corr(a, b):.4f}")


def _top(image: np.ndarray, percent: float) -> float:
    """The mean of the brightest `percent` of the pixels: a proxy for the brightest surface in view."""
    flat = np.sort(image.ravel())
    count = max(1, int(len(flat) * percent / 100.0))
    return float(flat[-count:].mean())


def _corr(a: np.ndarray, b: np.ndarray) -> float:
    ac = a - a.mean()
    bc = b - b.mean()
    denom = np.sqrt((ac * ac).sum()) * np.sqrt((bc * bc).sum())
    return float((ac * bc).sum() / denom) if denom > 0 else 0.0


def _at_zero(a: np.ndarray, b: np.ndarray) -> float:
    return _corr(a, b)


if __name__ == "__main__":
    main()

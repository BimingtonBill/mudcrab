#!/usr/bin/env python3
"""Copy the UESP reference screenshots into this checkout's git-ignored `local/reference/uesp/`.

Usage:
    python tools/research/copy_reference_shots.py [--source <dir>] [--destination <dir>]

The images are CC BY-SA game screenshots and are never committed: `local/` is git-ignored.  A
worker checkout starts without them, so the shots files under `tools/reference/` (which point at
`local/reference/uesp/<name>.jpg`) and `tools/compare_shots.py` cannot find their inputs until
this has run.  The default source is the lead's checkout; nothing is overwritten unless `--force`
is given, so an existing (possibly newer) copy is left alone.

Prints one line per file copied or skipped, then a summary.
"""

import argparse
import os
import shutil
import sys

DEFAULT_SOURCE = r"C:\Modding\MudcrabProject\local\reference\uesp"
DEFAULT_DESTINATION = os.path.join(
    os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
    "local",
    "reference",
    "uesp",
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default=DEFAULT_SOURCE, help="directory holding the references")
    parser.add_argument("--destination", default=DEFAULT_DESTINATION, help="where to copy them")
    parser.add_argument("--force", action="store_true", help="overwrite an existing copy")
    args = parser.parse_args()

    if not os.path.isdir(args.source):
        raise SystemExit(f"no such source directory: {args.source}")
    os.makedirs(args.destination, exist_ok=True)

    copied, skipped = 0, 0
    for name in sorted(os.listdir(args.source)):
        if not name.lower().endswith((".jpg", ".jpeg", ".png")):
            continue
        source = os.path.join(args.source, name)
        destination = os.path.join(args.destination, name)
        if os.path.exists(destination) and not args.force:
            print(f"  keep    {name}")
            skipped += 1
            continue
        shutil.copyfile(source, destination)
        print(f"  copy    {name} ({os.path.getsize(destination)} bytes)")
        copied += 1

    # SOURCES.txt is the attribution the design doc refers to; carry it along.
    sources = os.path.join(args.source, "SOURCES.txt")
    if os.path.exists(sources):
        shutil.copyfile(sources, os.path.join(args.destination, "SOURCES.txt"))
        print("  copy    SOURCES.txt")

    print(f"  {copied} copied, {skipped} already present, into {args.destination}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

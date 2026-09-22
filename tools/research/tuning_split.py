#!/usr/bin/env python3
"""Split the UESP shots into the calibration **tuning set** and the lead's **holdout**.

Usage:
    python tools/research/tuning_split.py [--shots <file>] [--out-dir <dir>]

The brief fixes the rule: tune only on the interior and Blackreach shots whose `name` ends in an
even digit, plus `SR-place-Blackreach`, `SR-place-Blackreach_02` and `SR-place-Blackreach_04`.
Everything else is the holdout, used only to check the result afterwards.  The Blackreach_04 pose
lives in its own file because the reference is 1718x1080 rather than 4:3, so it is written to a
tuning file of its own with that file's window size.

Writes `<out dir>/tuning_shots.json`, `<out dir>/tuning_blackreach_04_shots.json` and
`<out dir>/holdout_shots.json`, carrying each shot's fields through unchanged.  Prints the split.
"""

import argparse
import json
import os
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
NAMED_TUNING = {"SR-place-Blackreach", "SR-place-Blackreach_02", "SR-place-Blackreach_04"}
# `app.rs` `UNDERGROUND_WORLDSPACES`: Blackreach and the Alftand cavern it is reached through.
# The brief says "interior and Blackreach"; AlftandWorld (0x69857) is the same kind of space as
# Blackreach - an underground worldspace the engine lights with the underground ambient, no sun -
# and the brief names it as a space to compare, so it is tuned here rather than left as a
# holdout in a space nothing had been tuned for.
UNDERGROUND_WORLDSPACES = {0x0001_EE62, 0x0006_9857}


def is_tuning(shot):
    """The brief's rule, in one place."""
    underground = (
        shot.get("interior_cell_id") is not None
        or shot.get("worldspace_id") in UNDERGROUND_WORLDSPACES
    )
    if not underground:
        return False, "exterior (the daylight guard)"
    if shot["name"] in NAMED_TUNING:
        return True, "named by the brief"
    digits = "".join(character for character in shot["name"][::-1] if character.isdigit())[::-1]
    if not digits:
        return False, "no digit in the name"
    if int(digits[-1]) % 2 == 0:
        return True, "even digit"
    return False, "odd digit"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shots", default=os.path.join(REPO, "tools", "reference",
                                                        "uesp_shots_4x3.json"))
    parser.add_argument("--blackreach-04", default=os.path.join(
        REPO, "tools", "reference", "uesp_shots_blackreach_04.json"))
    parser.add_argument("--out-dir", default=os.path.join(REPO, "local", "calib"))
    args = parser.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    with open(args.shots, "r", encoding="utf-8") as handle:
        document = json.load(handle)
    tuning, holdout = [], []
    for shot in document["shots"]:
        keep, why = is_tuning(shot)
        (tuning if keep else holdout).append(shot)
        print(f"  {'tuning ' if keep else 'holdout'} {shot['name']:<32} {why}")

    def write(name, shots, width, height):
        if not shots:
            return
        path = os.path.join(args.out_dir, name)
        with open(path, "w", encoding="utf-8") as handle:
            json.dump({"width": width, "height": height, "shots": shots}, handle, indent=1)
        print(f"  wrote {path} ({len(shots)} shots, {width}x{height})")

    write("tuning_shots.json", tuning, document["width"], document["height"])
    write("holdout_shots.json", holdout, document["width"], document["height"])
    with open(args.blackreach_04, "r", encoding="utf-8") as handle:
        blackreach = json.load(handle)
    write("tuning_blackreach_04_shots.json", blackreach["shots"], blackreach["width"],
          blackreach["height"])
    print(f"  {len(tuning)} tuning, {len(holdout)} holdout (holdout renders are not looked at "
          f"until the end)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

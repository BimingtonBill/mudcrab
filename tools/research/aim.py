#!/usr/bin/env python3
"""Aim a camera at a point: print the shots-file yaw and pitch for a camera position.

Usage:
    python tools/research/aim.py <cam x> <cam y> <cam z> <target x> <target y> <target z>

Prints `yaw` (clockwise from north, Creation +Y), `pitch` (positive looks down, the shots-file
and Skyrim convention) and the distance, so a candidate pose can be generated from a landmark's
database position instead of guessed.  With several targets it prints one line each.
"""

import math
import sys


def aim(camera, target):
    dx = target[0] - camera[0]
    dy = target[1] - camera[1]
    dz = target[2] - camera[2]
    horizontal = math.hypot(dx, dy)
    yaw = math.degrees(math.atan2(dx, dy)) % 360.0
    pitch = -math.degrees(math.atan2(dz, horizontal))
    return yaw, pitch, math.sqrt(horizontal * horizontal + dz * dz)


def main(argv):
    if len(argv) < 7:
        print(__doc__)
        return 2
    values = [float(v) for v in argv[1:7]]
    camera, target = values[:3], values[3:6]
    yaw, pitch, distance = aim(camera, target)
    print(f"  yaw {yaw:.1f}  pitch {pitch:.1f}  distance {distance:.0f}  elevation "
          f"{-pitch:.1f} deg")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

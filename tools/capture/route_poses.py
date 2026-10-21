"""Turn a benchmark route file into a table of camera poses for Skyrim itself.

The engine's `--route` runner moves along straight legs between waypoints at `speed` units per
second, faces the direction of travel and stays `eye_height` above the terrain. This script samples
the same path at a fixed rate so the same flight can be filmed in Skyrim and compared pose for pose
with a Mudcrab run. Terrain heights come from the converted world database (read only): the same
33x33 VHGT height field the engine builds its terrain from, bilinearly interpolated.

    python tools/capture/route_poses.py --route scripts/routes/travel.json \
        --db D:/Mudcrab/SkyrimConverted-p2main/skyrim_world.db --out local/capture/travel-poses

Writes <out>.csv (t, x, y, z, yaw_deg, pitch_deg) and <out>.md (start/ends and a console recipe).
Yaw is Skyrim's: degrees clockwise from north (+y), as `getangle z` reports it.
"""
import argparse
import csv
import json
import math
import sqlite3
import struct
from pathlib import Path

CELL = 4096.0
STEP = CELL / 32.0


class Terrain:
    def __init__(self, db: Path, worldspace_form: str):
        self.conn = sqlite3.connect(f"file:{db.as_posix()}?mode=ro", uri=True)
        self.cache: dict[tuple[int, int], list[list[float]] | None] = {}
        self.ws = self.conn.execute(
            "select id from worldspaces where editor_id = 'Tamriel'"
        ).fetchone()[0]

    def grid(self, gx: int, gy: int):
        key = (gx, gy)
        if key not in self.cache:
            row = self.conn.execute(
                "select l.heightmap from cells c join land l on l.cell_id = c.id "
                "where c.worldspace_id = ? and c.grid_x = ? and c.grid_y = ?",
                (self.ws, gx, gy),
            ).fetchone()
            self.cache[key] = None if row is None else decode_vhgt(row[0])
        return self.cache[key]

    def height(self, x: float, y: float) -> float | None:
        gx, gy = math.floor(x / CELL), math.floor(y / CELL)
        heights = self.grid(gx, gy)
        if heights is None:
            return None
        fx, fy = (x - gx * CELL) / STEP, (y - gy * CELL) / STEP
        ix, iy = min(int(fx), 31), min(int(fy), 31)
        tx, ty = fx - ix, fy - iy
        h00, h10 = heights[iy][ix], heights[iy][ix + 1]
        h01, h11 = heights[iy + 1][ix], heights[iy + 1][ix + 1]
        return (h00 * (1 - tx) + h10 * tx) * (1 - ty) + (h01 * (1 - tx) + h11 * tx) * ty


def decode_vhgt(blob: bytes) -> list[list[float]]:
    offset = struct.unpack("<f", blob[:4])[0]
    deltas = struct.unpack("<1089b", blob[4:4 + 1089])
    rows, column0 = [], 0
    for r in range(33):
        column0 += deltas[r * 33]
        running, row = column0, [0.0] * 33
        row[0] = (offset + running) * 8.0
        for c in range(1, 33):
            running += deltas[r * 33 + c]
            row[c] = (offset + running) * 8.0
        rows.append(row)
    return rows


def sample(route: dict, terrain: Terrain, hz: float):
    wp, speed, eye = route["waypoints"], float(route["speed"]), float(route.get("eye_height", 120))
    legs, total = [], 0.0
    for a, b in zip(wp, wp[1:]):
        length = math.dist(a, b)
        legs.append((a, b, length, total))
        total += length
    last_z = None
    t, dt = 0.0, 1.0 / hz
    while t * speed <= total:
        d = t * speed
        a, b, length, start = next(
            (leg for leg in legs if d <= leg[3] + leg[2]), legs[-1]
        )
        f = min(max((d - start) / length, 0.0), 1.0)
        x, y = a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f
        ground = terrain.height(x, y)
        last_z = last_z if ground is None else ground + eye
        yaw = math.degrees(math.atan2(b[0] - a[0], b[1] - a[1])) % 360.0
        yield t, x, y, last_z, yaw
        t += dt
    return total


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--route", required=True, type=Path)
    ap.add_argument("--db", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--hz", type=float, default=30.0)
    args = ap.parse_args()
    route = json.loads(args.route.read_text(encoding="utf-8"))
    terrain = Terrain(args.db, route.get("worldspace", "0x3c"))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    rows = list(sample(route, terrain, args.hz))
    with args.out.with_suffix(".csv").open("w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["t", "x", "y", "z", "yaw_deg", "pitch_deg"])
        for t, x, y, z, yaw in rows:
            w.writerow([f"{t:.4f}", f"{x:.2f}", f"{y:.2f}", "" if z is None else f"{z:.2f}", f"{yaw:.2f}", "0"])
    missing = sum(1 for r in rows if r[3] is None)
    print(f"{len(rows)} poses, {rows[-1][0]:.1f} s, {missing} without ground -> {args.out.with_suffix('.csv')}")


if __name__ == "__main__":
    main()

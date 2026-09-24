#!/usr/bin/env python3
"""Ball-park camera poses for reference screenshots, from the place name alone, in seconds.

Usage:
    python tools/research/pose_ballpark.py <reference folder> <out prefix> [--db <world db>]

For every `SR-place-*` / `SR-interior-*` image in the folder the place name is looked up in the
world database, and a pose that frames it is written. The output is two shots files,
`<out prefix>_4x3.json` and `<out prefix>_wide.json`, one per reference aspect. A shot the
lookup cannot place starts high above Skyrim (confidence `unplaced`) for the user to fly from or
skip. Shots already in the output files are kept as they are; only new screenshots are added.

How a place is found, first match wins (an interior screenshot tries only step 3):
1. **City or realm**: a worldspace whose editor id is exactly `<name>World` (a city such as
   `WhiterunWorld`, so a city is not found through one of its jail doors). The camera looks at
   the centroid of its references from 3,500 out and 1,500 up.
2. **Exterior with an entrance**: a load door in a worldspace whose destination interior matches
   the name, as dungeons, caves, mines and forts have. The camera stands 2,500 units out from the
   door along the door's facing and 900 up, looking back at it.
3. **Interior**: a cell whose editor id contains the name (spaces and punctuation dropped; the
   id ending in the name wins, then the shortest). The camera stands in the room at eye height
   above its floor and looks along the longer axis of the placed objects.
4. **Any worldspace containing the name** (a DLC realm), framed as in step 1.
These are ball-parks for the automatic check and the by-hand tool
(docs/design/reference-pose-pipeline.md), not fits.
"""

import json
import math
import os
import re
import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"
# Places UESP names by region or by a name the game data doesn't use, pointed at a named place
# nearby. A ball-park only has to put the user within flying distance.
HINTS = {
    "therift": "riften",
    "thereach": "markarth",
    "whiterunhold": "whiterun",
    "whiterunredsky": "whiterun",
    "throatoftheworld": "highhrothgar",
    "seaofghosts": "winterhold",
    "dayspringcanyon": "dlc1dawnguardhq",
    "forgottenvale": "dlc01falmervalley",
    "cronvangrcave": "cronvangrhall",
    "ravenrock": "solstheim",
}
# Where a shot the lookup cannot place starts: high over the middle of Skyrim, for the user to
# fly from or skip with X.
UNPLACED = {"worldspace_id": 60, "interior_cell_id": None, "position": (0.0, 0.0, 30000.0), "yaw": 0.0, "pitch": 30.0}
IMAGE = re.compile(r"^SR-(place|interior|photo)-(.+?)(_\d+)?\.(jpe?g|png)$", re.IGNORECASE)


def squash(text):
    return re.sub(r"[^a-z0-9]", "", text.lower())


def aim(camera, target):
    dx, dy, dz = (target[i] - camera[i] for i in range(3))
    yaw = math.degrees(math.atan2(dx, dy)) % 360.0
    pitch = -math.degrees(math.atan2(dz, math.hypot(dx, dy)))
    return round(yaw, 1), round(max(-80.0, min(80.0, pitch)), 1)


class World:
    def __init__(self, path):
        self.db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        self.cells = [
            (cell_id, name.rstrip("\x00"), squash(name))
            for cell_id, name in self.db.execute(
                "SELECT id, interior_name FROM cells WHERE worldspace_id IS NULL AND interior_name IS NOT NULL"
            )
        ]
        self.worlds = [(wid, editor, squash(editor)) for wid, editor in self.db.execute("SELECT id, editor_id FROM worldspaces")]
        # Developer test and demo spaces ship in the game data too (`E3demoBleakFallsBarrow`,
        # `QASmoke`, `CWTestHold`); a screenshot never shows one.
        dev = re.compile(r"e3demo|qasmoke|^qa|test|debug|dummy|zzz|delete", re.IGNORECASE)
        self.cells = [row for row in self.cells if not dev.search(row[1])]
        self.worlds = [row for row in self.worlds if not dev.search(row[1])]

    # Editor ids (squashed) a lookup must not return: places already tried and found wrong.
    exclude = frozenset()

    def best(self, rows, key):
        # Prefer an exact match, then the shortest editor id containing the name (the main
        # room rather than `...Basement`), then any id the name contains.
        rows = [row for row in rows if row[2] not in self.exclude]
        exact = [row for row in rows if row[2] == key]
        if exact:
            return exact[0]
        containing = sorted(
            (row for row in rows if key in row[2]),
            key=lambda row: (not row[2].endswith(key), len(row[2])),
        )
        return containing[0] if containing else None

    def names_like(self, words, limit=60):
        """Interior cell and worldspace editor ids sharing a word (4+ letters) with `words`."""
        tokens = {w for w in (squash(word) for word in words) if len(w) >= 4}
        names = [editor for _, editor, squashed in self.cells + self.worlds if any(t in squashed for t in tokens)]
        # A city seen from outside its walls is in Tamriel, not in the city's worldspace: offer
        # `Outside:<CityWorld>` for every worldspace a Tamriel door leads into.
        cities = {wid: editor for wid, editor, squashed in self.worlds if any(t in squashed for t in tokens)}
        for world_id, editor in cities.items():
            if self.db.execute(
                "SELECT 1 FROM door_links d JOIN \"references\" r ON r.id = d.ref_id "
                "WHERE d.destination_worldspace_id = ? AND r.worldspace_id = 60 LIMIT 1", (world_id,)
            ).fetchone():
                names.append(f"Outside:{editor}")
        return sorted(set(names), key=len)[:limit]

    def interior(self, key):
        cell = self.best(self.cells, key)
        if not cell:
            return None
        points = self.db.execute(
            "SELECT pos_x, pos_y, pos_z FROM \"references\" WHERE cell_id = ?", (cell[0],)
        ).fetchall()
        if len(points) < 3:
            return None
        xs, ys, zs = zip(*points)
        centre = (sum(xs) / len(xs), sum(ys) / len(ys))
        floor = sorted(zs)[len(zs) // 10]
        spread_x, spread_y = max(xs) - min(xs), max(ys) - min(ys)
        if spread_x >= spread_y:
            camera = (centre[0] - spread_x * 0.35, centre[1], floor + 150)
            target = (centre[0] + spread_x * 0.35, centre[1], floor + 150)
        else:
            camera = (centre[0], centre[1] - spread_y * 0.35, floor + 150)
            target = (centre[0], centre[1] + spread_y * 0.35, floor + 150)
        yaw, pitch = aim(camera, target)
        return {"interior_cell_id": cell[0], "worldspace_id": None, "position": camera, "yaw": yaw, "pitch": pitch}, f"interior {cell[1]}"

    def entrance(self, key):
        cell = self.best(self.cells, key)
        if not cell:
            return None
        door = self.db.execute(
            "SELECT r.pos_x, r.pos_y, r.pos_z, r.rot_z, r.worldspace_id FROM door_links d "
            "JOIN \"references\" r ON r.id = d.ref_id "
            "WHERE d.destination_cell_id = ? AND r.worldspace_id IS NOT NULL LIMIT 1",
            (cell[0],),
        ).fetchone()
        if not door:
            return None
        x, y, z, rot_z, world = door
        # A Creation door faces along its clockwise heading (see transition::arrival_camera_rotation).
        forward = (math.sin(rot_z), math.cos(rot_z))
        camera = (x + forward[0] * 2500, y + forward[1] * 2500, z + 900)
        yaw, pitch = aim(camera, (x, y, z + 200))
        return {"worldspace_id": world, "interior_cell_id": None, "position": camera, "yaw": yaw, "pitch": pitch}, f"entrance to {cell[1]}"

    def city(self, key):
        # A city is its own worldspace (`WhiterunWorld`); only an exact `<name>world` match
        # counts, so `Riften Fishery` still finds the fishery's own door.
        for world_id, editor, squashed in self.worlds:
            if squashed in (key, key + "world") and squashed not in self.exclude:
                return self.worldspace(squashed)
        return None

    def outside(self, key):
        """The open world just outside a city: a door in Tamriel whose destination is the city's
        own worldspace (a city gate), seen from out on the approach. For screenshots of a city
        taken from outside its walls, which the city's worldspace itself cannot show."""
        for world_id, editor, squashed in self.worlds:
            if squashed in (key, key + "world") and squashed not in self.exclude:
                door = self.db.execute(
                    "SELECT r.pos_x, r.pos_y, r.pos_z, r.rot_z FROM door_links d "
                    "JOIN \"references\" r ON r.id = d.ref_id "
                    "WHERE d.destination_worldspace_id = ? AND r.worldspace_id = 60 LIMIT 1",
                    (world_id,),
                ).fetchone()
                if not door:
                    return None
                x, y, z, rot_z = door
                forward = (math.sin(rot_z), math.cos(rot_z))
                camera = (x + forward[0] * 4000, y + forward[1] * 4000, z + 1500)
                yaw, pitch = aim(camera, (x, y, z + 600))
                pose = {"worldspace_id": 60, "interior_cell_id": None, "position": camera, "yaw": yaw, "pitch": pitch}
                return pose, f"outside Outside:{editor}"
        return None

    def worldspace(self, key):
        world = self.best(self.worlds, key)
        if not world:
            return None
        row = self.db.execute(
            "SELECT AVG(pos_x), AVG(pos_y), AVG(pos_z), COUNT(*) FROM \"references\" WHERE worldspace_id = ?",
            (world[0],),
        ).fetchone()
        if not row or not row[3]:
            return None
        target = (row[0], row[1], row[2])
        camera = (target[0], target[1] - 3500, target[2] + 1500)
        yaw, pitch = aim(camera, target)
        return {"worldspace_id": world[0], "interior_cell_id": None, "position": camera, "yaw": yaw, "pitch": pitch}, f"worldspace {world[1]}"


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    folder, prefix = argv[1], argv[2]
    db = argv[argv.index("--db") + 1] if "--db" in argv else DB
    world = World(db)
    from PIL import Image

    files = {"4x3": {"width": 1400, "height": 1050, "shots": []}, "wide": {"width": 1400, "height": 788, "shots": []}}
    # Shots already in the output keep their pose (they may have been graded or placed by hand);
    # only new screenshots are added.
    existing = set()
    for tag, data in files.items():
        path = f"{prefix}_{tag}.json"
        if os.path.exists(path):
            data["shots"] = json.load(open(path, encoding="utf-8"))["shots"]
            existing |= {shot["name"] for shot in data["shots"]}
    missed = []
    for name in sorted(os.listdir(folder)):
        match = IMAGE.match(name)
        if not match:
            continue
        kind, place = match.group(1).lower(), match.group(2).replace("_", " ")
        key = HINTS.get(squash(place), squash(place))
        order = (
            [world.interior]
            if kind == "interior"
            else [world.city, world.entrance, world.interior, world.worldspace]
        )
        found = None
        for lookup in order:
            found = lookup(key)
            if found:
                break
        if not found:
            missed.append(name)
            found = (UNPLACED, "place not found: fly there from above Skyrim, or press X to skip")
        pose, how = found
        with Image.open(os.path.join(folder, name)) as image:
            aspect = image.width / image.height
        stem = os.path.splitext(name)[0]
        shot = {
            "name": stem,
            "worldspace_id": pose["worldspace_id"],
            "interior_cell_id": pose["interior_cell_id"],
            "position": [round(value, 1) for value in pose["position"]],
            "yaw": pose["yaw"],
            "pitch": pose["pitch"],
            "hfov": 75.0 if aspect < 1.5 else 90.0,
            "reference": os.path.relpath(os.path.join(folder, name), start=os.getcwd()).replace("\\", "/"),
            "note": f"Ball-park from the place name ({how}) by tools/research/pose_ballpark.py; not yet checked.",
            "confidence": "ballpark" if pose is not UNPLACED else "unplaced",
        }
        target = files["4x3" if aspect < 1.5 else "wide"]
        if stem in existing:
            continue
        target["shots"].append(shot)
        print(f"{stem:45} {how}")
    for tag, data in files.items():
        with open(f"{prefix}_{tag}.json", "w", encoding="utf-8", newline="\n") as handle:
            json.dump(data, handle, indent=2, ensure_ascii=False)
            handle.write("\n")
    total = sum(len(data["shots"]) for data in files.values())
    print(f"{total} shots in the files; not placed by name (start above Skyrim): {', '.join(missed) or 'none'}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

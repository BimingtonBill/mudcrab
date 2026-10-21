#!/usr/bin/env python3
"""Field statistics over the Spriggit dump of vanilla Skyrim: step 2 of the parity matrix.

For every record type and field path: how many records carry the field, the numeric range or the
distinct values, and three example EditorIDs. Only the winning version of each record counts: the
plugin loaded last (Skyrim, Update, Dawnguard, HearthFires, Dragonborn), which is what the game sees.

  python tools/parity/field_stats.py [--root D:/Mudcrab/skyrim-records] [--out D:/Mudcrab/parity/field_stats.json]

Field paths use dots for nesting and [] for list items: Name.Values[].String. The dump is a
generated text tree (see tools/skyrim_rec.py); this reads it with a small indentation scanner, not
a YAML library, because a full parse of every file is slow and only keys and scalars are needed.
"""
import argparse
import json
import os
import re
import sys
from collections import Counter
from multiprocessing import Pool

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from skyrim_rec import DEFAULT_ROOT, records  # noqa: E402

LOAD_ORDER = ["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"]
KEY = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*):(?: (.*))?$")
MAX_DISTINCT = 25
NUMBER = re.compile(r"^-?\d+(\.\d+)?$")


def scan(path):
    """Return {field path: scalar values seen} for one record file; a path with no value maps to []."""
    paths = {}
    stack = []  # (indent, name)
    with open(path, encoding="utf-8") as handle:
        for raw in handle:
            line = raw.rstrip("\n")
            body = line.lstrip(" ")
            if not body:
                continue
            indent = len(line) - len(body)
            if body.startswith("- "):
                while stack and stack[-1][0] > indent:
                    stack.pop()
                stack.append((indent + 1, "[]"))
                body, indent = body[2:], indent + 2
            elif body == "-":
                while stack and stack[-1][0] > indent:
                    stack.pop()
                stack.append((indent + 1, "[]"))
                continue
            hit = KEY.match(body)
            # A list marker sits one column right of its dash, so item keys (dash + 2) keep it
            # and a key back at the parent's column closes the list.
            while stack and stack[-1][0] >= indent:
                stack.pop()
            if not hit:
                if body and stack:  # scalar list item or a continuation line
                    name = ".".join(n for _i, n in stack).replace(".[]", "[]")
                    paths.setdefault(name, []).append(body)
                continue
            key, value = hit.group(1), hit.group(2)
            name = ".".join([n for _i, n in stack] + [key]).replace(".[]", "[]")
            if value is None or value == "":
                paths.setdefault(name, [])
                stack.append((indent, key))
            else:
                paths.setdefault(name, []).append(value)
    return paths


def work(job):
    rtype, edid, path = job
    return rtype, edid, scan(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", default=DEFAULT_ROOT)
    parser.add_argument("--out", default="D:/Mudcrab/parity/field_stats.json")
    parser.add_argument("--limit", type=int, default=0, help="stop after N records (testing)")
    args = parser.parse_args()

    winners = {}  # (type, FormKey) -> (rank, edid, path)
    present = [p for p in LOAD_ORDER if os.path.isdir(os.path.join(args.root, p))]
    for plug, rtype, edid, fid, path in records(args.root):
        rank = LOAD_ORDER.index(plug) if plug in LOAD_ORDER else -1
        # The FormKey master is in the file name, so overrides share a key with the original.
        key = (rtype, fid, os.path.basename(os.path.dirname(path)) if path.endswith("RecordData.yaml")
               else os.path.basename(path))
        key = (rtype, re.sub(r"^.* - ", "", key[2]))
        if key not in winners or winners[key][0] <= rank:
            winners[key] = (rank, edid, path)
    jobs = [(k[0], v[1], v[2]) for k, v in sorted(winners.items())]
    if args.limit:
        jobs = jobs[: args.limit]
    print(f"{len(jobs)} winning records from {', '.join(present)}", flush=True)

    types = {}
    with Pool() as pool:
        for rtype, edid, paths in pool.imap_unordered(work, jobs, chunksize=200):
            entry = types.setdefault(rtype, {"records": 0, "fields": {}})
            entry["records"] += 1
            for name, values in paths.items():
                field = entry["fields"].setdefault(
                    name, {"count": 0, "examples": [], "distinct": Counter(), "min": None, "max": None})
                field["count"] += 1
                if edid and len(field["examples"]) < 3:
                    field["examples"].append(edid)
                for value in values:
                    if NUMBER.match(value):
                        number = float(value)
                        field["min"] = number if field["min"] is None else min(field["min"], number)
                        field["max"] = number if field["max"] is None else max(field["max"], number)
                    if len(field["distinct"]) <= MAX_DISTINCT:
                        field["distinct"][value] += 1

    for entry in types.values():
        for field in entry["fields"].values():
            many = len(field["distinct"]) > MAX_DISTINCT
            field["distinct"] = None if many else dict(field["distinct"].most_common())
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump({"plugins": present, "types": types}, handle, indent=1, sort_keys=True)
    print(f"{len(types)} record types, {sum(len(t['fields']) for t in types.values())} field paths -> {args.out}")


if __name__ == "__main__":
    main()

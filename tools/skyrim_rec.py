#!/usr/bin/env python3
"""Look up vanilla Skyrim records in the Spriggit text dump.

The dump holds one YAML file per record, named "<EditorID> - <FormID6>_<Plugin>.yaml", under
<root>/<Plugin>/<RecordType>/. Records have no load-order prefix: the FormID is the 6 hex digits
the plugin that defines the record uses, and an override in Update.esm keeps the master's FormID.

  python tools/skyrim_rec.py find <text>        list records whose EditorID contains <text>
  python tools/skyrim_rec.py find <text> --type Doors --plugin Skyrim.esm
  python tools/skyrim_rec.py show 0003B6B       print the record(s) with this FormID
  python tools/skyrim_rec.py show <EditorID>    print the record when exactly one matches
  python tools/skyrim_rec.py sheet <SIG|Type>   one page per record type: vanilla fields, converter, schema
  python tools/skyrim_rec.py index              list every record type with coverage (read this first)
  python tools/skyrim_rec.py refresh            rebuild field stats and sheets (about 5 minutes)
  python tools/skyrim_rec.py dump               (re)build the dump with Spriggit (minutes)

Search the content of the whole dump with ripgrep: rg -i "<term>" <root>.
Root: --root, else $SKYRIM_RECORDS, else D:/Mudcrab/skyrim-records.
Spriggit CLI (0.41.0, Mutagen-Modding/Spriggit, GPL) is run as an external tool; its code is not
part of Mudcrab. Needs the .NET 10 SDK. Reads the game install, never writes to it.
"""
import argparse
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import zlib

DEFAULT_ROOT = "D:/Mudcrab/skyrim-records"
DEFAULT_PARITY = "D:/Mudcrab/parity"
DEFAULT_SPRIGGIT = "C:/Tools/Spriggit/cli/Spriggit.CLI.exe"
DEFAULT_DATA = "C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data"
PLUGINS = ["Skyrim", "Update", "Dawnguard", "HearthFires", "Dragonborn"]
SPRIGGIT_PACKAGE, SPRIGGIT_VERSION = "Spriggit.Yaml.Skyrim", "0.41.0"
NAME = re.compile(r"^(?:(?P<edid>.*) - )?(?P<fid>[0-9A-Fa-f]{6})_(?P<plugin>.+?\.es[mpl])(?:\.yaml)?$")
MAX_LISTED = 40


def records(root, plugin=None, rtype=None):
    """Yield (plugin, type, editor_id, form_id, yaml_path) for every record under root.

    A record is a flat "<EditorID> - <FormID6>_<Plugin>.yaml" file, or a directory of the same name
    (without .yaml) holding RecordData.yaml (cells, worldspaces, dialogue topics...). The type is
    the top folder; a record nested inside another record directory gets ".child" appended
    (cells inside a worldspace are "Worldspaces.child"). Group folders (GroupRecordData.yaml) are
    not records.
    """
    for plug_dir in sorted(os.listdir(root)):
        plug_path = os.path.join(root, plug_dir)
        if not os.path.isdir(plug_path):
            continue
        if plugin and plug_dir.lower() not in (plugin.lower(), plugin.lower() + ".esm"):
            continue
        for top in sorted(os.listdir(plug_path)):
            top_path = os.path.join(plug_path, top)
            if not os.path.isdir(top_path) or (rtype and top.lower() != rtype.lower()):
                continue
            yield from _walk_type(plug_dir, top, top_path, False)


def _walk_type(plug, top, here, inside_record):
    entries = sorted(os.listdir(here))
    for name in entries:
        path = os.path.join(here, name)
        if os.path.isdir(path):
            hit = NAME.match(name)
            if hit and os.path.isfile(os.path.join(path, "RecordData.yaml")):
                yield (plug, top + (".child" if inside_record else ""), hit["edid"] or "",
                       hit["fid"].upper(), os.path.join(path, "RecordData.yaml"))
                yield from _walk_type(plug, top, path, True)
            else:
                yield from _walk_type(plug, top, path, inside_record)
        elif name.endswith(".yaml") and not name.endswith("RecordData.yaml"):
            hit = NAME.match(name)
            if hit:
                yield (plug, top + (".child" if inside_record else ""), hit["edid"] or "",
                       hit["fid"].upper(), path)


def select(root, query, plugin, rtype):
    fid = query.upper().lstrip("0X") if re.fullmatch(r"(0[xX])?[0-9A-Fa-f]{1,8}", query) else None
    out = []
    for rec in records(root, plugin, rtype):
        if fid is not None and rec[3].lstrip("0") == fid.lstrip("0"):
            out.append(rec)
        elif fid is None and query.lower() in rec[2].lower():
            out.append(rec)
    exact = [rec for rec in out if fid is None and rec[2].lower() == query.lower()]
    return exact or out


def show(recs, root):
    for plug, kind, edid, fid, path in recs:
        print(f"===== {plug} / {kind} / {edid or '(no EditorID)'} / {fid}  ({os.path.relpath(path, root)})")
        with open(path, encoding="utf-8") as handle:
            print(handle.read().rstrip())


def store_deleted_compressed(src, dst):
    """Copy a plugin, storing every compressed record that is also flagged deleted uncompressed.

    Spriggit 0.41.0 (Mutagen) cannot read such a record ("unsupported compression method") although
    the zlib data is valid: Dawnguard.esm has exactly one (NPC 07932F). Group sizes are rebuilt.
    Returns how many records were rewritten.
    """
    data = open(src, "rb").read()
    count = 0

    def rebuild(off, end):
        nonlocal count
        out = []
        while off < end:
            tag, size = data[off:off + 4], struct.unpack_from("<I", data, off + 4)[0]
            if tag == b"GRUP":
                inner = rebuild(off + 24, off + size)
                out.append(data[off:off + 4] + struct.pack("<I", 24 + len(inner)) + data[off + 8:off + 24] + inner)
                off += size
                continue
            flags = struct.unpack_from("<I", data, off + 8)[0]
            if flags & 0x40000 and flags & 0x20:
                body = zlib.decompress(data[off + 28:off + 24 + size])
                out.append(data[off:off + 4] + struct.pack("<II", len(body), flags & ~0x40000) + data[off + 12:off + 24] + body)
                count += 1
            else:
                out.append(data[off:off + 24 + size])
            off += 24 + size
        return b"".join(out)

    head = 24 + struct.unpack_from("<I", data, 4)[0]  # the TES4 header record stays as it is
    with open(dst, "wb") as handle:
        handle.write(data[:head] + rebuild(head, len(data)))
    return count


def dump(root, spriggit, data):
    os.makedirs(root, exist_ok=True)
    failed = []
    for name in PLUGINS:
        out = f"{root}/{name}.esm"
        cmd = [spriggit, "serialize", "-i", f"{data}/{name}.esm", "-o", out,
               "-g", "SkyrimSE", "-p", SPRIGGIT_PACKAGE, "-v", SPRIGGIT_VERSION]
        print("running:", " ".join(cmd), flush=True)
        if subprocess.run(cmd, check=False).returncode != 0:
            shutil.rmtree(out, ignore_errors=True)  # never leave a partial plugin: lookups would miss records
            scratch = os.path.join(root, "_patched")
            os.makedirs(scratch, exist_ok=True)
            patched = os.path.join(scratch, f"{name}.esm")
            if store_deleted_compressed(f"{data}/{name}.esm", patched):
                print(f"retrying {name}.esm with its compressed deleted records stored uncompressed", flush=True)
                cmd[cmd.index("-i") + 1] = patched
                if subprocess.run(cmd, check=False).returncode == 0:
                    continue
            failed.append(name)
            shutil.rmtree(out, ignore_errors=True)
    shutil.rmtree(os.path.join(root, "_patched"), ignore_errors=True)
    if failed:
        sys.exit(f"Spriggit failed on: {', '.join(failed)} (partial output removed)")


def stale_notice(parity, root):
    """One line on stderr when the sheets lag the converter code or the dump."""
    try:
        with open(os.path.join(parity, "BUILD.json"), encoding="utf-8") as handle:
            build = json.load(handle)
    except (OSError, ValueError):
        return
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    changed = subprocess.run(["git", "-C", repo, "diff", "--name-only", build["converter_commit"], "HEAD", "--",
                              "crates/converter/src", "docs/research/data"], capture_output=True, text=True).stdout.split()
    newest = max((os.path.getmtime(os.path.join(root, d)) for d in os.listdir(root)
                  if os.path.isdir(os.path.join(root, d))), default=0) if os.path.isdir(root) else 0
    if changed or newest > build["stats_mtime"]:
        why = f"{len(changed)} converter or inventory file(s) changed since {build['converter_commit']}" if changed else "the dump is newer"
        print(f"NOTE: sheets may be stale ({why}). Refresh: python tools/skyrim_rec.py refresh", file=sys.stderr)


def read_or_exit(path):
    if not os.path.exists(path):
        sys.exit(f"Missing {path}. Build it with: python tools/parity/build_sheets.py")
    with open(path, encoding="utf-8") as handle:
        return handle.read().rstrip()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", default=os.environ.get("SKYRIM_RECORDS", DEFAULT_ROOT))
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("find", "show"):
        cmd = sub.add_parser(name)
        cmd.add_argument("query", help="FormID (hex) or part of an EditorID")
        cmd.add_argument("--plugin", help="Skyrim, Update, Dawnguard, HearthFires, Dragonborn")
        cmd.add_argument("--type", help="record folder, e.g. Doors, Cells, Weathers")
    sub.add_parser("dump")
    sub.add_parser("index")
    sub.add_parser("refresh")
    sheet_cmd = sub.add_parser("sheet")
    sheet_cmd.add_argument("what", help="record signature (STAT) or type folder (Statics)")
    args = parser.parse_args()
    if args.cmd == "dump":
        dump(args.root, os.environ.get("SPRIGGIT_CLI", DEFAULT_SPRIGGIT),
             os.environ.get("SKYRIM_DATA", DEFAULT_DATA))
        return
    if args.cmd == "refresh":
        here = os.path.dirname(os.path.abspath(__file__))
        for script in ("field_stats.py", "build_sheets.py"):
            if subprocess.run([sys.executable, os.path.join(here, "parity", script)], check=False).returncode:
                sys.exit(f"{script} failed")
        return
    if args.cmd in ("index", "sheet"):
        parity = os.environ.get("SKYRIM_PARITY", DEFAULT_PARITY)
        stale_notice(parity, args.root)
        if args.cmd == "index":
            return print(read_or_exit(os.path.join(parity, "INDEX-records.md")))
        folder = os.path.join(parity, "sheets")
        names = sorted(os.listdir(folder)) if os.path.isdir(folder) else []
        want = args.what.lower()
        hits = [n for n in names if n[:-3].lower().split("_", 1)[0] == want.rstrip("_")
                or n[:-3].lower().split("_", 1)[-1] == want or n[:-3].lower() == want]
        if not hits:
            sys.exit(f"No sheet for {args.what!r}. See: python tools/skyrim_rec.py index  "
                     f"(build with: python tools/parity/build_sheets.py)")
        for name in hits:
            print(read_or_exit(os.path.join(folder, name)))
        return
    if not os.path.isdir(args.root):
        sys.exit(f"No dump at {args.root}. Build it with: python tools/skyrim_rec.py dump")
    recs = select(args.root, args.query, args.plugin, args.type)
    if not recs:
        sys.exit(f"No record matches {args.query!r}")
    if args.cmd == "find" or (len({r[3] for r in recs}) > 1):
        for plug, kind, edid, fid, _path in recs[:MAX_LISTED]:
            print(f"{fid}  {plug:12} {kind:24} {edid}")
        if len(recs) > MAX_LISTED:
            print(f"... {len(recs) - MAX_LISTED} more; narrow with --type / --plugin")
        if args.cmd == "show":
            print("Several records match: show one by FormID.")
        return
    show(recs, args.root)


if __name__ == "__main__":
    main()

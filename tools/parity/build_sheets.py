#!/usr/bin/env python3
"""Build the agent-facing record sheets: one page per record type joining four sources.

  vanilla      tools/parity/field_stats.py output (the Spriggit dump of the five official masters)
  converter    docs/research/data/parity-converter-fields.tsv (what Phase 1 reads, with file:line)
  schema       PR 204's in-house record schema, cached read-only under <out>/schema-pr204 (unmerged)
  aliases      tools/parity/field_aliases.json: hand-checked links the name match cannot find

  python tools/parity/build_sheets.py [--stats D:/Mudcrab/parity/field_stats.json] [--out D:/Mudcrab/parity]

Output goes to <out>/sheets/<SIG>_<Type>.md and <out>/INDEX-records.md. It holds statistics and
example EditorIDs derived from the user's own game files, so it stays outside the repository.
Read it with: python tools/skyrim_rec.py sheet <SIG|Type>   and   python tools/skyrim_rec.py index
"""
import argparse
import csv
import json
import os
import re
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
BOOKKEEPING = {"FormKey", "VersionControl", "FormVersion", "Version2", "IsCompressed",
               "MajorRecordFlagsRaw", "SkyrimMajorRecordFlags", "SkyrimMajorRecordFlags[]"}
DEEP_LIMIT = 70  # list nested paths only for types with at most this many field paths


def load_type_map():
    with open(os.path.join(HERE, "type_map.json"), encoding="utf-8") as handle:
        return json.load(handle)


def load_aliases():
    path = os.path.join(HERE, "field_aliases.json")
    if not os.path.exists(path):
        return {}
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def nested_names(node, defs, found, offset=0, seen=()):
    """Collect (name, offset) for every named member inside one schema field, following definitions."""
    if not isinstance(node, dict):
        return
    ref = node.get("definition")
    if ref and ref in defs and ref not in seen:
        nested_names(defs[ref], defs, found, offset, seen + (ref,))
    for key in ("fields", "members"):
        for child in node.get(key) or []:
            if isinstance(child, dict):
                here = offset + (child.get("offset") or 0)
                if child.get("name"):
                    found.append((child["name"], here))
                nested_names(child, defs, found, here, seen)


def load_schema(out):
    """signature -> list of {sub, name, kind, nested: [(name, offset)]}, from the cached PR 204 schema."""
    folder = os.path.join(out, "schema-pr204")
    recs = {}
    if not os.path.isdir(folder):
        return recs, "(no schema cache)"
    for name in sorted(os.listdir(folder)):
        if not name.endswith(".json"):
            continue
        with open(os.path.join(folder, name), encoding="utf-8") as handle:
            doc = json.load(handle)
        defs = doc.get("definitions", {})
        for rec in doc.get("records", []):
            fields = recs.setdefault(rec["signature"], [])
            for fld in rec.get("fields", []):
                found = []
                nested_names(fld, defs, found)
                fields.append({"sub": fld.get("signature"), "name": fld.get("name"), "kind": fld.get("kind"),
                               "nested": found})
    prov = ""
    ppath = os.path.join(folder, "PROVENANCE.txt")
    if os.path.exists(ppath):
        prov = open(ppath, encoding="utf-8").read().strip()
    return recs, prov


def load_converter(tsv):
    rows = {}
    if not os.path.exists(tsv):
        return rows
    with open(tsv, encoding="utf-8") as handle:
        for row in csv.DictReader(handle, delimiter="\t"):
            for rec in re.split(r"[|]", row["record_sig"]):
                rows.setdefault(rec.strip(), []).append(row)
    return rows


def load_engine(tsv):
    """(table, column) -> engine_use, from the engine inventory (parity-engine-use.tsv)."""
    uses = {}
    if os.path.exists(tsv):
        with open(tsv, encoding="utf-8") as handle:
            for row in csv.DictReader(handle, delimiter="	"):
                uses[(row["table"].strip('"').lower(), row["column"].strip('"').lower())] = row["engine_use"]
    return uses


def engine_status(stored_in, uses):
    """Summarise what the engine does with the table.columns named in a converter row's stored_in text."""
    if not uses:
        return "?"
    groups = {}
    table = None
    for tab, col in re.findall(r"(?:(\w+)\.|(?<![\w.])\.)(\w+)", stored_in):
        table = tab.lower() if tab else table
        use = uses.get((table, col.lower())) if table else None
        if use:
            groups.setdefault(use, []).append(f"{table}.{col}")
    if not groups:
        if stored_in.strip() in ("-", ""):
            return "-"
        # Free text that only names a table ("matos (eleven float columns)"): summarise that table.
        for word in re.findall(r"[a-z_]{3,}", stored_in.lower()):
            counts = {}
            for (tab, _col), use in uses.items():
                if tab == word:
                    counts[use] = counts.get(use, 0) + 1
            if counts:
                return f"table {word}: " + ", ".join(f"{use} {n}" for use, n in sorted(counts.items()))
        return "?"
    if len(groups) == 1:
        return next(iter(groups))
    return "; ".join(f"{use}: {', '.join(cols)}" for use, cols in groups.items())


def norm(text):
    return re.sub(r"[^a-z0-9]", "", text.lower())


def match(field, schema_fields, aliases):
    """Return (subrecord signature, how, offset text, schema kind) for a vanilla field."""
    if field in aliases:
        return aliases[field], "alias", "", ""
    key = norm(field)
    for item in schema_fields:
        if item["name"] and norm(item["name"]) == key:
            return item["sub"], "name", "", item["kind"] or ""
    for item in schema_fields:
        for nname, offset in item["nested"]:
            if norm(nname) == key:
                return item["sub"], "member", f"+{offset:#x}", item["kind"] or ""
    for item in schema_fields:
        for nname in [item["name"] or ""] + [n for n, _o in item["nested"]]:
            other = norm(nname)
            if len(other) >= 5 and (other in key or key in other):
                return item["sub"], "name~", "", item["kind"] or ""
    return None, "", "", ""


def bookkeeping(name):
    """Mutagen's own bookkeeping fields, not record data."""
    return name in BOOKKEEPING or name.endswith("DataTypeState")


def short(field):
    if field["distinct"]:
        return ", ".join(f"{k} ({v})" if v > 1 else k for k, v in list(field["distinct"].items())[:5])[:70]
    if field["min"] is not None:
        return f"{field['min']:g}..{field['max']:g}"
    return ""


def sheet(rtype, sig, entry, schema, conv, aliases, prov, stats_head, engine):
    total = entry["records"]
    lines = [f"# {sig}  {rtype}", "",
             f"Vanilla: {total} winning records ({stats_head}). Converter and schema columns: see sources below.",
             f"Sources: dump stats (field_stats.json); converter inventory (parity-converter-fields.tsv, file:line); "
             f"schema {prov or 'none'}; engine use: parity-engine-use.tsv (columns the converter writes, read in crates/engine and crates/shared).", ""]
    fields = entry["fields"]
    top = {k: v for k, v in fields.items() if "." not in k and "[" not in k}
    skip = sorted(name for name in fields if bookkeeping(name) and "." not in name and "[" not in name)
    sch = schema.get(sig, [])
    conv_rows = conv.get(sig, [])
    conv_by_sub = {}
    for row in conv.get(sig, []) + conv.get("*", []):
        for sub in row["subrecord_sig"].replace("|", "/").split("/"):
            conv_by_sub.setdefault(sub.strip(), []).append(row)
    lines += ["## Vanilla fields (top level)", "",
              "field | records | % | values | schema subrecord | converter reads | engine uses", "--- | --- | --- | --- | --- | --- | ---"]
    matched_subs = set()
    for name, field in sorted(top.items(), key=lambda kv: -kv[1]["count"]):
        if bookkeeping(name):
            continue
        sub, how, where, kind = match(name, sch, {**aliases.get("*", {}), **aliases.get(sig, {})})
        reads = ""
        engine_col = "?"
        if sub:
            matched_subs.add(sub)
            rows = conv_by_sub.get(sub, [])
            reads = "; ".join(f"{r['how_read']} {r['evidence'].split('crates/converter/src/')[-1]}" for r in rows[:2]) or "no row"
            found = {engine_status(r["stored_in"], engine) for r in rows if r["how_read"] != "skipped"}
            found -= {"?", "-"}
            engine_col = "; ".join(sorted(found)) or ("-" if rows else "?")
            # A struct subrecord read "partially" says nothing about one member: do not claim it.
            inside = how in ("member", "name~") or (how == "name" and kind in ("struct", "union", "group"))
            if inside and any(r["how_read"] in ("partial", "raw", "presence") for r in rows):
                reads = f"subrecord {sub} read partially; this member unknown (see rows below)"
                engine_col = "?"
        sub_label = f"{sub}{where} ({how})" if sub else "?"
        pct = round(100 * field["count"] / total)
        nested = sum(1 for k in fields if k.startswith(name + ".") or k.startswith(name + "["))
        label = name + (f" (+{nested} nested)" if nested else "")
        lines.append(f"{label} | {field['count']} | {pct} | {short(field)} | {sub_label} | {reads or '?'} | {engine_col}")
    if skip:
        lines += ["", f"Bookkeeping fields present (ignored above): {', '.join(skip)}."]
    lines += ["", "## Schema fields (PR 204, unmerged) with no vanilla field matched", ""]
    unmatched = [s for s in sch if s["sub"] and s["sub"] not in matched_subs]
    lines += [f"- {s['sub']} {s['name'] or ''} ({s['kind']})" for s in unmatched] or ["- none"]
    lines += ["", f"## Converter reads for {sig} (docs/research/data/parity-converter-fields.tsv)", ""]
    if conv_rows:
        lines += ["subrecord | how | evidence | stored in | engine uses | notes", "--- | --- | --- | --- | --- | ---"]
        lines += [f"{r['subrecord_sig']} | {r['how_read']} | {r['evidence'].split('crates/converter/src/')[-1]} | "
                  f"{r['stored_in']} | {engine_status(r['stored_in'], engine)} | {r['notes'][:110]}" for r in conv_rows]
    else:
        lines += ["No row: the converter's hand parsers never name this record type (it is stored verbatim at most)."]
    lines += ["", "Rows that apply to every record type (record header, remap, MODL/MODS helpers, VMAD) are in INDEX-records.md."]
    if len(fields) <= DEEP_LIMIT:
        lines += ["", "## All field paths", "", "path | records | values"]
        lines += [f"{k} | {v['count']} | {short(v)}" for k, v in sorted(fields.items()) if not bookkeeping(k.split(".")[0].split("[")[0])]
    else:
        lines += ["", f"({len(fields)} field paths in total; nested paths are in field_stats.json)"]
    return "\n".join(lines) + "\n", len([n for n in top if not bookkeeping(n)]), len(matched_subs), len(conv_rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--stats", default="D:/Mudcrab/parity/field_stats.json")
    parser.add_argument("--out", default="D:/Mudcrab/parity")
    parser.add_argument("--tsv", default=os.path.join(REPO, "docs", "research", "data", "parity-converter-fields.tsv"))
    parser.add_argument("--engine", default=os.path.join(REPO, "docs", "research", "data", "parity-engine-use.tsv"))
    args = parser.parse_args()
    with open(args.stats, encoding="utf-8") as handle:
        stats = json.load(handle)
    tmap, aliases = load_type_map(), load_aliases()
    schema, prov = load_schema(args.out)
    conv = load_converter(args.tsv)
    engine = load_engine(args.engine)
    commit = subprocess.run(["git", "-C", REPO, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    head = "plugins " + ", ".join(p.replace(".esm", "") for p in stats["plugins"])
    os.makedirs(os.path.join(args.out, "sheets"), exist_ok=True)
    index = []
    for rtype, entry in sorted(stats["types"].items()):
        sig = tmap.get(rtype)
        if not sig:
            print("no signature for", rtype)
            continue
        text, nfields, nmatched, nconv = sheet(rtype, sig, entry, schema, conv, aliases, prov, head, engine)
        name = f"{sig}_{rtype}.md".replace("/", "_")
        with open(os.path.join(args.out, "sheets", name), "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
        index.append((sig, rtype, entry["records"], nfields, nmatched, len(schema.get(sig, [])), nconv))
    lines = ["# Record index (agents: read this first, then `python tools/skyrim_rec.py sheet <SIG>`)", "",
             f"Built from converter commit {commit}. One page per record type: vanilla fields with counts, the schema "
             "subrecord and what the converter reads. Local only (derived from the user's game files).", "",
             "Use: before changing how a record or field is read, run `sheet <SIG>`, then `show <EditorID>` for a real "
             "example. `?` in a column means unmatched, not absent.", "",
             "sig | type | records | vanilla fields | matched to schema | schema fields | converter rows",
             "--- | --- | --- | --- | --- | --- | ---"]
    lines += [" | ".join(str(x) for x in row) for row in sorted(index)]
    generic = conv.get("*", [])
    lines += ["", "## Converter rows that apply to every record type", "",
              "subrecord | how | evidence | stored in", "--- | --- | --- | ---"]
    lines += [f"{r['subrecord_sig']} | {r['how_read']} | {r['evidence'].split('crates/converter/src/')[-1]} | "
              f"{r['stored_in']}" for r in generic]
    with open(os.path.join(args.out, "INDEX-records.md"), "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")
    with open(os.path.join(args.out, "BUILD.json"), "w", encoding="utf-8", newline="\n") as handle:
        json.dump({"converter_commit": commit, "stats_mtime": os.path.getmtime(args.stats)}, handle)
    print(f"{len(index)} sheets -> {args.out}/sheets, INDEX-records.md")


if __name__ == "__main__":
    main()

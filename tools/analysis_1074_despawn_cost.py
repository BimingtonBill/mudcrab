# analysis-1074 helper: read-only inspection of the 2026-10-08 re-profile evidence.
# Usage: python tools/analysis_1074_despawn_cost.py
import json
import os
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

RUNS = r"D:/Mudcrab/evidence/reprofile-portal-2026-10-08/scenes-1/runs/20261008-091655-AMD-Ryzen-7-5800X-8-Core-Processor--NVIDIA-GeForce-RTX-3090-driver-32.0.16.1692/portal-door-tour/standing-still"


def dump(obj, depth=0, maxdepth=2):
    pad = "  " * depth
    if isinstance(obj, dict):
        for k, v in list(obj.items())[:40]:
            if isinstance(v, (dict, list)):
                print(f"{pad}{k}: <{type(v).__name__} len={len(v)}>")
                if depth < maxdepth:
                    dump(v, depth + 1, maxdepth)
            else:
                r = repr(v)
                print(f"{pad}{k}: {r[:120]}")
    elif isinstance(obj, list):
        if obj:
            print(f"{pad}[0] sample:")
            dump(obj[0], depth + 1, maxdepth)
    else:
        print(f"{pad}{obj!r}")


def frame1255():
    s = json.load(open(os.path.join(RUNS, "run-1", "streaming.json"), encoding="utf-8"))
    tl = s["timeline"]
    print("=== all timeline events at 1255 (deduped stages) ===")
    for e in tl:
        if e["frame"] == 1255:
            print(" ", e["stage"], e["subject"], e.get("value_ms"))
    print("=== Interior(117636) history ===")
    for e in tl:
        if e["subject"] == "Interior(117636)":
            print(" ", e["frame"], e["stage"], e.get("value_ms"))


def route():
    s = json.load(open(os.path.join(RUNS, "run-1", "streaming.json"), encoding="utf-8"))
    tl = s["timeline"]
    print("=== door_crossed events ===")
    for e in tl:
        if e["stage"] in ("door_crossed", "space_switch", "prestream"):
            print(" ", e["frame"], e["stage"], e["subject"], e.get("value_ms"))
    print("=== timeline 550-575 ===")
    for e in tl:
        if 550 <= e["frame"] <= 575 and e["stage"] != "requested":
            print(" ", e["frame"], e["stage"], e["subject"])
    print("=== timeline 845-860 ===")
    for e in tl:
        if 845 <= e["frame"] <= 860 and e["stage"] != "requested":
            print(" ", e["frame"], e["stage"], e["subject"])
    print("=== timeline 1240-1260 ===")
    for e in tl:
        if 1240 <= e["frame"] <= 1260 and e["stage"] != "unloaded":
            print(" ", e["frame"], e["stage"], e["subject"])


def evidence_counts():
    import re
    for run in ("run-1",):
        s = json.load(open(os.path.join(RUNS, run, "streaming.json"), encoding="utf-8"))
        tl = s["timeline"]
        f = 1255
        unloaded = [x for x in tl if x["frame"] == f and x["stage"] == "unloaded"]
        samples = {}
        for x in unloaded:
            subj = x["subject"]
            key = subj.split("{")[0].strip()
            samples.setdefault(key, subj)
        print("subject prefixes at unload:", {k: v for k, v in list(samples.items())[:10]})
        # committed events before the crossing: map cell subject -> refs count
        ref_counts = {}
        for e in tl:
            m = re.match(r"spawned references=(\d+) model_loads=(\d+)", e["stage"])
            if m:
                ref_counts[e["subject"]] = (int(m.group(1)), int(m.group(2)))
        unloaded_ext = [x["subject"] for x in unloaded if x["subject"].startswith("Exterior")]
        got = [(ref_counts.get(sub) or (0, 0)) for sub in unloaded_ext]
        with_refs = [g for g in got if g[0] > 0]
        print(f"unloaded exterior cells: {len(unloaded_ext)}, with committed references: {len(with_refs)}")
        if with_refs:
            tot = sum(r for r, _ in with_refs)
            totm = sum(m for _, m in with_refs)
            print(f"  refs total={tot} avg={tot/len(with_refs):.1f} max={max(r for r,_ in with_refs)}"
                  f" | model_loads total={totm} avg={totm/len(with_refs):.1f} max={max(m for _,m in with_refs)}")
        # subject formats differ? show samples of both sets
        print("  sample spawned-references subjects:", [e["subject"] for e in tl
              if e["stage"].startswith("spawned references")][:3])
        ws_un = {x["subject"].split("worldspace_id:")[1].split(",")[0].strip() for x in unloaded
                 if "worldspace_id" in x["subject"]}
        ws_sp = {e["subject"].split("worldspace_id:")[1].split(",")[0].strip() for e in tl
                 if e["stage"].startswith("spawned references") and "worldspace_id" in e["subject"]}
        print("  unloaded worldspaces:", ws_un, " spawned-references worldspaces:", ws_sp)
        # which frame did spawned-references land on, and what did unload look like per worldspace?
        by_ws = {}
        for x in unloaded:
            if x["subject"].startswith("Exterior"):
                ws = x["subject"].split("worldspace_id:")[1].split(",")[0].strip()
                by_ws[ws] = by_ws.get(ws, 0) + 1
        print("  unloaded exterior by worldspace:", by_ws)
        # trace a few spawned-references cells: when were they unloaded / re-requested?
        for sub in [e["subject"] for e in tl if e["stage"].startswith("spawned references")][:5]:
            ev = [(e["frame"], e["stage"]) for e in tl if e["subject"] == sub]
            print(f"  {sub}: {ev[:12]}")
        # unload events per frame for exterior subjects (whole run)
        uf = {}
        for e in tl:
            if e["stage"] == "unloaded" and e["subject"].startswith("Exterior"):
                uf[e["frame"]] = uf.get(e["frame"], 0) + 1
        print("  exterior unload events by frame (top):", sorted(uf.items(), key=lambda kv: -kv[1])[:8])
        # the interior unloaded at the crossing: full history
        for sub in {x["subject"] for x in tl if x["frame"] == 1255 and x["stage"] == "unloaded"
                    and x["subject"].startswith("Interior")}:
            ev = [(e["frame"], e["stage"], e["value_ms"]) for e in tl if e["subject"] == sub]
            print(f"  {sub}: {ev[:14]}")
        # interior cells' spawned references counts
        for e in tl:
            if e["stage"].startswith("spawned references") and e["subject"].startswith("Interior"):
                print("  interior refs event:", e["frame"], e["subject"], e["stage"])


def main():
    import re
    for run in ("run-1", "run-2", "run-3"):
        path = os.path.join(RUNS, run, "streaming.json")
        s = json.load(open(path, encoding="utf-8"))
        tl = s["timeline"]
        agg = s["aggregate"]
        print(f"=== {run} ===")
        print("aggregate despawned_entities:", agg.get("despawned_entities"),
              "retire_backlog_overflows:", agg.get("retire_backlog_overflows"),
              "references_spawned:", agg.get("references_spawned"))
        # door_crossed frames
        crossings = [e for e in tl if e["stage"] == "door_crossed"]
        print("door_crossed frames:", [e["frame"] for e in crossings])
        for e in crossings:
            f = e["frame"]
            unloaded = [x for x in tl if x["frame"] == f and x["stage"] == "unloaded"]
            kinds = {}
            for x in unloaded:
                subj = x["subject"]
                if subj.startswith("Exterior"):
                    k = "exterior"
                elif subj.startswith("Interior"):
                    k = "interior"
                else:
                    k = "other"
                kinds[k] = kinds.get(k, 0) + 1
            print(f"  frame {f}: unloaded events={len(unloaded)} kinds={kinds}")
        # spawned references per cell (model_loads tell scene spawn size)
        refs = []
        for e in tl:
            m = re.match(r"spawned references=(\d+) model_loads=(\d+)", e["stage"])
            if m:
                refs.append((int(m.group(1)), int(m.group(2))))
        if refs:
            tot_r = sum(r for r, _ in refs)
            tot_m = sum(m for _, m in refs)
            print(f"  spawned-references events={len(refs)} total_refs={tot_r} total_model_loads={tot_m} "
                  f"max_refs_one_cell={max(r for r,_ in refs)} max_models_one_cell={max(m for _,m in refs)}")
        # committed events for exterior full cells
        commits = [e for e in tl if e["stage"] == "committed" and e.get("value_ms") is not None]
        if commits:
            worst = sorted(commits, key=lambda e: -e["value_ms"])[:5]
            print("  worst commits (ms):", [(e['frame'], round(e['value_ms'],1)) for e in worst])


if __name__ == "__main__":
    import sys as _s
    arg = _s.argv[1] if len(_s.argv) > 1 else ""
    if arg == "refcells":
        s = json.load(open(os.path.join(RUNS, "run-1", "streaming.json"), encoding="utf-8"))
        tl = s["timeline"]
        unloaded_ext = {x["subject"] for x in tl if x["stage"] == "unloaded"
                        and x["subject"].startswith("Exterior")}
        refsubs = set()
        for e in tl:
            if e["stage"].startswith("spawned references"):
                refsubs.add(e["subject"])
        print("ref cells:", len(refsubs), "unloaded among them:", len(refsubs & unloaded_ext))
        grid = {}
        for sub in refsubs:
            g = sub.split("grid_x:")[1].split(",")[0].strip(), sub.split("grid_y:")[1].split("}")[0].strip()
            grid[sub] = g
        xs = [int(g[0]) for g in grid.values()]
        ys = [int(g[1]) for g in grid.values()]
        print("ref cell grid range x", min(xs), max(xs), "y", min(ys), max(ys))
        # unloaded exterior grid range
        ux = [int(x["subject"].split("grid_x:")[1].split(",")[0]) for x in tl
              if x["stage"] == "unloaded" and x["subject"].startswith("Exterior")]
        uy = [int(x["subject"].split("grid_y:")[1].split("}")[0]) for x in tl
              if x["stage"] == "unloaded" and x["subject"].startswith("Exterior")]
        print("unloaded exterior grid range x", min(ux), max(ux), "y", min(uy), max(uy), "count", len(ux))
        sys.exit(0)
    if arg == "stages":
        for run in ("run-1", "run-2", "run-3"):
            s = json.load(open(os.path.join(RUNS, run, "streaming.json"), encoding="utf-8"))
            tl = s["timeline"]
            stages = {}
            for e in tl:
                stages[e["stage"]] = stages.get(e["stage"], 0) + 1
            print(run, sorted(stages.items(), key=lambda kv: -kv[1]))
    elif arg == "route":
        route()
    elif arg == "f1255":
        frame1255()
    else:
        main()
        evidence_counts()
#!/usr/bin/env python3
"""Capture vanilla Skyrim SE reference shots for Mudcrab, from the same shots file the engine renders.

    python tools/capture/skyrim_ref.py check
        preflight: MO2 instance, plugin installed, game version, vanilla profile, resolution, nothing running.
    python tools/capture/skyrim_ref.py shots tools/reference/baseline_lod_far.json --out local/capture/refs/lod-far
        runs the game under the build gate (tools/capture/run_skyrim.py), one session per cell group, and
        writes <out>/<name>.png + <name>.json for every shot, plus report.json.
    python tools/capture/skyrim_ref.py manual
        starts a hand-flown session: free-fly to a view, press F8, and the plugin writes the exact pose as a
        --shots entry (position, yaw, pitch, hfov) next to a PNG.

The shots file is the engine's `--shots` file (docs/design/reference-shots.md): width, height, shots[] with
name, worldspace_id, position, yaw, pitch (positive looks DOWN), hfov. Positions are Creation units.
Full guide, setup and troubleshooting: docs/process/skyrim-reference-capture.md.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
RUN_SKYRIM = HERE / "run_skyrim.py"
MO2_ROOT = Path(r"C:\Modding\MO2-capture")
PLUGIN_MOD = MO2_ROOT / "mods" / "MudcrabCapture" / "SKSE" / "Plugins"
PLUGIN_CFG = PLUGIN_MOD / "MudcrabCapture"
GAME_DIR = Path(r"C:\Games\Steam\steamapps\common\Skyrim Special Edition")
DOCS = Path.home() / "Documents" / "My Games" / "Skyrim Special Edition"
OUT_ROOT = DOCS / "MudcrabCapture"
PREFS = DOCS / "SkyrimPrefs.ini"
DEFAULT_WEATHER = "0x0000081A"  # SkyrimClear
EYE_OFFSET = 120.5  # the first-person camera sits this far above the placed player reference (measured 2026-10-14)
GRID = 4096.0
# `--fog off` sends `setfog <start> <end>` (UESP: "2 floats, start and end depths"; Elys' Universal Fog Remover uses 5000
# and 500000 for "far fog only"). Both values are far beyond any view distance. UNVERIFIED in SE 1.7.104: spike it
# with `skyrim_ref.py manual --fog off` before trusting a run (docs/process/skyrim-reference-capture.md, "Fog").
FOG_OFF = (1_000_000, 2_000_000)


def hfov_to_console_fov(hfov_deg: float, width: int, height: int) -> float:
    """Skyrim's console `fov` is the horizontal field of view of the window it renders to.

    Measured 2026-10-14 (docs/process/skyrim-reference-capture.md): the earlier 4:3-basis formula
    `2 atan(tan(h/2) / (0.75 aspect))` put the engine's picture at exactly 0.75 times the size of the Skyrim
    one at 16:9 (SIFT match on Riverwood buildings, scale 0.7500, 0.7504, 0.7499, rotation 0, no other shift)."""
    return hfov_deg


def csv_rows(shots: list[dict], width: int, height: int, raw_fov: bool, move_method: str = "") -> list[str]:
    """Plugin route rows: t,x,y,z,yaw_deg,pitch_deg(up-positive),name,fov. The engine's pitch is down-positive."""
    rows = ["t,x,y,z,yaw_deg,pitch_deg,name,fov,method"]
    for index, shot in enumerate(shots):
        x, y, z = shot["position"]
        fov = float(shot["hfov"]) if raw_fov else hfov_to_console_fov(float(shot["hfov"]), width, height)
        rows.append(f"{index},{x},{y},{z},{shot['yaw']},{-float(shot['pitch'])},{shot['name'].replace(',', '_')},{fov:.3f},{shot.get('move_method', move_method)}")
    return rows


# Several poses per game start (fixed 2026-10-15). The "only the first pose lands" failure was a message box ("Turn on
# Survival Mode?") that stayed open, paused the game and froze the camera after the first pose; the plugin now closes it
# with Escape. Verified: 6/6 snow poses over two cells and 10/11 Tamriel poses up to 160,000 units apart in one launch.
# 0 means no limit; --shots-per-session N restores a cap.
SHOTS_PER_SESSION = 0
GROUP_RADIUS_CELLS = 100000  # an exterior shot shares a session with the group's first shot only in the same grid cell (SetPosition does not cross cells)


def _grid_of(shot: dict) -> tuple[int, int] | None:
    position = shot.get("position")
    if not position:
        return None
    return math.floor(position[0] / GRID), math.floor(position[1] / GRID)


def group_shots(shots: list[dict], per_session: int | None = None, radius: int | None = None) -> list[list[dict]]:
    """One game session per worldspace or interior cell, and per neighbourhood in an exterior.

    Moving far inside one session is slow and unreliable: the cells around a far pose are not loaded, so the
    camera ends up elsewhere (2026-10-14: poses 15,000 to 26,000 units from the entry cell came back 14 to 26
    thousand units off). An exterior shot therefore joins a group only if it is within GROUP_RADIUS_CELLS grid
    cells of that group's first shot (0: the same cell); otherwise it starts a new group, entered with its own `cow`."""
    groups: dict[tuple, list[list[dict]]] = {}
    for shot in shots:
        sessions = groups.setdefault((shot.get("worldspace_id"), shot.get("interior_cell_id")), [])
        cell = _grid_of(shot) if shot.get("interior_cell_id") is None else None
        for session in sessions:
            first = _grid_of(session[0]) if cell is not None else None
            if (per_session or SHOTS_PER_SESSION) and len(session) >= (per_session or SHOTS_PER_SESSION):
                continue
            if cell is None or first is None or max(abs(cell[0] - first[0]), abs(cell[1] - first[1])) <= (GROUP_RADIUS_CELLS if radius is None else radius):
                session.append(shot)
                break
        else:
            sessions.append([shot])
    return [session for sessions in groups.values() for session in sessions]


def worldspace_editor_id(worldspace_id: int, db: Path | None) -> str | None:
    if worldspace_id == 60:
        return "Tamriel"
    if db and db.exists():
        import sqlite3
        conn = sqlite3.connect(f"file:{db.as_posix()}?mode=ro", uri=True)
        row = conn.execute("select editor_id from worldspaces where id = ?", (worldspace_id,)).fetchone()
        return row[0] if row else None
    return None


def entry_command(group: list[dict], coc: str | None, db: Path | None) -> str:
    """The console command that starts a fresh game near the group's first shot."""
    if coc:
        return f"coc {coc}"
    first = group[0]
    if first.get("interior_cell_id") is not None:
        raise SystemExit("interior shots need --coc <cell editor id> (the database has no cell editor ids)")
    editor_id = worldspace_editor_id(int(first["worldspace_id"]), db)
    if not editor_id:
        raise SystemExit(f"unknown worldspace id {first['worldspace_id']}: give --coc or --db")
    x, y, _ = first["position"]
    return f"cow {editor_id} {math.floor(x / GRID)},{math.floor(y / GRID)}"


def fog_command(fog: str) -> str:
    """Console command for the --fog option: 'vanilla' (none), 'off', or 'start,end' in game units."""
    if fog == "vanilla":
        return ""
    if fog == "off":
        start, end = FOG_OFF
    else:
        try:
            start, end = (float(v) for v in fog.split(","))
        except ValueError:
            raise SystemExit(f"--fog takes vanilla, off or start,end (got '{fog}')")
        if not 0 <= start < end:
            raise SystemExit("--fog start,end needs 0 <= start < end")
    return f"setfog {start:.0f} {end:.0f};"


def capture_options(args: argparse.Namespace) -> dict:
    """What the picture was taken with; written into every shot's json and report.json."""
    return {"hour": args.hour, "weather": args.weather or None, "fog": args.fog,
            "fog_command": fog_command(args.fog).rstrip(";") or None, "image_space_effects": not args.no_effects}


def plugin_ini(args: argparse.Namespace, enter: str, settle: float, mode: str) -> str:
    hour = f"set gamehour to {args.hour};" if args.hour is not None else ""
    weather = f"fw {args.weather};" if args.weather else ""
    lod = "tll;tll;" if args.tll else ""
    fog = fog_command(args.fog)
    effects = "teofis;" if args.no_effects else ""
    return f"""; Written by tools/capture/skyrim_ref.py; edit that tool, not this file.
[Route]
File=Data/SKSE/Plugins/MudcrabCapture/route.csv

[Stills]
SettleFrames=12
SettleSeconds={settle}
EveryNthPose=1

[Camera]
UsePlayer=1
MoveMethod={args.move_method}
EyeOffset={EYE_OFFSET}

[World]
HideUI=1

[Auto]
Mode={mode}
RunName={args.run_name}
MainMenuCommands={enter}
MainMenuDelaySeconds=4
AssumeLoadedAfterMenu=1
DelaySeconds=6
StartupCommands=tcl;{hour}{weather}{lod}{fog}{effects}set timescale to 0
QuitWhenDone={'1' if mode == 'stills' else '0'}
DismissMessageBoxes=1
"""


def game_running() -> bool:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq SkyrimSE.exe", "/NH"], capture_output=True, text=True).stdout
    return "SkyrimSE.exe" in out


def check() -> int:
    problems: list[str] = []
    notes: list[str] = []
    if not (MO2_ROOT / "ModOrganizer.exe").exists():
        problems.append(f"MO2 not found at {MO2_ROOT}")
    if not (PLUGIN_MOD / "MudcrabCapture.dll").exists():
        problems.append("MudcrabCapture.dll is not installed: run tools/capture/skse-plugin/build.ps1 -Install")
    exe = GAME_DIR / "SkyrimSE.exe"
    if not exe.exists():
        problems.append(f"game not found at {GAME_DIR}")
    else:
        version = subprocess.run(["powershell", "-NoProfile", "-Command", f"(Get-Item '{exe}').VersionInfo.FileVersion"],
                                 capture_output=True, text=True).stdout.strip()
        notes.append(f"game version {version}")
    modlist = MO2_ROOT / "profiles" / "Default" / "modlist.txt"
    if modlist.exists():
        enabled = [line[1:].strip() for line in modlist.read_text(encoding="utf-8").splitlines() if line.startswith("+")]
        notes.append("enabled mods: " + ", ".join(enabled))
        for mod in ("Community Shaders", "ENB", "ReShade", "Universal Fog Remover"):
            hits = [name for name in enabled if mod.lower() in name.lower()]
            if hits:
                problems.append(f"'{hits[0]}' is enabled in the MO2 profile: a vanilla reference needs {mod} off (use --fog instead of a fog plugin)")
    if PREFS.exists():
        text = PREFS.read_text(encoding="utf-8", errors="replace")
        size = {k: re.search(rf"(?m)^{k}=(\S+)", text) for k in ("iSize W", "iSize H", "bFull Screen", "bBorderless")}
        notes.append("display: " + ", ".join(f"{k}={m.group(1) if m else '?'}" for k, m in size.items()))
    if game_running():
        problems.append("SkyrimSE.exe is already running")
    print("\n".join("note: " + n for n in notes))
    print("\n".join("PROBLEM: " + p for p in problems) or "ok")
    return 1 if problems else 0


def set_resolution(width: int, height: int) -> None:
    """Windowed at the shots file's size (the engine renders the same size). Keeps CRLF line endings."""
    raw = PREFS.read_bytes()
    text = raw.decode("utf-8", errors="replace")
    for key, value in (("iSize W", width), ("iSize H", height), ("bFull Screen", 0), ("bBorderless", 0)):
        text = re.sub(rf"(?m)^{re.escape(key)}=.*?(\r?)$", rf"{key}={value}\1", text)
    PREFS.write_bytes(text.encode("utf-8"))


def run_group(args: argparse.Namespace, shots: list[dict], width: int, height: int, out: Path, index: int) -> list[dict]:
    enter = entry_command(shots, args.coc, args.db)
    stage = Path(tempfile.mkdtemp(prefix="skyrim-ref-stage-"))  # copied into the plugin config once the slot and the capture lock are ours
    (stage / "route.csv").write_text("\n".join(csv_rows(shots, width, height, args.raw_fov, args.move_method)) + "\n", encoding="utf-8")
    (stage / "MudcrabCapture.ini").write_text(plugin_ini(args, enter, args.settle, "stills"), encoding="utf-8")
    before = set(os.listdir(OUT_ROOT)) if OUT_ROOT.exists() else set()
    print(f"group {index}: {len(shots)} shot(s), entering with '{enter}'", flush=True)
    code = subprocess.call([sys.executable, str(RUN_SKYRIM), "play", "--label", f"skyrim-ref-{args.run_name}", "--stage", str(stage)])
    shutil.rmtree(stage, ignore_errors=True)
    new = sorted(set(os.listdir(OUT_ROOT)) - before) if OUT_ROOT.exists() else []
    if code or not new:
        raise SystemExit(f"capture run failed (exit {code}, new output folders: {new}); see SKSE/MudcrabCapture.log in {DOCS}")
    results = []
    src = OUT_ROOT / new[-1]
    for shot in shots:
        stem = re.sub(r"[^A-Za-z0-9_-]", "_", shot["name"])[:80]
        png, meta = src / f"{stem}.png", src / f"{stem}.json"
        if not png.exists():
            results.append({"name": shot["name"], "ok": False, "error": "no image written"})
            continue
        shutil.copy(png, out / f"{stem}.png")
        record = json.loads(meta.read_text(encoding="utf-8"))
        record["capture_options"] = capture_options(args)
        (out / f"{stem}.json").write_text(json.dumps(record, indent=2), encoding="utf-8")
        actual = record["actual"]
        wanted = shot["position"]
        err_pos = max(abs(actual[a] - wanted[i]) for i, a in enumerate("xyz"))
        err_yaw = abs((actual["yaw_deg"] - shot["yaw"] + 180.0) % 360.0 - 180.0)
        err_pitch = abs(actual["pitch_deg"] + shot["pitch"])
        results.append({"name": shot["name"], "ok": err_pos < 1.0 and err_yaw < 0.2 and err_pitch < 0.2,
                        "max_position_error": round(err_pos, 3), "yaw_error": round(err_yaw, 3),
                        "pitch_error": round(err_pitch, 3), "image": str(out / f"{stem}.png")})
    return results


def cmd_shots(args: argparse.Namespace) -> int:
    spec = json.loads(Path(args.shots_file).read_text(encoding="utf-8"))
    width, height = int(spec["width"]), int(spec["height"])
    shots = spec["shots"]
    if args.only:
        wanted = set(args.only.split(","))
        shots = [s for s in shots if s["name"] in wanted]
    if not shots:
        raise SystemExit("no shots selected")
    if check() != 0 and not args.force:
        raise SystemExit("preflight failed (fix it, or --force)")
    if args.set_resolution:
        set_resolution(width, height)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    results: list[dict] = []
    for index, group in enumerate(group_shots(shots, args.shots_per_session, args.session_radius)):
        results += run_group(args, group, width, height, out, index)
    (out / "report.json").write_text(json.dumps({"width": width, "height": height, "capture_options": capture_options(args), "shots": results}, indent=2), encoding="utf-8")
    bad = [r for r in results if not r["ok"]]
    print(f"{len(results) - len(bad)}/{len(results)} shots ok; report {out / 'report.json'}")
    return 1 if bad else 0


def cmd_manual(args: argparse.Namespace) -> int:
    if check() != 0 and not args.force:
        raise SystemExit("preflight failed (fix it, or --force)")
    PLUGIN_CFG.mkdir(parents=True, exist_ok=True)
    (PLUGIN_CFG / "route.csv").write_text("t,x,y,z,yaw_deg,pitch_deg\n0,0,0,0,0,0\n", encoding="utf-8")
    args.run_name = "manual"
    ini = plugin_ini(args, args.enter or "coc Riverwood", args.settle, "none").replace("UsePlayer=1", "UsePlayer=0")
    ini = re.sub(r"(?m)^StartupCommands=.*$", f"StartupCommands=fov 73.74;tcl;tfc 1{';' + fog_command(args.fog).rstrip(';') if args.fog != 'vanilla' else ''}", ini)
    (PLUGIN_CFG / "MudcrabCapture.ini").write_text(ini, encoding="utf-8")
    print("Fly to the view (W/A/S/D, sucsm 5 slows the camera), press F8 for each pose, quit the game when done.\n"
          f"Poses and PNGs land in {OUT_ROOT}\\poses-<time>\\shots.json (engine --shots format).")
    return subprocess.call([sys.executable, str(RUN_SKYRIM), "play", "--label", "skyrim-ref-manual"])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check", help="preflight")
    shots = sub.add_parser("shots", help="render a shots file in vanilla Skyrim")
    shots.add_argument("shots_file")
    shots.add_argument("--out", required=True)
    shots.add_argument("--only", help="comma-separated shot names")
    shots.add_argument("--hour", type=float, default=11.0, help="pinned game hour (default 11; use -1 to leave it)")
    shots.add_argument("--weather", default=DEFAULT_WEATHER, help="forced weather form id (default SkyrimClear; '' to leave)")
    shots.add_argument("--settle", type=float, default=4.0, help="seconds to hold each pose before saving (default 4)")
    shots.add_argument("--coc", help="console cell to start in (default: cow <worldspace> <grid> from the first shot)")
    shots.add_argument("--db", type=Path, help="skyrim_world.db, to name worldspaces other than Tamriel")
    shots.add_argument("--tll", action="store_true", help="toggle distant LOD off and on before the shot (usually unneeded)")
    shots.add_argument("--raw-fov", action="store_true", help="pass hfov to `fov` unchanged (skip the 4:3 conversion)")
    shots.add_argument("--move-method", default="setpos", help="how the plugin moves the player between poses: setpos (default), setpos0, console; a shot's own move_method overrides")
    shots.add_argument("--shots-per-session", type=int, default=None, help=f"poses per game start (default {SHOTS_PER_SESSION}); more only while testing a move method")
    shots.add_argument("--session-radius", type=int, default=None, help=f"grid cells from a session's first shot that may share it (default {GROUP_RADIUS_CELLS})")
    shots.add_argument("--fog", default="vanilla", help="vanilla (default), off, or start,end in game units (console setfog)")
    shots.add_argument("--no-effects", action="store_true", help="toggle end-of-frame image space off (console teofis): no grade, blur or tint")
    shots.add_argument("--set-resolution", action="store_true", help="set a windowed display at the shots file's size first")
    shots.add_argument("--run-name", default="ref")
    shots.add_argument("--force", action="store_true")
    manual = sub.add_parser("manual", help="hand-flown session; F8 saves a pose")
    manual.add_argument("--enter", help="console command to start in (default coc Riverwood)")
    manual.add_argument("--hour", type=float, default=None)
    manual.add_argument("--weather", default="")
    manual.add_argument("--settle", type=float, default=4.0)
    manual.add_argument("--coc", default=None)
    manual.add_argument("--db", type=Path, default=None)
    manual.add_argument("--tll", action="store_true")
    manual.add_argument("--move-method", default="setpos")
    manual.add_argument("--fog", default="vanilla", help="vanilla (default), off, or start,end in game units")
    manual.add_argument("--no-effects", action="store_true")
    manual.add_argument("--raw-fov", action="store_true")
    manual.add_argument("--force", action="store_true")
    args = parser.parse_args()
    if args.cmd == "shots" and args.hour is not None and args.hour < 0:
        args.hour = None
    return {"check": lambda a: check(), "shots": cmd_shots, "manual": cmd_manual}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())

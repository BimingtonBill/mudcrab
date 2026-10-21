# Capturing vanilla Skyrim SE references

How to get ground-truth images and numbers from the real game for Mudcrab work (LOD comparisons, look
checks, benchmarks). The tool renders the same `--shots` file the engine renders, in vanilla Skyrim SE, so a
Mudcrab shot and its Skyrim reference share one camera.

## What is in the box

| Piece | Where | Does |
|---|---|---|
| `tools/capture/skyrim_ref.py` | repo | the command line: `check`, `shots`, `manual` |
| `tools/capture/run_skyrim.py` | repo | starts Skyrim through MO2 inside the build gate (`play`, `bench`) |
| `tools/capture/skse-plugin/` | repo | the MudcrabCapture SKSE plugin (C++, CommonLibSSE-NG); `build.ps1` builds and installs it |
| `tools/capture/route_poses.py` | repo | turns a route file (`scripts/routes/*.json`) into camera poses |
| MO2 instance | `C:\Modding\MO2-capture` | a portable Mod Organizer 2 with only the capture mods; the user's own setup is untouched |

## One-time setup (done on the main PC)

1. MO2 at `C:\Modding\MO2-capture` (portable), game path `C:\Games\Steam\steamapps\common\Skyrim Special Edition`, with an `SKSE` executable entry pointing at `skse64_loader.exe`.
2. SKSE64 matching the game exe (2.3.1 for 1.7.104) in the game folder; Address Library for the same runtime; the mods in the profile: MudcrabCapture (built from this repo), SSE Display Tweaks, SkyUI, MCM Helper, powerofthree's Tweaks, Photo Mode (optional; manual flying).
3. Build the plugin: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run --label capture-skse-plugin -- powershell -NoProfile -File tools/capture/skse-plugin/build.ps1 -Install`. Needs `C:\Tools\vcpkg`, a CommonLibSSE-NG checkout at `C:\Tools\CommonLibSSE-NG` (alandtse fork, release v12.1.0 or later: the first with 1.7.104 addresses) and the VS 2022 Build Tools.
4. **MO2 must run as administrator** (started by the user, left open): the launcher asks the running MO2 to start its `SKSE` entry, and a non-elevated MO2 did not start the game here. Check MO2 for a pop-up if a run says "SkyrimSE.exe did not start".
5. Display: windowed at the shots file's size (`--set-resolution` does it).

Run `python tools/capture/skyrim_ref.py check` to see what is missing.

## Capturing

```
python tools/capture/skyrim_ref.py shots tools/reference/baseline_lod_far.json --out local/capture/refs/lod-far
```

All shots of a worldspace run in one game start (several worldspaces or interiors: one start each; `--shots-per-session N` and `--session-radius CELLS` restore a cap). The plugin moves the player with `SetPosition` and `SetAngle` and pins the pose every frame while the shot settles. Per shot it writes `<name>.png` (1:1 swap-chain image, no HUD) and `<name>.json` (requested pose, the pose the
game's camera really had, weather, hour, cell, frame time), plus `report.json` (every shot checked within 1 unit
and 0.2 degrees). A run takes under a minute for a one-shot group (game start included). End-to-end test 2026-10-14: `tools/reference/baseline_lod_far.json` entered with `cow Tamriel 5,-12`, one shot, position/yaw/pitch errors 0.0, Whiterun and Dragonsreach visible.

Options worth knowing: `--only a,b` shots by name; `--hour 11` (default; `-1` leaves the clock) and `--weather
0x0000081A` (SkyrimClear) pin the lighting; `--settle 4` seconds per pose; `--coc <cell>` to start in a named
cell (needed for interiors; exteriors use `cow <worldspace> <grid>` from the first shot); `--db` names
worldspaces other than Tamriel; `--raw-fov` skips the fov conversion.

For a hand-picked view: `python tools/capture/skyrim_ref.py manual`, fly to it, press **F8**. The plugin
writes a PNG and a `shots.json` entry (position, yaw, pitch, hfov) under
`Documents\My Games\Skyrim Special Edition\MudcrabCapture\poses-<time>\`: paste it into a shots file.

For a moving shot (benchmark route): `python tools/capture/run_skyrim.py bench --until HH:MM` sets a timing hold,
runs, and writes `frame_budget.json` in the engine's shape. (The route runner still uses the free camera; see
limits.)

## Fog and image-space options (2026-10-15)

`--fog vanilla|off|start,end` (default vanilla) sends the console command `setfog <start> <end>` after entry; `off` uses
1000000 and 2000000. `--no-effects` sends `teofis` (toggle end-of-frame image space: no grade, blur or tint). Both are
deliberate deviations from vanilla for look-through comparisons (the engine's `--no-fog` is the pair). Every shot's json
and `report.json` carry a `capture_options` block (hour, weather, fog, fog command, image-space effects), so a picture
always says what it was taken with. `check` now also flags ENB, ReShade and fog-remover mods in the profile.

**Verified 2026-10-15** (SE 1.7.104, `tools/reference/baseline_lod_far.json`, Riverwood far valley, hour 11, SkyrimClear): the
console `setfog 1000000 2000000` removes the distant haze. Whiterun, the far peaks and the tundra show their own colours instead of
blue-white; nearby geometry is unchanged. Vanilla and fog-off pair: `local/capture/refs/fog-spike/{vanilla,off}/`. Still untested:
a fog-off run across a cell change within one session (each cell group is its own game start, and the command is sent in
every one, so shots files are fine), and whether the default 1000000/2000000 clips terrain LOD fog at extreme distance.
UESP gives no units for `SetFog`; Fallout 3's version overrides all weather and dungeon fog.

**One session at a time (2026-10-15).** `run_skyrim.py` takes `C:\Modding\MO2-capture\capture.lock` (owner pid and label; a dead owner's lock is taken over) after it holds a gate slot, and only then copies the staged `MudcrabCapture.ini` and `route.csv` into the plugin folder. A queued run therefore cannot overwrite the config of a running one. Hand-made launches (for example a RenderDoc entry) bypass the lock: write the ini right before launching and check `capture.lock` first. If MO2 is flagged "run as administrator", a plain start fails with error 740; the launcher then asks Windows to elevate (a UAC prompt appears, click Yes).

## Rules for a valid vanilla reference

- Vanilla means: no Community Shaders, ENB, ReShade, LOD mods or graphics add-ons in the MO2 profile
  (`check` flags Community Shaders). Inis at the game's own defaults (`Skyrim_Default.ini`, `Ultra.ini` values).
- A fresh game each run (the plugin starts one from the main menu with `cow`/`coc`): no save state.
- TAA stays on (vanilla). For pixel-exact diffs, set `bUseTAA=0` yourself and say so in the report.
- Pose: yaw clockwise from north, pitch in the shots file is positive DOWN (the plugin flips it). The camera
  is the player's first-person camera, 120.5 units above the placed reference (the plugin subtracts it).
- FOV: the console `fov` is the horizontal field of view of the window, so the tool sends the shot's `hfov`
  unchanged. Measured 2026-10-14 with `tools/capture/align_check.py` on Riverwood buildings at 1600x900: an
  earlier 4:3-basis formula gave a picture exactly 0.75 times the engine's; with the plain value the scale is
  1.0000 (0.9998 to 1.0002), rotation 0 and shift under 0.5 px, so the engine and Skyrim cameras coincide.
  `--raw-fov` is now the same as the default.

## Moving between poses (fixed 2026-10-15)

Symptom: only the first pose of a session landed; later poses came back at the first pose (pictures identical, 959 to 1303 units off). Cause: a message box ("Turn on Survival Mode?") that a new game opens stayed open, `GameIsPaused` went true and the camera root stopped following the player, while the player itself moved correctly (the plugin's per-second `settle` log lines show want / player / camera / focused / paused / open menus). The plugin's hide message did not close the box; it now also presses Escape (the box's cancel choice) every third try. Result: 6/6 snow poses over two cells, and 11/11 Tamriel exteriors up to 160,000 units apart (Riften to Solitude), in one launch each, twice. `Camera.MoveMethod` / a shot's `move_method` (setpos default, setpos0, update3d, update3dcam, console) are kept for experiments; all of them work once the box is gone. Names with commas are written with `_` into the plugin's route file (the file is comma separated).

Still to check when a new situation appears: interior shots (need `--coc`), other worldspaces (`--db`), and whether a very distant pose needs more than the default 4 s settle (it did not for the set above).

## Lessons (2026-10-14)

- **Do not use the free camera (`tfc`) for shots or flights that need far LOD.** The game streams the world
  and its distant LOD around the player; with the free camera the valley beyond the loaded cells was blank.
  Waiting 100 s, `tll`, and a second `coc` did not fix it. The plugin moves the player instead.
- The game loads to the title menu and Riverwood in about 2.5 s; a 4 s settle is enough. Longer waits changed
  nothing.
- Writing the game-hour global from a plugin does not move the sky; the console `set gamehour to` does.
  Time and weather are therefore pinned with console commands (`set gamehour`, `fw`, `set timescale to 0`).
- A new game asks "Turn on Survival Mode?" in a message box; the plugin closes message boxes during runs.
- Skyrim logs for diagnosis: `Documents\My Games\Skyrim Special Edition\SKSE\MudcrabCapture.log` and
  `skse64.log`. If `SkyrimSE.exe` ends without an image, check them first.
- Run only one session at a time; the gate refuses a timing window while another hold is set, and a normal run
  waits for a free slot.

## Limits

- The benchmark route (`bench`) still drives the free camera, so far LOD is missing there too; moving it to the
  player camera is the next step.
- One fixed time and weather per run; interiors need an explicit `--coc`; the plugin cannot read pixels
  from HDR back buffers (the game's is 8-bit).
- The vanilla LOD seam (an edge in the far valley when the free camera is used) is the only unverified item;
  everything else is read back from the game, and the camera match is measured with `align_check.py`.

## Credits and licences

Written fresh for Mudcrab from public APIs. Built on:

| Name | Author | Licence | Used for |
|---|---|---|---|
| Skyrim Script Extender 64 | SKSE team (Ian Patterson, Stephen Abel, Paul Connelly) | SKSE licence (not redistributed) | plugin host |
| CommonLibSSE-NG, alandtse fork | Ryan McKenzie, CharmedBaryon, powerof3, alandtse and contributors | GPL-3.0-or-later with the Modding and Linking Exceptions | game types and addresses |
| Address Library for SKSE Plugins | meh321 | per its Nexus page (not redistributed) | version-independent addresses |
| Mod Organizer 2 | MO2 team | GPL-3.0 | launching with the virtual file system |
| vcpkg | Microsoft | MIT | plugin dependencies |

The plugin links CommonLibSSE-NG, so its binary is a GPL-3 combined work: keep it local, do not ship it, until
the licence terms are decided. Nothing of any mod's code was copied; no game files are shipped.

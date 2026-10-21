# Goal
Render the same views of Skyrim's ground twice: once with today's terrain path, once with the merged terrain (#186), and measure the difference. The engine half (C:/Modding/Mudcrab/worktrees/p2-terrain, branch `phase2/terrain-merge`, head cda15405) now passes its GPU probe and all tests. This is the first check on real data. Track: Phase 2, team-queue item B1. Measurement and evidence only: do not change engine code. If you find an engine bug, report it with evidence.

# Context
- The two data sets come from the same source assets:
  - today: C:\Modding\Mudcrab\SkyrimConverted-p2main-v24, with no terrain/ folder, so the engine takes the per-quadrant path;
  - merged: D:\Mudcrab\SkyrimConverted-p2main-v24-terrain, Phase 1's metadata rebuild with --merged-terrain. It holds terrain/land-textures.json, 6 arrays and 3,589 chunk files. The engine opens it automatically when the folder has land-textures.json (app.rs ~123, `TerrainMerge::open`; streaming.rs `choose_merged_terrain`).
- Known differences that are not engine bugs:
  - entry 0 is a WHITE default texture; 80% of quadrants have no base texture, and today's engine leaves those untextured;
  - 13,350 null or unknown alpha layers are dropped;
  - cells 0x18DF and 0x18FD use Dawnguard's LAND in the merged data, while today keeps one at random.
  Phase 1's report: D:\Mudcrab\evidence\p1-evidence\convert-p2main-v24-terrain-report.json.
- Known capture problem (#1055): `--shots` can photograph a frame before terrain is drawn, which leaves holes. Shoot every view TWICE per side. A view counts only if its two shots on the same side match each other (under 0.2% of pixels differ by more than 8). If a side's pair disagrees, shoot that view a third time and use the matching pair, and say so.
- Views: 5 shots in one shots file, written to D:/Mudcrab/evidence/p2-terrain-diff-2026-10-08/views.json. Use the format of D:/Mudcrab/evidence/p2-terrain-diff-2026-10-08/riverwood-shots.json (width 960, height 540, worldspace_id, position in render units, yaw, pitch, hfov):
  1. riverwood-bridge;
  2. riverwood-high, both from that file;
  3. Whiterun tundra just outside the gate, with heavy grass, dirt and rock blending;
  4. a snow-to-rock mountain blend in the Throat of the World foothills above Ivarstead;
  5. a view straddling a four-cell corner, looking down at about 45 degrees.
  Pick the positions from the converted data: the cell grid times 4096 render units per cell, ground height from LAND or a terrain probe. Explain each choice. Tamriel's worldspace_id is 60.

# Steps
1. Build the engine: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --profile quick --bin engine`, with CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-terrain-target and BUILD_GATE_OWNER=p2, both already set.
2. Before each engine run, check `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`. If any hold is listed (a timing window or a conversion), WAIT and do not run the engine; check again every few minutes.
3. Run `<target>/quick/engine.exe --assets <set> --shots views.json --shots-out <dir>/<side>-<n>` for side today and merged, n = 1 and 2.
4. Diff per view:
   - today-1 against today-2, and merged-1 against merged-2 (the noise);
   - today against merged (the difference).
   Give the percent of pixels differing by more than 8, the max error, and a heat map PNG. Make a side-by-side PNG per view. Use Python with Pillow if available; otherwise write a small comparison with what is installed and say what.
5. Describe each view's differences in words: where, what kind (texture choice, brightness, tiling scale, seams, holes, white patches), and which known cause, if any, explains each. Look especially for: white where today shows a real texture (a lookup falling back to id 0), visible quadrant or cell seams, tiling-scale changes, normal-map lighting differences, and anything at cells 18DF and 18FD.

# Report
Status / Summary / Left / Next. Then the per-view table (noise today, noise merged, today vs merged, verdict), the explanations, the PNG paths, and any suspected engine or converter bug with file:line evidence. Copy the key images and a RESULTS.md into D:/Mudcrab/evidence/p2-terrain-diff-2026-10-08/.

# analysis-1092-portal-roomcull-image-check (track: Portal; queue B5). Runs the engine; writes only to the scratch folder.

## Goal
Prove on the portal demo's own data that room culling hides nothing visible, before impl-1085 makes it the default.
Repeat the #188 method (local/review/188-room-culling-image-test.md: 20 interior poses, 3 runs off + 3 on, a pixel
counts as a culling difference only if it differs (any channel delta > 8) from EVERY off run) on
C:/Modding/Mudcrab/SkyrimConverted-portal-rooms (has room tables: 4,246 rooms, 3,097 portals).

## Setup
- Code: worktree C:/Modding/Mudcrab/mudcrab/local/impl/impl-1085-portal-room-culling-default (commit 16fad023: default on,
  `--room-culling=off`). Build once: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --release --bin engine`
  with BUILD_GATE_OWNER=portal and CARGO_TARGET_DIR=<that worktree>/target. Check `df -h /c` first; below 15 GB stop and report.
- Output only under D:/Mudcrab/scratch/portal-roomcull-image/ (OWNER.txt exists). Never write into SkyrimConverted*.
- Shots: the engine's `--shots <json> --shots-out <dir>` (see tools/reference/*.json and crates/engine/src/shots.rs for
  the JSON format, including interior cells). Write your own shots JSON with ~20 interior poses that have rooms in the
  portal-rooms DB: query its skyrim_world.db read-only (room_bounds / reference_rooms tables; sqlite3 or python sqlite3
  with `?mode=ro`) to pick cells; include Bleak Falls Barrow, a Riverwood/Whiterun house, Dragonsreach, and poses that look
  through a doorway between rooms and down long corridors, and include the portal door view (tools/reference/doorway_shots.json)
  so the portal camera is in the set. Use poses from inside the room boxes, not outside the geometry.
- Runs: the same shots 3 times with `--room-culling=off` and 3 times with the default (on). Give each run a `--run-label`.
  Engine windows open on screen; that is expected. This is not a timing run: no profiling flags needed.

## Analysis
Per pose: noise = max channel delta between off runs; culling difference = pixels where every on run differs from every off
run by > 8. Also the engine log's culled counts per pose if available (impl-1085 adds a counter to the profile bundle; a
plain run may only log room/portal counts per cell). For any pose with culling-caused pixels, save a crop/zoom PNG and
describe what is missing (object at a room edge? through an open portal? editor-only marker?).

## Report (under 500 words)
Table pose | cell | off-off noise px | culling-diff px | culled meshes if known | verdict. Overall verdict: safe to make
default / not safe (with the failing cases). Paths of the shots JSON, images and diff maps. Scripts you wrote.

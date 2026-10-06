# Goal
Track: Phase 2 (upstream, issue Mudcrab-Team/mudcrab#112, slice 2 of "doors on main"). The engine on main
reads load-door links from an OPTIONAL `door_links` table in the world database, and every spawned
load-door reference knows where it leads. No behaviour change: nothing activates doors yet.

# Context
- Work in your own worktree on D: (C: is nearly full):
  `git -C C:/Modding/MudcrabProject worktree add D:/Modding-overflow/wt-692 phase2/door-preload --detach`
  and `CARGO_TARGET_DIR=D:/Modding-overflow/wt-targets/impl-692`. Base is origin/main 461517bf.
  Never edit C:/Modding/wkj-door-preload or any portal checkout.
- Upstream style: commit subjects like `feat(engine): read load-door links from the world database`, no
  `[claude]` prefix, upstream comment density; end each commit message with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No portal-only code, no portal schema numbers.
- Reference implementation (read only), the portal branch at `C:/Modding/wkj-portal-merge`:
  table shape `crates/converter/src/esm/exporter.rs` ~127-140 and ~550, ~951 (columns ref_id,
  destination_ref_id, pos_x/y/z, rot_x/y/z, destination_cell_id, destination_worldspace_id); engine reader
  `crates/engine/src/world/database.rs` ~517-604 (`DOOR_COLUMNS`, `has_door_links`, `map_reference`);
  data model `crates/engine/src/doors.rs` (`LoadDoor`, `DoorDestination`, `load_door` in streaming.rs
  ~1983). Port only the data half: no doorway anchors, doorway geometry, portal, door animation, or
  `DoorState` behaviour beyond what a later crossing needs (you may leave `DoorState` out).
- The converter does not write `door_links` yet (a later slice, held). So: the table is optional. Detect
  it once when the database opens (`sqlite_master`); absent means no reference is a load door, one INFO
  line says so, nothing fails. Do not bump any schema version. Do not touch `crates/converter`.
- Off limits (other sessions' work): `physics.rs`, `console.rs` if present, `crates/converter/**`,
  `crates/shared/**`.

# Scope
Owned files: `crates/engine/src/world/database.rs`, `crates/engine/src/world/mod.rs`, new
`crates/engine/src/doors.rs`, `crates/engine/src/lib.rs` (module line), `crates/engine/src/streaming.rs`
(only: carry the link on the reference row into `spawn_cell` and insert `LoadDoor`),
`crates/engine/src/bin/world-inspect.rs` (only if a small "list the load doors of a cell" option fits its
existing argument style; skip otherwise and say so).

Acceptance (in your worktree): `cargo fmt --all -- --check`, `cargo nextest run -p engine`. Run
`cargo clippy -p engine --all-targets -- -D warnings` and report it, but main's clippy is currently red
from a new Rust stable: fix only warnings in lines you wrote.

# Done when
Tests: a database with a `door_links` table yields the link on the reference row (destination ref, cell
or worldspace, arrival position and rotation in the units the engine uses); a database without the
table opens and yields no links; a link whose destination cell cannot be resolved is no load door; a
spawned door reference carries `LoadDoor` (headless app, as the existing spawn tests do).

# Report
Commits (hash, subject), files, line delta, tests, clippy result (and which warnings pre-exist on main),
the table shape you read (column list) so the converter slice can match it. Remove
`D:/Modding-overflow/wt-targets/impl-692` when done; keep the worktree.

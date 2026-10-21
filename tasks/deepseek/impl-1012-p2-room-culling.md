# Goal
Engine half of upstream issue #188 (Phase 2 track, engine foundations): interior room-and-portal culling for the main camera, read from the converter's new room tables. Gated behind a setting that is off by default.

# Context
- Worktree D:/Mudcrab/worktrees/p2-roomcull, branch phase2/room-culling. It is cut from phase1/room-graph d4f8381d, the converter branch that writes the tables, so it is stacked on that branch.
- Read first, on this branch:
  - docs/specs/converters/db-schema.md §15 "Interior Room Graph" (tables `room_bounds`, `room_portals`, `room_links`, `reference_rooms`, `room_graph_stats`);
  - docs/research/interior-room-graph-converter.md;
  - C:/Modding/Mudcrab/mudcrab/local/research/interior-room-culling-design.md (an older design: use only what still fits §15).
- How the engine reads the world DB: find the `world_database` / sqlite access in crates/engine/src (streaming.rs and the interior loading path) and follow its patterns. Old databases without the tables must keep working.
- Bevy 0.19 visibility: entities hidden per view through the visibility system (`VisibilitySystems`, `ViewVisibility`, `VisibleEntities`, `RenderLayers`). Sources: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_camera, bevy_render, bevy_pbr 0.19.x). Bevy 0.20 notes: see "This run".
- Required behaviour, from #188:
  - Probe for the tables once. If they are missing (an old DB), the feature is off and nothing changes.
  - For each view: find the camera's room, then walk from it through portals, narrowing the frustum at each portal (portal rectangle against the current clip region).
  - Draw every reached room's contained objects, plus every object that is NOT contained in exactly one room. Treat `contained=0` overlap rows conservatively: an object overlapping several rooms is drawn when any of them is reached.
  - XLRM links (`room_links`) are open edges: always traversable.
  - When the camera is in no room, draw everything.
  - Shadow views keep their own caster set: never hide a shadow caster from shadow passes because the main view culled it.

# Scope (owned files)
- New crates/engine/src/room_culling.rs.
- crates/engine/src/lib.rs and crates/engine/src/app.rs: registration and the setting only.
- The engine's config/settings file: one setting, off by default, with a CLI flag that matches the existing flag style.
- The file(s) that load interior references: only the lines needed to tag spawned references with their FormID or room membership.
- Say in the report which loader file(s) you changed.
- Do not change crates/converter/**, crates/shared/** or any schema. No new crate dependencies.

# Work (two items, one commit each)
1. **Load the room graph.**
   - A resource built when an interior cell loads: rooms (bounds as oriented boxes), portals (quad corners plus the two rooms each connects), links, and per-reference membership.
   - The probe for missing tables, with a clear log line once.
   - Unit tests on an in-memory SQLite DB built in the test with §15's schema: tables present, tables absent, an empty graph, overlap rows.
2. **Per-view culling.**
   - Find the camera's room (point-in-oriented-box; with several matches, pick deterministically).
   - Portal walk with frustum narrowing: screen-space rectangle clipping is fine, and so is a conservative plane test.
   - Hide unreached contained objects for the main camera only, by the Bevy-supported mechanism. Name it in a code comment with the Bevy source line.
   - Leave shadow views and other cameras untouched.
   - Behind the setting, off by default.
   - Unit tests on synthetic graphs: a chain A-B-C with C behind a portal outside the frustum (C is culled); a portal in view (reached); an XLRM link; the camera in no room (nothing culled); an object in two rooms (drawn if either is reached); setting off (nothing culled); shadow casters keep their shadow visibility.

The image-comparison test (culling on versus off on a real interior) is NOT in this run. Leave a TODO naming it.

# Rules
- Never use `git stash`: the stash list is shared by all worktrees.
- Commits: path-only (`git add <owned files>`), no amend, no rebase, no push.
- Commit style: `feat(engine): ...`. Each commit message ends with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Before every build, run `df -h /d`. Below 15 GB free, stop and report.
- Pass `--target-dir D:/Mudcrab/builds/p2-roomcull` on every cargo command. The folder starts empty, so the first build is cold.
- Do not run benchmarks or timed engine runs.
- Run no background hold, wait or sleep commands. Builds run in the foreground.

# Done when
These pass, with the new tests:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo nextest run -p engine --all-targets`

# Report
- Status and commits.
- Per item: what and where (file:line).
- The visibility mechanism and why it leaves shadows alone.
- How old DBs behave.
- Test counts.
- Anything in §15 that was unclear or that the engine needs and lacks.

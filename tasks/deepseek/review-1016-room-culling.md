# Goal
Review a local branch for upstream issue #188, engine half (Mudcrab, Rust/Bevy 0.19): interior room-and-portal culling for the main camera, read from the converter's new room tables, behind a `--room-culling` setting that is off by default. Decide whether it is correct, faithful to its brief and the spec, and safe. Find what used to work and now fails.

# Context
- Your working folder is the review folder:
  - `delta.diff` is the change from the converter branch head d4f8381d to 95ebd30a; `commits.txt` lists the commits.
  - `before/` and `after/` hold the old and new copy of every touched file. Read code only from there.
- In `reviews/`:
  - `brief.md` is the coder's brief;
  - `db-schema-spec.md` is the world-DB spec. Section 15, "Interior Room Graph", defines the tables.
  - `room-graph-design.md` is the converter's design note.
- What the coder reported:
  - Membership is keyed by the `FormId` already on spawned references, so no loader edits.
  - Culling edits the `Mesh3d` list of the main camera's `VisibleEntities` in `VisibilitySystems::CheckVisibility`, after `check_visibility_cpu_culling`.
  - Shadow sets (`CascadesVisibleEntities`, `CubemapVisibleEntities`, spot `VisibleMeshEntities`) are untouched.
  - The walk starts from every room containing the camera.
  - Portals are projected to NDC rectangles and clipped against the near plane; a doorway within 8 units counts as open.
  - XLRM links are traversed unchanged.
  - Objects in 0 or 2+ contained rooms are always drawn.
  - Graphs are read on the async compute pool.
- Bevy 0.19 sources: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_camera-0.19.0/src/visibility/mod.rs, bevy_light-0.19.0/src/lib.rs, bevy_pbr).

# What to check
1. **Off means off.**
   - With the setting off, or with an old DB lacking the tables, nothing changes: no systems, no reads, no culling.
   - Default config and CLI parsing are unchanged for existing flags.
2. **Spec fidelity.** Table and column names and meanings match §15, including:
   - `contained` versus overlap rows;
   - the coordinate space and units of bounds and portal corners;
   - rotation conventions (quaternion or Euler order) and scale.

   A wrong axis or rotation order silently culls the wrong rooms, so check this carefully against the spec text.
3. **Culling correctness.**
   - Portal projection and near-plane clipping: a portal straddling the near plane, a camera standing in a doorway, a portal behind the camera.
   - The region intersection.
   - The 4-widenings cap (can it drop a reachable room?).
   - Conservative behaviour wherever data is missing: an unknown FormId, an entity whose FormId is on an ancestor, an entity spawned before its graph has loaded.
4. **Bevy mechanism.**
   - Editing `VisibleEntities` after the CPU culling check is honoured by extraction and rendering in 0.19.
   - It does not leak into shadows, the reflection camera or other views.
   - It runs every frame without stale state when the camera moves between rooms or cells, or a cell unloads.
5. **Cost.** Per-frame cost of finding the camera's room and walking the graph, and any per-entity work. Is it bounded for large interiors?
6. **Tests.** They exercise the real systems and the real loader, and no existing test was weakened.

# Scope
Read only. Change nothing.

# Report
- A verdict line: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - fix`. At most 10.
- What you verified as correct, in short.
- What you could not check.
- Under 700 words.

# research-023 - Which Euler order and signs do tilted references use? Audit object placement

Read `tasks/deepseek/context.md` first.

## Goal

The lead just fixed the engine's object rotation (`crates/shared/src/coordinates.rs`,
`creation_euler_to_runtime_quaternion`, commit `933b95f`). Skyrim's `REFR` angles turn
**clockwise**, and an object is now rotated by `Rx(-x) * Ry(-y) * Rz(-z)`, the inverse of
`Rz(x) * Ry(y) * Rx(x)` built from the stored angles (the Gamebryo transposed-matrix convention).
The yaw part is proven from data: on 63 door models, 88% of doors are consistent against 42%
under the old convention. The **tilt** part is not proven: the order in which x and y combine,
and their signs. Settle it from Skyrim.esm's own data, and audit the rest of object placement
(scale, position) while you are there.

## Method (suggested)

- Many references are tilted to sit on sloped ground: rocks, snow drifts, clutter, trees. Their
  up vector should roughly follow the terrain normal where they stand. Decode the terrain heights
  from the database (research-018 has a LAND/VHGT decoder in `tools/research/uesp_space_map.py`
  and `world_db.py`; the database is read-only at `C:/Modding/SkyrimConverted/skyrim_world.db`).
  Take references with `|rot_x|` or `|rot_y|` above about 3 degrees in exterior cells. Compute the
  terrain normal at each one, and compare it with the reference's up vector under several
  candidate conventions:
  - (a) the engine's current `Rx(-x) Ry(-y) Rz(-z)`;
  - (b) `Rz(-z) Ry(-y) Rx(-x)`;
  - (c) and (d) the same two orders with the x and y signs flipped;
  - (e) the old `Rz(z) Ry(y) Rx(x)`.

  Which one aligns best, and by how much? Use many thousands of references, with a robust
  statistic such as the median angle and the fraction within 10 degrees. Break it down by kind of
  object: drifts and rocks should follow the slope, some clutter will not.
- Check the engine's actual function, not a re-derivation. Port it exactly, or call its behaviour
  through a small test harness you write under `tools/research/`. `cargo test -p shared` is
  allowed if you add nothing to `crates/` (you may not edit `crates/`).
- **Placement:** check that the engine applies `scale` and `position` as the data says. Read
  `streaming.rs` where references are spawned. Look for model-space offsets the converter might
  add or drop (NIF root transforms, glTF node transforms). If something looks off, say what, with
  file:line.

## Deliverables

- A script under `tools/research/`, runnable again.
- `docs/research/object-rotation-convention.md`: the result table (convention x median angle x
  fraction aligned, per object group), a verdict, and any placement problem found. Include the
  exact quaternion formula the engine should use, if it is not the current one.
- Commit your own paths only, with a `[deepseek]` subject and a handoff note in `docs/handoff/`.

## Report

The verdict with the numbers, then any placement problem, with evidence.

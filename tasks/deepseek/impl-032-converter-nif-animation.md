# impl-032 - Converter: export NIF door animations (Open/Close) as glTF animation clips

Owned files: crates/converter/src/nif_animation.rs,crates/converter/src/mesh.rs,crates/converter/src/lib.rs
Acceptance: cargo test -p converter --lib,cargo clippy -p converter --all-targets -- -D warnings,cargo fmt -p converter -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- the design `docs/design/animated-doors-and-seamless-crossing.md` (task 1 there is this task);
- research-027's handoff `docs/handoff/research-027-door-animation.md` and its byte-level decoder
  `tools/research/door_animation_nif.py`. Its fitted `NiTransformData` layout is your starting
  point.

## Goal

Door NIFs carry a `NiControllerManager` with `Open` and `Close` `NiControllerSequence`s. They
target named nodes through `NiTransformController`/`NiTransformInterpolator`/`NiTransformData`. The
converter currently exports no animation (`dwemersmalldoorload01.glb` has `"animations": []`).
Export each sequence as a **glTF animation named after the sequence** (`Open`, `Close`, and any
others), with channels on the matching glTF nodes. The engine will play them by name.

## Requirements

- **Decode `NiTransformData` completely**: rotation keys (all key types: linear, quadratic, TBC,
  and `XYZ_ROTATION_KEY` with its three per-axis float key groups), **translation keys** and
  **scale keys**. research-027 fitted only the rotation part and left translation unverified.
  The demo route's `DweDoorLarge01Load` doors barely rotate (5 to 9 degrees), and the lead
  suspects their real motion is in the translation keys: in the game those doors visibly open.
  Settle this from the bytes and report the motion (degrees and units) of `DweDoorLarge01Load`'s
  `Open`.
- The vendored NIF parser (`vendor/**`) is off-limits. Decode the blocks you need in
  `nif_animation.rs` from the raw bytes, as research-027's script does. Keep it strict: a layout
  that does not consume a block exactly is an error, not a guess.
- glTF output:
  - Euler XYZ rotation keys → quaternion samples. Resample at 30 Hz, plus the exact key times, and
    use `LINEAR` interpolation.
  - Translation and scale keys → `LINEAR` channels (or `CUBICSPLINE` where the source is
    quadratic, if simple).
  - Apply the same coordinate conversion the converter already uses for node transforms. Find out
    where the Z-up → Y-up change happens (root node or baked into vertices), and make the
    animated node's local transforms consistent with it.
  - At t = 0 the curve must equal the node's exported rest transform (research-027 found this
    holds for two Dwemer nodes). Test it.
- Sequences whose targets cannot be found are skipped with a warning. The conversion must not
  fail because of animation data.
- **Do not bump `CONVERTER_SCHEMA_VERSION`**. The lead does one bump for all of tonight's converter
  changes.

## Tests (write them to fail before your change)

- Unit tests on synthetic `NiTransformData` byte buffers, one for each key type.
- A real-data test on the extracted meshes at `C:/Modding/SkyrimConverted/vfs/meshes/`
  (read-only), gated like the existing real-data tests in `mesh.rs` (read how they find the data
  and skip when it is absent):
  - `dungeons/dwemer/door/dwemersmalldoorload01.nif` and the `DweDoorLarge01Load` model (find it
    via the database `statics` table, `C:/Modding/SkyrimConverted/skyrim_world.db`, read-only)
    export `Open`/`Close` animations whose t = 0 pose equals the rest pose;
  - a Nordic door swings about 90 to 135 degrees; research-027 found `FarmhouseAnimDoor01` at -95.
- `cargo test -p converter --lib` stays green.

## Report

Per the implementation template. Include:
- the decoded motion of the route doors;
- the glTF animation summary for 3 real doors (channels, duration, range);
- where the coordinate conversion happens.

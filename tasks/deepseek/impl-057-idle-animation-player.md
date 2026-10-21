# impl-057 - Play a model's own looping `Idle` clip on ordinary references

## Why

The converter exports every animation a NIF carries (`crates/converter/src/nif_animation.rs`), but
the engine only ever plays two of them: `crates/engine/src/door_animation.rs` looks clips up by the
names `Open` and `Close`, on load-door references only. Everything else that was built to move
stands still.

Riverwood's finale is a view across the river of the lumber mill's water wheel
(`docs/design/riverwood-demo.md`, section 3, waypoint 10). `Lumbermill01WaterWheel01.glb` carries
exactly one animation, named **`Idle`**, one channel, on the node chain
`Lumbermill01WaterWheel01 -> SawWaterWheel03 -> L1_SawWaterWheelHub`. Nothing drives it, so the
wheel is a still photograph of a working mill. The same is true of `MillLogPile.glb`
(`LoadDust`/`PileDust`).

This task makes a looping ambient clip play on any reference whose model has one.

## What to build

A new plugin module, `crates/engine/src/model_animation.rs`, self-contained:

```rust
/// Plays a model's own looping ambient clip on references that are not doors.
pub struct ModelAnimationPlugin;

/// The clip names this plugin will play, in preference order. A model's first match wins.
pub const IDLE_CLIP_NAMES: [&str; 1] = ["Idle"];
```

Behaviour:

- For each spawned reference that has a loaded model and **no** `DoorAnimation`/`LoadDoor`
  component, look for a clip whose name is in `IDLE_CLIP_NAMES` on the glTF the loader produced.
- When one is found, build an `AnimationGraph` for it, put it on the entity the loader gave the
  `AnimationPlayer`, and play it **looping** from a start offset so that two copies of the same
  model in view are not in lockstep. Derive the offset deterministically from the reference's
  FormID modulo the clip length - the same run must produce the same phase every time, because the
  reference-shot runs compare renders byte-for-byte against earlier ones.
- Resolve each model **once** and reuse the result, the way `door_animation.rs` does with its
  per-model caches. A village has many copies of one model; this must not rebuild a graph per
  reference.
- A model with no matching clip is marked resolved-with-nothing and never looked at again.
- Log once per model path, not once per reference, at `debug` - say the model, the clip name and
  its length.

Read `door_animation.rs` before writing this. It already solves the hard parts - waiting for the
asset to load, finding the `AnimationPlayer` the glTF loader spawned, building the graph, handling
a model that is still loading - and you should follow its shape closely rather than invent a
second one. Do **not** edit it, and do not factor shared code out of it: the lead will decide
whether to merge the two after this lands.

## Registration - state it, do not write it

`app.rs` and `lib.rs` are shared files another worker is editing. Do **not** touch them. Put the
exact two lines the lead must add in your handoff note, in the style of the existing
`crate::door_animation::DoorAnimationPlugin` registration at `app.rs:223`, and say which condition
they belong under.

## Owned files

- `crates/engine/src/model_animation.rs` (new)
- `docs/handoff/impl-057-idle-animation-player.md` - your notes, including the registration lines

## Tests to write

In the new module. They must run in CI, which has no game data and no GPU, so test the logic and
not the rendering:

- The clip-name match picks `Idle` and rejects `Open`, `Close`, `idle` (exact case, unless you find
  evidence in the converter that names are normalised - if so, cite it and test what it actually
  does).
- The start offset is deterministic for a FormID and always inside `[0, clip_length)`, including
  for a clip length of 0 and for a very large FormID. Test with at least a dozen FormIDs.
- Two different FormIDs on the same model get different offsets for a clip long enough to admit
  them.
- A model resolved to "no idle clip" is not re-resolved: assert on whatever cache you build.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

Since your module is not registered, `cargo clippy -D warnings` will flag it as dead code. Make it
`pub` from the module and say in your note that the dead-code warning disappears when the lead adds
the registration - do **not** add an `#[allow(dead_code)]` to hide it, and do not add the module to
`lib.rs` yourself. If clippy fails only for that reason, report it in `commands` with the exact
message; that is an expected result, not a failure of the task.

## Out of scope

The FX emitters (`PArray07-Emitter` and friends) draw nothing - `FXMistMillWheel01.glb` and its
relatives are empty glb scenes - so particles are a separate problem. Do not attempt them. Do not
touch the converter. Do not add a way to configure which clips play from the command line.

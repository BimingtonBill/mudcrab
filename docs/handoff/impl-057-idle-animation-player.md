# impl-057 - playing a model's own looping `Idle` clip on ordinary references

**Deliverable:** `crates/engine/src/model_animation.rs` (new), self-contained; `app.rs` and `lib.rs`
untouched. Base commit `ad7f138`.

## The two lines the lead must add

```rust
// crates/engine/src/lib.rs - between `pub mod metrics;` and `pub mod papyrus_runtime;`
pub mod model_animation;
```

```rust
// crates/engine/src/app.rs, inside `if interactive { ... }` (line 217), after the three plugins
// already there - next to `crate::door_animation::DoorAnimationPlugin` (line 223):
app.add_plugins(crate::model_animation::ModelAnimationPlugin);
```

**Condition: the `if interactive { ... }` block.** Idle clips are a visual, exactly like
`LightsPlugin` and `DoorAnimationPlugin` beside them, and `interactive` is `walk || demo_tour ||
shots` (`app.rs:184`) - so the walk demo, the demo tour and the reference shots all get them, while a
benchmark or acceptance run (neither of which sets `interactive`) keeps the world still and its frame
timings unchanged.

**One consequence to expect:** a `--shots` pose that frames a model with an `Idle` clip will differ
from the same pose shot before this plugin landed - the wheel is at a fixed, nonzero phase of its
loop. That is the point of deriving the phase from the FormID alone: a re-run reproduces it exactly,
but an *older* baseline will not match.

**Both lines are needed for the module to exist at all.** A `.rs` file no `mod` declaration names is
not compiled: without `pub mod model_animation;` the file is invisible to rustc, to clippy, to
rustfmt and to `cargo test`, and no dead-code warning is printed (the brief expected one; there is
none). What I measured, on this tree:

* final tree, `cargo clippy --workspace --all-targets -- -D warnings`: clean; `cargo test -p engine`:
  274 passed, 0 failed - **the module's 8 tests do not run**;
* with `pub mod model_animation;` added for one local run (then reverted):
  `cargo clippy --workspace --all-targets -- -D warnings` is still clean - every item in the module
  is `pub`, so nothing is dead code once the module is reachable - and `cargo test -p engine` is
  **282 passed, 0 failed** (8 of them this module's).

## What it does

`ModelAnimationPlugin` claims every entity with a `MeshHandle` and a `FormId` that is not a
`LoadDoor` (the door module's private `DoorAnimation` is only ever inserted on entities that carry
`LoadDoor` - `door_animation.rs:341-346`, `:383` - so excluding the door excludes both), resolves its
model **once per model path** into `IdleModelCache`, and starts the first clip of `IDLE_CLIP_NAMES`
(`["Idle"]`) on the entity the glTF loader gave the scene's `AnimationPlayer`, looping
(`RepeatAnimation::Forever`) from `start_offset(form_id, clip_seconds)`. Three chained `Update`
systems, the shape of `door_animation.rs`'s `request_door_models`/`attach_door_animations`:
`request_idle_models` (claim; a warm cache settles the reference in the same frame),
`resolve_idle_models` (one graph per model, shared by every reference of it), `start_idle_clips`
(wait for the scene's player, then play).

Logged at `debug`, once per model path: the model, the clip's own name and its length.

## Decisions worth a look in review

1. **Exact-case clip names.** The converter copies a `NiControllerSequence`'s name out of the NIF
   (`crates/converter/src/nif_animation.rs:703`) and writes it into the document unchanged
   (`:1157-1161`); `bevy_gltf` keys `named_animations` by that same string
   (`bevy_gltf-0.19.0/src/loader/mod.rs#L583-L585`). No normalisation anywhere, so `Idle` matches and
   `idle` does not (tested).
2. **A clip of no length is not played** - the model caches as `NoClip`. Bevy's own tick does
   `seek_time %= clip_duration` (`bevy_animation-0.19.0/src/lib.rs#L586`), which is a NaN for a
   zero-length clip; the converter skips clips with `duration <= 0.0` (`nif_animation.rs:838`), so
   this is a guard rather than a case that happens.
3. **A graph per model, not per reference**: every reference of a model shares one
   `Handle<AnimationGraph>`. Tested explicitly (`Assets<AnimationGraph>::len() == 1` for two
   references).
4. **The offset is the FormID modulo the clip's length**, in `f64` so a whole `u32` FormID keeps its
   low bits, and it is the only input - no entity, no frame, no randomness - so a re-run reproduces
   every phase (the reference shots' byte-for-byte comparison). Whole-second granularity is the
   rule's own limit: a clip shorter than a second gives every FormID of it the same phase.
5. **A model whose load has failed is `NoClip`, not "keep waiting"** - otherwise a reference to an
   unreadable model would hold its `PendingIdleModel` for the life of the cell.
6. **`commands.entity(...).insert(...)`, not `try_insert`.** This is the door module's own idiom; the
   same-frame-despawn race it accepts is the same one. If the reviewer would rather this module be
   silent about a cell that unloads between a system and its command application, `try_insert` on
   the *player* entity is the one line that needs it (that entity is the one this module does not
   itself hold a query on).
7. **Opt-in real-model test**, `the_wheel_names_its_clip_idle`, `#[ignore]`d and gated on
   `OPENSKYRIM_CONVERTED_DIR` (ADR-0002) like `door_animation.rs`'s. It reads the converted
   `.glb` and asserts the wheel names its one clip `Idle` - the premise of the whole feature. I ran
   it against `C:/Modding/SkyrimConverted` with the path passed literally for one run (the sandbox
   would not let me set the env var) and it passed; an AssetServer load of the real model in a
   headless app never completes (the same render-plugin limit `door_animation.rs` documents), which
   is why the test reads the document rather than the loader.

## What is verified, and what is not

Verified by reading the real asset bytes (`grep` would not open a `.glb`, so the JSON chunk was read
directly): `meshes/architecture/farmhouse/lumbermill01waterwheel01.glb` carries exactly one
animation, `name: "Idle"`, one `rotation` channel on node 2 (`SawWaterWheel03`), and its input
accessor runs `0 .. 16.666667` s - so the wheel is a 16.67 s loop, and `start_offset` can put two
references of it up to that far apart.

**Not verified: the wheel actually turning on screen.** That needs the GPU and the converted assets,
which is the lead's run - the same place the module's end-to-end wiring is proven (the plugin is
registered by the two lines above, and nothing else reads this module).

## Test list (`crates/engine/src/model_animation.rs`, all in CI)

`the_clip_match_takes_idle_and_nothing_near_it` (picks `Idle` out of `Idle`/`Open`/`Close`/`idle`;
rejects the near misses and a model with no clips) - `the_offset_is_a_phase_inside_the_clip` (13
FormIDs to `0xFFFF_FFFF` x 5 clip lengths, all in `[0, seconds)`, repeatable, and `0` for the zero,
negative, NaN and infinite lengths) - `two_form_ids_of_one_model_start_at_different_times` (64
consecutive FormIDs land on 64 different phases of a 64 s clip) -
`every_reference_of_a_model_plays_the_clip_from_its_own_phase` (two references, two phases, both
looping, one shared graph) - `a_model_with_no_idle_clip_is_never_looked_at_again` (the cache answers
a second reference with the model's asset removed) -
`a_reference_whose_model_cannot_load_is_settled_and_left_still`,
`a_scene_that_never_produces_a_player_is_left_still` (the `SCENE_WAIT_FRAMES` give-up), and
`a_load_door_is_not_claimed`.

One test note for CI: `a_reference_whose_model_cannot_load_is_settled_and_left_still` waits (bounded)
for the module's answer instead of stepping a fixed number of frames, because the asset server's own
reporting of a failed load runs off the frame clock. The fixed-frame version passed alone and failed
about one run in three under a full parallel `cargo test -p engine`; this one went 5 for 5 in full
parallel runs (and settles in 2 frames when the machine is quiet).

## Commands

```
cargo fmt --all -- --check                                  # clean
cargo clippy --workspace --all-targets -- -D warnings        # clean (only the pre-existing
                                                            # proc-macro-error2 future-incompat note)
cargo test -p engine                                         # 274 passed, 0 failed (module not compiled)
# with `pub mod model_animation;` added for one local run, then reverted:
cargo test -p engine                                         # 282 passed, 0 failed
cargo test -p engine --lib model_animation                   # 8 passed, 0 failed
```

# impl-561 — spawned-bounds validation loses precision at real-data world placements

Branch `impl/561-bounds-precision`, cut from `main` (upstream), worktree `D:/wkj-impl/impl-561`.

## The bug

`validate_spawned_transforms_and_bounds` in `crates/engine/src/streaming.rs` checked each
descendant mesh's bounds by computing `relative = root_inverse * global.affine()`, where
`root_inverse` is the *inverse of the root's absolute `GlobalTransform`* and `global` is the
descendant's own absolute `GlobalTransform`. At a real reference's world placement (tens of
thousands of units from the origin), both of those matrices carry that large magnitude baked into
their translation. Inverting the root's matrix and multiplying it back against the descendant's
matrix cancels that large translation out in f32, which loses more precision (~0.002–0.005 units)
than the bounds tolerance allows for a small model — `tolerance = (extent * 1e-4).max(1e-3)` can be
as tight as 0.001. About 0.8% of references (tilted rotation, large `|z|`) could fail the same way,
so long acceptance runs failed at random on assets that convert correctly.

Concretely: reference 000F6031 (`meshes/Clutter/Bones/HumanSkull.glb`) at world position
`(20790.89, -69970.35, 10984.74)`, rotation `(2.0587, 0.6207, 1.3418)` rad — a genuinely correct
model — was rejected because the recomposed bound differed from the converter's exact bound by
more than the tolerance.

## The fix

Never form the root's or a descendant's absolute `GlobalTransform` matrix for the bounds check.
Instead, walk the spawned hierarchy from the root's direct children downward, accumulating each
node's transform *relative to the root* by composing local `Transform`s (`Transform::compute_affine`)
along the path — the same operation Bevy's own transform propagation performs, just without ever
multiplying by the root's own placement and inverting it back out. Every intermediate value stays
mesh-scale (never touches the tens-of-thousands-unit world position), so no cancellation occurs.

- New function `accumulate_relative_bounds` (private, in `streaming.rs`) does this walk: for each
  descendant it validates the node's local/global transform exactly as before (`validate_transform`
  is unchanged), composes `relative_to_root = relative_to_root * local.compute_affine()`, and folds
  any mesh's transformed AABB into the running `actual_min`/`actual_max`.
- `validate_spawned_transforms_and_bounds` now calls this once per direct child of the root (with
  `Affine3A::IDENTITY` as the starting relative transform) instead of looping over
  `children.iter_descendants(root)` and computing `root_inverse * global.affine()` per node. The
  tolerance check, the "no bounded mesh" error, the `WorldTransform` cross-check, and
  `validate_transform`'s per-node checks are all unchanged.
- No converter changes, no schema bump, no new dependency: `Affine3A` is already part of the `glam`
  re-export `bevy` pulls in (`bevy::math::Affine3A`), just not previously imported here.

`crates/engine/src/world/components.rs` was not touched — no helper belonged there; `InstanceBounds`
and `ExpectedModelBounds` needed no changes.

## Test

`streaming::tests::spawned_bounds_validate_within_tolerance_despite_large_world_placement` in
`crates/engine/src/streaming.rs`.

The fixture mirrors the failing acceptance run: a root at reference 000F6031's real world position
and rotation, with a two-level node hierarchy modeled on `HumanSkull.glb` — a basis node with a
-90-degree X rotation, and a mesh-child node under it with scale 1.14 and translation
`(0, -7.7, -150.7)` — carrying a small synthetic cube mesh (local half-extent 6, similar order of
magnitude to the real skull's model-space bounds). The ground-truth model-space (relative-to-root)
bounding box is computed independently in f64 (`f64_relative_bounds`, composing the same
basis/mesh-local chain in `DAffine3`/`DQuat`/`DVec3`), standing in for the converter's own exact
computation.

The test:
1. Inlines the *old* algorithm (root-inverse × descendant-global, in f32) and asserts its error
   against the f64 ground truth **exceeds** the tolerance — reproducing the impl-561 acceptance
   failure.
2. Calls the real (fixed) `validate_spawned_transforms_and_bounds` and asserts it returns `Ok`.
3. Replicates the *new* algorithm's own bounds via `accumulate_relative_bounds` directly and
   asserts its error is **within** tolerance, for the numbers below.

Measured on this fixture (`tolerance = (extent * 1e-4).max(1e-3)`):

| | value |
|---|---|
| tolerance | 0.0013680008 |
| old method's error (root-inverse × global, f32) | 0.0046834946 (**3.4× over tolerance**) |
| new method's error (composed local transforms, f32) | 0.000015258789 (**~90× under tolerance**) |

All existing streaming and bounds tests (including the ones that expect a genuinely wrong bound to
be rejected — e.g. `world::components::tests::transforms_all_aabb_corners_for_rotation_and_scale`
and `world::components::tests::rejects_invalid_expected_model_bounds`) still pass unmodified.

## Checks run

```
$ cargo fmt --all -- --check
(no output — clean)

$ cargo clippy -p engine --all-targets -- -D warnings
    Checking engine v0.1.0 (D:\wkj-impl\impl-561\crates\engine)
    Finished `dev` profile [optimized + debuginfo] target(s) in 4.42s

$ cargo test -p engine
running 43 tests
...
test streaming::tests::spawned_bounds_validate_within_tolerance_despite_large_world_placement ... ok
...
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.53s

     Running unittests src\main.rs (target\debug\deps\engine-678cd4098bdc2be7.exe)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\bin\world-inspect.rs (target\debug\deps\world_inspect-3bfb433de86de37a.exe)
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.10s

   Doc-tests engine
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Not run: the engine binary or a real acceptance pass against
`C:/Modding/SkyrimConverted-upstream15/` — the brief scoped this to `cargo test` only (the GPU
belongs to another session).

## Report

**Files changed and the approach, in 3 lines:**
1. `crates/engine/src/streaming.rs` — added `math::Affine3A` to the bevy import.
2. Replaced the bounds loop in `validate_spawned_transforms_and_bounds` (which computed
   `root_inverse * global.affine()` per descendant) with a call to a new recursive
   `accumulate_relative_bounds`, which composes each descendant's local `Transform` down the path
   from the root's children, so the large-magnitude world matrix is never formed or inverted.
3. `docs/handoff/impl-561-p2-bounds-precision.md` (this file) — new.

**Test and errors on the failing input:**
`streaming::tests::spawned_bounds_validate_within_tolerance_despite_large_world_placement` —
tolerance `0.0013680008`; old method (root-inverse × global, f32) error `0.0046834946` (exceeds
tolerance, reproducing the impl-561 acceptance failure); new method (composed local transforms,
f32) error `0.000015258789` (well within tolerance).

# Goal
Track: Phase 2 (for upstream). The engine's strict spawned-bounds check must not reject correct
models because of f32 rounding. Today it composes each mesh's world transform, including the
reference's large world placement, and then multiplies by the inverse of the root's world
transform. That loses about 0.002 units at world coordinates of ~10,000-70,000, which is more
than the tolerance for small models.

# Context
- Your checkout is cut from `main` (upstream). Use upstream's commit style (`fix(engine): ...`),
  with no `[deepseek]` prefix.
- The check: `validate_spawned_transforms_and_bounds` in `crates/engine/src/streaming.rs` (on main
  around lines 950-1000; search for "spawned hierarchy bounds diverge from conversion"). For each
  descendant mesh it builds `relative = root_inverse * global.affine()` from Bevy
  `GlobalTransform`s, transforms the mesh's `compute_aabb()` box with
  `InstanceBounds::transformed` (`crates/engine/src/world/components.rs`), and compares the result
  with the converter's model-space bounds under `tolerance = (extent * 1e-4).max(1e-3)`.
- Real failure (a 5-minute acceptance run on real data): `meshes/Clutter/Bones/HumanSkull.glb`,
  reference 000F6031 at world position (20790.89, -69970.35, 10984.74) with rotation (2.0587,
  0.6207, 1.3418) rad. Expected (-6.405849, -0.4191467, -9.065458)..(6.40585, 14.073388, 9.065459),
  actual (-6.4077973, -0.41986084, -9.066015)..(6.4039, 14.072662, 9.0649), tolerance 0.00181.
  The converter's expected bounds are exact: an independent f64 recomputation from the GLB matches
  them. The error comes from composing and then un-composing the large world transform in f32.
  About 0.8% of references (tilted rotation, large |z|) could fail the same way, so long
  acceptance runs fail at random.
- The fix: compute each descendant's transform relative to the root by composing the local
  `Transform`s along the path from the root, never forming the root's world matrix. This is
  mathematically the same, but it never builds large-magnitude matrices whose inverse cancels
  lossily. Keep the tolerance as it is. The same function also validates each node's
  local/global transform; keep that check as it is.

# Scope
Owned files:
- `crates/engine/src/streaming.rs` (the validation function and its tests)
- `crates/engine/src/world/components.rs` (only if a helper belongs there)
- `docs/handoff/impl-561-p2-bounds-precision.md`

No converter changes, no schema bump, no new dependencies.

# Done when
- A test that fails on the old code and passes on yours: a small model (a mesh child with node
  scale about 1.14 and translation (0, -7.7, -150.7) under a -90 degree X basis node, as in
  HumanSkull.glb) placed at the failing reference's world position and rotation validates within
  tolerance. The old composition must be shown to exceed the tolerance for the same input. If you
  cannot reproduce the old failure in a unit test, say so and show the largest error each method
  produces.
- Every existing streaming and bounds test still passes, including the ones that expect a
  genuinely wrong bound to be rejected.
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings` and
  `cargo test -p engine` pass.

# Report
- Files changed and the approach in 3 lines.
- The test's name, and the error each method produces on the failing input.

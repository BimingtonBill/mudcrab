# impl-003 - Make the spawned-bounds check independent of world position

Owned files: crates/engine/src/streaming.rs
Acceptance: cargo test -p engine --lib streaming,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md` first.

## Goal

The engine's strict bounds check rejects a correct mesh once the camera is far from the origin,
because it measures the mesh through world-space transforms and loses f32 precision. Make the check
measure each mesh relative to its reference root without going through world space, so the result
does not depend on where in the world the reference is.

## Evidence (verified by the lead)

A real-asset acceptance run's stability scenario (camera flying far across Tamriel) logged:

```
ERROR engine::streaming: model, material, or image dependency failed strict validation
reference=00097844 base=0005AD48 cell=000097CC
path=meshes/Dungeons/Mines/Scaffolding/MineScaffoldFreeRopeEnd01.glb
spawned hierarchy bounds diverge from conversion:
expected Vec3(-7.7411976, -12.0, -11.412674)..Vec3(7.7413006, 12.0, 11.412674),
actual   Vec3(-7.7448425, -12.000905, -11.410722)..Vec3(7.7376404, 11.999095, 11.414628),
tolerance 0.0023999999
```

The actual box is the expected box **translated** by about (-0.0036, -0.0009, +0.0020) - not
scaled or noisy - which is the signature of precision lost in a large translation, not of a bad mesh.

The check is `crates/engine/src/streaming.rs:948-994`: it computes
`root_global.affine().inverse()` and then `root_inverse * global.affine()` for every descendant
(`:948`, `:973`). With the root tens of thousands of units from the render origin, both the inverse
and the product are computed in f32 at large magnitudes; the tolerance at `:983-984`
(`extent * 1e-4`, min `1e-3`) is sized for mesh-local error only.

## What to change

- Compute each bounded descendant's transform **relative to `root`** by composing the local
  `Transform`s along the parent chain from the descendant up to (not including) `root`, instead of
  `root_inverse * global`. Use the `ChildOf`/parent query or whatever this function already has
  access to; if it needs one more query parameter, add it and update its callers in the same file.
  Read the Bevy 0.19 source in the cargo registry for the exact parent API - do not guess it.
- Keep the check's meaning: same error messages, same tolerance formula, same rejection of
  non-finite values, no rejected case becomes accepted except precision loss from world position.
  Do **not** loosen the tolerance - that would hide real bound bugs.
- Keep `validate_transform` for each node as it is.

## Tests (in the same file's test module)

1. A hierarchy whose root sits at a large translation (e.g. `(120_000.0, -95_000.0, 8_000.0)`) and
   whose bounded child is offset and rotated: the check passes, and the same hierarchy at the origin
   gives the same computed bounds within 1e-5. Write it so that it **fails on the current code**:
   run it before your change and say in your result what it printed.
2. A genuinely wrong hierarchy (child mesh moved by 0.5 units) is still rejected at both the origin
   and the far position.
3. Existing streaming tests still pass.

If building a test hierarchy needs a Bevy `App` or `World`, look at how the existing tests in this
file do it and follow that pattern.

## Done when

The three acceptance commands pass in your checkout. Report per the implementation template, and in
`critical_evidence` include the failing output of test 1 on the original code.

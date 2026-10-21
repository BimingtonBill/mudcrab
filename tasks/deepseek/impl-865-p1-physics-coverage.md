# Goal
Widen the converter's NIF collision coverage (upstream issue #104 coverage follow-up, Phase 1 track):
convert `bhkSphereShape`, `bhkCylinderShape` and `bhkMultiSphereShape`, and accept the extra physical
collision layers found in the census, so more Skyrim clutter bodies reach the GLB collision extras.

# Context
- Worktree: `D:/Mudcrab/worktrees/p1-physics`, branch `phase1/physics-coverage` (already checked out),
  based on `448a51c2` (rigid-body extraction, draft PR #118). Do not touch other branches.
- All work is in `crates/converter/src/collision.rs`:
  - `extract_collision...` loop near line 63: per `bhkCollisionObject`, reads the body's layer at byte 4.
    Layer 15 returns silently (nonphysical flora). Then `ensure!(matches!(layer, 1 | 2 | 3 | 4 | 9 | 10 | 13 | 17 | 27 | 31 | 35))`.
  - the shape decoder (around lines 440-525): `bhkCapsuleShape`, `bhkBoxShape`, `bhkConvexVerticesShape`, ...;
    unknown shapes `bail!("unsupported shape {other}")`, which ends up in `asset.skipped`.
  - Helpers: `vec3_at`, `f32_at`, `u32_at`, `point(transform, v)`, `HAVOK_TO_CREATION`, `MAX_SHAPES`.
- The shared shape enum (`crates/shared/src/collision.rs`, `CollisionShape`) has Mesh, Capsule {a,b,radius},
  Box, Hull. **Decision: do not change `crates/shared` or the engine.** Represent:
  - a sphere as `CollisionShape::Capsule { a: c, b: c, radius }` (zero-length capsule = sphere; the engine
    builds it with `Collider::capsule(a, b, radius)` in `crates/engine/src/streaming.rs:1289`). Confirm by
    reading parry3d's capsule code in the cargo registry (`%USERPROFILE%/.cargo/registry/src/*/parry3d-*`)
    that a zero-length segment is valid (no NaN, no panic), and say what you found.
  - a cylinder as `CollisionShape::Hull` with two rings of 16 points each around the A-B axis
    (32 points), radius = cylinder radius. Degenerate (A == B, radius <= 0, non-finite) bails.
  - a multi-sphere as one sphere-capsule per sphere (bounded: nif.xml range 1..8; reject more).
  - Radii are scaled like the capsule code does (`* HAVOK_TO_CREATION * transform scale`), points through
    `point(transform, v * HAVOK_TO_CREATION)`. Copy the capsule arm's style.
- Byte layouts (Skyrim SE, from niftools nif.xml; VERIFY each against
  `D:/Mudcrab/worktrees/p1-scope/ref-byroredux/docs/legacy/nif.xml`, read-only, and the existing capsule
  arm whose offsets are known-good: material u32 @0, radius @4, unused 8 bytes, vertex A Vector4 @16, ...):
  - `bhkSphereShape` (inherits bhkConvexShape): material @0, radius @4. Centre is the origin.
  - `bhkCylinderShape`: material @0, shell radius @4, unused 8, vertex A Vector4 @16, vertex B Vector4 @32,
    cylinder radius @48 (length >= 52).
  - `bhkMultiSphereShape`: material @0, bhkWorldObjCInfoProperty (12 bytes) @4, num spheres u32 @16,
    then NiBound {center Vector3, radius f32} x n from @20.
  If the XML disagrees with these, follow the XML and say so.
- Layers (SkyrimLayer enum in that nif.xml): 0 UNIDENTIFIED, 5 WEAPON, 12 TRIGGER, 26 TRANSPARENT_SMALL,
  28 TRANSPARENT_SMALL_ANIM. Add 0, 5, 26, 28 to the allow-list if the census check below shows they are real
  physical clutter (props, weapons, small objects), not trigger or marker volumes. **12 TRIGGER stays out**:
  keep it as a skipped entry with a clear reason ("trigger volume (layer 12) is not physical").
  Cite the layer names in a comment like the existing enum citations.
- Census tool (read-only, already exists): `crates/converter/examples/physics_census.rs`.
  Clutter census command (a few seconds after the build):
  `OPENSKYRIM_CENSUS_CLUTTER_ONLY=1 cargo run --release --target-dir D:/Mudcrab/p1-physics-target -p converter --example physics_census -- D:/Mudcrab/SkyrimConverted-portal.assets-cache/.ingestion-cache D:/Mudcrab/p1-physics-nif-list.tsv`
  Baseline at 448a51c2: clutter meshes 1,745, bodies 1,686, dynamic 727. Targets: about 1,762 bodies and
  732+ dynamic, plus the sphere/cylinder ones (Chef's reference: 1,781 / 754). Nothing may be tuned to hit them.
  To check what layers 0/5/26/28/12 are, you may extend the census example to print, per layer, the count and
  up to 5 sample install paths (from the list file) of bodies on that layer. Keep it read-only.
- Tests: add to `crates/converter/tests/physics_bodies.rs` and/or `collision.rs`'s unit tests. Existing
  fixtures use hand-built byte blocks (see the `#[cfg(test)]` module in collision.rs around line 940 and the
  "hand-built byte block" test) - build new shape blocks BY HAND with explicit offsets, not via a writer
  that shares code with the decoder. Per shape: one decode test checking exact output values (centre,
  radius with unit scale, cylinder ring points at radius from the axis, both ends), one malformed/short
  block test (no panic, bails). One layer test: layer 5 accepted, layer 12 skipped with the reason.
- Main's clippy is currently red from the new Rust stable (3 `bail!` lines in `crates/dummy-content`);
  that is being fixed elsewhere. Do NOT fix it here. Run clippy on converter only.

# Scope
Owned files: crates/converter/src/collision.rs, crates/converter/tests/physics_bodies.rs, crates/converter/examples/physics_census.rs, docs/specs/converters/nif-to-gltf.md
- docs/specs/converters/nif-to-gltf.md: update section 6 (collision) to list the new shapes, their mapping
  and the layer allow-list.
- Change only the owned files. No new dependencies. No git commands (no commit, no stash, no checkout).
- Always pass `--target-dir D:/Mudcrab/p1-physics-target` to cargo (disk on C: is short).

# Done when
Acceptance: cargo fmt --all -- --check, cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/p1-physics-target -- -D warnings, cargo nextest run -p converter --target-dir D:/Mudcrab/p1-physics-target
- All three pass. New tests fail if an offset or the unit scale were wrong.
- The clutter census is run before (stash nothing: just report the baseline numbers above as given) and
  after your change; the after numbers are reported exactly.

# Report
- Changes and non-obvious decisions; any layout that differed from this brief.
- Per layer 0/5/12/26/28: count in clutter and 3-5 sample paths, and your verdict (physical or not) with why.
- Clutter census after: meshes, bodies, dynamic, and the remaining skipped reasons with counts.
- Commands run and their results. The parry3d zero-length capsule finding (file:line).

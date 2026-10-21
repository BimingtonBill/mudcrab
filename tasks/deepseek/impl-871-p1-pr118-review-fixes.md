# Goal
Apply confirmed pre-open review fixes to draft PR #118 "feat(converter): extract rigid-body dynamics into the collision data" (Phase 1 track, issue #104), as uncommitted edits in the worktree.

# Context
- Worktree `D:/Mudcrab/worktrees/p1-pr118`, branch phase1/physics-bodies, head 448a51c2, base a896b903. The PR's change: `git diff a896b903 HEAD`.
- Confirmed problems and fixes:
  1. Node binding: in the conversion path (`crates/converter/src/mesh.rs` around lines 157-163, `retain_bodies_with_nodes(.., require_unique_name = false)`) a body's glTF node index is checked against `model.static_nodes` names, while the annotate path (~172-182) reads the names from the exported GLB itself and requires a unique name. Make both paths verify against the node names of the GLB that is actually written (`glb_json_from_bytes` on the bytes already in hand) and require a unique, non-empty name in both. A body that fails goes to `skipped` exactly as today. Update the unit tests in mesh.rs that pin the old behaviour (~lines 803-831, ~1690) and add a test where the static-node order and the GLB node order would disagree, or explain in the report why the exporter cannot produce that and test the uniqueness rule instead.
  2. Census: `crates/converter/examples/physics_census.rs` counts via `MeshConverter::extract_collision`, which never runs the node verification, so its counts are extraction-stage. Add a clearly labelled line to its output counting bodies whose node verification would fail, if that can be done read-only from the NIF alone; otherwise only make the doc comment at the top say the counts are before node verification.
  3. Remove the no-op `Some(file).and_then(|_| ...)` wrapper in the census (~line 175).
- Do NOT change body classification (`kind`), the shared contract (`crates/shared`), or the engine; those were agreed with the engine owner.
- No new dependencies.

# Scope
Owned files: crates/converter/src/mesh.rs, crates/converter/examples/physics_census.rs, crates/converter/tests/physics_bodies.rs, docs/specs/converters/nif-to-gltf.md
- Spec: if the node-verification rule text in section 6 changes, update it.
- No git commands except `git diff`, `git status`, `git log`, `git show`.
- Always pass `--target-dir D:/Mudcrab/builds/p1-pr118` to cargo and use toolchain `+1.98.1`.

# Done when
Acceptance: cargo +1.98.1 fmt --all -- --check, cargo +1.98.1 clippy -p converter -p dummy-content -p shared --all-targets --target-dir D:/Mudcrab/builds/p1-pr118 -- -D warnings, cargo +1.98.1 nextest run -p converter -p dummy-content -p shared --target-dir D:/Mudcrab/builds/p1-pr118
- All pass; the new tests fail if the old static_nodes check is restored.

# Report
- What changed, per fix; decisions; commands run and results.

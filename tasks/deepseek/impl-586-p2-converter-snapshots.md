# Goal
Track: Phase 2 (for upstream). Add `insta` snapshot tests for the converter's output, so any
change to what the converter produces shows up as a readable diff in review instead of as a wrong
render hours later.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`test(converter): ...`), no
  `[deepseek]` prefix, no project-internal references in code, comments or docs.
- The converter crate is `crates/converter`. Its end-to-end fixture tests are in
  `crates/converter/tests/` (`fixture_round_trip.rs`, `fixture_interior_pipeline.rs`,
  `fixture_doors.rs`); they build a small synthetic game with the `dummy-content` crate (a dev
  dependency), run the conversion pipeline and assert on the output (glTF/GLB models, material
  data, the world database `skyrim_world.db`, the manifest). Read them first and reuse their
  set-up; don't build a second fixture.
- The decision on the dependency is made: add to `crates/converter/Cargo.toml`
  `[dev-dependencies]`: `insta = { version = "1.48", features = ["json", "redactions"] }`.
  The workspace root keeps its `[workspace.dependencies]` for runtime crates only; don't add insta
  there. Cargo.lock will change; that's expected.
- `cargo-insta` (the CLI) is NOT installed. Without it, a new snapshot's first run fails and writes
  a `*.snap.new` file beside the test (`crates/converter/tests/snapshots/`). That is fine: see
  Done when.

# What to snapshot
Pick the outputs that best show a converter regression, from the existing fixtures. Aim for 3 to 5
snapshot tests, each small enough to read in review (a few hundred lines at most):
1. One converted static model's glTF JSON (the GLB's JSON chunk, or the .gltf), with binary buffer
   contents left out: node tree, mesh primitives and their attribute names/counts, materials and
   their extras (`OPEN_SKYRIM_material` and friends), bounds.
2. The world database's key rows for the fixture: cells, references (with their transforms and
   flags), door links, lights; as sorted, stable rows (ORDER BY a key) serialised to JSON.
3. The conversion manifest / report fields that describe the result (counts, completeness,
   per-asset entries), with volatile fields redacted.
4. Optional: a parsed ESM record or the material JSON for an effect/alpha material, if the fixture
   has one and it adds coverage the others don't.
Use `insta::assert_json_snapshot!` (or `assert_yaml_snapshot!`), with insta redactions for
anything that changes from run to run or machine to machine: absolute paths, temp directory names,
timestamps, elapsed times, process ids, and hashes that include any of those. Snapshots must hold
no machine paths. Round floats only if the output itself is not stable (say why in a comment).
Put the tests in a new `crates/converter/tests/snapshots.rs` (sharing helpers by copying the small
set-up the round-trip test uses, or by a `mod common;` if one already exists). Add a short module
comment saying what the snapshots cover and how to accept a change (`cargo insta review`, or
rename the `.snap.new` to `.snap` after reading it).

# Scope
Owned files: `crates/converter/Cargo.toml` (the dev-dependency only), `Cargo.lock`,
`crates/converter/tests/snapshots.rs`, `crates/converter/tests/snapshots/*` (generated), and
`docs/handoff/impl-586-p2-converter-snapshots.md`. Change nothing in `src/`. If you need another
file, say so in the report instead of editing it.

# Done when
- `cargo fmt --all -- --check` passes.
- `cargo clippy -p converter --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings` passes.
- `cargo test -p converter --test snapshots --target-dir C:/Modding/shared-check-phase2`: the first
  run writes `.snap.new` files. Read each one and check it: no absolute paths, no temp names, no
  timestamps, readable size. Run the test a second time and confirm the `.snap.new` content is
  byte-identical to the first run (deterministic). Leave the `.snap.new` files in place: the lead
  accepts them. Every failure must be only "snapshot missing/new", nothing else.
- `cargo test -p converter --target-dir C:/Modding/shared-check-phase2` shows the other converter
  tests still pass.

# Report
The snapshot tests (name, what each covers, snapshot size in lines), every redaction and why, the
determinism check result, anything in the output you found surprising (a field that looked wrong or
unstable), and the diff summary per file.

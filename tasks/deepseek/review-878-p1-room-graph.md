# Goal
Review a local branch before it goes upstream to Mudcrab-Team/mudcrab (a Rust/Bevy engine running Skyrim assets converted ahead of time). Treat it like a stranger's PR: be strict. Decide whether it is correct, and above all whether anything that used to work now fails.

# Context
- Change: the converter half of issue #188, an interior room graph. Commits: `fix(converter): remap room and portal marker FormIDs on references`, `feat(converter): export interior room graph and room membership`, a spec wording fix, and two design notes.
- Review folder (read code ONLY from here; the checkout under `crates/` is a different branch, never cite it):
  - `local/review/pr-room-graph/full.diff`: the whole change against upstream main 2c11897b.
  - `local/review/pr-room-graph/base/`: main's version of every touched file. `.../head/`: the new version. `head/coordinates.rs.ref`: `crates/shared/src/coordinates.rs` (unchanged, for the axis conventions).
  - Design: `head/docs/research/interior-room-graph-converter.md`. Spec: `head/docs/specs/converters/db-schema.md` section 15.
- Facts already checked against Skyrim.esm bytes by the lead: XPRM 32 bytes (float[3] half-extents, float[3] colour, float, u32 shape 1 box); XPOD two u32 FormIDs; XRMR u8 count of following XLRM, u8 flags (0x80 = LNAM present, 0x40 = INAM present), u16 unknown; XLRM, LNAM, INAM one u32 FormID each. RoomMarker base 0x1F, PortalMarker 0x20.
- The author ran fmt, clippy -D warnings (workspace, all targets, all features) and nextest on converter and dummy-content: 599 passed.

# Check, with diff-line evidence
1. Remap fix: XPOD/XLRM/LNAM/INAM are remapped on REFR only; nothing else's remap changes (LNAM/INAM mean other things on other record types). Does the test prove it?
2. What used to work and might now fail: `create_tables` now adds tables to every database (including the movement annotator's subset export path and reused databases); `export_records` runs the room pass only on load-order exports; `finalize_world_database` now calls `add_overlap_rows` and reads stats. Any error path that could now fail a conversion that used to succeed (e.g. missing tables, a DB without `statics.bounds_*` columns, a NULL scale or bounds, i64/u32 conversions of FormIDs above 0x7FFFFFFF, rusqlite type errors reading REAL as f32)?
3. Geometry: the rotation convention `Rx(-x) Ry(-y) Rz(-z)` against `coordinates.rs.ref`; room-local transform; the overlap test's conservativeness; the glTF-axis to Creation-axis conversion of `statics.bounds_*` and the claim that no unit scale applies (look at how bounds are produced in `head/crates/converter/src/integration.rs` and mesh code if present in the diff).
4. Interior detection (no grid, no worldspace), and which placed record types are included.
5. Tests: would each new test fail if its feature were wrong (e.g. drop the REFR guard, swap rotation sign, skip the axis conversion)?
6. Spec section 15 matches the code.

# Scope
Read only. Change nothing.

# Report
Verdict line: ready / small fixes / fix first. Then findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 8, only verified ones, each marked blocker / should fix / nit. Under 450 words.

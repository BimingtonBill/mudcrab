# research-023 - which Euler order and signs do tilted references use?

**Task:** settle the tilt half of the object rotation convention from Skyrim.esm's own
data (the order in which x and y combine, and their signs), and audit position/scale
placement while there.

**Done.** The engine's current convention is confirmed on 82,731 references and needs no
change. Report: `docs/research/object-rotation-convention.md`; full numbers:
`docs/research/data/object-rotation-audit.json`; the tool:
`tools/research/rotation_convention.py`.

## The result in one line

Tilted references standing on sloped ground point their up axis at the terrain normal a
median of **8.18°** away under the engine's `Rx(-x) * Ry(-y) * Rz(-z)` (58.0% within
10°), against **19.45°** for the nearest alternative `Rz(-z) * Ry(-y) * Rx(-x)` and
**23.36°** for the pre-`933b95f` convention; per reference it wins 78.5% of head-to-head
comparisons with the former (z = 162) and 82.5% with the latter (z = 187). Every sign
flip on x or y costs 12-24° of median error, so the tilt signs are not in doubt either.

## What changed

| | |
|---|---|
| `docs/research/object-rotation-convention.md` | new: method, the 16-convention table, per-group table, the paired sign test, the placement audit, what is open |
| `docs/research/data/object-rotation-audit.json` | new: the audit's machine-readable result (conventions, groups, paired tests, port check) |
| `tools/research/rotation_convention.py` | new: `selftest`, `census`, `audit --cells N --json`, `placement`, `model <name>` |
| `docs/handoff/research-023-rotation-order-audit.md` | this note |

No file outside those four was touched.

## Verified

* The audit does not re-derive the rotation: `tools/research/rotation_convention.py:113`
  ports `crates/shared/src/coordinates.rs:37` line for line, and the module's own Rust
  tests are re-run against the port every run (`rotation_convention.py selftest`: PASS).
  The port's rotation matrix equals `XYZ -/-/-` to 1.0e-15; the nearest other candidate
  is off by 1.99.
* 82,731 references in all 11,186 LAND cells of Tamriel (worldspace 60) contributed:
  exterior, ≥3° tilt, within 128 units of the ground, ground sloped >5°. Median tilt
  16.0°, median slope 17.1°. Per-group medians behave as they should — flora 7.4°, rock
  9.3°, snow 10.0°, tree 8.3°, and clutter 28.0° (a crate is tilted on purpose, so it
  measures nothing).
* The order question is not a second-order effect after all: under `XYZ` the up axis is
  `(-sin y, sin x cos y, cos x cos y)` and does not depend on the yaw, while under `ZYX`
  the yaw rotates the tilt direction, so the two hypotheses are a median of 19.1° apart
  on references tilted in both axes (31.8° at >10° tilt). The ground decides, and it
  picks `XYZ`: 9.98° vs 34.06° at >10° tilt in both axes.
* Placement: the engine's spawn transform
  (`crates/engine/src/streaming.rs:763`, rotation `:1629`, scale `:776`) applies
  position, rotation and a uniform scale in that order; `XSCL` is one float
  (`crates/converter/src/esm/exporter.rs:405`), so nothing is lost. In the data, the
  reference origin lands within 64 units of the terrain for 69% of 228,187 references
  versus 0% for a random-cell control, and no scale is at or below zero.
* The converter adds no model-space offset: `statics.bounds_*`
  (`crates/converter/src/integration.rs:102`) reproduces a hand-computed axis swap of the
  glb's own POSITION accessor boxes exactly for the models checked.

## Not verified, and where the risk is

* **The z sign** cannot be measured this way — the up axis is invariant under `Rz`, so
  every convention appears twice in the table with identical numbers. It still rests on
  the door evidence in `933b95f` and the heading test at
  `crates/shared/src/coordinates.rs:172`. Nothing in this audit contradicts it.
* **Interior references** are not covered: few are tilted, and their "ground" is
  furniture rather than a heightmap. They go through the same function, but they are not
  independently measured here.
* **Trees carry a large underground section.** `TreePineForest04`'s converted geometry
  spans z −1188…2085 relative to the reference origin while the LOD meshes in the same
  glb span −88…1925 (the aspen is −691…1439 against −16…1235). Since the origin is
  measured to sit on the ground, the visible crown is the right size and the buried part
  is invisible, so this is not a defect — but I could not read the source NIF to confirm
  the offset is authored rather than dropped: the NIFs live in `Skyrim - Meshes.bsa` and
  no tool under `tools/research/` can read a BSA. Do not treat "the model box touches the
  ground" as a placement test for trees until someone checks one NIF's root translations.

## For the lead

1. Nothing to fix in `crates/`. The convention at
   `crates/shared/src/coordinates.rs:37` is the measured best of sixteen candidates by a
   wide margin; the rotation work of `933b95f` can be considered closed from the data
   side, and the doc's opening line is the citation to use for it.
2. The one thing worth a second pair of eyes is the tree geometry above — but it is a
   *node translation* question, not a rotation one, and it needs a BSA reader or a NIF
   extracted by hand to answer.
3. If another object class ever looks wrong on screen, the method generalises: add a
   candidate tuple to `CONVENTIONS` in `tools/research/rotation_convention.py` and
   re-run `audit --cells 0`. A well-chosen 400-cell subset already ranks the conventions
   the same way.

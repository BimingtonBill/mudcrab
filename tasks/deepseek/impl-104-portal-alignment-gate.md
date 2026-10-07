# impl-104 - Turn the door alignment measurement into a checked gate

Owned files: tools/research/portal_door_alignment.py,tests/test_portal_door_alignment.py

Acceptance: python -m unittest discover -s tests,python tools/research/portal_door_alignment.py check

Read first: `docs/research/portal-door-alignment.md` - every number in its sections 4, 5 and 9.2 came
out of this tool, and its section 11 lists the commands. The tool was written today and its numbers
were hand-checked once (research-110); nothing checks them again.

## Goal

This measurement is the evidence for an engine change, and it currently has no test: a wrong edit to
the port, or a reconversion that changes the data, would go unnoticed. Give it (a) unit tests for
its own arithmetic, and (b) a `check` command that asserts the documented characterisation of the
converted data and exits non-zero when it stops holding.

## Context

`tools/research/portal_door_alignment.py` ports the engine's door -> arrival map line for line
(`outward_from_return_link` crates/engine/src/doors.rs:87, `door_frame` transition.rs:237,
`arrival_camera_rotation` transition.rs:216, `door_to_arrival_rotation` transition.rs:287,
`portal_pose` transition.rs:303) and measures, over every door with a `door_links` row in
`C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` (converter schema 19), how far that map is from the map
that would make the two doorways coincide as geometry. Its `selftest` command already checks six of
the port's properties against the engine's documented behaviour, by hand, in `command_selftest`.

One direction per door is measured: 2,204 doors, 2,204 directions, 1,102 reciprocal pairs. A
direction is a door and the door its link leads to.

## What to build

1. **A `check` command** that asserts the characterisation below and prints one line per assertion
   (`ok` / `FAIL` with the expected and the measured value), returning 0 only when all pass. Read it
   from the database; skip with a clear "database not present" line and return 0 when
   `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` does not exist (this is a developer machine, not
   CI). Tolerances: 0.05 degrees, 2 units, 1% of a count.
   - counts: 2,204 doors with links, 2,204 links, 2,204 directions, 1,102 reciprocal pairs; all
     1,863 doors carry usable bounds; outward rule = 1,848 point, 356 heading; 259 doors have a
     reference scale other than 1
   - models: 41 of 104 models have a trustworthy axis convention (>= 4 placements, >= 80% within 15
     degrees of the mean); 1,468 of 2,204 directions have both doorways known
   - the map's yaw error: median 2.65 degrees over the directions whose two doorways are known, 5.9%
     of them over 45 degrees; median 4.56 degrees and 20.0% over 45 over all directions
   - the as-drawn offset over the known-doorway directions: median 75.5 units, 83.9% in 32..128
   - the anchored landing's height error: median 8.3 units
   - the tier split from report section 9.2, at the gate 256 units / 64 units: 1,356 same-model,
     185 both-conventions-known, 55 centre-only, 608 arrival anchor
   - Sven's House `0x0001CBB0` -> `0x0001CBAF`: yaw error +12.27 degrees, as-drawn 82.8 units
     (depth -80.8, sideways 17.9); the reverse direction +0.19 degrees and 106.9 units
2. **Unit tests** in `tests/test_portal_door_alignment.py` (unittest, importable as
   `tools.research.portal_door_alignment` - check how the existing tests in `tests/` import repo
   modules and follow that; if the tool is not importable, make it so without changing its command
   line behaviour). Cover at least:
   - the quaternion port: `q_mul` associativity and identity, `from_rotation_y`, `q_to_yaw_degrees`
     against the Creation heading convention (an object's reference yaw `z` faces `(sin z, cos z)`,
     crates/shared/src/coordinates.rs:24-50)
   - `outward_from_return_link`'s two rules and their boundaries at 16 and 512 units, including the
     doc's tower-door case (32 units east is east; an arrival at the door is the heading)
   - `door_frame`: its `-Z` faces the outward direction, and it survives a `None` outward
   - `Rigid`: composition and inverse round trip; that the engine map is rigid and keeps heights
   - `engine_map`/`geometry_map`: a point at the source position lands on the arrival point, and the
     geometry map takes the source doorway centre to the destination doorway centre
   - `histogram_table`/`percentile` boundaries
   - the Sven's House arithmetic of report section 5, computed from the raw rows pasted there, as a
     regression on the port's inputs: doorway centres (20676.02, -46406.30, -34.13) and
     (-511.49, -268.44, +72.00), own facings -30.147 and -1.282 degrees, the +12.27 degree yaw error
     and the 82.8-unit as-drawn offset
   - a `check`-level test that runs only when the database is present (skip otherwise) and asserts
     the same characterisation through the public functions, so the gate is tested as well as run
3. **Keep every existing command working** (`selftest`, `probe`, `summary`, `models`, `placements`,
   `pair`, `trace`, `worst`, `spots`, `list`, `modes`, `modes sweep`); the report's section 11 tells
   readers to run them exactly as they are. If you refactor, keep the printed shapes.

## Notes that will save you time

- `selftest` must keep passing; it is the port's only check against the engine's own documented
  behaviour. Do not weaken it.
- The database is read-only (`file:...?mode=ro`). Do not write to it, and do not touch `crates/`,
  the converter, or `docs/` - `docs/research/portal-door-alignment.md` is the report that quotes
  these numbers and another worker owns it.
- If an assertion above turns out to be wrong when you run it, do not change the tool to match the
  number: report the discrepancy, with the value you measured and the command that printed it. The
  report and the numbers must agree, and a real disagreement is a finding.

## Scope

Only the two owned files. No engine changes, no database writes, no new dependencies (standard
library only - the repo's tools use `sqlite3` and `math`). Report per the implementation template:
what changed, the test names and what each covers, the `check` output, and any discrepancy you found.

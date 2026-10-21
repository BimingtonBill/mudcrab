# impl-267 - the portal bench's look-out states

**Track:** portal. **Base:** 8ba6d7d. Adds two bench states that stand inside an interior and look
out through its load door at a sunlit exterior, so the destination sun's cost is measured. The
design is research-266 sections 2-5; the code follows the brief's summary of it.

**Nothing was run.** This session's shell has no usable `PATH` (every external command, `git`,
`cargo`, `python` and even `ls`, exits 127 with `command not found`), so no acceptance command and
no engine run were possible. See the report for the exact evidence. Everything below is read, not
executed.

## Changed

* `tools/bench/portal_bench_doors.py` (+83/-21): the `LOOKING_OUT` table (four hand-picked
  interior-side doors), `looking_out_doors()`, which checks each row against the database - the
  reference must exist, its `kind` must be `interior-to-exterior`, it must stand in the named
  interior cell, its link must lead to the named worldspace, and the far door must link back into
  it - and raises `LookingOutError` on a mismatch; `--view-out` (default
  `tools/bench/portal_bench_doors_out.json`); the near/beyond counts and labels are now filled by
  one helper, `fill_load()`, used by both lists, so the main list's bytes do not move. The main
  document is written **before** the look-out list is validated, so a bad row cannot change the
  main file.
* `tools/bench/test_portal_bench_doors.py` (+80): the `LookOutTests` class - an out door is written
  from the interior side; a door that leads indoors, a door with no return link, an unknown
  reference and a door that opens onto the wrong worldspace are each refused; the look-out table
  does not change the main list; the table names four different doors in two places.
* `crates/engine/src/portal_bench.rs` (+~150/-40): `BenchDoor.kind` (`#[serde(default)]`),
  `BenchDoor::looks_out()`/`first_state()`; `BenchState::{ClosedOut, OpenOutView}` with names
  `closed-out`/`open-out-view`, poses, `standoff()` and `is_closed()`, and `ALL` grown to 8;
  `After::OpenTheDoor` now carries the state to time once the door is open, and `Phase::Settle`
  carries the state it settles into, so a look-out door runs only its own two states and every
  other door keeps the six exactly as before; `door_pose()` takes the standoff as a parameter and
  the out states use `OUT_STANDOFF` (160) against `STANDOFF` (320); a look-out door whose `outward`
  is `None` is skipped at `FindDoor` with a `SKIP door ...` line and no model-axis fallback.
* `tools/bench/portal-bench.ps1`: the two state names added to `$states`, and the header documents
  `-Doors tools/bench/portal_bench_doors_out.json`.
* `docs/demo/README.md`: a paragraph under "The portal bench" with the out-run command.

## Not done, and why

* **`tools/bench/portal_bench_doors_out.json` is not written.** It is generated data: it needs
  `cells.id`, the door positions and the reference counts from
  `C:\Modding\SkyrimConverted\skyrim_world.db`, which only the script can read. Run

  ```
  python tools/bench/portal_bench_doors.py --db C:/Modding/SkyrimConverted/skyrim_world.db
  ```

  and both files are rewritten (the main one byte-identically). The four rows' editor ids and
  worldspaces are corroborated by `docs/research/portal-door-alignment.md:421-424`, which was
  written from the same database, so the table should validate first time; if it does not, the
  script says which row and exits 2.
* **The `the_checked_in_out_doors_file_parses` test is not added.** `include_str!` of a file that
  does not exist is a compile error, so adding it before the JSON exists would break
  `cargo test -p engine --lib`. Add it beside `the_checked_in_doors_file_parses`, reading
  `../../../tools/bench/portal_bench_doors_out.json`, once the file is generated.
* **The out-run was not made**, and could not have been: it needs a `quick` engine built from this
  branch, which needs cargo.
* `tools/bench/_probe_refs.py` is a throwaway probe this session created to dump the four doors
  from the database. It never ran (no python) and could not be deleted (no shell): remove it.

## Notes for the reviewer

* The camera side is right, and here is why: `LoadDoor::outward` is documented
  (`crates/engine/src/doors.rs:64-74`) as "the horizontal unit direction the door faces ... the
  side a player walks in from", taken from the return link's `XTEL`. For an interior door that
  side is inside the room, so `Side::Front` for `ClosedOut`/`OpenOutView` stands inside the house
  facing the door, as the brief asks.
* The out states use `ClosedOut` (quiet target `DOOR_QUIET_FRAMES`, the door still shut) and
  `OpenOutView` (the door has already been opened for), matching the old `Settle`/`OpenSettle`
  split.
* `research-266`'s report is at `local/agents/runs/research-266-portal-bench-looking-out.1/`,
  which is outside this session's readable roots, so the brief's summary was the design I worked
  from. Any deviation from the full design is therefore possible; the brief's items are all
  covered.

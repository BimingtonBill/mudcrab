# Alftand → Blackreach: what a seamless worldspace-transition demo needs

Task t02. Research note, no code changed. Everything marked **verified** was read out of the code at
commit `cc6e714` or measured out of `Skyrim.esm` with the probe script described in §1; everything
marked **inferred** is reasoned, not measured.

## 0. Verdict

The demo is reachable, and the job is smaller than the first reading suggested — but it is not what
the brief expected. Three corrections to that reading, all **verified**:

1. **The engine's interior path already exists end to end.** `CellKey::Interior(u32)`
   (`crates/engine/src/world/database.rs:21`), the interior query
   (`crates/engine/src/world/database.rs:288`), the interior transform branch
   (`crates/engine/src/streaming.rs:522`) and the interior root translation
   (`crates/engine/src/streaming.rs:1258`) are all implemented and unit-tested
   (`crates/engine/src/world/database.rs:470`). Nothing ever *produces* an interior key:
   `plan_cells` builds `CellKey::Exterior` only (`crates/engine/src/streaming.rs:194`). The interior
   half of the engine is finished code with no caller.
2. **A second worldspace needs no new streaming code at all.** Blackreach is an ordinary exterior
   worldspace (320 cells, 125 of them with `LAND` terrain — §2), streamed through exactly the same
   `exterior_spatial` path as Tamriel. The single blocker is that the worldspace id comes from an
   immutable resource (`crates/engine/src/config.rs:44`, read at
   `crates/engine/src/streaming.rs:195`).
3. **What is genuinely missing is the door.** `XTEL` — the subrecord that carries a load door's
   destination — appears **nowhere in the repository's code**:
   `grep -r XTEL` over the tree matches only the t02 brief and the probe script added by this task.
   Without it there is no door to cross. And even if a door existed, doors are `DOOR` records, which
   the schema does not carry (§3), so the demo's own door would be invisible.

The route also does not go where the brief assumed: Alftand's descent is **four** load doors, not
three, and it passes through a child worldspace (`AlftandWorld`) before reaching Blackreach.

## 1. How the numbers were derived

`tools/research/esm_route.py` — a read-only Python probe of a `.esm`/`.esp`/`.esl` that mirrors the
converter's own record walk (`GRUP` size includes its 24-byte header, cell context from group types
6/8/9/10, worldspace context from group type 1, zlib for the `0x00040000` compressed-record flag:
`crates/converter/src/esm/binary.rs:114`, `crates/converter/src/esm/binary.rs:140`,
`crates/converter/src/esm/binary.rs:165`). It is deliberately independent of the converter binary.

```text
python tools/research/esm_route.py "<Data>/Skyrim.esm" alftand blackreach nchuandzel
python tools/research/esm_route.py "<Data>/Skyrim.esm" understone markarth
```

Every FormID, EditorID, position, rotation and count in §2, §5 and §6 was printed by those two
commands; none is quoted from memory or from a wiki.

**The converted database could not be used.** `C:/Modding/SkyrimConverted` (and any other path
outside the session's readable roots) is refused by `permissions.blockReadsOutsideWorkingDirectories`,
so the brief's `mode=ro` queries and the real asset closure could not be run. §5 therefore counts the
*references and mesh paths reachable from the plugin*, which is exact, and **estimates** textures,
which is not. §7 lists the SQL that would replace the estimates if the lead opens the DB.

## 2. The route in the data

### 2.1 The worldspaces involved

| FormID | EditorID | `WNAM` parent | cells | cells with `LAND` | refs | distinct meshes |
|---|---|---|---|---|---|---|
| `0000003C` | Tamriel | `00000000` | (not counted) | — | — | — |
| `0001EE62` | **Blackreach** | `0000003C` | 320 | 125 | 4678 | 406 |
| `00069857` | **AlftandWorld** | `0000003C` | 44 | 35 | 783 | 170 |
| `00016D71` | MarkarthWorld | `0000003C` | 147 | 140 | 1740 | 344 |

Blackreach is a child worldspace of Tamriel; the parent link is already exported
(`crates/converter/src/esm/exporter.rs:151`, `WNAM` at `crates/converter/src/esm/exporter.rs:159`).

### 2.2 The four load doors

Each row is one crossing. `XTEL` (28 bytes) lives on the **source** door and holds
`destination_ref (u32)` + arrival `(x,y,z)` + arrival `yaw,pitch,roll`; the arrival point is *not*
the destination door's own position (they differ by tens to hundreds of units). Positions are
Creation-engine units, `X` north-ish / `Y` east-ish as the engine's converter expects; the engine
maps them through `creation_to_bevy` (`crates/engine/src/streaming.rs:1269`).

| # | from (worldspace / cell) | source door ref | base (`EDID`) | source position | `XTEL` → destination ref | arriving in |
|---|---|---|---|---|---|---|
| 1 | Tamriel, persistent cell `00000D74` (no `EDID`, grid 0,0) | `00015D48` | `AutoLoadDoor01` | 78049.18, 76985.00, −5859.11 | `000152CF` @ (−947.038, 3958.835, 591.917) rot (0, 0, 2.96989) | cell `000152C3` **Alftand01** (interior) |
| 2 | Alftand01 `000152C3` (interior) | `00092809` | `DweDoorLarge01Load` | 4057.00, 8792.00, −516.00 | `0005704B` @ (2879.831, 2718.830, −1828.0) rot (0, 0, 2.87979) | cell `00056C1B` **Alftand02** (interior) |
| 3 | Alftand02 `00056C1B` (interior) | `0009256A` | `DweDoorLarge01Load` | 9040.29, −2592.04, −9223.65 | `000699E8` @ (3693.815, 3074.645, 290.530) rot (0, 0, −1.83260) | cell `00069869` (no `EDID`) in worldspace **`00069857` AlftandWorld** |
| 4 | AlftandWorld `00069869` (grid 0,0) | `0006998D` | `DweDoorLarge01Load` | −4419.67, 740.95, 1304.83 | `0004E504` @ (21088.559, 18512.045, 2434.0) rot (0, 0, −1.87080) | cell `0002D4E0` (no `EDID`, grid 0,0) in worldspace **`0001EE62` Blackreach** |

Every one of the four is a reciprocal pair: the destination door's own `XTEL` points back at the
source (`000152CF` → `00015D48` arriving at 77583.23, 77411.89, −5817.21; `0005704B` → `00092809`;
`000699E8` → `0009256A`; `0004E504` → `0006998D`). Two doors on the route are marked `MinUse` in
their base `EDID` only for the lifts, not here.

The cells around the route:

| cell | `EDID` | kind | grid | refs | distinct bases | `LAND` |
|---|---|---|---|---|---|---|
| `00000D74` | — | Tamriel persistent | 0, 0 | (holds refs across the whole map) | — | no |
| `00008F63` | AlftandExterior01 | exterior | 19, 19 | 78 | 50 | yes |
| `00008F82` | AlftandExterior02 | exterior | 19, 18 | 86 | 64 | yes |
| `00008F83` | AlftandExterior03 | exterior | 18, 18 | 47 | 27 | yes |
| `000152C3` | Alftand01 | interior | — | 2331 | 444 | no |
| `00056C1B` | Alftand02 | interior | — | 2223 | 480 | no |
| `00069869` | — | AlftandWorld persistent | 0, 0 | (holds 3 of the world's doors) | — | no |
| `00069881` | AlftandWorldStart | exterior | 0, 0 | 171 | 60 | yes |
| `0006987A` | AlftandWorldEnd | exterior | −1, 0 | 398 | 125 | yes |
| `00069879` | AlftandWorldForge | exterior | −1, 1 | 54 | 36 | yes |
| `0006987F` | AlftandWorldOrigin | exterior | −2, 0 | 92 | 48 | yes |
| `0002D4E0` | — | Blackreach persistent | 0, 0 | (holds ~40 doors) | — | no |
| `0001EE7A` | BlackreachOrigin | exterior | 0, 0 | 53 | 30 | yes |
| `000237BD` | AlftandBlackreach01 | exterior | 5, 4 | 105 | 38 | yes |

**The persistent-cell pattern is the load-bearing detail.** Tamriel's entry door `00015D48` is not
stored under the exterior cell at grid (19,18) — it lives in Tamriel's persistent cell `00000D74`
with a position at 78049, 76985. The same is true at the far end: door 4 lands in Blackreach's
persistent cell `0002D4E0`, whose `XCLC` says grid (0,0) while the arrival point is at 21088, 18512
(grid 5,4). Both ends of the route are therefore *persistent references*, and the existing pipeline
already handles them:

- `insert_reference` derives `local_x`/`local_y` from the true position, not from the owning cell
  (`crates/converter/src/esm/exporter.rs:290`), because `is_exterior` is true whenever the record
  has a worldspace (`crates/converter/src/esm/exporter.rs:285`);
- it indexes the reference by its global position in `exterior_spatial`
  (`crates/converter/src/esm/exporter.rs:299`);
- `load_cell` finds references by spatial query, not by `cell_id`
  (`crates/engine/src/world/database.rs:270`), so persistent and non-persistent references at the
  same grid both render;
- `EXTERIOR_CELL_ID_SQL` prefers the cell at a grid that has `LAND`
  (`crates/engine/src/world/database.rs:15`), which is what makes two cells at grid (0,0)
  (persistent + visible) resolve to the visible one.

So the first and fourth crossings will work with **no change** to how references are stored or
loaded. That is the single most important thing to know before starting.

### 2.3 Other doors worth knowing

- The **Alftand Great Lift** is a direct Blackreach ↔ Tamriel pair, one door each way:
  `000EF7B8` in Blackreach cell `0002D4E0` → `000EF79B` in Tamriel persistent cell `00000D74`
  arriving at 80506.77, 55425.82, −2254.90 (grid 19,13, the `BlackreachQuickExitAlftand` cell), and
  back. Its base is `DweFacadeLiftLeverLoadUp01` / `DweFacadeLiftLeverLoadDown01_MinUse`, i.e. it is
  driven by a lever, not by walking through it. Useful as a *short* demo of the worldspace switch,
  not as the requested walk-in.
- `DwePtnDoor01NoName` refs in `AlftandExterior02` (`0002DDF9`, `0010F7DB`) carry **no `XTEL`**:
  they are decorative. Do not build the demo on them; the real entrance door is `00015D48`.
- Of 3535 door references in `Skyrim.esm`, 1722 carry an `XTEL` and 213 distinct `DOOR` base
  objects are used. The other ~1800 are decorative doors.

### 2.4 How the record layout expresses interiors

**Verified** from the plugin: an interior cell has no `XCLC` (no grid), no worldspace group above it,
and the converter's group walk therefore leaves `worldspace_form_id` `None`
(`crates/converter/src/esm/binary.rs:140`). `extract_cell_info` fills `interior_name` from `EDID`
and leaves the grid `NULL` (`crates/converter/src/esm/extractors.rs:64`). `Alftand01` and `Alftand02`
are exactly this shape, so they are already stored as `cells` rows with `worldspace_id IS NULL`,
`grid_x IS NULL`, `grid_y IS NULL` and an `interior_name` — which is precisely what
`CellKey::Interior(cell_id)` expects.

## 3. What exists, what is missing

| Layer | Needs | State | Evidence |
|---|---|---|---|
| converter | interior `CELL` records exported | **done** | `crates/converter/src/esm/exporter.rs:106`, `crates/converter/src/esm/extractors.rs:64` |
| converter | non-Tamriel worldspaces exported | **done** | `crates/converter/src/esm/exporter.rs:151` (all 37 `WRLD` rows land in `worldspaces`, `WNAM` at `:159`) |
| converter | worldspace parent link | **done** | `crates/converter/src/esm/exporter.rs:158` |
| converter | persistent exterior refs indexed by true position | **done** | `crates/converter/src/esm/exporter.rs:285`, `crates/converter/src/esm/exporter.rs:299` |
| converter | `XTEL` parsed | **missing** | `grep -r XTEL` matches no code; `is_form_id_subrecord` has no `XTEL` arm (`crates/converter/src/esm/mod.rs:144`) |
| converter | `XTEL` FormID remapped for merged plugins | **missing** | same guard: only 4-byte subrecords are remapped (`crates/converter/src/esm/mod.rs:145`), and `XTEL` is 28 bytes |
| converter | door/teleport data reachable from SQL | **missing** | the raw bytes survive only inside the opaque `data` blob (`crates/converter/src/esm/exporter.rs:292`); no column, no table (`crates/converter/src/esm/exporter.rs:36`) |
| converter | meshes for `DOOR`/`ACTI`/`FLOR`/`CONT`/`TREE` | **missing** | only `STAT`/`MSTT`/`FURN` populate `statics` (`crates/converter/src/esm/exporter.rs:177`) |
| converter | `NiTriStripsData` geometry | **missing** (risk) | no parser in the vendored crate (`vendor/project-wormhole-nif/src/legacy.rs:64`); `NiTriStrips` is only *declared* (`crates/converter/src/mesh.rs:210`) |
| database | `references` DDL as implemented | **spec is stale** | `docs/specs/converters/db-schema.md:105` shows a `references` table without `base_form_id`, `is_exterior`, `local_x/local_y` or scale defaults; the real DDL is `crates/converter/src/esm/exporter.rs:36` |
| database | schema version gate | **done** | `crates/shared/src/lib.rs:7`, enforced at `crates/engine/src/world/database.rs:186` |
| engine | load an interior cell by id | **done, unused** | `crates/engine/src/world/database.rs:288`, tested at `crates/engine/src/world/database.rs:470` |
| engine | spawn an interior cell (no terrain, cell-local transform) | **done, unused** | `crates/engine/src/streaming.rs:522`, `crates/engine/src/streaming.rs:1258` |
| engine | stream any worldspace | **done** | `crates/engine/src/streaming.rs:194` — the key carries `worldspace_id` |
| engine | choose the worldspace at runtime | **missing** | `EngineConfig` is immutable in the planning system (`crates/engine/src/streaming.rs:172`) and the id comes from the CLI (`crates/engine/src/config.rs:94`) |
| engine | request an interior at all | **missing** | `plan_cells` inserts exteriors only (`crates/engine/src/streaming.rs:194`) |
| engine | ever unload an interior | **missing** | `cell_within_unload_radius` returns `true` for every interior (`crates/engine/src/streaming.rs:404`), and the test at `crates/engine/src/streaming.rs:1773` asserts that as intended behaviour |
| engine | rebase correctly while inside | **missing / hazard** | `update_render_origin` moves the camera and only `ExteriorCellGrid` roots (`crates/engine/src/streaming.rs:1629`); an interior root carries no such marker (`crates/engine/src/streaming.rs:432`), so a rebase inside an interior would slide the world sideways relative to the camera |
| engine | any door, teleport or crossing detection | **missing** | `fly_camera` only translates the camera (`crates/engine/src/app.rs:1366`); the only teleport in the tree is the fixture jumping the camera by whole cells (`crates/engine/src/app.rs:301`) |
| engine | render a reference whose base is not `STAT`/`MSTT`/`FURN` | **missing** | `crates/engine/src/streaming.rs:554` — a `NULL` `model_path` spawns an entity with no mesh, silently |

The last row is the one that shapes the demo: on this route, `DOOR` = 134, `ACTI` = 557,
`FLOR` = 185, `CONT` = 107, `TREE` = 223, `IDLM` = 144 references would be invisible. The load doors
themselves, the harvestable flora and every activator are in that set. Either `statics` grows to
cover them or the demo has no doors to look at.

## 4. Implementation recipe

Ordered, smallest-first. Each step names the test that proves it. Steps 1–3 are converter-side,
4–8 engine-side, 9 the demo itself. Nothing here changes `crates/shared/` contracts except the two
version constants in step 3.

**Step 1 — parse `XTEL` and store the link (converter).**
Add an `XTEL` arm to `insert_reference` (`crates/converter/src/esm/exporter.rs:259`): read
`u32` destination ref at offset 0, `[f32; 3]` arrival at 4, `[f32; 3]` arrival rotation at 16 when
the subrecord is at least 28 bytes. Write a new table
`door_links(ref_id INTEGER PRIMARY KEY, destination_ref_id INTEGER, pos_x/y/z REAL, rot_x/y/z REAL)`
in `create_tables` (`crates/converter/src/esm/exporter.rs:14`). Resolve
`destination_ref_id` → its cell and worldspace in a post-pass at the end of `export_to_db`
(`crates/converter/src/esm/exporter.rs:242`) with a single `UPDATE ... FROM` over the `references`
rows already inserted, and add `destination_cell_id`/`destination_worldspace_id` columns. Do this
inside the existing transaction, not with a second pass over the plugin.

**Step 2 — remap the `XTEL` FormID (converter).** The destination ref is a FormID from the source
plugin's own index space, and the current remap skips it because the guard requires `len == 4`
(`crates/converter/src/esm/mod.rs:145`). Extend the `(record_type, tag, len)` table
(`crates/converter/src/esm/mod.rs:148`) with a `(b"REFR", b"XTEL")` arm that remaps **the first
four bytes only**, leaving the 24 bytes of floats alone. Test: a synthetic `REFR` from a non-first
plugin with `XTEL` must come out with the remapped index — the existing
`remaps_actual_form_id_subrecords` case (`crates/converter/src/esm/mod.rs:261`) is the pattern to
copy. Without step 2 the four route doors are still correct (they are all in `Skyrim.esm`, index 0),
which is exactly why the bug is easy to miss and worth fixing now.

**Step 3 — version bump (shared).** `crates/shared/src/lib.rs:7` → 4, and the schema `INSERT`
(`crates/converter/src/esm/exporter.rs:18`) with it. The engine already refuses an older database
with a clear message (`crates/engine/src/world/database.rs:186`). Test:
`crates/engine/src/world/database.rs:397` must keep failing for version 3.

**Step 4 — carry `DOOR`/`ACTI`/`FLOR`/`CONT`/`TREE` models (converter).** Extend the match arm at
`crates/converter/src/esm/exporter.rs:177` to those five record types, and widen the engine's join
(`crates/engine/src/world/database.rs:276` is `LEFT JOIN statics s`). Keep the table name or rename
it to `base_objects` in the same commit; the joining code is the only reader
(`crates/engine/src/world/database.rs:271`). Decide here whether the acceptance closure should also
cover them (`crates/converter/src/bin/asset-closure.rs:251` currently scopes `STAT`/`MSTT`/`FURN`):
making the doors visible widens the closure the asset preflight must satisfy. Test: extend the
fixture at `crates/engine/src/world/database.rs:328` with a `DOOR` base object and assert
`payload.references[0].model_path` is the door mesh.

**Step 5 — an active-cell resource (engine).** Add `ActiveCell { worldspace_id: u32,
interior: Option<u32> }` beside `RenderOrigin` (`crates/engine/src/streaming.rs:167`), seeded from
`config.worldspace_id` in `crates/engine/src/app.rs:107` with the camera. `plan_cells` reads it
instead of `config.worldspace_id` (`crates/engine/src/streaming.rs:195`). Leave
`initial_camera_ground_height` (`crates/engine/src/app.rs:1303`) alone — it is a startup-only query.
Test: a unit test on `plan_cells`'s key set, or extend the existing fixture
(`crates/engine/src/app.rs:301`) to change the resource mid-run and assert the resident cells move to
the new worldspace.

**Step 6 — request and retire interiors (engine).** In `plan_cells`, when
`ActiveCell.interior` is `Some(cell_id)`, insert `CellKey::Interior(cell_id)` into `wanted`
(`crates/engine/src/streaming.rs:191`) and do **not** insert exterior keys for the current
worldspace (or keep them, per the demo's return path — see step 9). Change
`cell_within_unload_radius` (`crates/engine/src/streaming.rs:399`) so an interior survives only while
it is the active one; the existing test `unload_radius_removes_distant_exteriors_but_keeps_interiors`
(`crates/engine/src/streaming.rs:1773`) encodes the old rule and must be rewritten with the new
argument. Test: resident-cell counts across a simulated crossing, and
`validate_streaming_lifecycle` (`crates/engine/src/streaming.rs:1646`) reporting zero orphans after
the interior unloads.

**Step 7 — freeze rebasing inside an interior (engine).** Guard `update_render_origin`
(`crates/engine/src/streaming.rs:1619`) on the active cell: while an interior is active, force
`shift` to zero. The alternative — also offsetting interior roots — is more code for no benefit,
because interior coordinates are already a few thousand units from the origin. Test: enter an
interior whose references sit at |x| > 4096 (Alftand02's door is at −9223) and assert
`origin_rebases` does not move and the reference's camera-relative position is unchanged; the
existing `repeated_rebasing_preserves_camera_and_cell_root_locality`
(`crates/engine/src/streaming.rs:1797`) is the template.

**Step 8 — the transition system (engine).** New system in `Update`, ordered **before** `plan_cells`
(`crates/engine/src/streaming.rs:54`), owning three responsibilities:

1. *Pre-stream.* Query near doors (the DB worker gains a query like
   `SELECT ... FROM door_links JOIN references ... WHERE minX BETWEEN ...`; the `exterior_spatial`
   rtree already gives the exterior side, `idx_references_cell` the interior side —
   `crates/converter/src/esm/exporter.rs:43`). When the camera is within a few hundred units of a
   door, request the destination cell: `CellKey::Interior(destination_cell_id)` when the destination
   has no worldspace, otherwise the 3×3 grid around the arrival point in
   `destination_worldspace_id`. This is the whole of "no loading screen": the destination is already
   resident before the crossing, and the crossing itself is a camera move.
2. *Crossing test.* The camera "passes" a door when its distance to the source ref drops below a
   threshold and it is on the far side of the door plane. Build the plane normal from the reference
   rotation the engine already has (`ReferenceRow::rotation`,
   `crates/engine/src/world/database.rs:38`). A demo-only variant is acceptable — a trigger sphere
   around the door, exactly as the vanilla `MinUse` lift doors behave — but the plane test is the
   general rule and is barely more code.
3. *Cross.* On crossing, set `ActiveCell` from the `door_links` row, then move the camera to the
   stored arrival position and rotation (`XTEL`'s own values, not the destination door's position —
   they differ) and post a `profiler.event` for the bundle.

Test: an engine test building the two cells, one door link and a scripted camera sweep, asserting
(a) the destination is `Resident` **before** the crossing frame, (b) the camera lands on the arrival
position, (c) `streaming_invariant_failures == 0` and `origin_rebases` unchanged.
`crates/engine/src/streaming.rs:1646` (`validate_streaming_lifecycle`) and the fixture contract at
`crates/engine/src/app.rs:340` are the existing shapes to extend.

**Step 9 — the demo scenario itself (acceptance).** A five-cell walk: Tamriel (19,18) → Alftand01 →
Alftand02 → AlftandWorld (0,0) → Blackreach (5,4). Four crossings, no loading screen. Two decisions
belong to the lead: whether the old worldspace's cells stay resident during the interior (the
return trip then also needs no load, at the cost of ~60 resident cells of Tamriel), and whether the
demo uses the Great Lift pair (§2.3) as a short-cut shot. It fits `scripts/phase2-acceptance.ps1`'s
world-scenario shape as a sixth scenario, or as a new `--transition-fixture`; either way the
screenshot gate (`crates/engine/src/metrics.rs:190`) applies.

The four steps that must land together to see anything at all are **1, 5, 6 and 8**; step 4 is what
makes the doors visible; steps 2, 3 and 7 are correctness.

## 5. Size of the job

Measured from the plugin (reference counts and distinct `MODL` paths over `STAT`/`MSTT`/`FURN`/
`ACTI`/`FLOR`/`DOOR`/`CONT`/`TREE`/`IDLM` base objects):

| Scope | references | distinct meshes |
|---|---|---|
| Blackreach worldspace (`0001EE62`) | 4678 | 406 |
| AlftandWorld (`00069857`) | 783 | 170 |
| Alftand01 (interior) | 2331 | 444 distinct bases |
| Alftand02 (interior) | 2223 | 480 distinct bases |
| **route total** (the 4 doors' cells, both ends, Tamriel side included) | ~16000 across the focus set | 1027 |
| of which Dropped (no `statics` row, §3) | `DOOR` 134 + `ACTI` 557 + `FLOR` 185 + `CONT` 107 + `TREE` 223 | — |
| the ~40 `BlackreachZcell*` interiors behind side doors | ~1700 | — (not on the route) |

**Textures — inferred, not measured.** Skyrim architecture meshes carry one diffuse and one normal
map, frequently a third for glow or detail (`crates/converter/src/material.rs:888`). At 1.5–2.5
textures per mesh, Blackreach's 406 meshes imply roughly **600–1000 texture files**, the whole route
maybe **1500–2500**. This is the number most likely to be wrong; §7 says how to replace it.

Per-cell density is the same order as the acceptance scenarios already run: Blackreach's named cells
hold 28–182 references each (median ~100), against the five real-world scenarios the campaign
already schedules (`scripts/phase2-acceptance.ps1:264`). The route is *not* a stress case; the interiors
(2331 and 2223 references in a single commit) are, because `max_cell_commits_per_frame` is 1 and
`max_commit_micros_per_frame` is 16 670 (`crates/engine/src/config.rs:48`,
`crates/engine/src/config.rs:49`). Pre-streaming (step 8)
is what keeps that off the crossing frame.

Converter gaps that can bite on this content: `NiTriStripsData` has no parser at all
(`vendor/project-wormhole-nif/src/legacy.rs:64`), and a NIF that declares geometry but converts to
nothing is a hard error (`crates/converter/src/mesh.rs:87`). SSE meshes are overwhelmingly
`BSTriShape`, so the risk is **inferred** to be low, but it is unmeasured: the closure over
`Blackreach*.nif` cannot be run without the converter (`target\release\nif-audit.exe`). Skinned
meshes and particles are already out of Phase 2 scope (`docs/roadmap/02-core-engine.md:51`), and
Blackreach's content is static architecture plus a handful of animated Dwemer machines, so their
absence costs visual fidelity, not the demo.

## 6. The fallback: Markarth → Understone Keep → Nchuand-Zel

Also four crossings, and a harder route than the brief implies:

| # | from | source door ref | base | `XTEL` → destination ref | arriving in |
|---|---|---|---|---|---|
| 1 | Tamriel persistent `00000D74` | `0001C389` | `DweDoorLarge01Load` | `0001C38B` @ (−173399.53, 5407.91, −3347.92) | cell `00016E02` (no `EDID`, grid 0,0) of **`00016D71` MarkarthWorld** |
| 2 | MarkarthWorld `00016E02` | `00016E47` | `DweDoorLarge01Load` | `0007F65E` @ (−1987.29, −2673.18, 6432.0) | cell `00016DF2` **MarkarthUnderStoneKeep** (interior) |
| 3 | Understone Keep `00016DF2` | `0007F681` | `DweDoorLarge01Load` | `0003EF54` @ (2994.86, −1081.76, 656.0) | cell `00015237` **NchuandZel01** (interior) |
| 4 | NchuandZel01 `00015237` | `00055384` | `DweDoorLarge01Load` | `000553A4` @ (3200.0, 4160.0, 704.0) | cell `00055215` **NchuandZel02** (interior), and on to `000571B4` / `00058B4D` / `00058B4C` |

The excavation beneath Understone Keep is **Nchuand-Zel**, cells `00015237`, `00055215`, `000571B4`,
`00058B4D`, `00058B4C` (843 / 982 / 526 / 1260 / 895 references). Markarth's exterior city is not a
region of Tamriel but a **child worldspace of its own**, so this route crosses a worldspace boundary
at door 1 as well.

Size: MarkarthWorld 1740 references / 344 meshes over 147 cells (140 with `LAND`); Understone Keep
2614 references / 487 distinct bases in one cell; Nchuand-Zel 4506 references over five cells.
Against Blackreach's 4678 + 4554 + 783.

The comparison that decides it:

| | Alftand → Blackreach | Markarth → Understone Keep |
|---|---|---|
| requested by the user | yes | fallback |
| crossings | 4 (Tam→int→int→child world→world) | 4 (Tam→**city world**→int→int→int) |
| exterior streaming while crossing | sparse ruin + huge cavern | dense city, buildings on all sides |
| largest single cell commit | Alftand01, 2331 refs | Understone Keep, 2614 refs |
| destination character | one continuous worldspace, ~406 meshes, a showpiece | a network of interiors, then a Dwemer ruin |
| what it proves that the other does not | interior→**worldspace** with terrain streaming | Tamriel→a dense **city worldspace** |

## 7. What I could not verify, and how to close it

1. **The converted database was unreadable** — `C:/Modding/SkyrimConverted` is outside the session's
   permitted roots and every read was refused
   (`permissions.blockReadsOutsideWorkingDirectories`). No SQL was run against
   `skyrim_world.db`. With read access the counts in §5 collapse to four queries:

   ```sql
   -- per-worldspace reference mass
   SELECT worldspace_id, COUNT(*) FROM "references" GROUP BY worldspace_id ORDER BY 2 DESC;
   -- the route's cells
   SELECT c.id,c.editor_id,c.interior_name,c.grid_x,c.grid_y,c.worldspace_id,COUNT(r.id)
   FROM cells c LEFT JOIN "references" r ON r.cell_id=c.id
   WHERE c.id IN (0x152C3,0x56C1B,0x237BD,0x2D4E0,0x8F82,0x69869) GROUP BY c.id;
   -- the route's meshes
   SELECT COUNT(DISTINCT s.model_path) FROM "references" r JOIN statics s ON s.id=r.base_form_id
   WHERE r.cell_id IN (0x152C3,0x56C1B,0x237BD,0x8F82,0x69869);
   -- required textures come from the closure report, not the DB
   ```

2. **Texture counts are inferred** (§5). The exact figures exist only as
   `asset-closure.exe` output, which needs the converter run.
3. **`NiTriStrips` reachability on this content is unknown.** It needs `nif-audit` over
   `vfs\meshes`, which needs the converter. The code fact — no `NiTriStripsData` parser — is
   verified; the data fact is not.
4. **The `XTEL` byte layout is taken from UESP** ("Skyrim Mod:Mod File Format/REFR", `XTEL =
   Teleport`: destination `FormID`, position, rotation — <https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/REFR>).
   It is corroborated by measurement: every route door's `XTEL` is ≥28 bytes and decodes to a
   *reciprocal* partner (`000152CF` ↔ `00015D48` arrive within ~500 units of each other's door), which
   a wrong offset would not produce.
5. **The lift doors' trigger semantics are inferred.** `MinUse` in the base `EDID`
   (`DweFacadeLiftLeverLoadDown01_MinUse`) is read as "activate on proximity"; no script was
   decompiled to confirm.
6. **Whether `plan_cells` should keep the exterior resident while inside an interior** is a design
   choice with a measurable cost (Tamriel at radius 2 is 25 cells) and a measurable benefit (the
   return crossing is also seamless). It is the lead's call, not a research finding.

## 8. Recommendation

**Build Blackreach, and stage it in two movements.**

- *Movement A* (smallest thing that proves the mechanism): Tamriel grid (19,18) → Alftand01 →
  Alftand02. Two crossings, both exterior→interior, both through persistent-cell doors the existing
  pipeline already indexes correctly (§2.2).
- *Movement B* (the ask): Alftand02 → AlftandWorld → Blackreach. One crossing into a child
  worldspace and one into the destination worldspace, arriving in the open Blackreach cavern.

Do steps **1, 4, 5, 6, 7, 8** (converter: `XTEL` + the wider base-object set; engine: active cell,
interior request/retire, rebase guard, transition system). Step 2 (the `XTEL` remap) and step 3
(version bump) should ride along because they are two lines each and step 2 becomes a real bug the
moment a non-master plugin contributes a door. Defer step 9's second decision (keeping the exterior
resident) until movement A renders.

Markarth is the better *city* demo — its Tamriel→MarkarthWorld crossing is the only one in the game
that switches between two **exterior** worldspaces, which is a distinct and interesting case — but
it is a worse *first* demo: the crossing happens in the middle of dense urban geometry, the
destination is a chain of interiors rather than one vista, and it needs the whole Markarth city
worldspace to look finished before the door-crossing mechanism is even testable. Pick it up after
Blackreach, as the second scenario, once step 8 exists.

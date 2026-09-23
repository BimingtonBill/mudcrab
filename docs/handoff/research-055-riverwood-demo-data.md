# research-055 — Riverwood demo data

**Task:** `tasks/deepseek/research-055-riverwood-demo-data.md`. **Deliverable:**
`docs/design/riverwood-demo.md`, the tool `tools/research/riverwood_demo.py`.

## What is verified

Read-only, from `C:/Modding/SkyrimConverted/skyrim_world.db` (converter schema 17 / DB schema 5),
`Skyrim.esm`'s `CELL`/`LAND` bytes and the converted glbs.

- **Where it is.** Riverwood is Tamriel (60) grids x 3..6, y -13..-10 — the cells are
  `Riverwood`, `Riverwood01`..`Riverwood06`, `RiverwoodEdge01`, `RiverwoodBridge`. Centre
  ≈ (22,100, -46,130). Every exterior load door lives in Tamriel's persistent cell `00000D74`,
  and the engine still loads them because exterior references are selected from the
  `exterior_spatial` r-tree by position (`crates/engine/src/streaming.rs:628-647`), not by
  `cell_id`. **No engine change is needed for that.**
- **Six house interiors + Embershard Mine**, with each door's `ref_id`, its interior cell, and both
  `XTEL` arrivals (outward arrival in Tamriel, inward arrival cell-local), the light-reference
  count, and the base record (`00029CB0` "FarmhouseLDoor01" ×7 — none auto-load; the two Embershard
  doors are the auto-load kind). All six house interiors share lighting template `000A1196`.
- **A route** of ten legs, 8,096 units (≈116 m), verified dry: the terrain was sampled every 100
  units along every leg, 0 of 77 steps below the river's -250 water plane. `DemoStart` values given
  (`[19600, -46650, -153]`, yaw `1.3360`). The mill is on the far bank with no dry approach, so the
  route ends on the bank looking at the wheel (1,763 units away).
- **The gaps, with counts** (1,950 references in the 5×5 stream box): no NPCs (14 refs), no grass
  (33 `GRAS` records, 0 placed), 189 static trees with bones but no animation, the mill wheel's
  `Idle` clip exported but unplayed, four FX models converted to **empty glb scenes**
  (`FXMistStreet01`, `FXSmokeRiverwoodSmith01`, `FXMotesForest01`, `FXMistMillWheel01`), the river
  water type `000E717C` with no flow normal path, and only 2 `LIGH` references in the whole village.
- **A `material_object` contract hazard:** of the 814 references carrying a `DNAM` max angle, only
  70 carry a real MATO; 744 have **0, not NULL**, although the column is documented "NULL when the
  static has none". An engine snow filter keyed on the column must test `> 0`, or gap 3 will
  snow-plaster fences, ivy, firewood and road chunks across the village. The house shells themselves
  are the plain bases (no MATO), so Riverwood's roofs stay bare.
- **No model in the village fails validation** except markers and FX cards (79 references), and
  every one of the 182 distinct models referenced in the village box has a converted glb.

## What is not verified

- Nothing was rendered: the camera poses in `docs/design/riverwood-demo.md` §6 are proposals, to be
  refined by rendering exactly as research-018 refined the Alftand poses.
- Walkability beyond the route: the terrain under the route is checked, but not the colliders
  (walkways, fences, the smithy's rotated footprint possibly blocking the wp10 sight line).
- Anything outside the village box: the Guardian Stones' bluff and the Bleak Falls approach were
  placed by position only, no terrain sampling.

## Next step

Lead: pick the beats and add the `"riverwood"` arm to `DemoStart::named`
(`crates/engine/src/config.rs:330-347`), then either run `--start-position 19600 -46650 -153
--start-yaw 1.3360` for a look with no build, or write the shots file (§6) and render the ten poses.
The cheapest visible win after that is playing the exported `Idle` clip on non-door references: the
mill wheel is the village's only one, and it is the finale.

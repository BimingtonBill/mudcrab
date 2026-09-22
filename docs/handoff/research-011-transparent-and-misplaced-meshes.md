# research-011 handoff - transparent and misplaced meshes

Result: `docs/research/transparent-and-misplaced-meshes.md`. Research only, no engine or
converter code was touched.

## Verified

* **The dominant "transparent" defect is the converter, not the engine.** A shape with no
  `NiAlphaProperty` but the `SLSF1_VERTEX_ALPHA` shader flag is published as glTF `BLEND`
  (`crates/converter/src/material.rs:990`), and the per-vertex alpha it was meant to read is
  not exported, so the diffuse texture's alpha channel becomes the opacity. 2,632 materials
  of 60,414 in the converted set; 730 placed references on the tour's 32 cells, 259 of them
  visibly see-through because their texture's alpha averages below 200. Reproduce with
  `tools/research/blend_cause_census.py` and
  `tools/research/transparent_route_models.py 1 --summary`.
* The reader used for the NIF side is validated against the converter's own output: for
  `ImpExtIceBldgICorner01:16` the property is `flags=0x12EC`, test `>` threshold 93, and the
  GLB publishes `MASK` with cutoff 0.3647 = 93/255.
* **Additive and multiplicative blends are collapsed to straight alpha blending**:
  4,335 + 435 of 36,129 `NiAlphaProperty` blocks (23.9% of the blend-enabled ones), 706
  placed references on the route. The visible half is Dwemer lantern, sconce, chandelier and
  torch glow shapes, which render as grey veils instead of glows.
* **Editor markers are drawn**: `EditorMarker` shapes inside otherwise legitimate models
  (`DwePtnDoor01`, `DweLever02`, `SpikeTrapDwemer`, every `Effects\FX*` NIF), because the
  engine only filters whole model families and file names
  (`crates/engine/src/streaming.rs:1610`).
* **The misplacement lead in the brief is confirmed, not refuted.** Pre-streamed destinations
  are drawn in the active cell's space (`crates/engine/src/streaming.rs:769`,
  `crates/engine/src/streaming.rs:1568`, `crates/engine/src/transition.rs:85`), and the
  database shows seven unrelated interiors sharing the AlftandWorld arrival's coordinates,
  the nearest 85 units away. `impl-009-door-portals` owns the fix.
* Refuted: LOD meshes beside full meshes (the engine never reads the `lod` table), dropped
  NIF node transforms, and rotation/scale conversion (the tour's door positions match the
  database's creation coordinates through `cell_translation` / `render_position` for two
  doors with different origin cells).
* The long thin brown bars in `01-door.png` / `02-door.png` are the Dwemer pipe kit and mine
  woodwork from the source data (~200 pipe pieces in Alftand01), not a defect.

## Unknown / unverified

* Which reference produces the flat white shapes in `03-survey-2.png`. The database cannot
  say which cells are resident in a frame; the streaming plan would have to be simulated, and
  the tour log has no cell census. Class C is the likely mechanism; the identification is
  inferred, not verified.
* Whether Skyrim draws class-A shapes opaque - asserted from the format (blend state lives in
  the `NiAlphaProperty`) but not confirmed against a running game.
* `mask_other` (652 materials) is the one class whose measurement is heuristic: the NIF block
  reader finds the alpha property by looking for its block index inside the shape block
  rather than by parsing the whole shape layout. Treat it as an upper bound.
* The cascade in section 2.D (a misplaced door pre-streaming its own destination, because
  `plan_door_prestream` measures a door's *render* transform) is inferred from the code; it
  was not observed in a run.

## Next step

Implement class A in the converter (`crates/converter/src/material.rs:975`, plus the test at
`crates/converter/src/material.rs:1466`) and re-run the demo tour; that alone should turn the
ghost ice, rock and floors solid. Class B needs an engine-side material kind for additive and
multiply blending; class C reuses the existing non-rendering exclusion path. Each fix should
be accepted with a fresh tour and a look at `00-survey-1.png`, which is the clearest
before/after frame.

# Upstream look vs Skyrim: first reference sheet (2026-09-24)

**What was rendered:** branch `phase2/render-check` (upstream `main` + the engine-side Phase 2 fixes:
REFR rotation, empty-model bounds, shared schema constant, scripts, terrain samplers and weights,
terrain seam weld, sun shadow cascades, plus the local `--shots` tool; not the vertex-alpha branch)
on `C:\Modding\SkyrimConverted-upstream`, a fresh schema-14 conversion made overnight with
upstream's converter + the pruned-texture fix (`complete: true`, 242,969 converted, 0 skipped,
511 textures pruned, integration report passed). References: UESP screenshots
(`local/reference/uesp`, `local/reference/riverwood`). Sheets (not in git):
`C:\Modding\wkj-tools\shots\riverwood-sheet\_sheet.jpg` and `...\uesp-sheet\_sheet.jpg`.
Interior poses are skipped by the local shots tool.

## What works
- Buildings, bridges, rocks, terrain and water geometry stream and place correctly around
  Riverwood, Alftand and Blackreach; rotations look right after the REFR fix.
- Terrain layers tile instead of smearing (Repeat sampler) and transitions look smooth.
- Rocks and buildings cast sun shadows over the streamed area (cascade fix).

## What is wrong, and which branch fixes it
| Symptom | Where | Cause | Fix |
|---|---|---|---|
| No trees, ferns, shrubs; the Guardian Stones themselves are missing | everywhere outdoors | TREE/FLOR/ACTI bases have no model in `statics` on upstream | `phase2/renderable-base-types` (needs a schema-15 conversion) |
| Ice and rock in Alftand are see-through / smeared | Alftand exteriors | vertex alpha published as BLEND | `phase2/material-publication` (schema 15) |
| Stone and thatch look wet and grey | Riverwood, Guardian Stones | glossiness read as a percentage | `phase2/material-publication` |
| Flat dark-grey sky, no fog | everywhere | upstream has no sky or fog | not a branch yet (lighting/atmosphere, issue #24) |
| Dim, cold, flat light; no warm tones | everywhere | ambient/sun calibration, no per-space lighting | lighting proposal (issue #24) |
| Blackreach has no glow | Blackreach | no emissive/HDR/bloom upstream | lighting proposal (issue #24) |
| Water pale and flat | Riverwood river, lake | water material/lighting | to investigate |
| Horizon ends at the stream radius | aerial poses | no distant LOD | `phase2/lod-*` (steps 1-3 built) |

## Next
The two schema-15 branches (base types, materials) are the biggest visual wins and need a
schema-15 asset set. Migrating in place writes a full staging copy (about 70 GB), and C: had
about 31 GB free on the morning of 2026-09-24, so that needs disk space first. The vertex-alpha
question (are trees dots on upstream?) can only be answered once trees are catalogued.

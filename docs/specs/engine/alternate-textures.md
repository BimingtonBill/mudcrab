# Alternate textures of base models

A base record (a static, tree, door, container, ...) can list **alternate textures**: for
named shapes of its model, a texture set to draw instead of the one the mesh was authored
with. Skyrim uses this to make one mesh match many surroundings. `DirtCliffs01FieldGrass01`
and `DirtCliffs01Tundra01` share `DirtCliffs01.nif`, whose grass shape is green; each
record swaps that shape to the grass texture its terrain uses.

About one static in ten has a list (971 of 9720 in Skyrim.esm; Update.esm adds 156,
Dawnguard 122, Dragonborn 100 more). Without it the object keeps the mesh's green and
no longer matches the terrain around it.

## Data

| Stage | What |
| :-- | :-- |
| Record | `MODS` on the base record: a count, then per entry a name length, the shape name, a texture set (`TXST`) FormID and a shape index. Armour's world model uses `MO2S` (male) or, when only a female model exists, `MO4S`. `MO3S`, `MO5S` and `DMDS` (destruction stages) have the same layout and are remapped but not used yet. |
| Converter | The load-order remap handles the list on every record type (strict for `GRAS`, optional elsewhere: a malformed list is dropped with a warning). Table `model_alternate_textures(base_id, shape_name, shape_index, texture_set_id)`, one row per entry, replaced as a whole when the record is exported. An entry with no texture set or no name is skipped. The table is optional, like the room graph tables: no schema bump, and a database without it draws every model as authored. |
| Engine | `AssetCatalog` joins each row to its texture set's converted diffuse and normal paths. The streamer puts the list on each reference of that base. When the reference's scene is spawned, every shape whose glTF mesh name matches (case-insensitive) gets a copy of its material with the replacement maps. One built material is shared by all references of a base. |

## What is replaced

The diffuse and normal maps, the two inputs the converted materials sample. The shape keeps
its shader flags, alpha mode and every other map; a texture set that lacks a slot leaves the
shape's own map in it. Shapes match by name: the converter keeps each NIF shape's name as
the glTF mesh name, and 1300 of the 1306 entries in Skyrim.esm name a shape present in the
converted model (the other six name shapes the converter dropped). The stored shape index
is not needed to find a shape.

## Not covered

- Per-reference overrides (`XMSP` material swaps, `XTNM`, scripts): a separate layer on top.
- Records other than world models' lists (`MO3S`, `MO5S`, `DMDS`): remapped, not stored.
- Two shapes of one model with the same name: both take the first entry for that name.
- Maps beyond diffuse and normal (glow, height, environment, specular masks): they keep the
  shape's own.
- Baked LOD objects and clusters draw their own textures.

## Checking it

Read every static with a list from the record dump and check each named shape against the
converted model's mesh names (the 1300 of 1306 above). The visible check is a Skyrim
reference pose that looks at `DirtCliffs01FieldGrass01`: the slab is golden like the grass
around it, not green.

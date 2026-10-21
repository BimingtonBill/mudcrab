# Data set extensions

A converted data set is a **core** plus **extensions**.

- The **core** is every table, column, file and manifest value `main`'s converter writes (schema numbers: converter 28,
  world database 9 at the time of writing; they are `shared`'s constants, never this tree's own). The `dev` converter
  writes the core byte for byte as `main` does.
- An **extension** is a named, versioned set of derived output `main` does not write: whole database tables (with their
  indexes), whole files, or whole glTF extension names. Nothing else: an extension never adds a column to a core table,
  never changes a core table's rows, and never adds keys inside a core glTF extension such as `OPEN_SKYRIM_material`.

**Moving an extension upstream is a core PR plus a version bump**: the item becomes core (schema change, numbers per
`main`'s rules), the extension is removed from `converter::extensions::specs` and from the manifest.

Code: `shared::extensions` (types, hash, `DataExtensions` for readers) and `converter::extensions` (the specs and the
manifest entries). Design: `local/research/op6-one-data-set-design-2026-10-16.md` (local note).

## Manifest

`conversion-manifest.json` gains one key, `extensions`, omitted when the list is empty (so a data set without extensions
has `main`'s manifest):

```json
"extensions": [
  { "id": "mudcrab.space_lighting", "version": 1,
    "owns": { "tables": ["space_lighting"], "indexes": [], "files": [], "gltf_extensions": [] },
    "sha256": "<64 hex digits>", "built_from": "<core build identity>",
    "stats": { "space_lighting": 2 } }
]
```

| Key | Meaning |
|---|---|
| `id` | Stable, dotted, never reused. |
| `version` | Format version of this extension, independent of the core numbers; bumped on any format change. |
| `owns` | Everything the extension wrote, each list sorted: `tables` and `indexes` in `skyrim_world.db`, `files` (paths relative to the data set root; an entry ending in `/` covers every file below it), `gltf_extensions` (names added to meshes). Two extensions never own the same item and no core item is owned (unit tests in `converter::extensions`). |
| `sha256` | Content hash of what `owns` lists (below). |
| `built_from` | The core build identity the extension was derived from: `lod_build.build_identity`. The empty string when the database has no `lod_build` row (a conversion without distant LOD). A runtime treats an extension whose `built_from` differs from the data set's current identity as stale. |
| `stats` | Row counts, one per owned table keyed by the table name; `mudcrab.lod_inventory` also counts `lod_block_terrain` and `lod_block_objects`. They replace the distant-LOD counters the integration report used to carry. |

An extension is listed when the converter wrote it, even with zero rows. If a table it owns is missing it is not listed.
The list is recomputed wherever the converter publishes a manifest after changing the database or meshes: a full
conversion, a metadata rebuild and an asset repair.

The current upstream core owns `door_links` and `emittance`. New manifests do not declare
`mudcrab.door_links`; refreshing an old manifest retires that declaration. A finalizer cannot claim
core tables or the retired door extension identity. The engine discovers complete core door tables
using the same schema checks as its ordinary reference reader. A legacy door extension declaration
still requires its supported version and matching core identity; a stale or unsupported declaration
cannot bypass validation by being treated as undeclared core data.

## Extensions written today

| id | version | Owns | Replaces (before 2026-10-16) |
|---|---|---|---|
| `mudcrab.lod_inventory` | 1 | tables `lod_block`, `lod_grid`, `lod_tree_instance`, `lod_tree_type`; index `idx_lod_tree_block`; files below `meshes/terrain/` | integration-report counters `lod_*` |
| `mudcrab.space_lighting` | 1 | table `space_lighting` (`XCLL`, `LGTM`) | unlisted table |
| `mudcrab.directional_snow` | 1 | tables `matos` (`MATO`) and `static_materials(id, material_object, material_max_angle)`, one row per static whose `DNAM` names a directional material | columns `statics.material_object` and `statics.material_max_angle` |
| `mudcrab.material` | 1 | glTF extensions `MUDCRAB_material` and `OPEN_SKYRIM_material_animation` | extra keys inside `OPEN_SKYRIM_material` |

`mudcrab.material` is listed only when at least one output mesh lists one of its glTF extension names
in `extensionsUsed`. A data set whose meshes use neither name has no material extension entry.

The integration report checks, only when `lod_block` or `lod_tree_type` exist, that every distant LOD mesh they name was
converted; a miss is an entry in `issues` and fails `passed`. The report's own fields are `main`'s.

## Content hash (`mudcrab-ext-hash-v1`)

SHA-256 over lines each ending in a line feed: the algorithm name, the id, the version, then every owned table (sorted
rows, then the column declarations), index (stored SQL), file (SHA-256 of the bytes) and glTF extension value (per mesh,
a hash of the canonical value) in a fixed order. Numbers and strings are hex-encoded so the Rust and Python
implementations agree without a shared float printer. The exact definition is the reference implementation
`tools/extension_hash.py`; `shared::extensions::ExtensionHasher` implements it, and
`crates/converter/tests/extensions_hash.rs` converts a dummy-content fixture and runs the Python script over the output
(`python tools/extension_hash.py <data set>` does the same check by hand: exit 0 means every hash matches).

`converter refresh-extensions <data set>` repairs content hashes and statistics under the exclusive asset lock.
It preserves recorded `built_from` identities and refuses stale, unsupported, differently owned, missing or
unlisted known payloads rather than claiming they were regenerated. Duplicate extension IDs are errors.
Unknown extension objects retain their JSON fields and values. A no-op leaves the manifest bytes unchanged.
Changed manifests are staged and renamed only after validation; refreshing hashes does not rebuild payloads.

## What the engine does with them

`engine::extensions` reads the list once when the data set is opened (`DataSetExtensions`, with the notices as
`ExtensionNotices`) and asks about each feature that reads extension data. A data set with no `extensions` key (a `main`
conversion) is valid: every feature below reports "missing".

| Feature | Extension (versions read) | Reads | Without a usable extension |
|---|---|---|---|
| Directional snow | `mudcrab.directional_snow` (1) | `matos`, `static_materials` | no snow catalogue, no snow system |
| Per-space lighting | `mudcrab.space_lighting` (1) | `space_lighting` | no catalogue; the engine's own atmosphere |
| Load-door links, portal graph | `mudcrab.door_links` (1) | `door_links` | no join, no graph; every reference reads as a non-door |
| Extra material data | `mudcrab.material` (1) | glTF `MUDCRAB_material`, `OPEN_SKYRIM_material_animation` | no extension blend/falloff/animation data read; no animation plugin or registry; core material handling remains active |

- Present (listed, a version the engine reads, `built_from` equal to the data set's `lod_build` identity): the reader runs.
  A listed extension whose table is missing or unreadable is an **error** that stops the run (a corrupt data set).
- Missing, stale or an unsupported version: nothing of the feature is built, the log gets one `warn!` line per feature
  (the `ExtensionStatus` text: what is wrong and to convert again), and an `ExtensionNotice` is kept for the settings
  menu to grey the entry with the same reason.
- A data set written by an older dev build under the retired private numbering (converter 102 401, world database
  100 701; any number from `shared::RETIRED_DEV_SCHEMA_FLOOR` up) is rejected with "converted by an older Mudcrab dev
  build; convert it again with this build".
- `world-inspect` prints the list (id, version, feature, status) under `extensions` when the manifest has one.

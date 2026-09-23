# Tree LOD file layouts (.lst, .btt)

**Status:** DeepSeek research-517, 2026-09-23, decoded from the shipped files and validated against the converted database (positions bit-exact for all resolvable Tamriel records; type index to TREE base 1:1). Scripts were kept in the session scratchpad, not the repo. Side finding: Dragonborn references from 0x0201B1BE upward are missing from the world database, so the ESM export may be truncating Dragonborn.esm (to investigate).

# Skyrim SE tree LOD: `.lst` and `.btt` byte layout (measured)

Scope of evidence: all 9 `.lst` files and all 386 `.btt` files under
`C:\Modding\SkyrimConverted\vfs\meshes\terrain\<worldspace>\trees\`, cross-checked against
`skyrim_world.db` (tables `references`, `records`, `statics`, `worldspaces`).
Scripts: `01_survey.py` … `27_lst_final.py` in this folder; raw output in `*_out.txt`.

## 1. `<worldspace>.lst` — billboard type table

Fixed 32-byte records, little-endian. **All 9 files satisfy `size == 4 + 32*count` exactly.**

| offset | type | meaning | evidence |
|---|---|---|---|
| 0 | u32 | `count` (Tamriel 34, Solstheim 36, …) | size check on all 9 files |
| 4 + 32*i | u32 | `index` — always equal to `i` | 120/120 entries across all files |
| +4  | f32 | `size_x` — billboard width in Creation units at scale 1 | 333.5 … 1199.8 |
| +8  | f32 | `size_y` — billboard height in Creation units at scale 1 | 741.7 … 2556.2 |
| +12 | f32 | `u0` | 120/120 rects land on non-empty atlas alpha |
| +16 | f32 | `v0` (0 = top row of the DDS, verified visually) | ditto |
| +20 | f32 | `u1` (always > u0) | ditto |
| +24 | f32 | `v1` (always > v0) | ditto |
| +28 | f32 | unused, 0.0 in all 120 entries | 0/120 non-zero |

Notes
* UVs can overshoot by up to 0.001 (u0 min −0.00046, v1 max 1.00098). Clamp or use
  clamp-to-edge sampling.
* The `index` field is not a foreign key; it is the entry's ordinal, and it is the value
  used as the group `type index` in `.btt`.
* Atlas `<worldspace>treelod.dds` is 1024×1024 RGBA (DXT5) for every worldspace measured.
  `v` maps straight to the image row (`row = v * height`), confirmed by cropping the rects
  and looking at them (upright trees, trunk on the bottom edge) — `22_sheet_tamriel.png`.
* Every rect was checked with Pillow: 120/120 have non-zero alpha inside
  (30–51 % of pixels non-transparent, max RGB 83–255) — `22_out.txt`.

## 2. `<worldspace>.4.<x>.<y>.btt` — instanced billboards

`x`,`y` are the block's south-west cell (multiples of 4, 1 cell = 4096 units); every record
position lies inside `[x*4096, (x+4)*4096) × [y*4096, (y+4)*4096)`.

| offset | type | meaning | evidence |
|---|---|---|---|
| 0 | u32 | `group_count` | walking the groups consumes the file exactly for 380/386 files |
| per group | u32 | `type_index` — index into the worldspace's `.lst` | in range `[0, .lst count)` for every group of every file |
| | u32 | `count` | record walk lands on the next group header / EOF |
| | `count` × 32 B | records (below) | |

### Record (32 bytes)

| offset | type | meaning | evidence |
|---|---|---|---|
| 0  | f32 | `x` world position, Creation units | bit-exact match to `references.pos_x` |
| 4  | f32 | `y` | `references.pos_y` |
| 8  | f32 | `z` | `references.pos_z` |
| 12 | f32 | `rotation` — billboard yaw, radians, `[0, 2π)` | observed range [0.00018, 6.28319]; 12-bin histogram of `(rotation − ref.rot_z) mod 2π` is flat (430–473 per bin of ~450 expected) ⇒ independent of the reference's rotation |
| 16 | f32 | `scale` — equals the reference's scale | 41633/41756 resolved records match `references.scale` exactly |
| 20 | u32 | `form_id` — the reference's FormID | exact hit in `references.id` |
| 24 | u32 | unknown A | equals the group's ordinal within the file (41536/41756); `0x????FBAC` in 8 DLC files (§4) |
| 28 | u32 | unknown B | 0 in 41 % of records, otherwise a small value; no correlation with base-object bounds or scale — **do not rely on it** |

Validated against ground truth (`08_validate.py`, `25_errbyws.py`):

| worldspace | records | unresolved | position err < 0.5 u | 0.5–4 u | > 4 u |
|---|---|---|---|---|---|
| tamriel | 40839 | 88 (DB gap) | **40699** | 0 | 52 (stale LOD, see below) |
| sovngarde | 519 | 0 | 519 | 0 | 0 |
| skuldafnworld | 224 | 0 | 224 | 0 | 0 |
| deepwoodredoubtworld | 42 | 0 | 42 | 0 | 0 |
| dlc1hunterhqworld | 146 | 0 | 12 | 131 | 3 |
| dlc01falmervalley | 74 | 0 | 0 | 43 | 31 |
| dlc2solstheimworld | 1489 | 1489 | — | — | — |
| total | 43333 | 1577 | 41496 | 174 | 86 |

* Tamriel, Sovngarde, Skuldafn, DeepwoodRedoubt: **100 % of resolvable records are
  bit-exact**. Scale: exact. Type index: for Tamriel, 40751/40751 resolved records map to
  the *same single* TREE base form for their `type_index` (**100 %**), i.e. `type_index` is a
  one-to-one key onto a tree model (`23_out.txt` has the full 34-row table, e.g.
  type 24 → `TreePineForest01`, type 29 → `TreeAspen05`).
* The 52 Tamriel records >4 u off are all refs owned by later plugins (HearthFires.esm,
  Dawnguard.esm): the shipped `.btt` predates those edits and still holds the original
  Skyrim.esm positions (max error 2955 u). Same cause for the Dawnguard-world mismatches,
  which additionally store all five floats rounded to **f16 precision** (e.g. 0.790039
  instead of 0.79).
* 1577 records (all Solstheim) point at Dragonborn refs with FormID ≥ 0x0201B1BE. The
  database has no rows at all above 0x0201B1BE (`records`, `formid_map`, `references` are
  all empty there), so those could not be cross-checked — this looks like a truncated
  Dragonborn extraction in the converter, not a format problem.

## 3. Counts

| worldspace | `.lst` types | `.btt` files | groups | records | records/block |
|---|---|---|---|---|---|
| tamriel | 34 | 329 | 2524 | 40839 | 1 … 625 |
| dlc2solstheimworld | 36 | 24 | 202 | 1489 | 2 … 226 |
| sovngarde | 15 | 12 | 77 | 519 | 5 … 137 |
| skuldafnworld | 9 | 8 | 36 | 224 | 1 … 82 |
| dlc1hunterhqworld | 13 | 4 | 33 | 146 | 1 … 58 |
| dlc01falmervalley | 8 | 5 | 21 | 74 | 1 … 40 |
| deepwoodredoubtworld | 5 | 4 | 19 | 42 | 4 … 19 |
| dlc01soulcairn / dlc2apocryphaworld | 0 | 0 | — | — | — |
| **total** | 120 entries | 386 | 2912 | 43333 | |

The `.btt` is a *subset* of the tree references: Tamriel has 112504 TREE refs inside the
329 blocks' footprints but only 40839 billboards, so the converter must treat the `.btt`
as the authority for what is drawn, not derive it from the ESM.

## 4. The one anomaly: 6 `dlc2solstheimworld` files

`4.12.4`, `4.12.8`, `4.12.12`, `4.16.4`, `4.16.8`, `4.16.12` have 192–2112 bytes **after**
the declared groups, and the file size is not consistent with "declared groups + whole
records" (e.g. 16.4 leaves 440 bytes = 13.75 records). Contents:
* further 32-byte records in the same layout (positions inside the block, FormIDs in
  Dragonborn's range 0x0202xxxx–0x0203xxxx);
* occasional `u32 type, u32 count` pairs that chain into valid groups running to EOF;
* some of them duplicate trees already listed in the declared groups with **identical
  position, scale and FormID but a different rotation**;
* unknown A is `0x????FBAC` throughout, constant per run, where `????` differs between the
  declared groups and the trailing run (e.g. 12.12: 0x2F83FBAC then 0x2F40FBAC).

The trailing bytes are real data (not garbage) but the framing is not determined by the
evidence available: the run sometimes starts mid-record (12.12 starts with a 16-byte tail),
and in 16.12 a 304-byte run of the same kind also appears *between* declared groups.
Because the database has no refs in that FormID range, ground truth could not settle it.
Recommendation for the converter: parse the declared groups, keep the tail as opaque bytes,
and log it; if the extra records matter later, re-extract Dragonborn.esm and re-check.

## 5. Rust layout for the converter

```rust
pub const CELL_SIZE: f32 = 4096.0;

/// `<worldspace>.lst` entry — one billboard type.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TreeLodType {
    pub index: u32,       // == array position; the .btt group type index
    pub size_x: f32,      // Creation units at scale 1
    pub size_y: f32,
    pub uv_min: [f32; 2], // (u0, v0), v measured from the top of the atlas
    pub uv_max: [f32; 2], // (u1, v1)
    pub unused: f32,      // always 0.0
} // 32 bytes on disk

/// one 32-byte instance in a `.btt` block
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TreeLodInstance {
    pub position: [f32; 3], // world units; z is the tree's base
    pub rotation: f32,      // yaw in radians, [0, 2*PI)
    pub scale: f32,         // multiplies size_x / size_y
    pub form_id: u32,       // reference FormID (same ids as world_database.references.id)
    pub unknown_a: u32,     // group ordinal (0x????FBAC in 8 DLC files)
    pub unknown_b: u32,     // unused
}

pub struct TreeLodGroup { pub type_index: u32, pub instances: Vec<TreeLodInstance> }

pub struct TreeLodBlock {
    pub grid_x: i32,             // from the file name
    pub grid_y: i32,
    pub groups: Vec<TreeLodGroup>,
    pub trailing: Vec<u8>,       // non-empty only for the six files in §4
}

pub fn parse_lst(bytes: &[u8]) -> Result<Vec<TreeLodType>, Error> {
    let count = read_u32(bytes, 0)? as usize;
    if bytes.len() != 4 + 32 * count { return Err(Error::BadSize); }
    Ok((0..count).map(|i| {
        let b = &bytes[4 + 32 * i..][..32];
        TreeLodType {
            index:       u32::from_le_bytes(b[0..4].try_into().unwrap()),
            size_x:      f32::from_le_bytes(b[4..8].try_into().unwrap()),
            size_y:      f32::from_le_bytes(b[8..12].try_into().unwrap()),
            uv_min:      [f32::from_le_bytes(b[12..16].try_into().unwrap()),
                          f32::from_le_bytes(b[16..20].try_into().unwrap())],
            uv_max:      [f32::from_le_bytes(b[20..24].try_into().unwrap()),
                          f32::from_le_bytes(b[24..28].try_into().unwrap())],
            unused:      f32::from_le_bytes(b[28..32].try_into().unwrap()),
        }
    }).collect())
}

pub fn parse_btt(bytes: &[u8]) -> Result<TreeLodBlock, Error> {
    let n_groups = read_u32(bytes, 0)? as usize;
    let mut off = 4usize;
    let mut groups = Vec::with_capacity(n_groups);
    for _ in 0..n_groups {
        let type_index = read_u32(bytes, off)?;
        let count      = read_u32(bytes, off + 4)? as usize;
        off += 8;
        let end = off.checked_add(count * 32).ok_or(Error::Truncated)?;
        if end > bytes.len() { return Err(Error::Truncated); }
        let instances = bytes[off..end].chunks_exact(32).map(|c| TreeLodInstance {
            position: [f32::from_le_bytes(c[0..4].try_into().unwrap()),
                       f32::from_le_bytes(c[4..8].try_into().unwrap()),
                       f32::from_le_bytes(c[8..12].try_into().unwrap())],
            rotation: f32::from_le_bytes(c[12..16].try_into().unwrap()),
            scale:    f32::from_le_bytes(c[16..20].try_into().unwrap()),
            form_id:  u32::from_le_bytes(c[20..24].try_into().unwrap()),
            unknown_a:u32::from_le_bytes(c[24..28].try_into().unwrap()),
            unknown_b:u32::from_le_bytes(c[28..32].try_into().unwrap()),
        }).collect();
        off = end;
        groups.push(TreeLodGroup { type_index, instances });
    }
    Ok(TreeLodBlock { groups, trailing: bytes[off..].to_vec(), ..Default::default() })
}
```

Rendering the instances (inferred, not byte-verified):
each record is one camera-independent quad of world size
`(size_x * scale, size_y * scale)` standing upright on the terrain:
base edge at `z`, top at `z + size_y * scale`, centred on `(x, y)`, yawed by `rotation`
about +Z, textured from `uv_min..uv_max` of `<worldspace>treelod.dds` with alpha cut-out.
The billboard art sits at the bottom of its atlas rect (verified visually), and the record's
`z` equals the reference's `z`, so the quad grows upward from the tree's base.

## 6. Repro: key queries and scripts

```
sqlite3 (read-only): file:C:/Modding/SkyrimConverted/skyrim_world.db?mode=ro

-- ref ids and their base form
SELECT r.id, r.base_form_id, r.pos_x, r.pos_y, r.pos_z, r.rot_x, r.rot_y, r.rot_z, r.scale
FROM "references" r
WHERE r.base_form_id IN (SELECT form_id FROM records WHERE record_type='TREE');

-- tree model names for the type table
SELECT id, editor_id, model_path FROM statics WHERE id = <base_form_id>;
```

* `08_validate.py` — record-by-record comparison of one block (prints DB ref next to record).
* `09_validate_full.py` — all 386 files, hit rate + position errors + type→base map.
* `23_final.py` — per-worldspace counts, type→base table, consistency rate (`23_out.txt`).
* `24_fields.py` — scale / word6 / word7 statistics (`24_out.txt`).
* `25_errbyws.py`, `26_outliers.py` — error breakdown per worldspace and per file.
* `22_atlas.py` — Pillow check of all `.lst` rects + `22_sheet_tamriel.png` contact sheet.

## 7. Not resolved

1. The trailing runs in the six Solstheim files (§4) — framing ambiguous, ~199 records
   (~13 % of Solstheim's billboards) are in there.
2. `unknown_a` / `unknown_b` semantics. `unknown_a` = group ordinal in most files, but its
   value in the DLC-flavoured records (`0x????FBAC`) is unexplained.
3. Whether the game renders both copies for trees that appear in both a declared group and
   a trailing run.
4. The exact purpose of the `.lst` `unused` float (0 everywhere) and whether `.lst` order is
   meaningful beyond keys (it is the `.btt` type index, so yes, positionally at least).

# Dragonborn "missing references": an ID-space mismatch, plus two real converter gaps

**Status:** DeepSeek research-531, 2026-09-23 (Python over the game files and the converted database, read-only). The worker could not run git, so "upstream is affected" rests on reading the code it had. Claude's summary: nothing is missing for Dragonborn; `.btt` tree LOD stores FormIDs with the authoring master index, which must be remapped to the load-order slot (Dragonborn 0x02 -> 0x04). Two real defects found on the way: `plugins.priority` disagrees with the FormID slot for every plugin after the first light plugin, and 274 Skyrim.esm + 4 Update.esm records looked lost; a follow-up (research-533) showed they are all deleted by later official plugins (flag 0x20), so section 5 below is superseded: the merge is correct.

# Report: Dragonborn references and the converted world database

## Headline

**No Dragonborn record is missing from `skyrim_world.db`.** The database contains 100 % of Dragonborn.esm's own records, set-exact per record type. The apparent "missing references above FormID 0x0201B1BE" is an **ID-space mismatch in the code that reads the tree-LOD files**, not a converter defect: the `.btt` files store Dragonborn FormIDs using Dragonborn.esm's *authoring-time intra-file master index* (0x02), while the database — correctly — keys Dragonborn at its *global load-order slot* (0x04).

## 1. Counts per plugin (ESM own records vs DB rows)

Compared as **sets** of `(record type, local FormID)`, mapping each plugin's ESM self-index through its `MAST` list (`09_setcompare.py`, `11_setcompare_all.py`, `25_cc_check.py`):

| Plugin | ESM own | DB | missing | extra | Notes |
|---|---|---|---|---|---|
| Skyrim.esm | 869,686 | 869,415 | **274** | 3 | real gap — see §5 |
| Update.esm | 10,443 | 10,440 | **4** | 1 | real gap — see §5 |
| Dawnguard.esm | 93,439 | 93,439 | **0** | 0 | |
| HearthFires.esm | 16,764 | 16,764 | **0** | 0 | |
| **Dragonborn.esm** | **178,327** | **178,327** | **0** | **0** | |
| ccBGSSSE001-Fish.esm | 5,336 | 5,336 | 0 | 0 | DB slot 0x05 |
| ccBGSSSE025-AdvDSGS.esm | 2,963 | 2,963 | 0 | 0 | DB slot 0x06 |
| _ResourcePack.esl | 373 | 373 | 0 | 0 | 0xFE idx 0 |
| ccBGSSSE037-Curios.esl | 151 | 151 | 0 | 0 | 0xFE idx 1 |
| ccQDRSSE001-SurvivalMode.esl | 508 | 508 | 0 | 0 | 0xFE idx 2 |

Dragonborn by type (all zero missing): REFR 78,731 · CELL 45,187 · LAND 34,505 · INFO 4,353 · DIAL 2,158 · NAVM 1,697 · ACHR 1,308 · STAT 1,085 · ARMO 740 · NPC_ 703 · WEAP 677 … zero missing in **every** type. `references` table: 80,039 ESM REFR+ACHR vs 80,041 DB rows — **0 missing**, 2 extra. The gap is therefore **not** a threshold, not one group/cell/worldspace, and not compressed records: the 34,515 compressed LANDs are all present (34,515/34,515).

## 2. Cause, with byte-level explanation

**The `.btt` distant-tree files are verbatim game data, not converter output.** All 386 `.btt` files in `SkyrimConverted/vfs/meshes/terrain/…` hash-identically (sha256) to the `Skyrim - Meshes1.bsa` members recorded in `conversion-manifest.json` — 386/386. No Rust file in the repository contains the string `btt`, and `AssetKind::Mesh::accepted_extensions` is `["nif","btr","bto","glb"]` at `crates/converter/src/asset_path.rs:29`. The converter never interprets their contents; it copies them.

Reverse-engineered `.btt` layout (validated by exact file consumption): `u32 version`, then repeating blocks of `u32 a, u32 n, n × record`, where a record is 32 bytes — `f32 x, y, z, rot, scale; u32 FORMID; u32 u1; u32 u2==0`. Record `+20` holds the tree's placed-reference FormID. Measured top byte per worldspace:

| Worldspace | authoring plugin | FormID top byte |
|---|---|---|
| tamriel, sovngarde, skuldafnworld, deepwoodredoubtworld | Skyrim.esm (self index 0) | **0x00** |
| dlc01falmervalley, dlc1hunterhqworld | Dawnguard.esm (self index 2) | **0x02** |
| dlc2solstheimworld | Dragonborn.esm (self index 2) | **0x02** |

Dawnguard.esm and Dragonborn.esm both have masters `["Skyrim.esm", "Update.esm"]`, so both author at index 2. For Dawnguard that **coincidentally equals its global slot 2**, so DLC1's tree LOD resolves by luck. For Dragonborn it does **not** equal its global slot 4 — so every DLC2 tree id misses.

**The 0x0201B1BE floor in the evidence is exactly that coincidence boundary.** `0x0201AAFE` is Dawnguard.esm's highest local FormID (`07_probe_ids.py`: `0x0201AAFE` exists in `references`; `0x0201AAFF` does not). A raw `0x02xxxxxx` value with local ≤ 0x1AAFE can accidentally hit a Dawnguard reference; above it, nothing can match — so failures *appear* to begin at a threshold just past 0x1AAFE. (In my decode it is a near-miss, not an exact boundary: 984 of 996 unmatched ids are ≥ 0x0201B1BE; the 12 below are simply ids never assigned in Dawnguard.)

**Decisive confirmation** (`23_validate_trees.py`), on 1,001 Solstheim tree entries I decoded:

- resolve **as-is**: **0**
- resolve after `(id & 0xFFFFFF) | 0x04000000`: **991**
- of those, base object is a Dragonborn **TREE** record: **914**
- coordinates match: `.btt` `(51904, 75968, 7540)` vs reference `(51892, 75979, 7540)`; same for all sampled entries (LOD position is the rounded reference position)

**Where in the converter? Nowhere.** `remap_record_form_ids` (`crates/converter/src/esm/mod.rs:168-207`, closure at `:175-192`) resolves the top byte through the plugin's `MAST` list (`:180-184`) and maps the owner to `normal_indices`/`light_indices` built at `:51-68`. For a Dragonborn record, local index 2 → `normal_indices["dragonborn.esm"] = 4` → `0x04xxxxxx` (`:191`). Light plugins are packed to `0xFE000000 | (index << 12) | (local & 0xFFF)` (`:186`) — verified correct for all three `.esl` files. I found **no** group/record parse error, size miscount, record cap or truncation: my independent scanner consumed 64,664,159/64,664,159 bytes (**100.00 %**) of Dragonborn.esm with zero group or record errors, and its own-record range (0x000800..0x03D5E6, REFR up to 0x03D5E4) matches the DB exactly.

## 3. Is upstream affected?

**No — and for this symptom there is nothing for upstream to differ on,** because the defect is not in the ESM parse/export path at all.

Evidence, with a caveat: the working tree I read is **upstream's**, not the fork's — it defines none of the fork-only tables (`door_links`, `lights`, `space_lighting`) anywhere in the repo (confirmed with a positive-control grep), and its table set matches `docs/specs/converters/db-schema.md`, which also lacks them; yet the produced DB has them. Since `git show portal:<path>` was blocked, I could not produce a diff. Its remap logic reproduces the DB's slot assignment exactly on all ten plugins, so the fork's ESM code is equivalent here. The fork's ESM-directory change is confined to `esm/exporter.rs` (mtime 20:08 vs 14:48 for `mod.rs`, `binary.rs`, `records/`), i.e. the export step that owns the `CREATE TABLE` statements — consistent with the brief's "door links, lights, per-space lighting", none of which touch FormID remapping.

## 4. Minimal fix proposal

Fix the **consumer** of the tree LODs, not the converter:

1. **Primary.** When resolving a `.btt` tree FormID, remap the top byte from the LOD's authoring master index to the global slot: `global = (slot_of_owner_plugin << 24) | (formid & 0x00FFFFFF)`, with the owner determined by which plugin owns the worldspace directory (for `terrain/dlc2solstheimworld/trees/*` → Dragonborn = 4). A safe minimal form: try the id as-is; on miss, retry with the owning plugin's slot. For `tamriel` the remap is a no-op (slot 0), so this is backward-compatible.
2. **Hardening.** Add `"btt"` to `AssetKind::Mesh::accepted_extensions` (`crates/converter/src/asset_path.rs:29`) if the engine is to consume tree LOD, plus a unit test asserting 0x02→0x04 for a `dlc2solstheimworld` file.
3. **Unrelated, worth its own ticket:** `plugins.priority` is written as the raw plugin enumeration index (`esm/mod.rs:32`) but slots come from `normal_indices`/`light_indices` (`esm/mod.rs:186-191`), so they disagree for every plugin after the first light plugin — `ccBGSSSE001-Fish.esm` has priority 6 but FormIDs at 0x05, `ccBGSSSE025-AdvDSGS.esm` priority 7 but 0x06, and the three `.esl` files priorities 5/8/9 but live at 0xFE. Anything decoding a `form_id` via `plugins.priority` is off by one. Write the real slot into `priority` (or add a `slot` column).

## 5. Other findings (in scope for Q4, but not the Dragonborn symptom)

- **Skyrim.esm: 274 records genuinely lost** — REFR 184, NAVM 71, ACHR 10, STAT 2, IDLE 2, and one each of EXPL/INFO/NPC_/SMQN/SPEL; scattered across 0x0138AF..0x2A6D2E, so not truncation. I checked the obvious benign explanation and it fails: **0 of the 274 are delete-flagged by any later plugin**, and all 274 are declared only by Skyrim.esm with the delete flag clear (`21_deleted_check.py`). The DB also has 3 rows with no ESM counterpart: GMST 0x23C00E, LAND 0x179DB, LAND 0x222222 (the last looks like a sentinel). **Update.esm: 4 REFRs lost**, 0x00003119–0x0000311C (a contiguous run). These are real, small, unexplained losses — 0.03 % of Skyrim — and deserve a separate look.
- **10 stale tree ids**: after remapping, 10 of my 1,001 Solstheim ids still resolve to nothing, and none of them exist anywhere in Dragonborn.esm (0x0296E6, 0x02ADC0/1, 0x02AED5/6, 0x02B7B8, 0x02B7BA–BD). Likewise 88 of 40,839 `tamriel` entries. Bethesda data stale after reference removal, not converter loss.

## 6. Uncertainties

- **I could not reproduce the exact figure 1,577.** My best decoder (`18_btt_v3.py`, parses 381 of 386 `.btt` files) yields 1,001 tree entries (999 distinct FormIDs) from 19 of the 24 `dlc2solstheimworld` files; the other 5 (22,620 bytes) contain a record variant I did not finish reverse-engineering — the byte dumps show entries with the position fields omitted (16- vs 32-byte records), so my grammar under-counts. Extrapolating by byte density gives ≈1,680. **The conclusion does not depend on the exact count**: every record I could decode in `dlc2solstheimworld` carries top byte 0x02, and 991/1,001 resolve after the remap.
- Whether upstream's converter is byte-for-byte identical in `esm/` is inferred (no git), per §3.

## 7. Scripts (all in the work folder)

`C:\Users\Brannock\AppData\Local\Temp\claude\C--Modding-OpenSkyrimProject\5ae7d30e-755d-4206-af2b-5d4791d54da4\scratchpad\dbwork\`

`NOTES.md` indexes all 25. The load-bearing ones: `09_setcompare.py` (set-exact ESM↔DB comparison), `11_setcompare_all.py` (all plugins), `25_cc_check.py` (CC/light plugins against the 0xFE packing), `18_btt_v3.py` (best `.btt` decoder), `23_validate_trees.py` (the decisive remap/coordinate validation), `13_btt_decode.py` (sha256 provenance of `.btt` vs BSA), `07_probe_ids.py` (the 0x1AAFE boundary).

The database was opened read-only (`mode=ro`, `PRAGMA query_only=1`) throughout and never written or copied; nothing outside the work folder was modified.


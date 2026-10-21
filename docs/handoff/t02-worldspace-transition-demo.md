# t02 — worldspace transition demo (handoff)

**Deliverable:** `docs/research/worldspace-transition-demo.md` — the Alftand → Blackreach route with
every FormID derived from the plugin, a converter/database/engine "exists vs missing" table with
`path:line`, a nine-step implementation recipe with the test that proves each step, the size of the
job, the Markarth → Understone Keep → Nchuand-Zel fallback, and a recommendation.

**Also written:** `tools/research/esm_route.py` — a read-only plugin probe added by this task.

**Scope kept:** documentation plus the one script under `tools/research/`. No converter, engine or
`scripts/*.ps1` run. `Skyrim.esm` was read with the script only, never with `Read`. Nothing was
written to the Skyrim install.

## Verified (read in the code at commit cc6e714, or measured out of Skyrim.esm)

- The vanilla route is **four** load doors, not three, and it passes through a child worldspace:
  Tamriel (`0000003C`, door ref `00015D48` in persistent cell `00000D74`) → Alftand01
  (`000152C3`) → Alftand02 (`00056C1B`) → AlftandWorld (`00069857`, cell `00069869`) → Blackreach
  (`0001EE62`, cell `0002D4E0`). Each `XTEL` was decoded, including the arrival position and
  rotation, and each pair is reciprocal.
- Blackreach is an ordinary exterior worldspace (320 cells, 125 with `LAND`, 4678 references, 406
  distinct mesh paths) with `WNAM` = Tamriel, so the engine's existing exterior streaming path
  applies unchanged apart from the id.
- The engine's **interior path is complete and uncalled**: `CellKey::Interior`
  (`crates/engine/src/world/database.rs:21`), the interior query
  (`crates/engine/src/world/database.rs:288`), the interior transform branch
  (`crates/engine/src/streaming.rs:522`) and the interior unit test
  (`crates/engine/src/world/database.rs:470`) all exist; `plan_cells` only ever inserts
  `CellKey::Exterior` (`crates/engine/src/streaming.rs:194`).
- `XTEL` appears **nowhere** in the repository's code (`grep -r XTEL` matches only the t02 brief and
  the new script); the raw bytes survive only inside `references.data`
  (`crates/converter/src/esm/exporter.rs:292`). `is_form_id_subrecord` remaps 4-byte subrecords only
  (`crates/converter/src/esm/mod.rs:145`), so an `XTEL` destination FormID from a non-master plugin
  is currently left unremapped — correct only because all four route doors are in `Skyrim.esm`.
- `statics` holds `STAT`/`MSTT`/`FURN` only (`crates/converter/src/esm/exporter.rs:177`) and the
  engine drops any reference whose `model_path` is `NULL` (`crates/engine/src/streaming.rs:554`), so
  on this route 134 doors, 557 activators, 223 trees, 185 flora and 107 containers would render as
  nothing before step 4 of the recipe.
- `docs/specs/converters/db-schema.md:105` is stale: its `references` DDL lacks `base_form_id`,
  `is_exterior` and `local_x/local_y`, and its `land` table names a `vtext` column that does not
  exist. The real DDL is `crates/converter/src/esm/exporter.rs:36`. Worth fixing separately.
- `python tools/check_citations.py docs/research/worldspace-transition-demo.md` — 0 broken
  citations, 0 to check by hand.

## Actually run

- `python tools/research/esm_route.py "<Data>/Skyrim.esm" alftand blackreach nchuandzel` and the
  same with `understone markarth`. Two full passes over the plugin (~250 MB) each; every ID, cell,
  position, rotation and count in the note comes from those two outputs.

## Blocked / unknown

- **The converted database could not be read.** `C:/Modding/SkyrimConverted` is outside this
  session's permitted roots, so the brief's `mode=ro` queries were refused
  (`permissions.blockReadsOutsideWorkingDirectories`). Section 5 of the note therefore counts
  references and mesh paths exactly, and **estimates** the texture closure (600–1000 for Blackreach,
  inferred from the material slot contract). Section 7 lists the four SQL queries that would replace
  the estimate; run them if the DB exists.
- Whether any NIF on this route uses `NiTriStripsData` — no parser exists
  (`vendor/project-wormhole-nif/src/legacy.rs:64`), and the check needs `nif-audit` over
  `vfs\meshes`, i.e. a converter run. Inferred low risk (SSE content is mostly `BSTriShape`), not
  measured.
- The `XTEL` byte layout is from UESP, corroborated by the reciprocal-pair measurement; the lift
  doors' `MinUse` proximity trigger is inferred from the base `EDID`.
- Whether an exterior should stay resident while an interior is active (return trip also seamless,
  at the cost of ~25 Tamriel cells) is a design choice left to the lead.

## Next step for the lead

1. Decide stage A only (Tamriel (19,18) → Alftand01 → Alftand02) and land recipe steps 5–8
   (`ActiveCell`, interior request/retire, rebase guard, transition system). Step 4 (wider base
   objects) is needed to *see* the doors.
2. Then converter steps 1–3 (`XTEL` parse + FormID remap + schema bump), which is what turns the
   hidden route into a data-driven one.
3. Then stage B (Alftand02 → AlftandWorld → Blackreach) and only afterwards consider Markarth,
   whose Tamriel → MarkarthWorld crossing is the one case the Alftand route does not cover.

# doc-065 — the demo guide covers two demos

**Task:** `tasks/deepseek/doc-065-riverwood-in-the-demo-guide.md`. Edited `docs/demo/README.md` only
(plus this note). No engine, converter, script or launcher was run or touched, per the brief.

## What changed in `docs/demo/README.md`

1. **Title and opening** now name both demos with a sentence each — what each shows and why a
   newcomer would pick it — and keep Riverwood's selling point (into an interior and back out, in
   daylight): **8 crossings of 8, 0 dark frames in 2,056 captured frames**, given as the lead's own
   measurement and cited as `local/demo/riverwood2/tour.txt` (the log confirms `tour PASSED after 8
   crossings`). The links list now carries `docs/design/riverwood-demo.md` beside the Blackreach
   design.
2. **Section 1's table** became a two-column table (Alftand → Blackreach | Riverwood) with the same
   four rows: Start, Route, Crossing, Also. Riverwood's row facts come from
   `docs/design/riverwood-demo.md` sections 1-3 (start, cells, the four houses, 8,096 units /
   ≈116 m / ≈54 s at 150 u/s) and from `crates/engine/src/config.rs` (the `"riverwood"` start) and
   `crates/engine/src/demo_tour.rs` (`RIVERWOOD_ROUTE`, eight doors).
3. **Known-gaps list** checked entry by entry (details below), Riverwood's gaps added, and a closing
   paragraph saying which of them `docs/research/visual-gaps-spec.md` does and does not specify.
4. **Command table**: `--demo alftand|blackreach|riverwood`; `--demo-tour` rewritten (route of the
   demo the run started in, Alftand's route when there is none, the `--walk` walk-through and the
   four-second `W` walk test).
5. **Troubleshooting**: the stale-schema entry updated to schema 18 with the reconversion command
   from section 4.2, the reason a folder can go stale without the contributor changing anything, and
   the pointer to `converter_schema_version` in `crates/engine/src/app.rs`. Section 4.3's schema
   numbers went 15 → 18.
6. Smaller coherence edits: section 3 says what `play-demo.ps1 -Start` accepts and how to start
   Riverwood; section 9 gained rows (Riverwood design, both animation modules, per-space lighting,
   the tour) and now mentions `play-riverwood-demo.cmd` beside the Blackreach launchers; the
   "start is not where you expect" entry gained Riverwood's start (first doorway ≈1,100 units up the
   road, section 3's first leg).

## Stale entries found and how each was resolved

All numbers below were read from the tree at `59f1f20`, not from memory.

| Old entry | State | Evidence |
|---|---|---|
| No glow or emissive | **Fixed** — moved to "What works" (HDR camera + bloom + `EMISSIVE_EXPOSURE`) | `crates/engine/src/lights.rs:144`, `crates/engine/src/app.rs:1990-2005`, `render.rs` `is_deliberate_glow`; handoff log, `23adbb4` |
| No per-cell lighting templates (`XCLL`/`LTMP`) or interior fog | **Fixed** — replaced with the light's real remaining gaps (no shadows, cone, flicker; falloff exponent/`FOV`/near clip read but not applied) | `crates/engine/src/world/lighting.rs`, `app.rs` `space_atmosphere`/`atmosphere_fog` (`SpaceFog::Own`), `lights.rs:67-76`; converter `26324d5`, engine `66ee786` |
| No weather and no snow shader; sky a plain gradient; roofs bronze | **Split** — snow is real and data-driven (`snow.rs` reads `matos` + `statics.material_object`/`material_max_angle`, degrades gracefully on older DBs); the weather record now sets sun colour, daylight and horizon haze; what remains is no sky dome, no clouds, no weather change (the sky is the camera's clear colour, one colour per space) | `crates/engine/src/snow.rs:225-273`, `app.rs:1508-1544`, `:1784-1786`; `ClearColorConfig::Custom(atmosphere.backdrop)` at `app.rs:1818`; no skybox/atmosphere code in the crate |
| Doors do not animate | **Fixed** — moved to "What works" with the twin/scale-up rule for the Dwemer doors | `crates/engine/src/door_animation.rs:1-65`; handoff log `50bc0f1`, `b535a30`, `c02737b` |
| Two Blackreach models fail the bounds gate; 4334-unit light floods the ceiling | **Fixed** — replaced with the general rule (empty-scene models are skipped and counted) | `crates/engine/src/streaming.rs:1467-1475`, `:1750-1790`; `lights.rs:112-121` (`INTENSITY_REFERENCE_RADIUS = 256.0`) |
| Terrain layer blending is blocky, "not yet fixed" | **Stale — moved to "What works"** (see judgement calls) | `crates/engine/src/shaders/terrain.wgsl:45-69` (`grid_weight`, bilinear), `render.rs:1205-1210` (the test is named "Defect A") |

Kept verbatim because nothing in the tree says they changed: ice reads as rock; `BlackPlane01`
voids; scripts converted but unused for gameplay; a portal destination's lights can reach the
active space; no audio; no saves; no physics beyond walking.

## Judgement calls, and things I could not confirm

- **The terrain-blending entry.** The old text called research-004's Defect A unfixed. The current
  shader reads the weights as the game's own 17×17 `VTXT` sample grid, interpolated bilinearly
  between samples, with the unit test named after the defect; the handoff log puts the fix in
  impl-010 (`0ece570`, 2026-09-22). I moved it to "What works" **claiming only what the code shows**
  — no pixel-level claim, because no doc records that the visual review was redone. If the lead
  knows the blockiness survives somewhere the shader cannot fix, this is the line to revert.
- **The Alftand start grid** was written as "(19,18)". `config.rs`'s own test asserts `(18, 18)` and
  `grid_of(77583.23, 77411.89)` is `(18, 18)` — cells are 4096 units. Corrected to 18,18.
- **Terrain ring / Defect B.** The old list had no water-plane entry; the new river bullet covers the
  cell-aligned plane, which is the same family as research-004's Defect B. I did not claim Defect B
  itself is fixed or unfixed — nothing in the tree says either.
- **Converter schema.** `CONVERTER_SCHEMA_VERSION` is 18 (`crates/converter/src/cache.rs:11`) and
  `converter_schema_version()` in `crates/engine/src/app.rs:1350-1354` matches. The database schema
  is still 5. **The lead's own converted folder is schema 17 and `complete: true`, so an engine
  built from this tree will refuse it** until the reconversion in impl-062's note runs; that is why
  the troubleshooting entry matters for the lead as much as for a contributor.
- **`docs/research/visual-gaps-spec.md` coverage**, for the paragraph the brief asked for: of the
  Riverwood gaps I added, **none** is in the spec — its four gaps are glow, per-space lighting/fog,
  snow/ice and the broken objects. The only overlap is that the *conversion* of an empty glTF scene
  is its gap 4.2 (the bounds-gate fix). So the village's missing smoke, mist and spray, its missing
  NPCs and grass, its flat river and its missing LOD are recorded in
  `docs/design/riverwood-demo.md` section 4 and nowhere else; if the lead wants them in the spec's
  system, that is a new gap, not an existing one.
- **Left alone on purpose.** I did not touch the prerequisites, disk/time table, conversion section,
  controls, reference-shot section or any other text the brief did not ask about.

## What is not mine

- **`tools/demo/play-demo.ps1` does not know about riverwood.** Its `-Start` is
  `[ValidateSet("alftand", "blackreach")]` (line 18), so `-Start riverwood` is rejected before the
  engine is called. I documented that in section 3 (start Riverwood with the engine directly) and
  left the script alone, as the brief required. **The lead should decide whether to add the name to
  the `ValidateSet` and the header comment.**
- **`tools/demo/setup-demo.ps1`** writes a launcher fixed to `-Start alftand` (`Write-Launcher`),
  and **`play-riverwood-demo.cmd`** in the repository root is the lead's own hard-coded-path
  launcher. Neither was touched; section 9 now names the Riverwood launcher beside the Blackreach
  pair.

## Verified / not verified

Verified by reading the tree: every claim in the new text, with the file:line evidence above.
Not verified: any *visual* consequence of any of it — no engine run, no render, no script, per the
brief. The tour measurement in the opening is the brief's number, cross-checked only for the
`tour PASSED after 8 crossings` line in the lead's `tour.txt`.

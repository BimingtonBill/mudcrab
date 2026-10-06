# Phase 2 branch audit

**Status:** DeepSeek lead-515 with 12 read-only reviewers, 2026-09-23 evening; the lead opened every cited line for blocking/should-fix items. Claude's notes: the review of `vertex-alpha-not-opacity` also suggests upstream's converter may not export vertex colours for ordinary statics at all, so the "trees as dots" bug may not exist upstream (check on screen). The schema-15 clash is resolved on `phase2/integration` by one bump in `crates/shared`. Fixes are queued as one worker per branch after tonight's conversion.


**Language note:** the Goal line of my brief ends with ", ru". I read that as an ambiguous directive about the report language and stayed in English, the language of the task, the checklist and the codebase. Say the word and I will reissue it in Russian.

**Method.** One read-only reviewer per branch, two waves of six (`spawn_workers`, effort high). Workers here have **no shell**, so each brief carried the branch's full diff verbatim plus the list of files that are byte-identical to the branch on disk (so they could Read real context), and told them to judge only from the diff for files the integration checkout has moved on. I did not re-run their work; I opened every cited line myself for each blocking/should-fix item, and dropped what I could not confirm. Reviews passed fmt/clippy/tests already; nothing here duplicates that.

---

## 1. `phase2/refr-rotation` — review-001-refr-rotation.1

- `crates/shared/src/coordinates.rs:29-35` — **should-fix** — the new doc's justification ("Measured on Skyrim.esm's load doors (2026-09-22): … 88 % … against 42 %", 63 door models) is not reproducible from the repo, and near-symmetric doors match under either convention. `coordinates.rs` is branch-identical on disk; I read the comment. Trim to the convention and move the measurement to the PR/ADR. The clause "the counter-clockwise one used until then" is fork-local history in a permanent doc.
- `crates/shared/src/coordinates.rs:162` — **should-fix** — `converted_rotation_matches_converted_source_vector` builds its expectation with the same helpers, the same order and the same outer conjugation, so it mirrors the implementation and would pass under either convention (it was edited in lockstep with the code). Confirmed textually. `docs/roadmap/02-milestone-1-implementation-plan.md:47-53` — which I read — states the basis contract and explicitly requires the REFR Euler order to be "validated with canonical fixtures"; a hand-computed expectation should be added.
- `crates/engine/src/streaming.rs` (test `maps_creation_position_and_rotation_through_the_same_basis`) — **nit** — its name no longer describes what the changed assertion proves, and the same convention is now asserted in two places.
- **Dropped:** "(the same as OpenMW does …)" as a separate item — folded into the doc item (same lines, unverifiable attribution).

The code itself I accept: `conj(Rz·Ry·Rx)` conjugated by the basis is the inverse composition, and the reviewer's hand-derivation (yaw π/2 takes `+X → +Z`) matches the changed assertions.

## 2. `phase2/shared-converter-schema` — review-002-shared-converter-schema.1

- `crates/converter/src/cache.rs:13` and `crates/engine/src/app.rs:1222` — **should-fix (merge order)** — this branch deletes the exact literal `pub const CONVERTER_SCHEMA_VERSION: u32 = 14;` that `phase2/material-publication` and `phase2/renderable-base-types` each edit to 15, and replaces the engine's hand-synced copy with `shared::CONVERTER_SCHEMA_VERSION`. I read all three branch diffs: a resolution that "takes ours" on this branch leaves the single definition at **14** while both bump branches write 15 to the manifest — the engine then rejects freshly converted assets ("reconvert … schema 14") and the migration gate (`matches!(…, 12..=14) && CONVERTER_SCHEMA_VERSION == 15`) becomes dead. The number must end at 15 in `crates/shared/src/lib.rs`.
- `scripts/phase2-acceptance.ps1:35` / `phase2-visual-baseline.ps1:26` — **should-fix (merge order)** — the regex matches `pub const CONVERTER_SCHEMA_VERSION: u32 = (\d+);`; after this branch the converter file holds only a re-export, so the search must hit `crates/shared/src/lib.rs`, which is first in the list (verified in `phase2/scripts-schema-from-source`).
- `crates/engine/src/app.rs:1198` — **nit** — engine test and converter test now compare the same symbol, so drift is no longer detectable by tests.
- I confirmed the branch's "no behaviour change" claim and that `converter` re-export keeps every call site compiling.

## 3. `phase2/terrain-samplers-weights` — review-003-terrain-samplers-weights.1

- `crates/engine/src/render.rs:266` — **should-fix** — `TerrainExtension::fixture` sets `vertex_weights_only`, and `crates/engine/src/app.rs`'s canonical terrain fixture (which *has* overlay layers in its `TerrainSnapshot`) pairs every quadrant with it. I read both: the only no-game-data renderable scene still renders overlays 1-3 through the sharpened packed tangent, so half the fix is unexercised in the demo and the visual baseline.
- `crates/engine/src/render.rs:318` — **should-fix** — the fixture's six layer textures come from `images.add(fixture_image(...))` with `image.sampler = ImageSampler::linear()` (verified at `app.rs:385,401,412,421`), so the Repeat fix is likewise invisible in the fixture.
- `crates/engine/src/shaders/terrain.wgsl` (`grid_weight`, ~:52) — **should-fix** — no test can catch an error in the actual WGSL: the test module's `shader_weight` re-implements it including the clamps, and the only shader assertions are `source.contains(...)` string matches. A transposed index or a wrong clamp passes.
- `terrain.wgsl:52` — **nit** — `blend` is derived from the unclamped `corner` while `base` is clamped, so a coordinate a ulp below 0 blends sample 1 into the edge. (I read the shader; branch-identical on disk.)
- `terrain.wgsl:97` — **nit** — five `grid_weight` calls per fragment even for base-only quadrants; `crates/engine/src/streaming.rs:1420` — **nit** — a third un-pinned copy of the `17` grid literal.
- **Dropped:** the `Vec4` padding/`SHADER_SIZE` assertion suggestion — kept as an optional nit, not a defect.

The reviewer verified the uniform layout, the uv→grid transform and the VTXT bounds by hand; I checked the layout claim against the branch's own layout test and the shader text, and agree.

## 4. `phase2/terrain-seam-weld` — review-004-terrain-seam-weld.1

- `crates/engine/src/streaming.rs:1693` — **should-fix** — corner welds: `TerrainEdgeSide::ALL` welds West, East, South, North with the later side winning, and a corner belongs to two sides (index 0 ∈ West∩South, 1088 ∈ East∩North; I derived the index sets from `points()` in the diff). The arriving corner can end up agreeing with one neighbour and disagreeing with the other — the same class of crack the branch removes — and the point is pushed into `welded_points` twice, double-counting the new metric. Welding corners last to a single chosen neighbour, or rejecting when the two resident corners disagree, would close it.
- `crates/engine/src/streaming.rs:1606` — **should-fix** — `MAX_WELDABLE_EDGE_DELTA = 512.0` is four sample steps (4096/32 = 128; arithmetic confirmed) but 21× the worst real seam the doc itself cites (24 units), and `docs/roadmap/02-completion-plan.md:157` — which I read — states seams above the tolerance are **rejected**. Derive the bound from the measurement or record the policy change; the metric counts moved points but not magnitude.
- `crates/engine/src/streaming.rs:1718` — **should-fix** — `terrain_seam_points_welded` is not reported anywhere: the branch's `crates/engine/src/profiling.rs` summary lists "Terrain seams validated" among ~25 counters and this one is absent (I read the list), and `docs/roadmap/02-integration-and-acceptance.md:20-21` promises streaming diagnostics record terrain seams.
- Nits I confirmed: the dimension/non-finite guards duplicate `validate_terrain_snapshot` and scan the whole 1089-point field twice per arriving cell; `side.points()` allocates twice per weldable side and `TerrainEdges::of` adds four more temporaries on the commit-budget path; `recompute_packed_normals` re-implements the converter's VNML packing (`crates/converter/src/esm/cell_cache.rs`) behind a doc link only.
- **Dropped:** nothing.

## 5. `phase2/material-publication` — review-005-material-publication.1

- `crates/converter/src/material.rs:1788` — **should-fix** — `alpha_property(0x12EC, 26)`: bit 0 (blend) is **clear** in `0x12EC` (I computed `0x12EC & 1 = 0` and read the test at 1773-1802 on the branch-identical file), so `blend_factors` returns `None` from the `!blend_enabled()` early return, not from the "test bit wins over the blend bit" precedence the comment claims. Use a code with both bits set (e.g. `0x12ED`) and assert the factors reach the contract but not the published extension.
- `crates/converter/src/material.rs:736` — **should-fix (comment accuracy)** — "would be drawn as ordinary transparency without them" describes a runtime benefit that does not exist yet: grep for `blendSource|blendDestination` hits only `converter` and `docs`, and the engine reads none of it. Say they are published for a future consumer.
- `docs/specs/converters/nif-to-gltf.md:84` — **should-fix** — line 80 documents the new blend factors while line 84 still says the extension records only premultiplied-alpha and screen-door requirements (I read 74-86). The prose contradicts the table.
- `scripts/phase2-acceptance.ps1:168` — **should-fix (merge)** — this branch's hard-coded `-eq 15` is the same line `phase2/scripts-schema-from-source` replaces with the source-reading call (verified in both diffs); the resolution must keep the source-reading version.
- `crates/converter/src/material.rs:213` — **nit** — the doc's `EditorMarker:12` example: the contract's `shape_name` comes straight from the string table (`material.rs:289-298`), no block suffix, so the `':'` split is harmless but mis-explained.
- **Dropped:** the nit that screen-door/premultiplied still publish `BLEND` — the commit body states that as deliberate, so it is intent, not a defect.

The roughness maths I re-checked (`(2/(n+2))^0.25`, `n=30 → 0.5`, fixture 32 → 0.49248) and the alpha-branch inversion (`vertex_alpha_shader_flag_is_not_a_blend_state` inverts the old assertion) are sound.

## 6. `phase2/lod-mesh-containers` — review-006-lod-mesh-containers.1

- **Framing question answered, no defect:** I read `vendor/project-wormhole-nif/src/nif_file.rs` — blocks are parsed from `take(block_size_index[index])(data)`, i.e. per-block slices, so `if i.len() >= 4` in `BSMultiBoundNode::parse` cannot steal bytes from the next block. The reviewer's conclusion holds.
- `crates/converter/tests/fixture_lod.rs:264,274,276` — **should-fix** — the ignored test's doc hard-codes `WKJ_LOD_SAMPLE="C:/Modding/SkyrimConverted/vfs/…"`. I grepped: it is the only fixture env var not named `OPENSKYRIM_*` (every other ignored test uses `MUDCRAB_*_FIXTURE`, the convention documented in `docs/adr/0002-…`), and it is a machine path — a fork-only reference that must not go upstream.
- `vendor/project-wormhole-nif/src/bs/sub_index_tri_shape.rs:38` (+ `vendor/project-wormhole-nif/src/tests.rs`) — **should-fix** — the parse consumes two header words where the replaced code consumed three, and the new fixture (2 segments / 42 primitives over a 0-triangle shape) violates the doc's own invariant that segment primitive counts sum to the shape's triangle count, so a wrong layout would parse "successfully" and only surface as a leftover-bytes warning. Assert the invariant or the zero leftover.
- `vendor/project-wormhole-nif/src/nif_file.rs:620` — **should-fix** — `is_distant_lod_container` is per file, so every shape of any NIF containing one `BSMultiBoundNode` gains `COLOR_0`, while the legacy `NiTriShape` path (~:470) keeps colours unconditionally and the skinned-partition fallback (~:685) does too. The flag's stated reason ("would change how most converted statics render") is not what the code does.
- `crates/dummy-content/src/nif.rs:429` — **should-fix** — `write_lod_shape_block` appends a `u32` to a block typed `BSTriShape`, which the dispatcher warns about ("bytes left over after parsing BSTriShape", `nif_block.rs:296-301`). The author's own comment says shipped LOD shapes carry that word, so on real `.btr` this warns per shape — either consume it or drop it from the fixture.
- `crates/dummy-content/src/nif.rs` (new writers) — **should-fix (project gate)** — `docs/adr/0005-…:14`, which I read, requires deterministic truncation and bounded mutation sweeps "for every generated format"; the two new writers have none.
- Nits confirmed: `COLOR_0` presence-only assertion (its alpha is rewritten by branch 7 anyway); `Vec::with_capacity(112 + data_size)` under-reserves; three `convert_kind` passes over the vfs list (progress bars restart); `crates/converter/src/archive/mod.rs:288` still classifies only dds/nif/pex.
- The reviewer's "no schema bump needed" note I accept: the manifest format is unchanged and `.btr`/`.bto` GLBs are newly produced.

## 7. `phase2/vertex-alpha-not-opacity` — review-007-vertex-alpha-not-opacity.1

- `crates/engine/src/render.rs` (`force_opaque_vertex_colours`) — **needs discussion** — the system rewrites *every* loaded mesh, including the `.btr`/`.bto` colours branch 6 now exports with all four channels from the file. If LOD tint alpha means anything, this destroys it at load with no way back; if it means nothing, branch 6 should say so. The two authors must decide.
- `crates/engine/src/render.rs:447` (doc) and the commit body — **should-fix** — the "alpha-tested foliage/trees" story needs a mesh-class qualifier. I verified in the vendored reader that SSE `BSTriShape` colours are gated on `preserve_vertex_colors` (LOD containers only) and that the other sources are the legacy `NiTriShape` path and the skinned-partition fallback — so ordinary `TREE` meshes carry no `COLOR_0` at all, and the frames where the symptom appeared should be named.
- `crates/engine/src/render.rs:488` — **should-fix** — a mesh whose data is already in the render world is silently not fixed, at `debug!`, sharing one `Err` with the deliberately-skipped encodings; a failing fix should be at least a one-shot `warn!`.
- `the_rewrite_runs_on_an_added_mesh_and_does_not_retrigger_itself` — **should-fix (coverage)** — the only system-level test builds its own `App` and adds the system directly, so deleting the plugin registration (and every encoding arm but `Float32x4`) keeps the suite green. No test for `Unorm10_10_10_2`, `Float64x4`, the render-world `Err`, or a mesh loaded through the glTF path.
- Nits confirmed: a 22-line doc carrying Bevy source paths and line numbers; the render-world `Err` note; the 1/2/3-component arms are unreachable through `ATTRIBUTE_COLOR`.
- I accept the change-detection analysis (`bypass_change_detection` + `into_mut`-style `into_inner`, no self-retrigger) and the encoding-by-encoding review; `Unorm10_10_10_2`'s bit order rests on the reviewer's knowledge of the format and is untested here.

## 8. `phase2/renderable-base-types` — review-008-renderable-base-types.1

- `crates/converter/src/cache.rs:69` + `crates/engine/src/app.rs:1218` — **blocking as a pair, with branch 5** — this branch and `phase2/material-publication` both stamp 15 with the same `matches!(…, 12..=14) && CONVERTER_SCHEMA_VERSION == 15`. Landing them as two commits means the second branch's publication change is silently skipped on any manifest already at 15 (nothing reconverts, nothing is rejected). Verified by reading both diffs: **land them in one merge, or renumber the second to 16.**
- `crates/converter/src/esm/exporter.rs:199` (+ `crates/converter/src/integration.rs`) — **should-fix** — the new rows enter the asset-closure gate: I read `finalize_world_database`, which increments `missing_model_count` when the source `.nif` exists but its GLB does not, and `report.passed` requires zero, while the engine refuses to launch on `!passed`. Six record types — TREE alone is ~126 k references per the commit body — now put every one of their models behind a hard gate.
- `crates/converter/src/esm/exporter.rs:195` — **should-fix** — `get_string(b"MODL").filter(|p| !p.is_empty())` stores the path verbatim; `crates/converter/src/asset_path.rs:133` bails on control characters (`bail!("asset path contains a control character")`, read), and that error propagates out of `finalize_world_database`, aborting the run. Reject control characters in the arm.
- `crates/converter/src/esm/exporter.rs:193` — **should-fix (scope statement)** — the loader resolves *every* REFR base through `LEFT JOIN statics s ON s.id=r.base_form_id` (I read `crates/engine/src/world/database.rs:276,294`), so item bases (MISC/WEAP/ARMO/BOOK/…) still spawn model-less; the PR should say that is the next step.
- Nits confirmed: no test that a manifest already at 15 is left alone; `catalogued_model` is the only new helper without a doc comment.
- I verified the `continue` cannot skip other work (the match is the loop's last statement) and that the NULL-row vs absent-row distinction the tests rely on is real (`query_row` errors on absent, returns `Ok(None)` for NULL).

## 9. `phase2/pruned-texture-complete` — review-009-pruned-texture-complete.1

- `docs/specs/converters/dds-to-ktx2.md:42` — **should-fix** — "only becomes a complete schema-12 manifest when every asset and required GLB texture URI validates" (read), while the branch makes even a *required* base-colour map prunable — its own end-to-end test prunes `absent.opensky-srgb.ktx2` and expects `complete`. Update the completeness contract (and `docs/roadmap/01-asset-pipeline.md`, `nif-to-gltf.md:84`).
- `crates/converter/src/pipeline.rs:800` — **should-fix** — the per-GLB record is rebuilt from scratch each run, so a resumed run (`config.resume_staging`, `existing_is_valid` — I confirmed both exist at :100-110 and :575-589) that reuses an already-pruned GLB reports zero prunes and still says complete. Carry the previous manifest's entries forward for reused GLBs.
- `crates/converter/src/cache.rs:49` — **nit/should-fix** — grep for `pruned_texture_references` hits only the converter (pipeline.rs, cache.rs, main.rs); the engine, launcher, `world-inspect` and the acceptance preflight read none of it, so the new manifest map is write-only. Surface it or say it is human-only.
- Nits confirmed: `#[serde(default)]` on `pruned_texture_references: u64` in `PipelineReport` is inert (the struct is only serialized); `.push(reference)` without sort/dedup; `ProgressStage::Textures` counters after that stage already reported totals (inherited from `main`'s `record_skip`, not a regression); the CLI summary on stdout while detail goes to stderr.
- I confirmed the branch does achieve its goal: the engine needs only `schema_version == shared::CONVERTER_SCHEMA_VERSION && complete` plus a passed integration report, and the launcher the same — a prune-then-complete set now passes all of them.

## 10. `phase2/empty-model-bounds` — review-010-empty-model-bounds.1

- `crates/engine/src/streaming.rs:966` — **should-fix** — the predicate is one-sided: "no converted bounds and no `Mesh3d` descendant" now passes silently, so any bounds-less scene that merely holds no mesh (nodes only, geometry the converter dropped by mistake, a scene read before its entities exist) is waved through where `main` failed loudly. Narrow the tolerated class and document the residual tolerance.
- `crates/engine/src/streaming.rs:728` — **should-fix** — `empty_model_references` is not in the profiling summary (I read the "Streaming and assets" counter list in `profiling.rs`: it carries "Terrain seams validated", "Assets ready", "Meshes validated" and ~25 others, not this one), and `docs/roadmap/02-integration-and-acceptance.md:20-21` promises streaming diagnostics; nor does the acceptance script read it. The only trace is the per-path `profiler.event`.
- `crates/engine/src/streaming.rs:973` — **should-fix (coverage)** — no test proves the assumption the fix rests on (that a real spawned model's `Mesh3d` primitives are descendants of the entity holding `WorldAssetRoot`): both primitive cases hand-build `ChildOf(reference)`, and the empty case spawns nothing at all. Add a case whose `WorldAsset` world really holds a mesh entity.
- Nits confirmed: the new metric doc's long lines vs the file's ~100-column style; naming (`empty_model` vs "holds no render primitive", and the `assets/empty_model_references` / `"empty_model"` spellings vs the sibling `asset_ready`/`asset_failed` vocabulary); the `usize` shadowing the `primitives` query; the fixed 8-update settle loop.
- I verified the fix is real (on `main` the absent-bounds arm returned `Err` unconditionally), that the new path removes `PendingAssetProfile` and decrements the pending count through the normal budget, and that both sibling branches' halves agree (branch 5 emits empty scenes for marker-only models, keeps the `statics` row, leaves `bounds_valid=0`).

## 11. `phase2/scripts-schema-from-source` — review-011-scripts-schema-from-source.1

- `scripts/phase2-acceptance.ps1:187` and `scripts/phase2-visual-baseline.ps1:63` — **should-fix** — both now read the converter schema from source but still hard-code the integration/world-database schema 3 (I grepped both lines), so the helper's comment "defined once in Rust" is only half true. Parameterise the helper by constant name.
- `scripts/phase2-visual-baseline.ps1:23` / `phase2-acceptance.ps1:35` — **nit** — "first file that matches wins" silently accepts a wrong value if a bad merge leaves a stale `pub const` in `crates/shared/src/lib.rs` while `cache.rs` says otherwise; collecting the distinct values and throwing on disagreement matches the function's purpose.
- `scripts/phase2-acceptance.ps1:184` — **nit** — the helper call sits inside the manifest `try`, so a missing constant is reported as an invalid `conversion-manifest.json`; hoist it or give it its own preflight name.
- **Merge note:** this branch's hunk rewrites the same line of `phase2-acceptance.ps1` that `phase2/material-publication` edits; keep this branch's version.
- I confirmed the regex matches the real source line, the search order is right, and the callers are loud (failed required preflight → exit 1 in acceptance; a propagating throw in the baseline).

## 12. `phase2/sun-shadow-cascades` — review-012-sun-shadow-cascades.1

- `crates/engine/src/app.rs:1317` — **should-fix** — the range is one ring short: `sun_shadow_cascades` uses `stream_radius + 1` cells (17,378 units at r = 2) while cells stay resident and drawn out to the **unload** radius (I read `config.rs`: `unload_radius: 3`, and `--stream-radius` sets `unload = stream + 1`), i.e. √2·4·4096 ≈ 23,170 units, and the camera's far plane is `4096·(stream_radius+2)·2 = 32,768` (`app.rs:1341-1344`, read), so the outer ring is visible and lit flat. The doc's "nothing beyond the grid is streamed to cast anything" is false.
- `crates/engine/src/app.rs:1365` — **should-fix (coverage)** — nothing tests that the config is actually attached to the sun: deleting the component and the `DirectionalLightShadowMap` insert from `setup_world` leaves both new tests passing, because they only call the helper.
- `crates/engine/src/app.rs:1302` — **should-fix** — the commit body defers the cost to a benchmark that was not run, and this repo gates frame time on recorded evidence: `docs/roadmap/02-profiling.md:54` fails a campaign on regressions over 10 % (read). Measure the synthetic scenario (which runs `setup_world`) before proposing.
- Nits confirmed: `CREATION_UNITS_PER_METRE = 70.0` vs the comment's "1.43 cm" (1.43 cm is 1/69.93 m); `maximum_distance` is derived horizontally while the acceptance-screenshot camera sits 20,000 units up (`app.rs:1335-1337`); `overlap_proportion: 0.2` is unjustified and only range-checked; `stream_radius` is an unvalidated `i32`, so `--stream-radius 100000` asks for gigametre cascades; the `DirectionalLightShadowMap` insert is a no-op at the current constant.
- The geometry checks I repeated agree: 0.1·70 = 7, 10·70 = 700, 4096·3·√2 ≈ 17,378, and Bevy's cited default (10/24.7/60.8/150) is the geometric split the tests assume.

---

## Cross-branch: conflicts, ordering, and the items that need a decision

1. **Schema 15 twice (blocking for the pair)** — `phase2/material-publication` and `phase2/renderable-base-types` each edit the same three lines (`converter/src/cache.rs`, `converter/src/pipeline.rs`, `engine/src/app.rs`) to the same value 15 and share the `12..=14 → 15` migration. Landing them as two upstream commits leaves the second branch's contract unreconverted; land together or renumber.
2. **`phase2/shared-converter-schema` moves the definition** — it deletes the line both bump branches edit and puts `= 14` in `crates/shared/src/lib.rs`. Merged last or resolved to "ours", the single definition reads **14**, the converter writes 15, and the engine plus the acceptance preflight reject every fresh asset set. The final tree must carry one definition at 15 in `shared`.
3. **`scripts/phase2-acceptance.ps1`** — `phase2/material-publication` and `phase2/scripts-schema-from-source` edit the same line; the source-reading version must win, and `phase2/pruned-texture-complete` silently changes what the neighbouring `conversion-complete` preflight means (the field it reads is still `manifest.complete`).
4. **`crates/engine/src/streaming.rs` (four branches)** — `terrain-seam-weld`, `terrain-samplers-weights`, `empty-model-bounds`, `refr-rotation` touch different functions and are semantically compatible (nothing else writes `heights`/`normals`; the weight field reads only `layers`/`VTXT`), but all four append to the same `mod tests` tail and add imports — expect textual conflicts only.
5. **`crates/engine/src/render.rs` (two branches)** — compatible (different functions); watch the duplicated `VertexAttributeValues` import when merging 3 and 7.
6. **`crates/converter/src/cache.rs` / `pipeline.rs` (four branches)** — `shared-converter-schema`, `material-publication`, `renderable-base-types`, `pruned-texture-complete` all touch the manifest literal, the constant and the migration; `pruned-texture-complete` adds a manifest field to a struct literal with no `..Default::default()`, so a merge that drops a field is a compile error (which is the safe direction).
7. **`crates/converter/src/mesh.rs` (`material-publication` × `lod-mesh-containers`)** — no overlap: the marker drop sits in `convert_nif_to_glb`, the LOD extension arm in `convert_kind`.
8. **Two decisions the authors cannot make separately:** (a) does LOD vertex-colour alpha mean anything — branch 7 erases what branch 6 deliberately exports; (b) a marker-only DOOR now streams as an invisible, unbounded reference while `renderable-base-types` still catalogues it — fine in Phase 2 (no collision/animation), but its editor-marker bounds were the door's only interaction volume; flag for Phase 4.
9. **Suggested landing order:** `shared-converter-schema` (with the constant taken to the current value) → the two schema-15 branches as one merge → `pruned-texture-complete` → `scripts-schema-from-source` → `lod-mesh-containers` → `vertex-alpha-not-opacity` (after the alpha decision) → the three `streaming.rs`/`render.rs` branches → `sun-shadow-cascades` (after its benchmark).

## Verdicts

| Branch | Verdict |
|---|---|
| `phase2/refr-rotation` | **fix first** — code verified correct; the doc's unverifiable measurement and the self-mirroring test should be fixed before proposing. |
| `phase2/shared-converter-schema` | **fix first** — clean change, but its merge wording must say the one definition is `crates/shared/src/lib.rs` and must end at 15. |
| `phase2/terrain-samplers-weights` | **fix first** — the sampler and the weight field are invisible to every no-game-data scene, and the WGSL has no test that can fail. |
| `phase2/terrain-seam-weld` | **fix first** — corner welds can leave a crack and double-count the metric; the 512-unit bound and the unreported counter need work. |
| `phase2/material-publication` | **fix first** — one test does not prove what it claims (`0x12EC`), the spec prose contradicts the new field, and it collides with two branches on the schema line. |
| `phase2/lod-mesh-containers` | **fix first** — a machine path and a fork-only env name in a test doc, no ADR-0005 sweeps, and the segment layout cannot fail loudly. |
| `phase2/vertex-alpha-not-opacity` | **needs discussion** — does LOD vertex alpha mean anything? Branch 6 exports it on purpose and this branch erases it. |
| `phase2/renderable-base-types` | **fix first** — the second schema-15 stamp (land with branch 5 or renumber), plus the asset-closure gate and the malformed-`MODL` abort. |
| `phase2/pruned-texture-complete` | **fix first** — the completeness contract in `docs/` still says the opposite, and a resumed run loses the record it exists to keep. |
| `phase2/empty-model-bounds` | **fix first** — the tolerance accepts any bounds-less primitive-less scene, and the new counter is visible nowhere. |
| `phase2/scripts-schema-from-source` | **ready** — does what it claims; the remaining items are nits plus one merge-line conflict. |
| `phase2/sun-shadow-cascades` | **fix first** — the cascades stop one ring short of the drawn world, nothing tests the wiring, and the cost is unmeasured against a documented 10 % gate. |

**What I could not do:** nothing requiring game data or Bevy sources (Bevy is not vendored in this worktree and the workers had no shell), so every claim about Bevy defaults, the `Skyrim.esm` statistics, and the `.btr`/`.bto` layout that rests on a measurement stays unverified here. Line numbers for branches whose files have moved on in the integration checkout come from the diff hunks and may be off by a few lines for `render.rs`, `streaming.rs`, `app.rs`, `cache.rs` and `pipeline.rs`; all others are exact. No files were changed by me or by any worker.


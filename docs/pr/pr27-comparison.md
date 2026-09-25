# PR #27 vs our staged branches — decision aid

Upstream PR #27 (`realfakenerd/OpenSkyrim`, author TaylorTurnerIT, `fix/white-pines-cutout-alpha`,
open, unreviewed, 0 comments from maintainers, mergeable/BLOCKED). Compared against our local
`phase2/renderable-base-types`, `phase2/vertex-alpha-not-opacity`, `phase2/shared-converter-schema`,
`phase2/material-publication` (worktree `C:/Modding/wkj-phase2`, all cut from `main`).
**Checked** = I read the actual diff/code or measured real data. **Inferred** = reasoned from that
evidence, not independently proven.

## 1. What #27 changes (checked, read the full diff)

- `esm/exporter.rs`: model extraction 3 base types → 19 (adds TREE/FLOR/CONT/DOOR/ACTI/LIGH/WEAP/
  MISC/BOOK/AMMO/ALCH/INGR/SLGM/KEYM/SCRL/ARMO); ARMO reads `MOD2` then `MOD3` (gendered slots); a
  type with no model gets a row with `model_path = NULL`, matching STAT/MSTT/FURN today.
- `mesh.rs`: `normalize_cutout_vertex_alpha()` forces `COLOR_0.w = 1.0`, but **only** on shapes whose
  material contract resolves to `NifAlphaMode::Cutout`; Blend/Opaque shapes are untouched.
- `cache.rs`/`pipeline.rs`: `CONVERTER_SCHEMA_VERSION` 14→15; migration arm widened `12|13`→`12..=14`.
- `app.rs`: mirrors the schema constant to 15; adds a sky-colour `ClearColor` + `DistanceFog` during
  `--acceptance-screenshot` only; screenshot gate now also requires `loading_cells==0 &&
  resident_cells>0`.
- `config.rs`: new `--screenshot-camera-offset x,y,z`.
- `streaming.rs`: `converted_model_path` now lowercases the **whole** path before matching (real bug
  fix — mixed-case NIF paths were failing to resolve); `plan_cells`/`validate_streaming_lifecycle`
  anchor streaming on the start cell, not the camera, during acceptance screenshots.
- `world/database.rs`: same full-lowercase fix for `converted_texture_path`.

## 2. The four overlapping areas

**Base types.** Ours (`renderable-base-types`) covers only DOOR/ACTI/FLOR/CONT/TREE/LIGH, all via
plain `MODL`, no ARMO, and silently skips the row when no model exists (inconsistent with
STAT/MSTT/FURN's always-insert-a-row behaviour). #27 is a strict superset: same 6 types plus 10 more
(placed weapons, books, potions, scrolls…) and correct ARMO handling. **#27 is more faithful**:
placed inventory objects and worn armor displays are common Skyrim set-dressing our branch leaves
invisible, and ARMO's `MOD2`/`MOD3` split is real Bethesda record structure. (Checked, both branches
read directly.)

**Vertex alpha / white pines.** #27 forces alpha=1.0 converter-side, scoped to Cutout shapes only —
destroys that data permanently, leaves Blend meshes alone. Ours forces it engine-side at mesh load —
**unconditionally, for every alpha mode** (checked: `force_opaque_vertex_colours` has no
`AlphaMode`/material check) — non-destructive but broader than the bug requires. I independently
measured real per-vertex alpha on `C:/Modding/SkyrimConverted-upstream15` (our own schema-15 GLBs,
untouched by either fix) with a hand-written GLB parser over all 429 tree/plant files plus a
3,000-file general sample. **#27's "244 of 25,391" count is corroborated** (~246 extrapolated), but
the "small, harmless" framing is not: concentrated in trees (26.3% of tree files vs 0.97%
corpus-wide), and where present it's large — 67.6% of an affected tree's vertices sit below 0.95
alpha, 49.2% below 0.5. Non-tree hits (dragon/falmer wings, banners, wheat) suggest this is a general
Creation Engine wind/fade parameter, not foliage-specific. Inferred (not proven): the gradient
correlates more with radial distance from a tree's trunk than height — a canopy-density fade, not a
simple wind ramp. Neither fix preserves the effect; #27 is more surgical (mode-scoped) but
destructive, ours is non-destructive but over-broad.

**Schema bump.** Mechanically identical (14→15, same migration-arm widening) in both. Our
`shared-converter-schema` branch also deduplicates the constant into `crates/shared`; #27 doesn't,
re-introducing the duplication our branch exists to close. Not a fidelity question.

**Screenshot/sky/fog.** #27 adds a real capability we don't have staged as a phase2 branch: fog,
sky backdrop, a settled-streaming gate, and start-cell anchoring during acceptance screenshots.
Checked: zero file overlap with any of our four branches (`phase2/render-check` and portal's
`atmosphere.rs` are much larger, differently-scoped, and not phase2 PR candidates). Clean adopt.

## 3. Conflicts if #27 merges first

| File | Conflict | Difficulty |
|---|---|---|
| `esm/exporter.rs` STAT/MSTT/FURN match arm | Direct, substantive — `renderable-base-types` becomes largely redundant | Moderate–high (design call, not mechanical) |
| `cache.rs` schema constant + migration arm, `pipeline.rs` compat arm | 3-way: #27, `renderable-base-types`, `material-publication` all make the same edit | Low (mechanical, same transform each time) |
| `mesh.rs`, inside `impl MeshConverter` conversion fn (~lines 84-89, 107-113) | `material-publication`'s edits sit at #27's insertion points for `normalize_cutout_vertex_alpha` | Moderate (textual conflict, semantically compatible) |
| `app.rs` `converter_schema_version()` line | Same line touched by both our schema-bump branches | Low |
| `streaming.rs`, `world/database.rs` | None — zero overlap with any of our branches | None (cherry-pickable today) |

Conceptual: once #27 merges, it already fixes the white-pine symptom, so
`vertex-alpha-not-opacity`'s marginal value shrinks to its unverified effect on Blend meshes.

## 4. Options

**(a) Rebase on #27, send only the gap.** Drop `renderable-base-types` (superseded); send
`material-publication` almost unmodified; send `shared-converter-schema` as a cleanup on top of #27;
narrow `vertex-alpha-not-opacity` to non-Cutout modes, or drop it pending evidence it's needed.
**(b) Review notes on #27.** Low effort, no integration risk, but our unrelated work keeps waiting.
**(c) Propose combining**, once the send-hold lifts: offer `material-publication` +
`shared-converter-schema` + the screenshot/fog idea as follow-ups on #27, flagging the base-types and
vertex-alpha overlap so the maintainer picks a side instead of merging two competing schema-15 PRs.

**Recommend (c).** #27 is unreviewed and unmerged, so there's room to coordinate. Its base-types and
vertex-alpha work is a genuine equal-or-better superset in most respects, not worth contesting; our
other branches have no real overlap and only stand to lose from racing a second, conflicting
schema-15 PR.

## 5. Adopt locally / flag as wrong

**Adopt regardless of outcome:** the full-lowercase path fix in `streaming.rs`/`database.rs` (real,
present bug on our `main` today, zero conflicts); ARMO's `MOD2`/`MOD3` handling and the 10 extra
record types; always-insert-a-row consistency for new base types; the settled-streaming screenshot
gate and start-cell anchoring.

**Worth flagging to the author:** #27 bumps the schema but never updates
`docs/roadmap/02-core-engine.md`'s Compatibility line (checked: still says "14"; #27 touches no
docs) — our send-order rule requires this. It re-duplicates `converter_schema_version()` in `app.rs`,
exactly what `shared-converter-schema` fixes. And its "Tradeoff" section undersells the loss ("wolf
fur cards render slightly fuller") — our measurement shows roughly half of affected tree-canopy
vertices below 0.5 alpha, a real silhouette/density change, not a subtle edge case — worth a note
even though it doesn't invalidate the fix.

## Lead's note (Claude, Phase 2 Dev)

Checked against our earlier research (research-059/impl-063): on tree NIFs the `COLOR_0` alpha is a
shader parameter (tree animation), not opacity. Skyrim only uses vertex alpha as opacity when the
shader's `SLSF1_VERTEX_ALPHA` flag is set *(corrected 2026-09-25: wrong; see the update below — it is
SLSF2_Vertex_Colors, minus Tree_Anim)*. So "half the canopy vertices below 0.5 alpha" is data
the game does not draw as transparency, and neither fix loses anything visible today. #27 still
throws that data away for the wind animation a later phase will want.

The faithful fix is neither PR's: the converter already records `shaderFlags1`, and the engine
should honour vertex alpha as opacity exactly where `SLSF1_VERTEX_ALPHA` is set, and ignore it
elsewhere. That is the concrete improvement to offer under option (c), whichever side the
maintainer picks. The call on sending is the user's, and the send hold still stands.

## Update 2026-09-25 (Upstream session): `phase2/vertex-alpha-faithful` supersedes ours

`phase2/vertex-alpha-faithful` (39c3497, local only, reviewed and approved by Portal Dev) replaces
`phase2/vertex-alpha-not-opacity`. It fixes the vertex-alpha question in the converter:

- The vendored exporter dropped vertex colours on every static BSTriShape, so every SSE-native
  shape lost them. It now keeps them.
- They are published per the shader's own rule. Without SLSF2_Vertex_Colors the colours are dropped.
  On a lighting shape with Tree_Anim the RGB is kept and the alpha is forced to 1 (it is wind
  amplitude). Everywhere else the alpha is kept as opacity.

Against #27: both are converter-side and both change the output, so both need a schema bump. #27
forces alpha to 1 on Cutout shapes. Ours decides by shader flag, so it keeps tree RGB shading and
effect-card alpha, which #27 would lose or never have. This is the faithful fix that option (c)
above asked for. The two PRs still conflict in `mesh.rs` and in the schema number.

Settled points (Phase 2 Dev, 2026-09-25):
- **The lead's note above is wrong on SLSF1_VERTEX_ALPHA; the branch's rule is right.** In Community
  Shaders' Lighting.hlsl the vertex colour reaches the pixel shader only under `VC` (otherwise
  `1.0.xxxx`), and the fragment multiplies alpha by `input.Color.w` except under TREE_ANIM /
  LODOBJECTS / LODOBJECTSHD. Effect.hlsl multiplies by `input.Color.w` under `VC`. Neither shader
  reads SLSF1_Vertex_Alpha. That `VC` is the technique bit SLSF2_Vertex_Colors selects is
  *inferred* from the flag names and CS's technique descriptors, not read from the executable. On
  real data the two rules never diverge: in a schema-22 scan of every GLB with alpha below 1 (trees
  and plants plus a 4,000-file sample) every shape had both SLSF1 bit 3 and SLSF2 bit 5, and the
  123 tree shapes also had Tree_Anim.
- The added `project-wormhole-shared` is a **dev-dependency** (one test, glam/BSVec4), not a
  runtime one; #27 adds the same line.
- #27's author is still holding it to build a test scene with every foliage and tree type (comment
  2026-09-24 18:34Z). Its head is unchanged since 6834e5e (2026-09-24 17:34Z).

## Caution 2026-09-25 13:05 (Upstream Dev)

Don't offer the vertex-alpha-faithful rule to #27 or upstream yet. On Portal Dev's schema-23
reconversion (the first with that branch), Whiterun's walls, rocks and road edges show see-through
patches, and the suspect is vertex alpha being published as opacity where the shader doesn't read
it (team log 13:05; `local/gallery/s23-check/`). The rule and its "never diverge on real data" scan
need rechecking by Phase 2 Dev first.

## Correction 2026-09-25 (Phase 2 Dev, confirmed on real data)

The schema-23 data disproved the vertex-alpha-faithful rule (39c3497, held, unsent): Whiterun's
alpha-tested (MASK) walls got holes because their vertex alpha (down to 0.5, even 0.08) went into
the alpha test, while Skyrim draws them solid. The corrected rule: vertex alpha is opacity only on
BLEND materials; MASK and opaque shapes get alpha 1; no SLSF2_Vertex_Colors drops the colours;
Tree_Anim still gives alpha 1. So **#27's "force alpha 1 on Cutout" is right** for alpha-tested
shapes, and our comparison above overstated the gap there. What our branch still adds over #27:
the BSTriShape vertex-colour export, and the fade on blended cards (the hearth flames).
The branch is fixed: `phase2/vertex-alpha-faithful` head **ee0e6d7** (on 39c3497; Upstream Dev
reviewed the diff: `alpha_mode != Blend` joins Tree_Anim in `ColourOnly`, and the model test adds
an alpha-tested VC shape that must get alpha 1). It still needs a converter schema bump before any
send, and the caution above stands until a reconversion with it shows Whiterun solid again.

**Recheck 2026-09-25 (Portal Dev, schema 25 with ee0e6d7): NOT passed.** Whiterun's walls and rocks
still show the same see-through patches (`local/gallery/s25-check/`), so the vertex-alpha rule may
not be the (only) cause; Phase 2 Dev is testing a depth-prepass hypothesis. The caution stands.

**Why schema 25 failed (Phase 2 Dev, later 2026-09-25): a merge, not the rule.** Portal merged the
bundle and then reverted it (22731d7); the later re-merge (72d9eb8 / b2b1395) silently dropped the
rule, so the s25 data was converted with schema 23's rule (rockpilem01.glb: MASK/opaque shapes with
vertex alpha 0..0.55, where d479310's converter gives 1.00). `vertex-alpha-faithful` ee0e6d7 is
unchanged and correct. Portal's fix is `bundle/portal-s26-fix` 319832f (schema 26; Upstream Dev
checked it has the BLEND-only line). Caution stands until the schema-26 recheck; then re-judge.

**Caution lifted 2026-09-25 (Portal Dev's schema-26 recheck passed).** Whiterun's walls and rocks
are solid (`local/gallery/s26-check/`; Upstream Dev looked at SR-place-Whiterun). The rule to offer
is ee0e6d7's: vertex alpha is opacity only on BLEND; MASK/opaque and Tree_Anim get 1; no VC flag
drops the colours. It agrees with #27 on cutout shapes; what we would add is the BSTriShape
vertex-colour export and the blended cards' fade. Any comment on #27 is to a human, so it goes
to the user as a previewed draft first.

**Refinement in progress (Phase 2 Dev, later 2026-09-25): don't offer ee0e6d7's rule as final.**
websearch-578: Community Shaders' Lighting.hlsl multiplies `input.Color.w` into alpha *before* the
`DO_ALPHA_TEST` discard, so vertex alpha does reach the alpha test on some lighting shapes.
research-578: `SLSF1_Vertex_Alpha` (bit 3) separates them (gravel skirts set, 6/6; Whiterun wall caps,
fence bases, rugs clear, 6/6). Candidate rule (`phase2/vertex-alpha-flag`, not ready): lighting
shapes use vertex alpha as opacity iff `SLSF1_Vertex_Alpha && !Tree_Anim`; effect rule unchanged.
Plus an engine fix (`phase2/prepass-vertex-alpha`): Bevy 0.19's prepass alpha discard ignores
COLOR_0. That makes #27's blanket "alpha 1 on Cutout" too strict for SLSF1_Vertex_Alpha shapes;
re-judge when both branches pass their GPU check.

**2026-09-25 13:20Z: HazielMagallanes reviewed #27** (approve in his opinion). His design note 2:
forcing vertex alpha to 1 on every Cutout shape touches 244 GLBs (wolf fur cards and similar), not
just pine needles; he suggests an ADR/issue and scoping the normalization by material semantics. That
is what our candidate rule does (`SLSF1_Vertex_Alpha`, `vertex-alpha-flag`), so a comment on #27 has
a natural opening once the s27 check passes. Any such comment is to humans: the user decides.

**2026-09-26: schema-27 recheck passed (Portal Dev; `local/gallery/s27-check/`).** With
`vertex-alpha-flag` + `prepass-vertex-alpha`: Whiterun's walls solid, gravel skirts without sky rings,
prepass patch active (Upstream Dev looked at SR-place-Whiterun and cap-07). **The rule to offer is
the flag rule** (vertex alpha is opacity on lighting shapes iff SLSF1_Vertex_Alpha && !Tree_Anim;
effect shapes when blended), sent as two PRs: prepass first, then the converter rule with its schema
bump. Against #27: #27's blanket Cutout -> alpha 1 is right for the 68 solid shapes and wrong for
the 262 that fade (fact sheet D), which answers Haziel's design note 2. A #27 comment is the user's call.

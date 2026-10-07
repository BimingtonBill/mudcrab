# research-059 - The forest is invisible: alpha test versus alpha blend

## What was seen

`local/reference/rw_sweep/_crop-tree.png` (cropped from
`local/reference/rw_sweep/A-village-19000--48800-900.png`, an aerial view of Riverwood) shows the
pine trees rendering as **a spray of scattered dots against the sky**. The trunks are solid; the
needles are almost entirely gone. The same speckled dissolve eats the edges of roof eaves, thatch
and stone walls in the same frame. Compare `local/reference/riverwood/SR-place-Riverwood.jpg`,
where the pines are dense and dark.

Riverwood is a forest village: `docs/design/riverwood-demo.md` section 4 counts **189
`Landscape\Trees\*` references** and **1,021 `Landscape\Plants\*`** in the streamed box. All of
them convert, all have valid bounds, and on screen they are barely there. This is the largest
visual gap the Riverwood renders have shown, and it is not in `docs/research/visual-gaps-spec.md`.

## The hypothesis to test - and to try to falsify

Skyrim's foliage uses **alpha testing**: `NiAlphaProperty` carries a threshold and a test-enable
flag, and a leaf texel is either drawn or discarded. `crates/engine/src/render.rs:662-664` maps
Skyrim's blend state to Bevy's `AlphaMode`, and its fallback is:

```rust
Self::StraightAlpha | Self::Unsupported => AlphaMode::Blend,
```

If an alpha-*tested* material reaches that arm, Bevy blends it instead of discarding. With
mip-mapped leaf textures, whose alpha averages toward zero as the mip level rises, a blended leaf
card fades to nearly nothing at any distance - which is exactly the picture. `Msaa::Off` is set
everywhere (`app.rs:595`, `844`, `1011`, `1216`, `1970`, `portal.rs:702`), so alpha-to-coverage
dithering is **not** the explanation; do not spend time on it beyond confirming that.

**Try to break this hypothesis, not to confirm it.** If the data shows foliage already arrives as
`Mask`, say so and find the real cause; a wrong fix here could cost a full reconversion.

## What to establish

1. **Does the converter read `NiAlphaProperty`'s test flag and threshold at all?** Find where NIF
   alpha state is parsed in `crates/converter/`, and what it writes into the glTF material
   (`alphaMode` `OPAQUE`/`MASK`/`BLEND` and `alphaCutoff`). Cite `path:line`. Note that a previous
   fix in this area exists - commit `b4ce0d9`, "vertex alpha no longer means blend" - read it, so
   you do not re-tread or undo it.
2. **What the real assets carry.** For `Landscape\Trees\TreePineForest03.nif` and at least three
   more of the tree and plant models Riverwood actually places (get the list from
   `docs/design/riverwood-demo.md` section 4, resolve paths in
   `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db`), report from the **source NIF**: the
   `NiAlphaProperty` flags word, the decoded test-enable bit, the threshold byte, and the blend
   factors. The BSA reader is `crates/converter/src/archive/bsa.rs`.
3. **What the converted glb carries.** For the same models, the `alphaMode` and `alphaCutoff` in
   the glb's JSON chunk under `C:/Modding/Mudcrab/SkyrimConverted/meshes`. A table of model → NIF flags →
   glb alphaMode/cutoff is the core deliverable.
4. **What the engine does with it.** Follow the glb `alphaMode` through `render.rs` and the
   `CanonicalMaterialKind` checks at `app.rs:650-670` to the `StandardMaterial` that is finally
   rendered. Does anything override it? Cite `path:line`.
5. **Scale.** How many distinct converted materials across the whole dataset carry a NIF alpha
   test, and how many of those end up `BLEND` in the glb? This number decides whether the fix is
   worth a reconversion.
6. **The fix, and where it belongs.** Say whether the correct threshold can be recovered
   **engine-side** from data already in the glb (in which case no reconversion is needed), or only
   by re-reading the NIF (in which case it is a converter change plus a reconversion). Give the
   arithmetic for Skyrim's threshold byte to Bevy's `alphaCutoff`. Name the two or three
   `rw_sweep` or `rw1` frames that would show the fix working, and say what a correct render looks
   like.

## Web lookups

You may search for the `NiAlphaProperty` flag-bit layout and how the threshold byte is interpreted.
Cite what you use; copy no code from other projects.

## Deliverable

`docs/research/foliage-alpha-test.md`. Lead with a two-sentence answer to "why are the trees
invisible", then the tables, then the recommendation and where it belongs. **Do not change any
code in `crates/`.**

## Owned files

- `docs/research/foliage-alpha-test.md`
- `local/research-059/` - scratch scripts and dumps

## Out of scope

Do not render anything (the lead has the GPU) and do not touch the shots files. Do not touch
`docs/research/specular-gloss-mapping.md`: research-058 is writing it right now, on the separate
question of why surfaces look wet. If you find the two problems share a root cause, say so in your
own document rather than editing theirs.

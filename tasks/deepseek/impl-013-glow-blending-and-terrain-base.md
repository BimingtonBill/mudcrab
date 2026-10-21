# impl-013 - Engine: additive/multiplicative glows, textureless terrain bases, clean door labels

Owned files: crates/engine/src/render.rs,crates/engine/src/streaming.rs,crates/engine/src/world/database.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md` first. Your checkout's
`target/` is seeded from the lead's build. **You cannot compile shaders here, and Bevy only compiles
WGSL at runtime** - if you touch a `.wgsl` file, keep to constructs already used in it and state it in
your result; prefer solving these in Rust.

## 1. Glows: honour the converter's blend factors

The converter (impl-012, merged in `b4ce0d9`) now writes, for every material published as glTF
`alphaMode: BLEND`, two fields in its `OPEN_SKYRIM_material` extension: `blendSource` and
`blendDestination`, spelled as nif.xml's `AlphaFunction` (`SRC_ALPHA`, `INV_SRC_ALPHA`, `ONE`, `ZERO`,
`DEST_COLOR`, ...). See `crates/converter/src/material.rs` around line 703 for exactly what is written.
Additive `SRC_ALPHA`/`ONE` (4,335 blocks: torch and Dwemer lantern glow cards, e.g. torch `GlowAddMesh`)
and multiplicative (435 blocks) currently render as ordinary alpha-over, i.e. grey veils.

Make the engine read those two fields and set the loaded `StandardMaterial`'s `alpha_mode`:
`SRC_ALPHA`/`ONE` (and `ONE`/`ONE`) -> `AlphaMode::Add`; `DEST_COLOR`/`ZERO` or `ZERO`/`SRC_COLOR` ->
`AlphaMode::Multiply`; `SRC_ALPHA`/`INV_SRC_ALPHA` stays `Blend`; anything else stays `Blend` with a
one-time `warn!` naming the pair. Bevy 0.19's glTF loader can hand you material extensions through
`GltfExtensionHandlers` (see `bevy_gltf-0.19.0/src/lib.rs` around lines 161, 278 and 296, and
`src/loader/mod.rs` around 161 and 1526 in the cargo registry), or read the glTF JSON yourself if that
is simpler - check the real API; do not guess it. Wherever the engine already validates or adjusts
streamed materials (`streaming.rs`, `validate_standard_material` and the material setup path), make
sure the new modes pass validation. Put the handler in `render.rs` if it needs registering on the app.

## 2. Terrain quadrants with no base texture

Some `LAND` quadrants have no base texture (`BTXT` missing; the converter synthesizes
`texture_form_id == 0`, `crates/converter/src/esm/cell_cache.rs` around line 245). They render their
base weight against Bevy's 1x1 white fallback and show as blocky white squares (Tamriel (0,0) quadrant
1, (4,-21) quadrant 2; see the white patch in
`C:/Modding/MudcrabProject/local/demo/rural-top.png`, which you can `Read`). The lead's design
decision: such a quadrant **borrows the base texture of a neighbouring quadrant in the same cell** (the
first of the other three that has one, in quadrant order); if none has one, it uses its own
highest-weight overlay layer's texture as the base. The terrain material setup is in `render.rs`
(`from_quadrant`) and the quadrant data comes through `streaming.rs`.

## 3. Door labels end in a NUL

Door labels read `Alftand01` + an invisible trailing character (the prompt shows `Alftand01□`): the
destination cell's `interior_name` from the database carries a trailing NUL. Trim trailing NULs and
whitespace where the label is built (`world/database.rs`, the door query that selects `interior_name` /
worldspace `editor_id`).

## Tests (each must fail before your change)

- Blend factor mapping: each pair above maps to the expected `AlphaMode`.
- A material carrying `blendSource: SRC_ALPHA, blendDestination: ONE` ends up `AlphaMode::Add` through
  whatever load path you implement (a unit test of the extension parsing plus the mapping is fine).
- A quadrant with base `texture_form_id == 0` and a textured sibling gets the sibling's base; with no
  textured sibling it gets its strongest overlay.
- A label `"Alftand01\0"` comes out `"Alftand01"`.

## Scope

Only the three owned files. No crosstalk partner. Report per the implementation template, including
which glTF extension path you used and why.

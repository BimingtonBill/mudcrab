# impl-170 - Effect shaders colour their greyscale texture through their palette

**Done by Claude instead (`b54dced`, 2026-09-24): do not launch.** The spend pace allowed only low-effort workers that night.

Owned files: crates/engine/src/effect_palette.rs,crates/engine/src/shaders/effect_palette.wgsl,crates/engine/src/render.rs,crates/engine/src/streaming.rs,crates/engine/src/lib.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal (the engine the demo renders with). Look-gaps item 3 (`docs/research/look-gaps-2026-09-24.md`): every hearth fire, candle flame and light shaft renders as a white-grey shape. Cause, found by render on the converted assets: Skyrim's effect textures are **greyscale** (`textures/effects/fxfireatlas04`), and the game colours them through a second **palette** texture. No engine applies the palette, so fire comes out white.

## What the converted data carries (schema 20, `C:\Modding\Mudcrab\SkyrimConverted`, read-only)

An effect-shader material (`BSEffectShaderProperty`) in a GLB, e.g. `meshes/clutter/woodfires/fireplacewood01burning.glb`, material `Flames:0`:
- `extensions.OPEN_SKYRIM_material`: `"shaderFamily": "effect"`, `"shaderFlags1": 3221225592` (0xC0000078), `"textureSlots": [{"semantic": "greyscale", "slot": 1, "texture": 1, "colorSpace": "linear"}]` - `texture` is the glTF texture index of the palette.
- `emissiveTexture` = the greyscale source texture; `emissiveFactor` = the effect's base colour RGB (grey 0.22 for this fire); `KHR_materials_emissive_strength` = its base colour scale (1.75); `baseColorFactor` = `[0, 0, 0, alpha]` (coverage alpha kept); blend pair in the extension (`blendSource`/`blendDestination`), which `render.rs` already turns into an `AlphaMode`.
- `ShaderFlags1` bit 4 (0x10) = greyscale-to-palette **colour**, bit 5 (0x20) = greyscale-to-palette **alpha**.

## The rule (from a web search, websearch-169; Community Shaders' `Effect.hlsl` is the code source)

- Colour (bit 4): `rgb = palette.sample(u = source.g, v = baseColor.r).rgb * baseColorScale`, where `baseColor.r` is the material's base colour red (published here as `emissiveFactor[0]`) times the vertex colour red if the mesh has vertex colours. The palette result **replaces** the greyscale texture's colour; it is not multiplied by it.
- Alpha (bit 5): `alpha = palette.sample(u = source.a, v = baseColor.a).a`.
- Without bit 4 the material keeps today's path (texture times emissive factor).
- Sample the palette with clamp-to-edge and linear filtering.

## What to build

1. A `MaterialExtension` on `StandardMaterial` in the new `effect_palette.rs` + `shaders/effect_palette.wgsl`, following the existing snow extension exactly (`crates/engine/src/snow.rs`, `crates/engine/src/shaders/snow.wgsl`, registered in `render.rs` with `MaterialPlugin`). It binds the palette texture and a small uniform (the two flags, base colour red and alpha, base colour scale) and computes the emissive as above, then Bevy's usual lighting and output.
2. In `render.rs`'s `SkyrimMaterialHandler::on_material`, for an effect material with bit 4 or 5 and a `greyscale` slot: get a `Handle<Image>` for the palette texture (from the glTF texture index, through the load context) and record, for that material's `StandardMaterial` label, what the swap needs (palette handle, flags, factors). Choose how the record reaches the swap (a labelled asset, a resource keyed by material asset id, or a component); explain the choice.
3. Swap the mesh's `MeshMaterial3d<StandardMaterial>` onto the extended material **after** the scene is validated, exactly as `streaming.rs` does for snow (`DirectionalSnow` + `SnowMaterialCache`, around the system that takes `snow_materials: Option<ResMut<Assets<SnowMaterial>>>`): the readiness pass validates `StandardMaterial`s, so the swap must come after it. Cache one extended material per base material.
4. Effect materials must not get `EMISSIVE_EXPOSURE` (x100) any more. That scale is for lighting-shader glows (`is_deliberate_glow` in `render.rs`); give effect materials their own constant `EFFECT_EMISSIVE_EXPOSURE` in `effect_palette.rs` (not `lights.rs`, which is not yours), start it at 10.0, and say in the report that it needs tuning by render.

## Checks

- Unit tests: flag parsing (bit 4, bit 5, neither), the palette slot lookup from the extension JSON, the uniform values, and the swap in a small Bevy `App` (as `streaming.rs`'s `a_validated_scene_is_swapped_onto_the_snow_material` does for snow).
- `cargo build --profile quick --bin engine`, then render the fire shots: `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --shots tasks/deepseek/impl-170-fire-shots.json --shots-out local/t170`. Look at `RW-09-inn-common-room.png` and `SR-interior-Candlehearth_Hall.png` in `local/t170` and say what colour the hearth fire is now. (Read only `local/t170` under `local/`.)
- Everything else draws as before: a non-effect material and an effect material without the palette flags are not swapped.

## Report

Files changed and why; how the palette reaches the swap; test names; the fire's colour in the two renders; anything left.

# Goal

Explain why Blackreach's glowing plants render burnt-out white in the portal engine while Skyrim shows them a saturated blue-green, and propose the smallest rule change that fixes it. Research only: change no file.

# Context

Track: portal. Project C:\Modding\MudcrabProject (Rust, Bevy 0.19). Look-gaps item 8 (`docs/research/look-gaps-2026-09-24.md`): on the three Blackreach reference shots our render has up to 17 % "flat white" pixels (bright and unsaturated) where Skyrim keeps the mushroom caps and hanging plants a saturated teal.

The glow rule lives in `crates/engine/src/render.rs`: `is_deliberate_glow` (a material whose `KHR_materials_emissive_strength` is above 1) gets its emissive multiplied by `EMISSIVE_EXPOSURE` (100, `crates/engine/src/lights.rs`, with a long doc comment on how it was fitted). Effect-shader materials (`shaderFamily` "effect" in the `OPEN_SKYRIM_material` extension) with the greyscale-palette flags now go through `crates/engine/src/effect_palette.rs` instead; the rest still take the x100. The camera has HDR, bloom (`Bloom::NATURAL`) and Bevy's default tonemapper (see `crates/engine/src/app.rs`, search for `Bloom`).

The converted models are in `C:\Modding\Mudcrab\SkyrimConverted\meshes\` (read-only GLB files; the JSON chunk is readable text at the start of each file). Blackreach's big mushrooms are `meshes/dungeons/dwemer/blackreach/...` or `meshes/plants/...` - search for names containing `blackreach`, `mushroom`, `glow`. For each glowing material, read its `emissiveFactor`, `emissiveTexture`, `KHR_materials_emissive_strength`, `baseColorTexture` and the `OPEN_SKYRIM_material` extension (`shaderFamily`, `shaderFlags1`, `shaderFlags2`).

Do not read anything under `C:\Modding\MudcrabProject\local\` except `local\reference\look-gaps\pairs-uesp_shots_4x3\` (side-by-side pictures, Skyrim left, ours right: look at the three `SR-place-Blackreach*` / `SR-interior-Blackreach*` files there, whatever their exact names).

# Done when

1. You list the Blackreach glow materials (model path, material name, the fields above).
2. You explain which of these produce the white: an emissive colour that is near white or grey (the colour lives in the texture), the x100 scale pushing a coloured texture past the tonemapper's white point, bloom, or something else. Use numbers: e.g. emissive factor times strength times 100 against what the tonemapper maps to white.
3. You propose one or two rules (a value or a condition, not code), e.g. a lower scale for lighting-shader glows whose emissive texture is coloured, or scaling by the texture's own brightness, and say what each would do to the Riverwood windows and the Alftand ice (Own_Emit surfaces the current rule deliberately leaves alone).

# Report

Markdown with headings: Materials, Why white, Proposed rules. Cite file paths (and line numbers for code).

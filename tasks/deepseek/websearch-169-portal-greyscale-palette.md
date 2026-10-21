# Goal

Find exactly how Skyrim (Special Edition) colours an effect-shader texture through its "greyscale to palette" texture, so an engine can reproduce it in a fragment shader.

# Context

Skyrim's `BSEffectShaderProperty` (used for fire, smoke, magic, light shafts) can carry a "greyscale texture" (palette), enabled by a shader flag commonly called `SLSF1_Greyscale_To_PaletteColor` and/or `SLSF1_Greyscale_To_PaletteAlpha`. The fire texture `textures/effects/fxfireatlas04.dds` is a greyscale flame atlas; the palette texture turns grey values into fire colours. We need the precise lookup rule.

Questions:
1. Which channel of the source texture is the lookup value (red, luminance, alpha)? Is it the texture colour alone or the texture colour times the vertex colour / base colour first?
2. What are the palette texture's u and v coordinates? (u = grey value? v fixed at 0.5, or v = something else such as the base colour's alpha or a "palette scale"?)
3. What do the two flags `Greyscale_To_PaletteColor` and `Greyscale_To_PaletteAlpha` each do, and which bits are they in `ShaderFlags1` (SSE)?
4. How do `base_color`, `base_color_scale` and the falloff (start/stop angle and opacity) combine with the palette result into the final colour and alpha?
5. Any known reimplementations (NifSkope's renderer shaders, Community Shaders, OpenMW, xEdit/nif.xml docs) that show the formula in code - quote the relevant lines' meaning (not long code).

# Report

A short list of answers to 1-5. For each: the rule, the source URL(s), how many independent sources agree, and your confidence. Say clearly where sources disagree or where something is unknown.

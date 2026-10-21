# What Skyrim's normal-map alpha scales

**Status:** websearch-060, 2026-09-23. Answered from the web by a DeepSeek websearch worker; the
citations below were fetched and read by it, and this page is the lead's filing of its report.
Nothing here was measured locally.

**The answer: the normal map's alpha multiplies the specular *intensity*, never the exponent.**
Confidence high. Skyrim's vanilla lighting shader computes

```
pow(saturate(dot(N, H)), shininess) * specularColor * specularStrength * normalMapAlpha
```

so the mask dims the highlight without widening it. In glTF terms the mask can be published as
`KHR_materials_specular.specularTexture` pointing at **the normal map's own texture index** -
glTF samples that texture's alpha channel, which is exactly the mask. No roughness map has to be
derived and no texture re-encoded. This settles the open question in
[`specular-gloss-mapping.md`](specular-gloss-mapping.md) section 5.2 in favour of its cheap route.

## The naming trap

The word "glossiness" means two different things either side of the boundary, which is why forum
guides are unreliable here:

| Where | Name | What it is |
|---|---|---|
| `BSLightingShaderProperty` (the NIF, what the converter reads) | `Glossiness` | the **exponent** - "inverse width of the specular highlight. Higher values are narrower" |
| the vanilla shader | `Shininess` (`SpecularColor.w`) | the same exponent, after upload |
| the vanilla shader | `glossiness` (`normal.w`) | the **mask** sampled from the normal map's alpha - a level, not a width |

So the NIF field the converter maps to roughness really is the exponent, and the shader variable
that shares the name is a different quantity. Community Shaders' own wiki says it outright: the
alpha gives "variable highlight intensity (but not highlight size)", and "the specular level is
named 'glossiness' in the shader, but it does not behave as glossiness".

## Sources

| # | Source | Weight |
|---|---|---|
| 1 | Community Shaders, `package/Shaders/Common/LightingEval.hlsli` - `VanillaSpecular(context, material.Shininess, ...) * material.SpecularColor * material.Glossiness * ...`; the exponent parameter is untouched and the mask appears only as a multiplier | Highest reached: a decompilation-derived reimplementation whose non-PBR path exists to reproduce vanilla |
| 2 | Community Shaders, `package/Shaders/Lighting.hlsl` - `// normal in xyz, glossiness in w if not modelspacenormal`, `glossiness = normal.w;`, `float shininess = SpecularColor.w;` | Same class as #1; this is what proves the mask is the alpha |
| 3 | Community Shaders wiki, "Shading introduction" - "variable highlight intensity (but not highlight size)" | The same reverse-engineers, in prose |
| 4 | Beyond Skyrim / Arcane University, *Texture Slots* - slot 1 alpha is the specular map, "black is minimum intensity (no specular highlight)", BC7 in SSE "because of the importance of the alpha layer" | Community wiki, **independent** of #1-#3, and agrees |
| 5 | Beyond Skyrim / Arcane University, *BSLightingShaderProperty* - Glossiness is "inverse width of the specular highlight"; Specular Strength is a "multiplier on specular highlight intensity" | Same, and it is what fixes the naming above |

No source anywhere claimed the alpha scales the exponent, and the worker found none that disagreed.

## Caveats worth carrying

- **No published decompiled vanilla SSE listing exists.** #1-#3 are a reimplementation by people
  who disassembled the original; #4-#5 corroborate the parameter semantics but not the arithmetic.
  If Community Shaders' non-PBR path deviates from vanilla, the answer could move. Nothing found
  suggests it does.
- **Model-space-normal meshes** take the mask from **texture slot 7's R channel** instead of the
  normal map's alpha. Any implementation has to branch on that flag.
- **Meshes without `SLSF1_Specular`** get `shininess = 0` - no specular term at all.
- **Landscape** blends a per-layer `LandscapeTextureXIsSpecPower` for the exponent rather than
  taking one from a shader property.
- **The eye shader was not checked.**
- Two claims are the worker's own inference from the fetched source, not anything a source states:
  that `specular_strength` and the mask combine purely multiplicatively (the cbuffer has one
  `float4 SpecularColor` and no separate strength constant, so strength is folded into the colour
  before upload), and that in SSE the mask also dims cubemap reflections.
- **NifSkope's preview is not evidence about the engine.** It binds strength and glossiness to
  separate uniforms and is known not to reproduce Skyrim's behaviour.

## If it is ever worth settling locally

Take one mesh with `SLSF1_Specular`, no environment mapping, glossiness 100-200 and strength 1.
Write three copies of its `_n.dds` with identical RGB and alpha 0 / 128 / 255. Same camera, same
single light, ENB off, one screenshot each, then measure a luminance cross-section of the
highlight. Intensity-scaling gives the same lobe width at about half the peak for alpha 128;
exponent-scaling would show a measurably wider lobe after normalising the peak.

# websearch-075 - Environment maps in glTF and Bevy 0.19 (external facts only)

## Goal

Answer four external questions with sources and dates. You cannot see any files; everything you need
is in this brief. Prefer primary sources (Khronos registry, Bevy docs and repository, nif.xml, UESP,
NifSkope docs) and check every link you report.

## Context

A Rust/Bevy project exports Skyrim assets to **glTF 2.0** (`.glb` plus `.ktx2` textures, Basis
Universal/UASTC) and loads them with `bevy_gltf` on **Bevy 0.19**. Skyrim materials carry an
**environment cube** (a DDS cubemap), an environment mask texture and a scalar "environment map
scale". The exporter already writes the cubemap as a KTX2 cubemap and lists it in its own vendor
extension on the material; nothing binds environment lighting at runtime yet. The question is what
the right published representation is.

## Questions

1. **glTF 2.0 and per-material environment maps.** As of 2026, is there any ratified or de-facto
   extension that attaches a reflection/environment cube map to a glTF *material* - past or present
   (`KHR_materials_common`, `KHR_materials_pbrSpecularGlossiness`, any `EXT_*` reflection/IBL
   extension, `MSFT_*`)? If none, say so plainly and cite the Khronos extension registry. Then: what
   do mainstream exporters (Blender's glTF exporter especially, and 3ds Max/Substance) do with an
   environment cube, and what do engines do instead (scene-level IBL and probes)?
2. **Bevy 0.19 environment lighting.** Which APIs and components exist (`EnvironmentMapLight`,
   `LightProbe`, `ReflectionProbe`, `IrradianceVolume`, anything else)? Which can be attached to an
   individual entity or material rather than only to a camera? Can a glTF asset carry any of them
   (through glTF extensions or otherwise), or must the application attach them at load time? Cite the
   0.19 release notes/docs/source with dates.
3. **KTX2 cubemaps in Bevy.** What does Bevy's KTX2 loader require for a texture to be usable as an
   environment map: `faceCount: 6`, mip levels present, UASTC/Basis compression versus uncompressed,
   transfer function and colour space (linear vs sRGB)? Does an uncompressed (non-supercompressed)
   KTX2 cubemap load? Cite `bevy_image`/`bevy_gltf`/the `ktx2` crate documentation or source.
4. **Skyrim's environment map scale.** What does the Creation Kit's "Envmap Scale" on a
   `BSLightingShaderProperty` actually do, where does it sit in the block, and what values are
   typical? Are Skyrim's cubemaps stored as prefiltered mip chains or a single blurred cube, and is
   the cube sampled with a roughness-derived mip level? Sources: nif.xml, UESP's NIF/shader
   documentation, NifSkope, or the Skyrim modding wikis.

## Rules

- Name the source, its date and the URL for every claim; mark anything you could not verify.
- Say explicitly when a question has no answer you could find rather than filling the gap.
- Do not speculate about the exporting project; answer the general questions only.

## Done when

Four sections, one per question, each with the answer, sources with dates, and a confidence line,
followed by a short "contradictions and unknowns" list.

## Report

Markdown: the four sections, then contradictions/unknowns. Keep it under two pages.

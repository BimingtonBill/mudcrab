# websearch-060 - What Skyrim's normal-map alpha channel actually scales

## The question

In Skyrim (Creation Engine / Gamebryo, `BSLightingShaderProperty`), the **alpha channel of the
normal map** (`_n.dds`, usually DXT5) is widely described as a "specular mask" or "gloss map".
We need to know, precisely, **which term of the specular equation it multiplies**:

- **(A)** the specular *intensity* - i.e. `spec = pow(N·H, glossiness) * specular_strength *
  specular_color * mask`, so the mask only dims the highlight; or
- **(B)** the *glossiness exponent* - i.e. the effective exponent is `glossiness * mask` (or some
  remap of it), so the mask also widens the highlight; or
- **(C)** something else, or both, or it differs between shader types.

Skyrim SE specifically (the 64-bit 2016 remaster), not Oblivion, Fallout or Skyrim VR, though a
source that says the behaviour is shared is useful.

## Why it matters here

We convert Skyrim materials to glTF PBR. If the answer is **(A)**, the mask can be published as a
glTF `KHR_materials_specular.specularTexture` pointing at the normal map's own texture index -
cheap, no texture re-encoding. If it is **(B)**, it has to become a roughness map, which means
deriving and encoding a new texture per normal map. The two cost very different amounts, so we
want the answer before we write the code.

## What to look for

Good sources, roughly in order of weight:

1. Reverse-engineered shader documentation or decompiled shader code for Skyrim's lighting shader
   (the community has disassembled these; the SSE Shader Tools / Community Shaders / ENB
   developer notes are the sort of thing that carries it).
2. NifSkope / NifTools documentation or source on `BSLightingShaderProperty` and how it previews
   the specular.
3. Bethesda modding documentation on authoring `_n.dds` alpha, especially anything that says what
   happens visually when the alpha is set to black or white - a description of the *effect*
   settles it: if a black alpha kills the highlight entirely but a grey one gives a highlight of
   the same size and lower brightness, that is (A); if the highlight also gets broader, that is (B).
4. Other open reimplementations' handling (OpenMW's Bethesda support, SkyrimSE-Re, xEdit's
   material docs).

Beware of sources that conflate Skyrim with Oblivion or Fallout 3, and of forum posts that
describe the *authoring* convention ("paint it dark for matte") without saying which term it hits -
that phrasing is compatible with both (A) and (B).

## Deliverable

**Your report is the deliverable** - you cannot see or write this project's files, so answer in
the report itself and the lead will file it. Structure it as:

- The answer: **A**, **B**, **C**, or "the sources disagree" - with a confidence word.
- Two to five citations, each a URL plus the sentence or code line that carries the claim, and a
  word on how authoritative you judge it (decompiled shader beats wiki beats forum post).
- If sources disagree or none is authoritative, say so plainly and propose the cheapest
  **experiment** we could run locally to settle it - we have Skyrim SE installed and can render
  the same mesh both ways and compare.
- Anything you learn about whether `specular_strength` and the mask are combined multiplicatively,
  and about the range the mask is expected in.

**Do not** report a confident answer you cannot cite. "The sources disagree, here is the
experiment" is a complete and useful result.

## Out of scope

No code changes and no repository analysis - this is a web question, and you have no access to
the project's files in any case. Do not reproduce code from other projects; cite it and describe
what it does.

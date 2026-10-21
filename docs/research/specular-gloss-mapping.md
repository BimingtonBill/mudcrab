# research-058 — Why every surface reads wet: the specular/gloss mapping

**Answer, in two sentences.** The sheen is the converter's glossiness → roughness mapping:
`roughnessFactor = 1 - glossiness/100` (`crates/converter/src/material.rs:516`) reads Skyrim's Blinn-Phong
*specular exponent* as a linear percentage, so glossiness 80 — the value the farmhouse timber, the porch
posts, the thatch and the barrel all carry — becomes `perceptual_roughness 0.2` (the Blinn-Phong
equivalent of an exponent of ≈1,250), and the 14,825 materials at glossiness ≥ 100 become `0.0`, which Bevy
clamps to `0.089²` — a mirror. Nothing downstream corrects it — `metallic` is 0.0 on every one of the 59,268
published materials, no material carries a roughness map, and the engine passes roughness through untouched
— so the rest of the story is what is *missing*: Skyrim multiplies that specular by the gloss mask in the
normal map's alpha (a DXT5 alpha of 0.06–0.14 on the posts and the barrel), which the converter drops.

The data is present and correctly read; the *mapping* is wrong, and one term of the input is discarded.
This is a converter-side defect in one expression, not a parsing bug, and it affects every render the
project has made.

---

## 0. Method, and what this worker could not do

Harness limits, stated up front because they bound the evidence:

- **Denied by the sandbox:** `python` (inline and script), `sqlite3`, `dd`, all built binaries
  (`converter.exe`, `glb-audit.exe`, `world-inspect.exe`), loops and command substitution. **Read-only
  shell** (`od`, `grep`, `head`, `tail`, `tr`, `find`, `ls`) plus Read/Grep/Glob worked throughout.
- **Consequence 1:** the brief's "find the barrel in `skyrim_world.db`" could not be done. The barrel below
  is `C:/Modding/SkyrimConverted/meshes/clutter/barrel01.glb`, the village's common barrel; its material is
  representative of the `clutter/` set. Every model I could measure independently (exterior, interior,
  Dwemer) lands in the same three roughness buckets, so the identification does not change the conclusion.
- **Consequence 2:** no web lookup is available to this worker (no web tool, no sibling named in the brief).
  The Blinn-Phong ↔ GGX relation and the *semantics* of the normal map's alpha are therefore argued from
  local code and bytes only, and §8 lists exactly what a websearch worker should confirm.
- **What I did instead:** GLB JSON chunks were extracted with `grep -a -b` (byte offsets) +
  `head -c | tail -c +N | tr ',' '\n'`, and NIF bytes with `od` at computed offsets. Both are exact; the
  raw dumps are reproducible from the offsets quoted below.

---

## 1. What the converter writes

`crates/converter/src/material.rs`, `publish_material` / `build_lighting_material`:

| NIF field (`BSLightingShaderProperty`) | Read at | Written to the glTF as | At |
|---|---|---|---|
| `glossiness: f32` | `material.rs:906` | `pbrMetallicRoughness.roughnessFactor = 1 - (glossiness/100).clamp(0,1)` | `material.rs:516` |
| — (no NIF input) | — | `pbrMetallicRoughness.metallicFactor = 0.0` (hardcoded) | `material.rs:515` |
| — | — | `metallicRoughnessTexture`: **never written** | — |
| `specular_strength: f32` | `material.rs:908` | `KHR_materials_specular.specularFactor = strength.clamp(0,1)` | `material.rs:640` |
| `specular_color: vec3` | `material.rs:907` | `KHR_materials_specular.specularColorFactor = color.clamp(0,1)` | `material.rs:641` |
| texture slot 7 (`NifTextureSemantic::Specular`, `material.rs:1052`) | `material.rs:512` | `KHR_materials_specular.specularColorTexture` (sampled sRGB) | `material.rs:643-648` |
| `alpha: f32` | `material.rs:873` | `baseColorFactor[3]` | `material.rs:901` |
| **normal map's alpha channel** (Skyrim's gloss/spec mask) | **never read** | **nothing** | — |

- `KHR_materials_specular` is published only when `specular_strength > 0.0` or a slot-7 texture exists
  (`material.rs:635-638`); `BSEffectShaderProperty` materials get `glossiness: 0.0`,
  `specular_strength: 0.0` by construction (`material.rs:977-979`), so they publish no extension and land
  at `roughnessFactor 1.0`.
- The extension keyword used to be dropped on purpose: `publish_gltf_materials` strips any
  `KHR_materials_pbrSpecularGlossiness` the exporter left behind (`material.rs:431-462`). Nothing in the
  converter ever *emits* specular-glossiness.
- The normal map's alpha **is** carried into the KTX2 (the DDS is decoded to RGBA8 and re-encoded as
  UASTC, `texture.rs:273-281`, `texture.rs:919-929`; the repo has a round-trip test for exactly this,
  `texture.rs:1048-1067`), but no consumer ever names it. It is unused data, not absent data.
- Two more shader fields are also never read at all — `refraction_strength` and `lighting_effect_1/2` (the
  0.3 / 2.0 pair every property carries, table 4.1) — and `uv_offset`/`uv_scale` are not written either.
  None of them is the wet look (they are, in order, a refraction index, two glow/pulse parameters and a UV
  transform), but a review of this file should not be surprised by them.

## 2. What the engine does with it

| Step | Behaviour | Where |
|---|---|---|
| glTF → `StandardMaterial` | Bevy's own loader: `perceptual_roughness = pbr.roughness_factor()`, `metallic = pbr.metallic_factor()` | `bevy_gltf-0.19.0/src/loader/mod.rs:1421-1422` |
| `KHR_materials_specular` → reflectance | `reflectance = specularFactor.unwrap_or(1.0) * 0.5` | `bevy_gltf-0.19.0/src/loader/mod.rs:1483` |
| `specularColorTexture` → `specular_tint_texture` | only with the `pbr_specular_textures` feature — **enabled** | `bevy_gltf-0.19.0/src/loader/extensions/khr_materials_specular.rs:70-78`; `Cargo.toml:18` |
| Skyrim hook | `skyrim_material()` re-publishes the loaded material with **only** `alpha_mode` and `emissive` changed (`..material.clone()`); `None` when neither changes | `crates/engine/src/render.rs:767-788`, handler at `:809-845` |
| Roughness / metallic / reflectance | **never overridden, scaled or clamped anywhere in the streamed path** | grep of `crates/engine/src`: the only `perceptual_roughness`/`metallic` literals belong to terrain (`streaming.rs:1102`, `app.rs:813`), water (`streaming.rs:1157-1158`, `app.rs:830-831`), the snow/ice fixtures and the app's own test fixtures |

Arithmetic Bevy then applies (`bevy_pbr-0.19.0`):

- `perceptualRoughnessToRoughness(p) = clamp(p, 0.089, 1.0)²` — so `roughnessFactor 0.0` is not 0 but
  `0.089² = 0.0079`, the mirror floor (`pbr_lighting.wgsl:548-553`).
- `F0_dielectric = 0.16 * reflectance²` → `reflectance 0.5` gives `F0 = 0.04`, the textbook dielectric
  (`pbr_functions.wgsl:286-293`).
- If a `specularTexture` were ever published, `reflectance *= texture.a * 0.5` (`pbr_fragment.wgsl:298`);
  if a `specularColorTexture` is, `reflectance *= texture.rgb` (`pbr_fragment.wgsl:328`). Ours never has the
  first and has the second on 2,598 materials (see §7).

**So: the engine is a pass-through. Every roughness number in the render is the converter's.**

## 3. The numbers on real assets

Extracted from the GLB JSON chunks (compact, BTreeMap-ordered; offsets quoted so the dumps are re-runnable).
`roughnessFactor` ⇄ glossiness is exact, because the mapping is invertible for roughness > 0.

**Table 3.1 — the models the crops show**

| Converted glb | glTF material | base colour | NIF glossiness | `roughnessFactor` | `metallicFactor` | MR texture | `KHR_materials_specular` |
|---|---|---|---|---|---|---|---|
| `architecture/farmhouse/farmhouse02.glb` | `Farmhouse02:5 - L2_Rope:5 - L1_Posts:5` *(the porch posts)* | `woodpost02` | **80** (bytes read) | **0.2** | 0.0 | none | factor 1.0, colour [1,1,1] |
| | `Farmhouse02:4` *(thatch)* | `thatch02` | 80 | 0.2 | 0.0 | none | 1.0, [1,1,1] |
| | `Farmhouse02:6` | `woodwall01` | 80 | 0.2 | 0.0 | none | 1.0, [1,1,1] |
| | `Farmhouse02:7` | `farmhouse01` | 80 | 0.2 | 0.0 | none | 1.0, [1,1,1] |
| | `Farmhouse02:14` | `farmwindowinterior01` (+glow) | 80 | 0.2 | 0.0 | none | 1.0, [1,1,1] |
| | `Farmhouse02:1` *(stone wall)* | `stonewall01` | 50 | 0.5 | 0.0 | none | 1.0, [1,1,1] |
| | `Farmhouse02:25` | `thatch03` | 30 | 0.7 | 0.0 | none | 1.0, [1,1,1] |
| | `Farmhouse02:19 - ...` | `woodwalkway01` | 30 | 0.7 | 0.0 | none | 1.0, [1,1,1] |
| | `L2_Rope:18` / `:22` | `rope01` / `ropealpha01` | 30 | 0.7 | 0.0 | none | 1.0, [1,1,1] |
| `architecture/farmhouse/inn01.glb` | `Inn01:4/:5/:6/:7/:14` | — | **80** (bytes read) | **0.2** | 0.0 | none | 1.0, [1,1,1] |
| | `Inn01:1` | — | 50 | 0.5 | 0.0 | none | 1.0, [1,1,1] |
| | `Inn01:19/:22/:25` | — | **30** (bytes read) | 0.7 | 0.0 | none | 1.0, [1,1,1] |
| `clutter/barrel01.glb` | `Barrel01:0` *(the crop's barrel: this is the model, but see §0)* | `barrel01` | **80** (bytes read) | **0.2** | 0.0 | none | 1.0, [1,1,1] |
| `architecture/farmhouse/interior/farmintinnwall01.glb` *(the inn's own interior kit)* | `FarmIntInnWall01:5/:6/:14/:15` | — | 80 | **0.2** | 0.0 | none | 1.0, [1,1,1] |
| | `FarmIntInnWall01:16/:18/:19` | — | 30 | 0.7 | 0.0 | none | 1.0, [1,1,1] |
| `architecture/farmhouse/interior/farmintwoodwall01.glb` | `FarmIntWoodWall01:5/:6/:15` | — | 80 | **0.2** | 0.0 | none | 1.0, [1,1,1] |
| | `FarmIntWoodWall01:19` | — | 30 | 0.7 | 0.0 | none | 1.0, [1,1,1] |
| `dungeons/dwemer/facades/dwefacadetowerroof01.glb` *(a real metal)* | `DweFacadeTowerRoof01:6` | `dwemetaltiles01` + cube `bronze_e` | **400** (bytes read) | **0.0** | 0.0 | none | **factor 0.8**, [1,1,1] |

**Table 3.2 — every material in the conversion, by roughness** (`grep -a -h -r -o '"roughnessFactor":[0-9.]*'`
over the 22,761 glbs, grouped):

| `roughnessFactor` | materials | share | implied glossiness | equivalent Blinn-Phong exponent of that roughness (`2/r⁴ - 2`) |
|---|---|---|---|---|
| `0.0` | **14,825** | 25.0 % | ≥ 100 (clamped away) | ≈ 31,900 (the 0.089 clamp) |
| `0.2` (two adjacent f32 values) | **25,681** | 43.3 % | 80 | ≈ 1,250 |
| `1.0` | 6,485 | 10.9 % | 0 (effect materials) | 0 |
| `0.8` / `0.7` / `0.5` | 1,237 / 1,195 / 898 | 5.6 % | 20 / 30 / 50 | 2.9 / 6.3 / 30 |
| `0.84` / `0.4` / `0.88` / `0.6` / `0.58` / `0.38` / `0.77` / `0.75` | 785 / 669 / 528 / 491 / 490 / 477 / 451 / 444 | 7.3 % | 16 / 60 / 12 / 40 / 42 / 62 / 23 / 25 | — |
| the remaining ~90 distinct values | 4,612 | 7.8 % | — | — |
| **total** | **59,268** | 100 % | | |

**68.3 % of every material the project publishes sits at roughness ≤ 0.2.** The 0.2 bucket is exactly
glossiness 80, which is what `Farmhouse02`'s timbers, `Inn01`'s timbers, the barrels, the interior kit and
(as the crops show) most of Riverwood carry.

A second result worth stating plainly, because it removes a whole class of hypotheses:
**`metallicFactor` is `0.0` on all 59,268 materials and no material anywhere in the 1.3 GB tree has a
`metallicRoughnessTexture` (0 occurrences).** So there is no false metal and no per-pixel roughness to
soften the wrong constant — roughness is one flat number per material.

## 4. What the NIFs actually carry

Direct reads of the source NIFs (`C:/Modding/SkyrimConverted/vfs/meshes/…`), via `od` at byte offsets
computed from the field layout in `vendor/project-wormhole-nif/src/nif_block.rs:683-703` and verified
against the flags words the glbs publish.

**Field offsets inside a `BSLightingShaderProperty` block** (`block+0` = `shader_type`), confirmed on
`dwefacadetowerroof01.nif`: byte `20800` holds `81 03 40 82` = `shaderFlags1 0x82400381` and byte `20804`
holds `21 80 00 00` = `shaderFlags2 0x8021`, both exactly as published in that glb's
`OPEN_SKYRIM_material` extension; +40 `texture_set`, +64 `alpha`, +72 `glossiness`, +88 `specular_strength`.

**Table 4.1 — the four properties, read out of the source bytes**

| NIF | block start | alpha | refraction | **glossiness** | specular colour | **specular strength** | lighting effects |
|---|---|---|---|---|---|---|---|
| `architecture/farmhouse/farmhouse02.nif` | 115,628 (field at 115,700) | 1.0 | 0.0 | **80.0** (`00 00 a0 42`) | (1, 1, 1) | **1.0** | 0.3 / 2.0 |
| `architecture/farmhouse/inn01.nif` | 161,096 (at 161,168) | 1.0 | 0.0 | **80.0** | (1, 1, 1) | **1.0** | 0.3 / 2.0 |
| | 508,716 (at 508,788) | 1.0 | 0.0 | **30.0** (`00 00 f0 41`) | (1, 1, 1) | **1.0** | 0.3 / 2.0 |
| `clutter/barrel01.nif` | 143,267 (at 143,331) | 1.0 | 0.0 | **80.0** | (1, 1, 1) | **1.0** | 0.3 / 2.0 |
| `dungeons/dwemer/facades/dwefacadetowerroof01.nif` | 20,784 (at 20,856) | 1.0 | 0.0 | **400.0** (`00 00 c8 43`) | (1, 1, 1) | **0.8** (`cd cc 4c 3f`) | 0.3 / 2.0 |

Everything the converter published matches these bytes exactly (`0.2 = 1 - 80/100`, `0.5 = 1 - 50/100`,
`0.7 = 1 - 30/100`, `0.0` for the 400, `specularFactor 0.8` for the Dwemer roof). **The data is there, the
parse is right; only the arithmetic that turns it into a PBR roughness is wrong.** Glossiness 30/80/400 do
not mean "70 %/20 %/0 % rough".

**Table 4.2 — the normal maps (Skyrim's gloss/spec mask lives in the alpha)**

| texture | format | dims | alpha at scattered blocks (`alpha0`,`alpha1` of the DXT5 alpha block) |
|---|---|---|---|
| `textures/clutter/barrel01_n.dds` | DXT5 | 1024×512 | `0x23/0x15`, `0x1f/0x18`, `0x1a/0x10` → ≈ 0.06–0.14, varying |
| `textures/architecture/farmhouse/woodpost02_n.dds` | DXT5 | 2048×2048 | `0x04/0x12`, `0xbe/0x8b`, `0x0a/0x6a` → varying, 0.02–0.75 |
| `thatch02_n.dds`, `stonewall01_n.dds`, `farmhouse01_n.dds` | DXT5 | — | — |

**All 12,313 `*_n.dds` normal maps in the conversion are DXT5** (12,313 DXT5 vs 1 `ATI2`; BC3 carries an
8-bit alpha channel, and only 1 file in the tree uses a 2-channel format). So a per-pixel gloss mask exists
on essentially every normal-mapped material in the game and **nothing in the pipeline reads it.**

## 5. The mapping to recommend

### 5.1 Glossiness → `perceptual_roughness`

Skyrim's glossiness is the exponent of the specular lobe (`spec = pow(N·H, glossiness) * strength * colour *
mask`). Bevy's `perceptual_roughness p` is **not** an exponent; it is the square root of the GGX α
(`alpha = clamp(p, 0.089, 1)²`). The standard Blinn-Phong ↔ GGX equivalence is `α = sqrt(2/(n+2))`, so

```
perceptual_roughness = (2 / (glossiness + 2)) ^ 0.25        // == sqrt(alpha), alpha = sqrt(2/(n+2))
```

One expression, monotone on the whole domain, no clamp needed (`glossiness 0 → 1.0`, `→ 0` as
`glossiness → ∞`), and it collapses to the current behaviour only at the very bottom of the range.
The only implementation note: above `glossiness ≈ 30,000` the result drops under Bevy's own `0.089` floor
and would clamp to a mirror again; the largest value I read out of a source NIF is 400, so this is a
guard for hypothetical data, not a live case (`material.rs:1182-1199` rejects negatives but sets no ceiling).

| glossiness | today (`1 - g/100`) | today's true exponent (`2/r⁴ - 2`) | recommended `(2/(g+2))^0.25` |
|---|---|---|---|
| 0 (effect cards) | 1.0 | 0 | **1.0** |
| 5 | 0.95 | 0.46 | **0.731** |
| 10 | 0.9 | 1.05 | **0.639** |
| 20 | 0.8 | 2.9 | **0.570** |
| 30 (thatch, rope, walkways) | 0.7 | 6.3 | **0.500** |
| 50 (stone walls) | 0.5 | 30 | **0.443** |
| 80 (timber, posts, barrels) | **0.2** | **1,250** | **0.395** |
| 100 | 0.0 (mirror) | 31,900 | **0.374** |
| 200 | 0.0 (mirror) | 31,900 | **0.315** |
| 400 (polished Dwemer metal) | 0.0 (mirror) | 31,900 | **0.266** |
| 1000 | 0.0 (mirror) | 31,900 | **0.211** |

The two things this fixes, in the renderer's terms: the timber's lobe is ~3.5× too tight today (a
Blinn-Phong lobe's angular width scales as `1/√n`, and the current value puts wood at `n ≈ 1,250` where the
NIF says 80), and the 14,825 mirror-clamped materials become a graded shiny class (`0.374` at 100 → `0.211`
at 1000) instead of all landing on `0.089²`.

**Metallic: keep `0.0`.** Bevy 0.19's own default is `0.0`, not 0.5 (`bevy_pbr-0.19.0/src/pbr_material.rs:866`),
so the brief's "0.5 default" does not apply to this engine version, and the converter's hardcoded `0.0` is
already correct for wood, thatch, stone, cloth and hide. The one class that genuinely is metal (Dwemer
bronze, iron, gold, glass) cannot be told from a wood beam by any field this converter reads — Skyrim
carries no metalness channel. The only data-side signal is the **environment map**: 7,823 materials carry a
`environment_cube` slot and 7,152 declare `"lightingShaderType":"environment_map"` (the Dwemer roof is one:
cube `bronze_e`, glossiness 400). **Do not set `metallic = 1.0` yet**: in a metallic workflow the diffuse
goes to zero and the colour comes from the environment, and this engine binds no `EnvironmentMapLight`,
`LightProbe` or `IrradianceVolume` anywhere (`docs/research/visual-gaps-spec.md`, gap 3, "the engine
discards them"). Setting metalness before the env-cube binding lands would make the Dwemer metal *darker*,
not shinier. Sequence: roughness fix first, env-cube binding second, metalness third, fitted on a holdout.

### 5.2 The specular/gloss mask (the other half of the fix)

The converter drops the normal map's alpha, so every pixel of a material gets the full material-level
specular. On the posts and the barrel that alpha is 0.06–0.14 — Skyrim's own render is up to ~10× dimmer in
specular than what the converter produces, and that difference is the difference between weathered timber
and varnished timber. Two ways to put it back; the choice depends on which quantity the mask scales, which
I could not settle locally (§8):

- **If it scales the specular strength** (the cheap route, no new textures): publish
  `KHR_materials_specular.specularTexture` pointing at **the normal map's own texture index** — glTF samples
  the alpha channel of that texture, which is exactly the mask, and Bevy parses it
  (`khr_materials_specular.rs:60-68`) and applies `reflectance *= sample.a * 0.5`
  (`pbr_fragment.wgsl:298`). Because of that extra 0.5, `specularFactor` has to be published as `2 ×
  strength` (the workaround documented in Bevy's own extension comment, `khr_materials_specular.rs:16-23`;
  note that 2.0 is outside the glTF spec's `[0, 1]` range and is only safe because this engine is the sole
  consumer).
  No texture re-encoding: the KTX2s already carry the alpha.
- **If it scales the gloss exponent** (`n_eff = n · mask`): the mask belongs in a roughness map — publish a
  `metallicRoughnessTexture` whose green channel is `roughness(n_eff)` and blue is 0. That needs a derived
  texture per normal map (a texture re-encode in the converter, still no engine change, and Bevy's
  `metallic_roughness_texture` path is a single multiplication, `pbr_fragment.wgsl`).

Either reading makes the surface *more* matte, so the direction of the fix is not in doubt; only the form.
I would land §5.1 alone first (it is one line and takes the render from "everything is wet" to "nothing is
wet but everything is uniformly matte"), then do the mask as a second, separately reviewed change.

### 5.3 How I would check it

Three of the twelve poses, chosen because the crops already came from them:

| pose | reference | what it shows | what "correct" looks like |
|---|---|---|---|
| `RW-04-inn-front` | `SR-place-Sleeping_Giant_Inn.jpg` | the Sleeping Giant Inn's front, its porch posts and the barrel — the exact content of `_crop-gloss.png` | the broad white band down the posts and the staves is gone; the wood keeps its albedo and keeps only a faint, soft sheen on the lit side, as in the reference |
| `RW-09-inn-common-room` | `SR-interior-Sleeping_Giant_Inn_Common_Room.jpg` | the common room's log pillars and beams — `_crop-gloss2.png` | the vertical smeared highlights on the pillars flatten; the room's contrast drops without any change in overall exposure |
| `RW-02-street-to-the-inn` | `SR-place-Riverwood_02.jpg` | the whole street: thatch + timber + stone in one frame | a whole-village test that the change is not only visible 2 m from a post |

Metric: `tools/research/exposure_stats.py` on the same shots file, before and after, plus the same two crops.
Roughness does not touch albedo, so the frame's p50 should move by only a few percent while the
near-90 %/clipped fractions on the wood fall. Per the delegation policy, hold out two poses the implementer
does not tune on — `RW-11-alvors-house` and `RW-08-guardian-stones` are the natural pair (interior timber /
stone monoliths, no barrel or porch post in either).

## 6. Blast radius, and what the fix costs

- **Affected:** all **59,268** published materials across **22,761** glb files (one glb per converted NIF;
  `find meshes -name "*.glb" | wc -l` = 22,761 = the NIF count in `vfs/meshes`). Every material is wrong in
  the same way; the only materials the roughness change leaves alone are the 6,485 effect cards, which are
  already at 1.0.
- **Converter-side or engine-side?** The number lives in the glb (`roughnessFactor`), so the correct fix is
  **converter-side, no engine change**: one expression at `material.rs:516`, tests updated
  (`material.rs:1448-1451` asserts 0.68 for glossiness 32 — it becomes 0.4925).
- **Does it force a reconversion?** A **mesh (glb) rebuild**, not a texture rebuild. The cache is keyed by
  source and output hashes per asset plus a schema version (`cache.rs:11-19`), and the project already has
  the idiom: bump `CONVERTER_SCHEMA_VERSION` 17 → 18 and add the migration arm beside `cache.rs:61-74`,
  which keeps every non-`.glb` entry (archives, textures, scripts) and forces all glbs through the new
  contract. The measured price of exactly that run is in the repo — `Converted 22761, reused 220208,
  skipped 0 in 843045 ms (complete: true)`, `local/conversion-0923b.out.log` — i.e. **≈14 minutes**, plus
  the publish step. No `.dds` is decoded again and no `.ktx2` is re-encoded.
- **The alternative — engine-only — is lossy exactly where it matters.** `skyrim_material()`
  (`render.rs:767`) could invert the mapping (`glossiness = 100·(1 - roughness)`) for the 44,443 materials
  with roughness > 0, but the 14,825 at `0.0` have already lost their gloss (everything ≥ 100 collapses to
  the same number), and those are precisely the polished surfaces — ice, Dwemer metal, glass, ore. It would
  also be a permanent hack that the next reconversion silently fights. **Not recommended**; the converter
  line is cheaper than the workaround.
- **Not part of this change:** `metallic` (§5.1), the env-cube binding, and anything in `crates/engine/`.

## 7. What is *not* the cause (negative controls)

- **Not metalness.** `metallicFactor` is `0.0` on all 59,268 materials; Bevy's 0.19 default is also 0.0
  (`pbr_material.rs:866`), so the brief's "0.5 is almost certainly wrong for wood" is right as a principle
  but is not what is happening here.
- **Not a missing roughness map alone.** No material has a `metallicRoughnessTexture` (0 occurrences in
  1.3 GB) — that is a real fidelity gap (§5.2) but it cannot *cause* a specific wrong value.
- **Not `reflectance`.** 52,698 materials publish `KHR_materials_specular`; `specularFactor 1.0` on 42,935
  of them → `reflectance 0.5` → `F0 0.04`, the textbook dielectric value, not an over-bright one. The 6,570
  materials with no extension at all get Bevy's default 4 % specular where Skyrim may have none — a small
  error in the *opposite* direction, and it lands on effect cards.
- **Not the engine.** `skyrim_material()` changes only `alpha_mode` and `emissive` (`render.rs:783-787`).
- **One thing to log for later, not the cause:** on the 2,598 materials with a slot-7 texture, Bevy folds
  `specularColorTexture` into `reflectance *= tint.rgb` (`pbr_fragment.wgsl:328`), i.e. it uses a
  *specular colour* map as a *reflectance* multiplier, sampled as sRGB. Whether Skyrim's slot 7 is even the
  same quantity is worth a web check (§8); it is not what makes Riverwood wet.

## 8. Open questions for a websearch worker

1. **The gloss mask's semantics**: does `*_n.dds`'s alpha scale the specular *strength* or the *gloss
   exponent*? NifSkope names texture slot 1 "Normal/Gloss"; the SSE shader is what settles it. This decides
   between the two options in §5.2.
2. **The Blinn-Phong ↔ GGX relation** used in §5.1 (`α = sqrt(2/(n+2))`) — worth one citation from a
   primary source (Filament / Frostbite / Disney BRDF notes) rather than from this report's arithmetic.
3. **What other open reimplementations do** (Skyrim → PBR converters, NifSkope's glTF export, OpenMW's
   Skyrim work) with glossiness 30/80/400 — as a cross-check on the proposed mapping's output range.
4. **Whether slot 7 is the specular colour map** in `BSLightingShaderProperty` (the converter assumes it,
   `material.rs:1052`; 2,598 materials use it).

## 9. Files this report rests on

| path | what for |
|---|---|
| `crates/converter/src/material.rs:502-652`, `:849-917`, `:1016-1062` | every field read and written |
| `crates/converter/src/cache.rs:11-98` | schema-versioned cache and the migration idiom |
| `crates/engine/src/render.rs:767-788`, `:809-845` | the only engine hook on a streamed material |
| `Cargo.toml:18` | `pbr_specular_textures` is on |
| `C:/Modding/SkyrimConverted/meshes/**/*.glb` | the 22,761 published materials |
| `C:/Modding/SkyrimConverted/vfs/meshes/**, vfs/textures/**` | source NIFs and DDS |
| `local/reference/rw1/` | the renders the crops came from |
| `docs/research/visual-gaps-spec.md:55` | the same mapping noted as a cause of the bronze Dwemer roof, before it was identified as the cause of the wet look |

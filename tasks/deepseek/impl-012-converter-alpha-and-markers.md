# impl-012 - Converter: correct alpha modes, keep blend factors, drop editor markers

Owned files: crates/converter/src/material.rs,crates/converter/src/mesh.rs
Acceptance: cargo test -p converter --lib,cargo clippy -p converter --all-targets -- -D warnings,cargo fmt -p converter -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, and the research this
task implements: `docs/research/transparent-and-misplaced-meshes.md` (research-011, reviewed by the
lead; its class A/B/C recipes are your spec). Your checkout's `target/` is seeded from the lead's build.

## Goal

Stop the converter from making solid Skyrim objects see-through, keep the information the engine needs
to draw glows correctly, and stop exporting editor-only marker shapes. The user sees ghostly ice, rock
and floors, grey veils where lantern glows should be, and flat white marker shapes in the Alftand ->
Blackreach demo.

## Class A - vertex-alpha flag is not a blend state (the big one)

`alpha_contract` (`crates/converter/src/material.rs`, around lines 974-1000, verified by the lead):
with no `NiAlphaProperty`, the shader flags `SLSF1_VERTEX_ALPHA`, `SLSF1_SCREENDOOR_ALPHA_FADE` or
`SLSF2_PREMULTIPLIED_ALPHA` alone produce `NifAlphaMode::Blend`. In Skyrim the blend/test state comes
from `NiAlphaProperty`; without one the shape is drawn opaque. Research-011 counted 2,632 of 60,414
materials affected (e.g. `glacierslab`, `dirt02`, `rocks01`, `snow01` textures, whose alpha channel is
not an opacity map). Make shapes without an `NiAlphaProperty` opaque. Check research-011's reasoning
for whether any of the three flags genuinely needs blending without a property (screen-door fade is
a distance-fade effect, not transparency); decide, and state it with the evidence.

## Class B - additive and multiplicative blending

`NiAlphaProperty` carries source/destination blend factors
(`vendor/project-wormhole-nif/src/nif_flags.rs`, around line 164), but the converter keeps only the
enable bit. 4,335 additive (`SRC_ALPHA`/`ONE`) and 435 multiplicative blocks lose their meaning. Write
the factors into the material's `OPEN_SKYRIM_material` glTF extension (the existing extension object
the converter already writes - find it in `material.rs`) as two string fields, e.g.
`"blendSource": "SRC_ALPHA"`, `"blendDestination": "ONE"`, only when blending is enabled. Keep glTF
`alphaMode` `BLEND` for them. The engine will read these fields in a later task; do not touch the
engine.

## Class C - editor markers

Shapes named `EditorMarker` (and the other editor-only markers research-011 lists) inside otherwise
legitimate models (`DwePtnDoor01`, `DweLever02`, `SpikeTrapDwemer`, `Effects\FX*`) are exported and
drawn as flat white/grey shapes. Exclude them from the exported geometry through the converter's existing
non-rendering exclusion path (research-011 points at it, around `material.rs:249`; the geometry side
may be in `mesh.rs`). A model whose only geometry was a marker must still convert cleanly (empty or
skipped the same way other non-rendering models are - follow the existing rule, do not invent one).

## Tests (each must fail before your change)

- A shape with the vertex-alpha flag and no `NiAlphaProperty` exports `OPAQUE`; one with an
  `NiAlphaProperty` blend bit still exports `BLEND`; the test-enabled case still exports `MASK` with
  its cutoff (the existing test near `material.rs:1466` is the pattern).
- An additive property exports `blendSource`/`blendDestination` `SRC_ALPHA`/`ONE`; a normal alpha
  blend exports `SRC_ALPHA`/`INV_SRC_ALPHA`; an opaque shape has neither field.
- A model containing an `EditorMarker` shape plus a real shape exports only the real shape.

## Real-data check (read-only)

After your tests pass, if feasible within the crate's reader API, convert one real affected NIF from
the extracted copies under `C:/Modding/Mudcrab/SkyrimConverted/vfs/meshes/` in a test marked `#[ignore]`
(pick one research-011 names, e.g. an ice pile) and assert its materials are opaque. Do not run the full
converter and do not write into `C:/Modding/Mudcrab/SkyrimConverted`.

## Scope

Only the two owned files. If the cache/config hash that decides reconversion does not change when
material output changes, say so in your result (the lead must then force a reconversion) - do not edit
`cache.rs` or `pipeline.rs`. No crosstalk partner: the engine workers running now own engine files only.

## Done when

Acceptance commands pass. Report per the implementation template, including the census you expect
after reconversion (materials by alpha mode, before/after, from research-011's numbers) and whether a
forced reconversion is needed.

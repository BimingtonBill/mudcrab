# impl-063 - Vertex-colour alpha is not opacity: stop it eating the forest

## Why

`docs/research/foliage-alpha-test.md` (research-059) is the diagnosis; **read it in full first**,
especially sections 4 and 6.

Riverwood's pines render as a spray of dots against the sky
(`local/reference/rw_sweep/_crop-tree.png`). They are **not** mis-classified as `BLEND`: the
converter publishes them correctly as glTF `MASK` with the NIF's own threshold. They vanish
because Bevy's PBR shader tests `vertex_color.a * texture.a` against the cutoff, and Skyrim's tree
NIFs carry a `COLOR_0` whose alpha channel is a **shader parameter, not opacity**: 176 of the 296
vertices of `TreePineForest03_1:1` sit below its 0.4392 cutoff, 110 of them at 0.0.

The project already ruled on this question. `crates/converter/src/material.rs:1118-1131`
(commit `b4ce0d9`) says `SLSF1_VERTEX_ALPHA` is a request to *read* per-vertex alpha, not a
declaration of transparency - and treating it as transparency made 2,632 ice, rock and floor
materials see-through. That ruling stands. What has changed is its stated premise: the comment says
"the converter exports no vertex colours for it to read", and that is **no longer true** - the
vendored exporter has written `COLOR_0` since `33682e6`, and 1,337 converted models carry one.
This task closes the gap that left.

## What to build

**Engine-side, at mesh load. No reconversion, no converter change, no `vendor/` change.**

A system that rewrites loaded meshes: for any `Mesh` carrying `Mesh::ATTRIBUTE_COLOR`, set the
**alpha component of every vertex to 1.0** and leave RGB untouched (Skyrim's baked per-vertex
shading is real and must survive). Register it on `AssetEvent<Mesh>` for added and modified
assets, next to the existing material-handler pattern at `crates/engine/src/render.rs:807-853`.

Requirements:

- **Idempotent.** Running it twice changes nothing; it must not loop by re-triggering its own
  `AssetEvent::Modified`.
- **Handles every colour format the loader can produce.** `ATTRIBUTE_COLOR` can arrive as
  `Float32x4`, `Float32x3`, `Uint8x4` normalised, and others. A three-component colour has no
  alpha and must be left alone, not widened. An unexpected format must be skipped with a single
  `debug!`, never a panic and never a silent wrong write.
- **Cheap.** One pass per mesh, only for meshes that actually have the attribute.
- Say in a `debug!`, once per mesh, how many vertices were rewritten - and keep it at `debug`, it
  will fire 1,337 times.

**Also correct the snow shader's premise.** `crates/engine/src/shaders/snow.wgsl:66-71` says *"No
converted mesh does today (`crates/converter/src/mesh.rs` writes no vertex colours), so this is
dead code"* and then multiplies snow coverage by `in.color.a`. That premise is false, and once the
snow data lands a 0.251-alpha roof would silently keep a quarter of its snow. With this task's
rewrite the alpha is 1.0 and the branch becomes what it was written to be - so fix the **comment**
to say that, accurately, and cite this task. Do not change the shader's arithmetic.

**Correct the same stale premise where it is written down**, in one sentence each, without
rewriting the documents: `crates/converter/src/material.rs:1118-1131` (the comment only - the
*logic* is correct and stays exactly as it is) and
`docs/research/transparent-and-misplaced-meshes.md:43` ("GLB attributes are
`NORMAL,POSITION,TEXCOORD_0` only").

## Owned files

- `crates/engine/src/render.rs`
- `crates/engine/src/shaders/snow.wgsl` - the comment only
- `crates/converter/src/material.rs` - **the comment at 1118-1131 only**, not one line of logic
- `docs/research/transparent-and-misplaced-meshes.md` - the one stale sentence
- `docs/handoff/impl-063-vertex-alpha-is-not-opacity.md` - your notes

**A conflict warning:** another worker (impl-062) is editing `crates/converter/src/material.rs`
right now, in `publish_material` and the glossiness mapping. Touch **only** the comment block at
1118-1131 and nothing else in that file, so the lead can merge both by hand if they collide.

## Tests to write

They must run in CI, which has no game data and no GPU, so build `Mesh` values in the test:

- A `Float32x4` colour mesh with a spread of alphas (0.0, 0.251, 0.4392, 1.0) comes out with every
  alpha exactly 1.0 and **every RGB triple bit-identical** to what went in.
- A `Uint8x4` colour mesh is handled correctly in its own encoding (alpha 255), RGB unchanged.
- A `Float32x3` colour mesh is returned untouched.
- A mesh with no colour attribute is untouched and not counted.
- Idempotence: rewriting twice equals rewriting once.
- The real case, written as data: a vertex at alpha 0.0 with a cutoff of 112/255 fails the mask
  test before the rewrite and passes it after - i.e. assert on the product the shader computes,
  `vertex_alpha * texture_alpha >= cutoff`, for a texture alpha of 1.0. This is the test that
  states what the bug was.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
cargo test -p converter
```

## Out of scope

Do not run the engine or render anything - the lead will re-shoot
`local/reference/rw1/RW-01-village-from-the-south.png` and `RW-07-village-from-the-south-hill.png`
and check that the canopies fill in while the ferns, houses and trunks do not move. Do not touch
the alpha-mode classification, the cutoff arithmetic, `Msaa`, or the `Own_Emit`/emissive split -
research-059 section 4 notes the surviving needles are also drawn self-lit, and that is a separate
gap (`docs/research/visual-gaps-spec.md`, gap 1). Do not add a command-line switch for this.

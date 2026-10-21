# impl-063 - vertex-colour alpha is not opacity

**Task:** the lead's brief `impl-063-vertex-alpha-is-not-opacity`; the diagnosis it builds on is
`docs/research/foliage-alpha-test.md` (research-059), sections 4 and 6 especially.

**Acceptance run here** (final state of the files below): `cargo fmt --all -- --check` (no output),
`cargo clippy --workspace --all-targets -- -D warnings` (clean; the only note is the pre-existing
`proc-macro-error2` future-incompat report from a dependency), `cargo test -p engine` (287 passed,
0 failed, 2 ignored, plus 3 in `world-inspect` and 2 ignored doctests), `cargo test -p converter`
(152 passed, 0 failed, 19 ignored, plus the small integration/doc groups). Eight new tests, all in
`render::tests`.

## What changed

### `crates/engine/src/render.rs`

- **`force_opaque_vertex_colours`** - the system. It reads `AssetEvent<Mesh>` for `Added` and
  `Modified`, takes the mesh, and writes only when something really changes.
- **`force_opaque_vertex_alpha(mesh) -> Result<usize, &'static str>`** - the rewrite itself: one
  pass over the colour attribute, the alpha component of every vertex to full opacity in the
  attribute's own encoding, RGB untouched; the count of changed vertices, or the reason nothing was
  written.
- Registration in `VercidiumRendererPlugin::build`, next to `register_skyrim_material_handler`:
  `PostUpdate`, `after(AssetEventSystems)`.
- Tests (all names `render::tests::*`): the float spread (alphas 0.0 / 64-255ths / 112-255ths / 1.0
  in, all 1.0 out, RGB compared **by bits**), the byte encoding (`Unorm8x4`, alpha 255, RGB bytes
  unchanged), three components untouched (and not widened), no colour attribute untouched and
  uncounted, an unnormalised integer encoding skipped rather than guessed at, idempotence (twice ==
  once, second pass reports 0), the real case as data, and the system-level event/loop test.

### The three premises the brief asked to correct (comments only, no logic)

- `crates/engine/src/shaders/snow.wgsl` - the `#ifdef VERTEX_COLORS` comment now says that converted
  meshes *do* carry `COLOR_0`, that the alpha is a Skyrim shader parameter, and that this engine
  forces it to 1.0 at load, so the multiply is the identity for every streamed mesh. **The
  arithmetic is byte-for-byte what it was** (`let coverage = bare_coverage * in.color.a;`).
- `crates/converter/src/material.rs` - the comment block at the old lines 1118-1131 only, as the
  brief required: the ruling stands and the reasoning is kept, with the "the converter exports no
  vertex colours for it to read" premise marked out of date and the new fact (1,337 models, engine
  rewrite, impl-063) added in a parenthetical. **No line of logic touched** - `alpha_contract` is
  identical, so a hand-merge against impl-062's `publish_material`/glossiness work is a
  comment-block merge and nothing else.
- `docs/research/transparent-and-misplaced-meshes.md:43` - the one stale clause ("GLB attributes are
  `NORMAL,POSITION,TEXCOORD_0` only") replaced in place; class A's conclusion is unchanged.

## Decisions the lead may want to overrule (all one-line changes)

1. **"`Uint8x4` normalised" is read as `Unorm8x4`.** The brief asks for the 8-bit encoding whose
   opaque alpha is 255, and that is `VertexAttributeValues::Unorm8x4`; a *byte* test covers it. The
   **unnormalised** integer encodings (`Uint8x4`, `Sint8x4`, `Uint16x4`, `Sint16x4`, `Uint32x4`,
   `Sint32x4`) are reported and skipped instead of written, because `in.color` is a `vec4<f32>` in
   Bevy's shader - a `u32`/`i32` colour is never read as one, so "opaque" has no defined value in
   such a channel and writing one would be a silent wrong write. A test
   (`unnormalised_integer_colour_is_skipped_not_guessed`) pins that reading; say the word and it
   becomes `set_opaque(colours, u8::MAX)`.
2. **`Float16x4` is skipped and reported**, not written: `half::f16` has no `From<f32>`, this crate
   cannot name the type (`half` is `bevy_mesh`'s dependency, and adding one is the lead's call, not
   mine), and no pipeline here produces a half-float colour.
3. **`AssetEvent::Modified` is kept, not suppressed.** See the note below on the one redundant
   re-upload.

## The findings that shape the fix

**Only `Float32x4` can reach `ATTRIBUTE_COLOR` in this build, and that is enforced.** Bevy 0.19's
`Mesh::insert_attribute` refuses values whose format differs from the attribute's declared one
(`bevy_mesh-0.19.0/src/mesh.rs:396-403`, "Invalid attribute format for Vertex_Color"), and
`Mesh::ATTRIBUTE_COLOR` is declared `Float32x4`
(`bevy_mesh-0.19.0/src/mesh.rs:317-318`). The glTF loader moreover widens every `COLOR_0` - u8, u16
and f32, three- and four-component - to `Float32x4` (`bevy_gltf-0.19.0/src/vertex_attributes.rs`
`:196-215`) and errors on anything else (`:333-345`). So the other encodings are insurance against a
loader or an engine change, not live paths; the tests reach them through a fixture that declares the
encoding it uses and carries the same id the engine looks the attribute up by (that is what
`colour_mesh_in` documents).

**Why `PostUpdate` and not `Update`.** `SubApps::update` runs the whole main schedule and only then
the extract schedule (`bevy_app-0.19.0/src/sub_app.rs:566-582`), so a rewrite in any main schedule of
the frame a mesh is published reaches the render world's extraction of that same frame.
`handle_internal_asset_events` inserts loaded assets in `PreUpdate` and `Assets::<Mesh>::asset_events`
publishes `AssetEvent`s in `PostUpdate` (`bevy_asset-0.19.0/src/lib.rs:661-666`), so
`PostUpdate.after(AssetEventSystems)` is the first place the event exists and the last before
extraction. In `Update` the event would be a frame late - still workable, but it would depend on the
main-world copy surviving extraction, which for a mesh loaded without `MAIN_WORLD` it does not.

**Why the rewrite cannot feed itself.** `Assets::get_mut` returns an `AssetMut` whose
`AssetEvent::Modified` is queued only when the value is really taken mutably
(`bevy_asset-0.19.0/src/assets.rs:636-690`). Taking it through `bypass_change_detection`, writing,
and calling `into_inner()` only when a vertex changed means: a mesh whose alphas are already opaque
is never written and queues nothing. The system-level test runs three frames - `Added`, the one
`Modified`, then nothing - and fails if the second frame marks the asset modified again.

**The one cost, deliberately paid.** The `Modified` queued in frame N is published in frame N+1
(`asset_events` has already run for frame N), so every rewritten mesh is extracted twice: once with
the rewritten vertices, once again identically. That is one redundant upload of ~1,337 meshes at
load, and it is what makes the fix independent of whether extraction takes or clones main-world mesh
data. Suppressing it with `get_mut_untracked` would save the upload and lose that independence.

**Terrain is unaffected.** `build_terrain_quadrant_mesh` already writes `Float32x4` colours with
alpha 1.0 (`crates/engine/src/streaming.rs:2411-2420`), so the rewrite reports 0 for every quadrant
and marks nothing modified; `terrain.wgsl` never reads `in.color` either.

## Mutation checks (before the final run, then reverted)

- Making the "nothing to rewrite" arm mark the asset modified made
  `the_rewrite_runs_on_an_added_mesh_and_does_not_retrigger_itself` fail with "a pass with nothing to
  rewrite publishes nothing" - the loop the brief warns about is what that test detects.
- Making `set_opaque` write unconditionally (dropping the `colour[3] != opaque` test) failed four
  tests with their intended count assertions (`Ok(3)` vs `Ok(4)`, `Ok(0)` vs `Ok(1)`, `Ok(2)`, and
  the loop test), so the counts are load-bearing and not decoration.

## Not done, and what I could not check

- **No engine run and no render**, per the brief: the visual claim (canopies fill in, ferns, houses
  and trunks do not move) is the lead's two reference frames to shoot.
- **The "1,337 converted models" and the leaf histogram are research-059's measurements**, used as
  given; nothing in this task read the converted set or the game data. The tests build their own
  `Mesh` values, so they run in CI with no assets and no GPU.
- **`.deepseek-agents.json` was already modified in this worktree when I started** (and
  `.ds-impl.json` is the harness's own file); I did not touch either.

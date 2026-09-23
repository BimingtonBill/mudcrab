# lead-070 - What the converter throws away

You are a **lead**: you plan, write briefs, spawn coding workers with `tools/ds_impl.ps1`, review
their diffs, and report. You do not write production code yourself.

## Your territory, and nothing outside it

**You own `crates/converter/**` and nothing else in the repository.** Two other leads are running
right now on `crates/engine/` and neither may be touched by you or your workers. A worker that
edits outside `crates/converter/` has its work reverted, which wastes the run.

## Hard constraints

- **At most ONE `ds_impl.ps1` coding worker at a time.** Each seeds an ~11 GB checkout and the
  machine has about 50 GB free. Discard each worktree (`-Discard`) as soon as the lead has read
  its diff.
- **Nobody runs the converter or the engine.** Converting takes 18 minutes and the lead's user
  plays the demo on that machine. You propose; Claude runs.
- **Every converter change costs a reconversion, so batch them.** Land everything behind **one**
  schema bump. `CONVERTER_SCHEMA_VERSION` is currently 18 (`crates/converter/src/cache.rs`) and
  the schema-18 reconversion has already been run, so your first change that alters published
  output must bump it to 19 and add the migration arm in the established shape.

## The work, in the order I would do it

**1. Land the specular mask that is already written.** `local/impl/impl-064-specular-mask-from-normal-alpha/`
is a finished, accepted worktree against base `59f1f20` that publishes Skyrim's specular mask (the
normal map's alpha) as `KHR_materials_specular.specularTexture`. Read
`docs/handoff/impl-064-specular-mask-from-normal-alpha.md` and its diff. It was written on the
assumption that schema 18's reconversion had not happened yet; it has, so it now needs the bump to
19. Also in that handoff: `mesh.rs`'s `prune_document_images` remaps `normalTexture` and friends
but **not** anything under `extensions.KHR_materials_specular`, so a material whose normal map was
pruned keeps a stale texture index. That is a real correctness bug and it is yours.

**2. Ice reads as rock.** The oldest open visual gap in the project
(`docs/research/visual-gaps-spec.md`, and the handoff log in `AGENTS.md` has carried it since
2026-09-22). `docs/research/specular-gloss-mapping.md` section 5.1 is the current state of the
argument: 7,823 materials carry an `environment_cube` slot and 7,152 declare
`lightingShaderType: environment_map`, the converter reads them and the engine discards them, and
**metalness must not be set before the engine binds an environment map** or Dwemer metal gets
darker rather than shinier. Work out what the converter should publish - glTF has no direct
equivalent of Skyrim's env-cube, so this needs a real design, and the answer may be "publish X and
the engine needs Y", in which case say so precisely and stop at the converter boundary.

**3. An audit: what else does the converter silently drop?** Today two bugs of exactly this shape
surfaced by accident - the glossiness exponent read as a percentage, and vertex-colour alpha
published as opacity. Both had been wrong for every asset since the project began and were found
by looking at a render, not by anything systematic. Send a research worker over
`crates/converter/src/material.rs` and `mesh.rs` to list every NIF field that is **read and then
not written**, and every glTF field written from a constant rather than from the data. Rank by how
many materials each affects. That list is a deliverable in its own right.

## How to report

A brief per task in `tasks/deepseek/`, each named `impl-NNN-slug` or `research-NNN-slug`, numbered
from 073 upwards. Write `docs/research/converter-dropped-fields.md` for item 3. Your own final
report says what landed, what each change will cost when the reconversion runs, and what you chose
not to do.

## What "done" means for each worker you spawn

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p converter
cargo test -p engine
```

Every patch gets your independent review before you call it done: read the whole diff, and check
the claims that matter rather than trusting the summary.

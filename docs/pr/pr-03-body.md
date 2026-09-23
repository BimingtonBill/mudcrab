# PR 3 - fix(converter): a pruned dangling texture does not make a conversion incomplete

Branch `phase2/pruned-texture-complete` (`fabacd8` on `2105bfd`). Written by DeepSeek impl-083 from
fork commit `23f68f6`, reviewed by Claude. Not yet sent.

---

Since #21, every texture reference pruned from a GLB is recorded with `record_skip`: a manifest
failure, a warning and a skip. `manifest.complete` requires none of those, and a stock Skyrim SE
install has dangling texture references (#21 counted 225 across 247 GLBs), so it can never convert
`complete`. The converter then exits non-zero, the launcher doesn't accept the assets, the engine
refuses them without `--allow-incomplete-assets`, and the Phase 2 acceptance preflight
(`conversion-complete` in `scripts/phase2-acceptance.ps1`) can't pass.

A pruned reference is a fact about the game data, not a failed conversion, so it is now recorded
separately:

- `PipelineReport::pruned_texture_references` counts them, and `conversion_is_complete` ignores them;
- `ConversionManifest::pruned_texture_references` lists them per GLB (`#[serde(default)]`, so older
  manifests still load);
- the per-URI console warning stays, and the run summary prints the total.

Failed archives and meshes still skip, warn and make the run incomplete.

**Tests** (no game data): the completeness predicate; an older manifest still loads; a pipeline run
over `dummy-content` NIFs with a missing base colour and a missing normal map ends `complete` with
both references recorded; a broken archive still makes the run incomplete.

No schema bump or reconversion: converted artifacts are unchanged. Serves Marco 0 item 2 and the
acceptance preflight (Marcos 6-7) of `docs/roadmap/02-completion-plan.md`.

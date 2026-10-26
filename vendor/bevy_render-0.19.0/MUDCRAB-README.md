# bevy_render 0.19.0, vendored for Mudcrab Plus (portal branch only)

This folder is `bevy_render` 0.19.0 from crates.io. It is used through
`[patch.crates-io] bevy_render = { path = "vendor/bevy_render-0.19.0" }` in the workspace `Cargo.toml`.
It exists on the `portal` branch (Mudcrab Plus) only. Nothing of it goes to `main`, `phase*/*` or upstream
Mudcrab branches.

## Why

Portal Dev's trace of portal-heavy (trace-1125, 2026-10-09) showed that about 70% of each view's Core3d time is
`RenderContextState::apply`: every render-graph system's command encoder is finished (`finish()`, where wgpu
encodes the recorded passes into backend command buffers) one after another on the render thread, at the end of
the Core3d run. Analysis-1131 found no engine-only way to move or parallelise that work in Bevy 0.19. The smallest
fix is in this crate's `render_context.rs`: queue the encoders unfinished, in the same order, and finish them in
parallel (ComputeTaskPool scope, results collected in spawn order) when the frame's command buffers are taken for
the single submit. Expected saving: about 2.5-3.5 ms per frame on portal-heavy, the same image.

Research notes (local, not in git): `local/research/engine-foundations/analysis-1131-parallel-encoder-finish.md`
and `trace-1125-render-flush-2026-10-09.md`.

## Provenance

- Upstream crate: `bevy_render` 0.19.0, `registry+https://github.com/rust-lang/crates.io-index`.
- crates.io checksum (from `Cargo.lock` before this change):
  `a89b2cc850a4c3fd12a6a088a4fc3e80118b2e1b569da8936f6d4d99ca1b6ee7`.
- Upstream commit recorded in `.cargo_vcs_info.json`: `c6f634ca9f406d68ba5109d921247b654cb42c10`
  (`crates/bevy_render`).
- Decision: the user, 2026-10-09 (via Team Lead): yes to vendoring `bevy_render` 0.19.0 into the portal branch
  with the parallel-finish patch behind a flag (flag off = stock behaviour).
- Task: impl-1132 (queue B2d).

## What is patched

- Commit 1 (this folder as first added): the pristine published crate plus this README. Everything else is
  byte-identical to the registry copy (`.cargo-ok`, a cargo bookkeeping marker, is not copied).
- The original patch touches three files:
  - `src/renderer/render_context.rs`: an ordered list of finished buffers and unfinished encoders in
    `PendingCommandBuffers` and `RenderContextState`, the public `set_parallel_finish(bool)` / `parallel_finish()`
    switch (off by default), `last_parallel_finish_count()` (how many encoders the last take finished through the
    pool; a proof that the parallel path ran), and the doc-hidden `finish_in_order` / `Pending` pair that holds the
    ordering logic generically so it can be tested without a GPU device.
  - `src/renderer/mod.rs`: a 3-line change to the existing `pub use render_context::{...}`, re-exporting those items.
  - `src/render_phase/mod.rs`: a read-only `RenderMultidrawableBatchSet::entities()` iterator, so the destination readiness proof (`crates/engine/src/swap_bridge/systems.rs`) can ask whether an entity is queued in a multidrawable set. No behaviour change.

- Task #2143 adds owner-maintained change revisions in two more files:
  - `src/render_resource/pipeline_cache.rs`: `PipelineCache::readiness_revision()` changes when a cached
    pipeline becomes available or unavailable. Shader replacement/removal, synchronous creation, asynchronous
    completion, and blocking completion use the same state transition accounting. Nonready retries and
    unchanged queue processing do not advance the revision. Queuing a new, unavailable pipeline does not
    advance it either; consumers must also track changes to their required pipeline IDs.
  - `src/erased_render_asset.rs`: `ErasedRenderAssets::revision()` changes on insertion/replacement, removal
    of an existing entry, mutable access to an existing entry, mutable iteration, and device-recovery
    removals. Immutable reads and missing-entry lookups/removals do not advance it. Mutable access is
    conservative because the collection cannot observe writes through a returned mutable reference.

Both revisions start at zero and remain monotonic for the lifetime of their owning collection. Reading a
revision takes constant time and does not scan pipelines or assets. These APIs let cluster readiness cache
its material/pipeline proof between owner events. GPU-free tests cover the availability transition helper,
actual asset mutation APIs, and the device-recovery removal system. Live rendering and performance checks
belong to the engine integration.

- The post-ready cost work adds two read-only membership queries in `src/render_phase/mod.rs`:
  `BinnedRenderPhase::contains_main_entity(MainEntity)` uses the native cached bin key and checks actual
  membership in that exact bin, across all four phase types. Missing/empty cache entries and stale removed
  members return false. It follows the native dequeue-before-requeue contract and does not recover orphaned
  public-bin edits. `SortedRenderPhase::contains_main_entity(MainEntity)` probes the existing item map with
  IndexMap's `raw_entry_v1` API, preserving custom/duplicate render carriers and batch followers. Its hash
  lookup relies on Bevy 0.19's `EntityHash` replacing the hash on each `write_u64`, so a tuple key hashes like
  its final `MainEntity`; an explicit test checks this contract with the actual map hasher. Neither query adds
  an index, enumerates the phase, nor changes order, ranges, or membership. GPU-free native tests cover
  differential enumeration, bin transitions/removals, stale caches, prepare lifetimes, hash generations,
  and absence of sorted phase-item value visits. Test execution and live cost checks belong to integration.

Task #2153 also adds exact shadow dependency facts and native parity support:

- `render_asset.rs`, `erased_render_asset.rs` and `mesh/allocator.rs` publish
  content/changed-ID/allocation revisions at producer mutations. Immutable
  lookup does not certify arbitrary writes through escaped raw GPU handles.
- `render_phase/draw.rs` tracks structural draw-registry mutations in production.
  Binned and sorted phases skip missing draw functions and continue with the
  remaining phase items. Command Skip/success/failure outcomes are preserved.
- `render_parity.rs`, `render_phase/draw_state.rs`, `view/mod.rs` and `lib.rs`
  implement/export paired native attachment readbacks and actual-bind validation
  only with the default-off `gpu-reuse-verify` Cargo feature. The module, native
  pipeline/raw-pass checks, command/missing-draw audit hooks and extra attachment
  copy flags compile out without the feature. `NativeRenderAudit` exposes that
  safety scope to the PBR native emitter; it grants no retained-map eligibility.
  The raster audit reads other settings only with a nonempty
  `MUDCRAB_RASTER_PARITY_DIR`; malformed frame thresholds log a diagnostic failure
  and disable the audit without panicking or enabling copy flags.
- `texture/fallback_image.rs` omits unnecessary render-attachment capability
  from initialized sampled fallback textures. Sampling/copy capabilities and
  initial bytes remain; uninitialized depth/MSAA targets retain attachment use.

Reuse-specific raster, render-asset, erased-asset and fallback-image tests live
in adjacent `*_tests.rs` modules. Raster tests require `gpu-reuse-verify`.

For the complete invalidation and cooperative texture writer contract, see
`../../docs/design/cluster-gpu-exact-reuse.md`. Compare with the registry copy
allowing all files named in this README and the omitted registry bookkeeping.

## How to drop it

Remove the `[patch.crates-io]` `bevy_render` line from the workspace `Cargo.toml`, delete this folder, and
`cargo update -p bevy_render` (or any build) restores the registry source and checksum in `Cargo.lock`. The
engine's `--render-parallel-finish` flag, its call to `set_parallel_finish`, and consumers of the additional
phase iterator, membership queries and revision APIs must then be removed or migrated too.

## Plan

Offer the change to Bevy upstream through Upstream Dev (with the user's yes), so this fork has a planned end.
On a Bevy bump: 0.19.1 should apply as is. At 0.20, drop this crate and the `[patch.crates-io]` line instead: upstream bevyengine/bevy#24957 (in 0.20) has the same parallel finish, so this patch is a 0.19 backport. Do not rebase it.

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
- The patch touches exactly two files:
  - `src/renderer/render_context.rs`: an ordered list of finished buffers and unfinished encoders in
    `PendingCommandBuffers` and `RenderContextState`, the public `set_parallel_finish(bool)` / `parallel_finish()`
    switch (off by default), `last_parallel_finish_count()` (how many encoders the last take finished through the
    pool; a proof that the parallel path ran), and the doc-hidden `finish_in_order` / `Pending` pair that holds the
    ordering logic generically so it can be tested without a GPU device.
  - `src/renderer/mod.rs`: a 3-line change to the existing `pub use render_context::{...}`, re-exporting those items.

To verify the rest is untouched:
`diff -r <cargo registry>/bevy_render-0.19.0 vendor/bevy_render-0.19.0` lists only this README, the two patched
files above and the missing `.cargo-ok`.

## How to drop it

Remove the `[patch.crates-io]` `bevy_render` line from the workspace `Cargo.toml`, delete this folder, and
`cargo update -p bevy_render` (or any build) restores the registry source and checksum in `Cargo.lock`. The
engine's `--render-parallel-finish` flag and its call to `set_parallel_finish` must then be removed too.

## Plan

Offer the change to Bevy upstream through Upstream Dev (with the user's yes), so this fork has a planned end.
On a Bevy bump: 0.19.1 should apply as is. At 0.20, drop this crate and the `[patch.crates-io]` line instead: upstream bevyengine/bevy#24957 (in 0.20) has the same parallel finish, so this patch is a 0.19 backport. Do not rebase it.

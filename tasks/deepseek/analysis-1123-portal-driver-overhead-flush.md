# analysis-1123-portal-driver-overhead-flush (track: Portal; queue B2d). Read-only.

## Question
On portal-heavy, `render/graph/driver_overhead` is about 4.2-4.5 ms of the ~14.5 ms render thread
(median of 4 runs, 2026-10-09 13:19-13:26; local/review/portal-1121-window-2026-10-09/, render-cycles.csv per run).
driver_overhead = camera_driver - (camera_views + shadow_views): the part of each camera's Core3d run that no lap sees.
analysis-1106 ranks its owners (report: local/agents/runs/analysis-1106-portal-render-set-systems.1/report.md):
mainly the executor's end-of-run deferred flush (Core3d has auto_insert_apply_deferred false, so every system's
RenderContext encoder is finished and pushed after all systems end), then upscaling/ui_pass after our last lap,
the early gpu_preprocess chain and msaa_writeback before our first.

On Bevy 0.19.0 (we stay on it; local/research/engine-foundations/research-1119-bevy-020-impact.md for 0.20),
what can cut this cost? Rank options with an estimated ms saving each and a cheap way to measure each.

## Read first
- `rg -i "render thread|camera_driver|encoder|deferred|views" local/research/INDEX.md`, and the notes it lists.
- analysis-1106 report above; impl-1107 lap code: crates/engine/src/render_timing.rs (columns, what each lap covers).
- Registry root: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f (bevy_* 0.19.0 sources).
- render_skip.rs (impl-1121): a view can now skip its Core3d run on a frame while staying extracted.

## Cover at least
1. Which views pay the flush and how much each: from the render-cycles.csv columns (view_count, views,
   camera_views, shadow_views, driver_overhead) split cycles by view_count (3 vs 2 when a cadence skip drops one)
   and estimate the per-view cost; say what the data can and cannot separate.
2. Fewer views or encoders per frame: skip on cadence (the reflection already can via impl-1121; the portal camera
   via portal_every), merging views, or cutting passes per view (how many RenderContext encoders does each
   Core3d system open? count them per view from the sources: file:line).
3. Batching: can the per-system encoder finish/push be shared (one encoder per view, or per frame) with engine-side
   code only, e.g. our own systems, schedule config on Core3d (auto_insert_apply_deferred, ApplyDeferred points),
   or a RenderContext setting? Is it the finish() calls (wgpu/driver CPU work) or the push that costs?
4. A small local patch without forking Bevy: [patch.crates-io] of one file, a replacement system, or a
   schedule edit from our plugin. Say what each would touch, the risk, and what 0.20 changes.
5. The tail and head systems (upscaling, ui_pass, msaa_writeback, early gpu_preprocess): is any of them doing
   work we do not need for an image-target view (reflection, portal) and can it be switched off per view by
   config (e.g. Msaa::Off, no UI on that camera, no upscaling for an image target)?

## Foundations
No shortcuts: no fewer views, lower resolution, dropped passes or lower settings unless the look stays identical
(say so per option). Per view and per frame cost; data-driven, nothing tied to Riverwood or these routes.
For each option: what would break for a mod or another worldspace?

## Report
Ranked table: option, estimated ms (with how you got it), measurement (which column or lap, which scenario, how
long), effort, risk, 0.20 effect. Cite file:line for every source claim. No build, no engine runs. Do not edit
anything except your report.

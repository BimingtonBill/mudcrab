# Per-frame render-asset upload budget (`--max-upload-mib-per-frame`)

Phase 2 (upstream) task 622. Streaming stress runs show frame worst ~18-21 ms
against p50 ~4.7 ms, and the frames that spike are the ones where freshly
streamed models finish loading. The engine now caps how many bytes of new
render assets (meshes, textures) the renderer prepares per frame, using Bevy's
own `RenderAssetBytesPerFrame` limiter, so a cell's models arrive over a few
frames instead of in one upload burst.

## What changed

- `crates/engine/src/config.rs`: `EngineConfig::max_upload_mib_per_frame`
  (`usize`, default `16`), the `--max-upload-mib-per-frame <MiB>` arm in
  `from_args`, and `EngineConfig::max_upload_bytes_per_frame() -> Option<usize>`
  (bytes, or `None` when the option is `0`). The option is read once at
  startup. Bevy offers no setter, and no engine code writes the resource
  afterwards — extraction copies it out of the main world every frame, so a
  live graphics knob would be possible, but nothing does that today.
- `crates/engine/src/app.rs`: `upload_budget(&EngineConfig) ->
  RenderAssetBytesPerFrame` and one `insert_resource(render_asset_budget)` in
  `run`, next to the `EngineConfig` and `RenderOrigin` inserts and before
  `add_plugins(DefaultPlugins)`. `RenderPlugin` calls
  `init_resource::<RenderAssetBytesPerFrame>()`, which only fills the resource
  in when it is absent, so the configured value survives; the render world
  copies it out of the main world on every extraction
  (`bevy_render-0.19.0/src/lib.rs:396-402`).

No other file changed.

## How Bevy's limiter behaves (vendored `bevy_render` 0.19.0)

The engine compiles against `bevy_render` **0.19.0** (`Cargo.lock`), not
0.19.1; `render_asset.rs`, `erased_render_asset.rs`, `mesh/mod.rs` and
`texture/gpu_image.rs` are byte-identical between the two, so the line numbers
below match 0.19.1 as well.

- **Main-world resource.** `RenderAssetBytesPerFrame { max_bytes:
  Option<usize> }`, `render_asset.rs:492-498`; unlimited by default and
  initialised by `RenderPlugin` (`lib.rs:387`). Its render-world twin
  `RenderAssetBytesPerFrameLimiter` (`render_asset.rs:519`) is filled from it
  each extraction (`render_asset.rs:485-489`) and reset each render frame in
  `RenderSystems::Cleanup` (`lib.rs:433`).
- **Asset types covered.** Only two `RenderAsset` impls override the default
  `byte_len -> None` (`render_asset.rs:69-71`): `GpuImage`
  (`texture/gpu_image.rs:58-60`, `image.data.len()`, i.e. the decoded or
  KTX2-transcoded CPU bytes) and `Mesh` (`mesh/mod.rs:152-162`, summed
  attribute strides × vertex count + index bytes). Materials, bind groups and
  everything else report `None` and are never throttled; a `None` asset also
  writes 0 bytes to the budget, so it does not consume it.
- **Over-budget assets are deferred, never dropped.** In `prepare_assets` /
  `prepare_erased_assets` an asset whose `byte_len` is `Some` is pushed to
  `PrepareNextFrameAssets` when `bpf.exhausted()` and retried on a later frame
  (`render_asset.rs:402-405` and `:442-445`, `erased_render_asset.rs:394-397`
  and `:433-436`); on the next frame the queued list is drained first, oldest
  entry first (`render_asset.rs:390-391`). A queued entry is dropped only if
  the source asset was removed or re-extracted in the meantime
  (`render_asset.rs:392-394`), which is the correct outcome — the fresh
  extraction replaces it.
- **An asset larger than the whole budget still gets through, and no asset
  starves.** `exhausted()` is a *pre*-check on bytes already written
  (`render_asset.rs:561-568`), so the first sized asset of a frame is prepared
  whatever its size and only then writes its bytes (`render_asset.rs:415`).
  Bevy's own comment (upstream `render_asset.rs:398-401`) states the intent:
  at least one sized asset per frame, so the queue always drains. The budget
  is therefore a soft limit: it bounds a frame's upload work except when a
  single asset exceeds the budget on its own (a single 8K texture can still
  spike one frame).
- **One budget for every asset type, claimed by whichever system runs
  first.** `PrepareNextFrameAssets` is per type (`render_asset.rs:362-374`),
  but the limiter is a single resource holding an `AtomicUsize`
  (`render_asset.rs:519-523`) that every `prepare_assets` system draws on, and
  those systems are not ordered against each other. Under a sustained run of
  images, mesh preparation can therefore lose the race for several frames
  while the image queue drains. That is the intended trade (the frame gets
  one budget, not one budget per type), but it means "no starvation" is a
  per-type forward-progress guarantee, not a fairness guarantee between
  images and meshes.
- **Where it does and does not act.** The limiter gates the render world's
  `prepare_assets` systems (texture/buffer creation and the bytes written to
  the GPU). The main-world `extract_commands_and_assets` phase — where the
  image CPU data is cloned and the mesh data moved into the render world — is
  not gated: an asset is extracted once, and deferral only postpones its
  `prepare`. The measurement quoted in the brief attributes the worst frame to
  `extract_commands_and_assets`, so if the spike is extract-side rather than
  prepare-side this option will not remove all of it. I could not confirm that
  span's owner: the name does not appear anywhere in the vendored crate
  sources (`bevy_render-0.19.0/0.19.1`), so it comes from other tooling. This
  is the main uncertainty in the change and the first thing a GPU run should
  check.

## Default: 16 MiB

`--max-upload-mib-per-frame 0` is unlimited (identical to the pre-change
behaviour), which is what `RenderAssetBytesPerFrame::default()` means; the
default value is **16 MiB**.

- A 2K BC7 or UASTC texture with a full mip chain is ~5.3 MiB
  (`4 × 2^20 × 4/3`), so 16 MiB admits about three of them per frame — "a few
  ms of work" — while dozens of ordinary meshes (tens to hundreds of KiB each)
  still fit.
- The measured worst render-thread frame is ~13.4 ms against a p99 of 1.7 ms,
  i.e. a burst worth several times the steady-state work; a 16 MiB cap spreads
  that burst over a handful of frames without changing the steady state, where
  fewer than 16 MiB of new assets arrive per frame anyway.
- A cell that streams in with, say, ten 2K textures (~53 MiB) finishes
  uploading in about four frames (~67 ms at 60 fps) — a pop-in of a few
  frames, not a stall, which is the trade the option makes explicit.
- Lower (4-8 MiB) would leave most frames able to prepare under one 2K
  texture and stretch a cell's pop-in to a fifth of a second; higher (32+)
  would start to allow 6+ simultaneous 2K uploads, which is the burst this
  task exists to prevent. There is no GPU measurement behind the exact number
  (this task was tests-only); it is a reasoned starting point and the flag is
  there to tune it against a real stress run.

## Asset readiness

`track_asset_readiness` (`crates/engine/src/streaming.rs:829`) completes a
model on `asset_server.is_loaded_with_dependencies(...)` plus validation of
the spawned transforms, bounds, meshes, materials and images in the
**main-world** `Assets<...>` stores (`streaming.rs:873-885`). None of that
consults GPU residency, and the limiter defers only the render-world
`prepare`, so `metrics.assets_ready`, `max_asset_ready_micros` and the
`assets/model_ready` phase are unaffected by the throttle. What the throttle
delays is GPU residency only: a model can be counted ready and be drawn one to
a few frames later. No streaming-invariant path reads `RenderAssets`, so
nothing can stall or fail as a result.

## Tests

- `config::tests::defaults_to_a_sixteen_mib_upload_budget_per_frame` — the
  default resolves to 16 MiB of bytes.
- `config::tests::parses_the_upload_budget_and_lets_zero_mean_unlimited` — a
  value parses and `0` means unlimited.
- `app::tests::the_upload_budget_resource_carries_the_configured_option` —
  `upload_budget()` inserted into a real `App` world puts `Some(16 MiB)` /
  `None` in `RenderAssetBytesPerFrame`. This is the closest to "the built app"
  that a test can get without a GPU: `run()` builds the resource with this
  same function and inserts it with one `insert_resource` call. Nothing else
  in the repo reads or writes that resource.

## Merge note (portal branch)

`impl-621-p2-engine-help-and-unknown-flags` adds a `HELP_TEXT` const to
`crates/engine/src/config.rs` (one line per option, and a test holding its
option tokens equal to the parser's match arms) on a tree that does not exist
in this upstream checkout. When the two land together, that help text needs a
line for `--max-upload-mib-per-frame <MiB>` (0 = unlimited) and its token list
needs the new flag, or that test will fail. The parser arm itself is a
one-liner in the same flat `match`, so the hand-merge is small.

## Corrections after review (MiMo review-622, lead)

- The throttle DOES act inside the measured spiking phase: the render-timing tool cuts
  `extract_commands_and_assets` after `PrepareMeshes`, so `prepare_assets` falls inside it. The
  "main uncertainty" above resolves in the change's favour.
- Starvation: image and mesh prepare are ordered (`RenderMesh` after `GpuImage`), not racing, so
  meshes make no progress on a frame where images use the whole budget. The doc comment was fixed
  in cb917c9; commit eb7c1f8's body still overstates ("never starves") and is not amended.

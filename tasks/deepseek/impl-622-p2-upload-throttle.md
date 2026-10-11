# Goal
Track: Phase 2 (for upstream). Stop the worst-frame spikes when newly loaded models arrive while
streaming, by throttling how many bytes of new render assets (meshes, textures) the renderer
prepares per frame, using Bevy's own limiter.

# Context
- Your checkout is cut from upstream `main`. Upstream commit style (`perf(engine): ...`), no
  `[deepseek]` prefix, no project-internal references in code or commits.
- Measured on a stress run (camera flying, stream radius 3): frame worst ~18-21 ms against p50
  ~4.7 ms. The render thread's `extract_commands_and_assets` phase has p99 1.7 ms but worst 13.4 ms
  (rare big bursts), and those frames are the ones where models finish loading. The world-origin
  rebase costs nothing.
- Bevy 0.19 has a built-in throttle: `bevy::render::render_asset::RenderAssetBytesPerFrame` (a
  main-world resource, unlimited by default, `bevy_render-0.19.x/src/lib.rs` ~387) and
  `RenderAssetBytesPerFrameLimiter` in the render world (`render_asset.rs` ~385,
  `erased_render_asset.rs` ~377). Read how it works in the vendored source first
  (`C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_render-0.19.1/`):
  which asset types report a byte size (images? meshes?), what happens to an asset that is over the
  budget (deferred to the next frame, never starved?), and whether an asset larger than the whole
  budget still gets through.
- Add an engine option, `--max-upload-mib-per-frame <MiB>` (0 = unlimited), in
  `crates/engine/src/config.rs`, and insert the resource in the app setup (`crates/engine/src/app.rs`,
  where the render plugins are configured). Pick a default from what you learn (the aim: no frame
  uploads more than a few ms of work; a 2K BC7 texture with mips is ~5.3 MiB, a UASTC-transcoded one
  similar). Say why you chose it. If Bevy's limiter cannot be set per run (e.g. only at startup),
  that is fine: the option is read at startup.
- Also report the streaming benchmark's asset readiness: if throttling uploads delays when a model
  counts as "ready" (`track_asset_readiness` in `crates/engine/src/streaming.rs`), say so; a cell's
  models appearing a few frames later is acceptable, a stall is not.

# Scope
Owned files: `crates/engine/src/config.rs` (the new option, its default and parse test),
`crates/engine/src/app.rs` (inserting the resource only), and
`docs/handoff/impl-622-p2-upload-throttle.md`. Do not run the engine with a window or GPU; tests only.

# Done when
- The option parses (tests: default, a value, 0 = unlimited) and the resource is inserted with that
  value (a test on the built app if practical, otherwise say how it is wired).
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets -- -D warnings`
  `cargo nextest run -p engine`
- One commit, upstream style.

# Report
How Bevy's limiter behaves (file:line in the vendored source): asset types covered, over-budget and
larger-than-budget behaviour. The default you chose and why. Test names.

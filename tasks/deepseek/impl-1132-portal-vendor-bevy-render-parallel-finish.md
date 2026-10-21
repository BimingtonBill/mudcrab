# impl-1132-portal-vendor-bevy-render-parallel-finish (track: Portal / Mudcrab Plus only; queue B2d)

The user's decision (2026-10-09, via Team Lead): YES to vendoring bevy_render 0.19.0 into the portal branch with the
analysis-1131 parallel-finish patch behind a flag (off = stock), and YES to Vulkan-only backends for portal builds,
measured as a separate A/B row. Nothing of this goes to phase*/upstream Mudcrab branches.

Design: local/research/engine-foundations/analysis-1131-parallel-encoder-finish.md (read it all), evidence
local/research/engine-foundations/trace-1125-render-flush-2026-10-09.md.

## Do, as separate commits
1. **Pristine vendor.** Copy bevy_render-0.19.0 exactly as published (from the cargo registry
   C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_render-0.19.0) to
   vendor/bevy_render-0.19.0, and add `[patch.crates-io] bevy_render = { path = "vendor/bevy_render-0.19.0" }` to the
   workspace Cargo.toml. Check Cargo.lock now resolves bevy_render to the path and nothing else changes version. Add
   vendor/bevy_render-0.19.0/MUDCRAB-README.md: why (analysis-1131, trace-1125), upstream version 0.19.0 and its
   crates.io checksum from Cargo.lock before the change, the user's decision date, the one patched file, how to
   drop it (remove the [patch] line and the folder), and the plan to offer the change upstream. Commit 1 =
   pristine copy + [patch] + README only; build and tests must pass unchanged.
2. **The patch** (commit 2, only render_context.rs plus the minimum elsewhere in the vendored crate): analysis-1131
   §2 variant B. An ordered `Vec<Pending>` with `Pending { Buffer(CommandBuffer), Encoder(CommandEncoder) }` in
   PendingCommandBuffers and RenderContextState; with the flag on, flushing pushes the unfinished encoder; take()
   finishes all encoders in a ComputeTaskPool scope and returns buffers in the original order. Public APIs keep
   returning finished buffers. The flag: a public `set_parallel_finish(bool)` (AtomicBool) in render_context.rs.
   Flag off must be the stock code path (same order, finish where it was).
3. **Engine** (commit 3): `--render-parallel-finish` (default off) in crates/engine/src/config.rs + --help, calling
   the setter once at startup; and `--render-backends <all|vulkan>` (default all = today; portal builds' intended
   default becomes vulkan only after the window, so do NOT change the default now). With vulkan, set Bevy's
   WgpuSettings.backends to Backends::VULKAN in the RenderPlugin setup (find where the engine builds RenderPlugin /
   DefaultPlugins), so Bevy drops VALIDATION_INDIRECT_CALL (bevy_render settings.rs:130-144). Log the chosen adapter
   backend at startup (info).

## Tests
- Patch: unit tests in the vendored crate or the engine that the take() order with the flag on equals the order
  with it off for a mixed sequence of encoders and add_command_buffer buffers (use a headless wgpu device if one is
  available in tests; if no device can be created in CI-like tests, test the ordering logic on a stand-in type and
  say so).
- Flags parse (defaults, values, bad values); the backends choice reaches WgpuSettings.

## Constraints
Same image, same passes, same submission order. No change to default behaviour. Bevy 0.19.0 only. Keep the vendored
crate otherwise byte-identical (verify with a diff against the registry copy and report it).

## Done when
Through the gate with BUILD_GATE_OWNER=portal and your own CARGO_TARGET_DIR on D:
(D:/Mudcrab/builds/impl-targets/claude-impl-1132-target; first build is cold, check df -h /d, stop below 25 GB):
`cargo fmt --all -- --check` (exclude the vendor folder if fmt touches it; it must stay pristine),
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`,
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`,
and `cargo build --release --bin engine` through the gate. No engine runs (Portal Dev does the window).

## Report
Commits (hashes, one line each), the vendor diff check result, design notes with file:line, tests, check summary
lines, the release binary path, and anything unverified.

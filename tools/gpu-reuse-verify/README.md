# Development-only GPU reuse verification

Production builds do not select this directory or its core-pipeline copy. Root
Cargo.toml patches only bevy_render and bevy_pbr. The default-off engine feature
`gpu-reuse-verify` compiles the cull/shadow readbacks, replay safety checks,
verification CLI and explicit SceneState inventory diagnostics.

Cargo cannot enable a `[patch]` entry with a feature. Complete native camera raster
verification therefore needs both an explicit overlay and the feature:

```powershell
cargo clippy --workspace --all-targets --features engine/gpu-reuse-verify --config tools/gpu-reuse-verify/config.toml -- -D warnings
cargo nextest run --workspace --features engine/gpu-reuse-verify --config tools/gpu-reuse-verify/config.toml
cargo build --release -p engine --features tracy,renderdoc,gpu-reuse-verify --config tools/gpu-reuse-verify/config.toml
```

Run heavy commands through the shared build gate, with the worktree's own target.
PBR is excluded from the root workspace, so its own test targets need a separate
selection. Select the engine too, which owns the forwarding feature; Cargo rejects
feature selection when only an excluded dependency package is selected. Nextest
validates package filters against workspace names, so exclude the selected engine
package rather than naming the excluded PBR package in the filter:

```powershell
cargo clippy -p engine -p bevy_pbr --all-targets --features engine/gpu-reuse-verify --config tools/gpu-reuse-verify/config.toml -- -D warnings
cargo nextest run -p engine -p bevy_pbr --all-targets --features engine/gpu-reuse-verify --config tools/gpu-reuse-verify/config.toml -E 'not package(engine)'
```

The overlay is never auto-loaded: do not copy it into `.cargo/config.toml`.
It selects the dev-only exact 0.19.0 core-pipeline source with native emitters.
That crate requires its `gpu-reuse-verify` feature (default only for this dev copy),
which enables the renderer safety module. It is outside the production workspace.
The engine feature alone enables cull and shadow diagnostics against registry
core_pipeline; add the overlay when validating camera depth/normal/motion/HDR.

Cargo updates the core_pipeline lock entry when toggling this overlay. After dev
checks run `cargo metadata --offline --format-version 1` without the overlay and
retain the registry core_pipeline source/checksum in the committed root lockfile.
Do not use `--locked` while switching the overlay. Do not publish a dev binary.

Default production build:

```powershell
cargo build --release -p engine
```

No parity module, diagnostic resource, audit CLI field, GPU audit readback/copy flag,
replay loop or thread-local actual-bind validator is compiled in that build.
Force-recompute controls and production cache telemetry remain available for
same-binary quiet comparisons. All captures are TEST, never FPS acceptance.

Provenance and native protocol remain in
`bevy_core_pipeline-0.19.0/MUDCRAB-README.md`; the historical root-patch statement
there is superseded by this explicit overlay. Licenses and upstream LUT assets
are preserved. This directory is owned by #2153; no retention deletion is implied.

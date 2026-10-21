# Goal
Track: Phase 2 (for upstream). The engine draws a Skyrim-like sky instead of a flat clear colour: a
gradient dome around the camera, coloured like vanilla's clear-day sky, fading into a matching clear
colour at the horizon. Engine only; no converter or schema change.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`feat(engine): ...`), no
  `[deepseek]` prefix, no project-internal references (portal, DeepSeek, task numbers) in code or docs.
- Design: `local/research/sky-design-phase2.md` section 1 and PR 1 of its series; the underlying
  measurements are in `local/research/sky.md` §1.2 (the dome's vertex-mask table: R = Horizon,
  G = Sky-Lower, B = Sky-Upper by elevation; alpha 0 at 0° rising to 1 at 2.1°), §1.4/§3.2 (why the
  horizon band is transparent and the clear colour shows through as fog) and §2.6 (SkyrimClear day
  colours: Upper 21,77,117; Lower 60,135,183; Horizon 125,163,183; Fog Far 116,168,203, sRGB).
- Today the streaming camera has no clear colour of its own, no fog and no sky
  (`crates/engine/src/app.rs`, where the world camera is spawned with `Camera3d`, `StreamingCamera`,
  `DepthPrepass`, `OcclusionCulling`). Sky meshes are skipped at load (`crates/engine/src/streaming.rs`,
  the `meshes/sky/` case).
- The shape of the solution (from the design; improve it if you find a better fit, and say why):
  - a procedural hemisphere that follows the camera's position (not its rotation), large enough to sit
    behind the streamed terrain and inside the far plane;
  - an unlit material extension whose fragment computes `R*horizon + G*lower + B*upper` from the view
    elevation using the §1.2 mask table as piecewise-linear ramps, with alpha from the same table;
  - alpha-blended, depth-tested, no depth write, so it stays out of the depth prepass;
  - the camera's clear colour set to Fog Far, so the transparent band reads as haze;
  - hidden when the camera is in an interior cell.
- Put the colours and the mask table in one place as named constants, with a comment citing the
  vanilla data they come from, so a later PR can feed them from weather records.
- Keep the engine's existing fixtures (`--material-fixture`, `--terrain-water-fixture`, the streaming
  fixture, benchmarks) working. If the sky would change a benchmark's frame cost, say so.

# Scope
Owned files: new `crates/engine/src/sky.rs`, new `crates/engine/src/shaders/sky.wgsl`,
`crates/engine/src/lib.rs` (the module), `crates/engine/src/app.rs` (registration, clear colour, the
camera), a short `docs/specs/engine/sky.md`, and your handoff note `docs/handoff/impl-576-p2-sky-dome.md`.
No converter, database or schema changes. No new dependencies.

# Done when
- Unit tests pin the CPU-side pieces: the mask table's ramps at chosen elevations (0°, 2.1°, the
  table's breakpoints, 90°), the colour mix for SkyrimClear day, and the dome following the camera.
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings`,
  `cargo test -p engine` pass.

# Report
What you built and why that shape; files changed; test names; check output; what a render should show
(the lead renders it on real assets).

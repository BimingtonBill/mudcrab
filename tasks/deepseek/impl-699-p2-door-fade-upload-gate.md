# Goal
Track: Phase 2 (upstream #112, the door-crossing draft). The fade-in after a door crossing must not start
before the landing cell's textures and meshes are on the GPU. Today it waits only for the streamer
(`pending_asset_instances`/`pending_surface_instances` = 0); main's per-frame upload budget
(`RenderAssetBytesPerFrame`, `--max-upload-mib-per-frame`, default 16) can still hold uploads back, so the view
fades in with parts missing. Real-data evidence: Riverwood -> Sven's house, the stone hearth and chimney are
missing 1.5 s after fade-in at the default budget (`D:/Modding-overflow/door-evidence-run1/out_00173.png`) and
present with `--max-upload-mib-per-frame 0` (`D:/Modding-overflow/door-evidence-run2/out_00173.png`).

# Design (fixed)
1. While the screen is fully black (from the space switch until the fade-in starts), lift the upload budget
   (`RenderAssetBytesPerFrame { max_bytes: None }`); restore the configured value when the fade-in starts (and
   on the failure/restore path, and if the crossing is dropped). A stutter behind black is invisible.
2. Add a readiness check to the hold: every mesh, material and texture used under the landing cell's root is
   loaded (`AssetServer` load state `Loaded` or `Failed` for `Mesh3d`, `MeshMaterial3d<StandardMaterial>` and the
   material's image handles, plus any other material type the streamer spawns; say which you covered), then wait
   2 more frames so render extraction/prepare runs with the unlimited budget. Keep the 10 s timeout and WARN.
3. The INFO line keeps `milliseconds=` from E to fade-in start.

# Context
Base: branch `phase2/door-preload` head the lead names in "This run". Own worktree on D:
(`git -C C:/Modding/MudcrabProject worktree add D:/Modding-overflow/wt-699 <base> --detach`,
`CARGO_TARGET_DIR=D:/Modding-overflow/wt-targets/impl-699`). Upstream style `fix(engine): ...`, trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Owned: `crates/engine/src/door_crossing.rs` (and
`app.rs` only if the budget's configured value must be read from config there). Read-only everything else;
never physics.rs, console.rs, crates/converter, crates/shared. Overlap to name in the report: issue #117 plans
one "world is ready" signal for all gates; this is the crossing's local version.

Rules: no git push; commit named paths; no engine windows; NO background hold/wait/sleep commands (foreground
builds, timeout up to 600000 ms).

Acceptance: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo nextest run --workspace --all-targets`, `cargo test --workspace --doc`.

# Done when
Headless tests: the budget is lifted at the switch and restored at fade-in start (and on the failure path); the
fade-in waits while a landing-cell texture or mesh is still loading and starts once loaded (+2 frames); the
timeout still fades in.

# Report
Commit, how readiness is checked (which asset types), tests, exact acceptance results. Remove the target dir.

# Goal
Track: Phase 2 (for upstream). Add distance fog to the engine, fitted to Skyrim's clear-day
weather fog, so distant terrain and objects fade into the sky dome's horizon colour. Engine only;
no converter or schema change. This is PR 2 of the sky series; PR 1 (the sky dome) is the base.

# Context
- Your checkout is cut from `phase2/sky-dome` (599d5f2: `crates/engine/src/sky.rs`,
  `shaders/sky.wgsl`, `docs/specs/engine/sky.md`, the clear colour = Fog Far 116,168,203).
  Upstream commit style (`feat(engine): ...`), no `[deepseek]` prefix, no project-internal
  references (portal, DeepSeek, task numbers) in code or docs.
- Design: `C:\Modding\MudcrabProject\local\research\sky-design-phase2.md`, section 3 ("Fog:"
  and "Where the dome meets the fog at the horizon") and row 2 of the PR table in section 4.
  Measurements: `C:\Modding\MudcrabProject\local\research\sky.md` §3.5 (vanilla fog is
  `min(Max, t^Power)` with a Near→Far colour lerp; the fitted `FogFalloff::Exponential` density and
  its error table) and §2.6 (SkyrimClear fog values). Read both sections before writing code.
- Bevy 0.19 `DistanceFog` reaches StandardMaterial meshes automatically, and the terrain and
  water shaders through `main_pass_post_lighting_processing`. The sky dome's shader must stay
  unfogged (it does not call `apply_fog`; keep it that way).
- Every camera that renders the world needs the fog: the streaming camera and the fixture
  cameras in `crates/engine/src/app.rs` are separate spawns (the design lists them).
- The scale caveat in the design: vanilla fog reaches Max only at 35-72k units, while the engine's
  terrain ring ends nearer. Make the density a named constant with a comment, and choose it from
  the §3.5 fit; say in your handoff whether the far edge of the streamed ring shows a seam against
  the dome, from reasoning or from a `--shots`/screenshot run if you can run the engine.
- Fog off in interior cells (they have no weather), matching the dome's interior check.

# Scope
Owned files: `crates/engine/src/sky.rs`, `crates/engine/src/app.rs` (the camera spawns),
`docs/specs/engine/sky.md` (a fog section), and your handoff note
`docs/handoff/impl-580-p2-sky-fog.md`. No new dependencies.

# Done when
- A unit test pins the Near/Far/Power/Max → density fit against numbers from sky.md §3.5 (pure
  arithmetic), and one checks that fog is disabled for an interior.
- These pass in your checkout (shared warm check folder, so dependencies don't rebuild):
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo test -p engine --target-dir C:/Modding/shared-check-phase2`
- The handoff says which cameras got fog, the density chosen and why, and what a benchmark
  scenario's frame cost might see (fog is a per-fragment term).

# Report
Diff summary per file, the density and its derivation, the test names and results, open doubts.

# impl-1128-portal-water-ssr-prototype (track: Portal / Mudcrab Plus "Water Plus"; queue B13)

Base: portal head. A Plus feature: its own on/off setting, default unchanged (byte-identical default run).

## Goal
A setting `--water-reflection <planar|ssr>` (default `planar` = today). With `ssr`, the water reflects by a
screen-space ray march inside crates/engine/src/shaders/water.wgsl, and the planar reflection camera
(WaterReflectionCamera, render.rs ~800-1010, prepare_water_reflection ~1158-1254) is never demanded, so it never
renders (it stays switched off through the view policy: no demand = is_active false, view_policy.rs).

Design and evidence: research-1126 (local/agents/runs/research-1126-portal-water-ssr.1/report.md, read it all) and
websearch-1127 (local/agents/runs/websearch-1127-portal-water-ssr-practice.2/report.md). Summary:
- Bevy 0.19's ScreenSpaceReflections is deferred-only and cannot touch our forward, AlphaMode::Blend water.
- Main camera has DepthPrepass (app.rs ~2343): water.wgsl can read the opaque depth via prepass_utils::prepass_depth.
- There is no opaque-colour texture for a Blend material: add ONE copy of the main view's colour target after the
  opaque pass and before the transparent pass (a Core3d system, same slot Bevy's transmission copy uses:
  bevy_core_pipeline-0.19.0 / bevy_pbr transmission node ordering; research-1126 §2), into a per-view texture the
  water material binds. Only when the setting is ssr; only for views that draw water (main and portal views).
- March: linear + bisection along reflect(-V, N) in view space against linearised prepass depth (port the core of
  bevy_pbr-0.19.0/src/ssr/raymarch.wgsl or write it; ~16 linear + 6 bisection steps, a thickness test so rays
  behind objects miss), sample the opaque copy at the hit UV.
- Misses and fades: fall back to the sky colour the sky dome uses (share the function with sky.wgsl if practical,
  else the same space colours, atmosphere.rs); fade hits to the fallback near screen edges, for long rays, and for
  rays pointing back at the camera (websearch-1127: Unreal/Witcher 3 practice). Blend the result exactly where
  `reflection_color` is blended today (water.wgsl ~57-59), keeping the WATR F0/reflectivity uniforms (render.rs
  ~804-814), so every water type and mod water keeps working.
- A per-frame counter is a nice-to-have, not required.

## Constraints
- Default (`planar`) path unchanged: no copy, no new binding work, same schedule. Prove it in a test (the copy system
  does not run / the camera is still demanded under planar).
- Under `ssr`: the reflection camera is not demanded and never active; no shadow cascades for it.
- `--water-reflection-preset` keeps working for planar; with ssr it is ignored (say so in --help).
- No FormIDs, paths or scene knowledge; works for every water surface and worldspace; interiors with no water pay
  nothing. Portal view: water seen through a doorway must also use its own view's depth/colour (per-view textures).
- Bevy stays 0.19.0; no vendoring.

## Scope (owned files)
crates/engine/src/shaders/water.wgsl, crates/engine/src/render.rs, crates/engine/src/config.rs (flag + help),
crates/engine/src/view_policy.rs (only if the demand needs a switch), a new crates/engine/src/water_ssr.rs (copy
node/system, per-view texture, plugin), crates/engine/src/lib.rs (module line). Never stash, amend, rebase or push.
Commit only these paths, subject `[deepseek] impl-1128: ...`.

## Tests
Flag parsing (planar default, ssr, bad value errors); under ssr the reflection camera gets no demand and stays
inactive; under planar the copy system is not scheduled/does nothing; the shader compiles (a headless pipeline
check if one exists in the tests, otherwise naga validation of water.wgsl with the ssr shader def).

## Done when
Builds through the gate (BUILD_GATE_OWNER=portal); `df -h /c` and `df -h /d` first, below 15 GB stop and report.
If a timing hold is set the gate waits: keep working and retry.
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 150 steps; if longer, commit what passes and report what is left. Visual checks and timing are Portal Dev's
(engine runs with --shots); do not run the engine.

## Report
Design (file:line), the copy's placement and why it is before the water, the march and fade parameters, what the
default path does, tests, check results, and what you could not verify without running the engine.

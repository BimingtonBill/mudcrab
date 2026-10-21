# research-1126-portal-water-ssr (track: Portal / Mudcrab Plus "Water Plus"; queue B13). Read-only.

## Goal
Water reflections without the planar reflection view. Today a second Camera3d (WaterReflectionCamera,
crates/engine/src/render.rs ~800-1010, view_policy.rs) renders the mirrored scene into an image that
crates/engine/src/shaders/water.wgsl samples. That view costs ~1.4 ms of fixed render-thread time per frame plus
its own shadow passes (analysis-1123: local/agents/runs/analysis-1123-portal-driver-overhead-flush.1/report.md).
Question: can screen-space reflections (SSR) on the water replace it, behind a setting, at equal or better look?

## Read first
`rg -i "water|reflection|ssr|lighting modes" local/research/INDEX.md`; in particular local/research/water.md,
local/research/fun/lighting-modes.md, local/agents/runs/research-722-fun-renderer-inventory.1/report.md,
research-269 (reflection camera indoors), the analysis-1123 report. Bevy sources (0.19.0) in
C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f.

## Answer, with file:line
1. Bevy 0.19 `ScreenSpaceReflections` (bevy_pbr ssr module): what it needs (deferred? depth prepass, normal prepass,
   motion vectors?), which materials it applies to, when it runs relative to the main opaque and transparent passes.
   Can it serve our custom water material, which is drawn after opaque (alpha-blended? check water.wgsl and the
   material's AlphaMode / render phase in render.rs), or not? Is our main view forward-only (it is per research-722)?
2. If not: the alternative of marching SSR inside water.wgsl against the main pass's depth and an opaque-colour copy.
   What Bevy 0.19 gives a transparent material: the depth prepass texture (ViewPrepassTextures, binding names),
   a copy of the opaque colour (ViewTransmissionTexture via `specular_transmission` / screen-space transmission
   steps in bevy_pbr; or our own copy node). Cost estimate per frame at 1600x900 (ray steps x pixels covered).
3. Fallback for rays that miss or leave the screen: sky colour from our sky (sky.wgsl), an environment map (does the
   portal view have one? EnvironmentMapLight on the camera?), reflection probes (Bevy LightProbe +
   EnvironmentMapLight), or a blend. What does each cost and what does it look like at grazing angles.
4. What we would drop: the reflection camera's components and passes (count, from render.rs and the 1121 window's
   render-cycles.csv: local/review/portal-1121-window-2026-10-09/), the shadow cascades it renders, its target image.
5. What SSR cannot show that the planar view can (objects off screen or behind others, e.g. the far bank when
   looking down) and how bad that is for Riverwood's river poses (local/review/portal-water-preset-image-2026-10-09/
   water_poses.json).
6. Bevy 0.20 changes to SSR or transmission (research-1119 note) that matter.

## Foundations
No shortcuts (state any look loss per option); data-driven (works for any water surface, any worldspace, WATR
records); moddable (mods' water types keep working); Plus feature: its own on/off setting, default unchanged until
priced against 160 fps and the user says yes.

## Report
Recommended approach with a rough design (which passes, which textures, which shader changes, which files), a
ranked list of alternatives, estimated ms saved and added, risks, and how to measure. Cite file:line. Do not edit
anything except your report; no builds or engine runs.

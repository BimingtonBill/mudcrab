# Goal
Review the next slice of upstream issue #187 on the local branch `phase2/view-policy` (Mudcrab, Rust/Bevy 0.19): a shadowless route for the water reflection camera, a selectable `--water-reflection-preset`, and two measurement switches (`--water-reflection off`, `--shadows off`). Decide whether it is correct and safe, and find what used to work and now fails.

# Context
- Your working folder is the review folder: `delta.diff` (49961394 to 3008d8f1, see `commits.txt`), and `before/` and `after/` copies of every touched file. Read code only from there. `reviews/design.md` is the design study; this slice is its "Commit 2" plus presets and switches.
- What the coder says it did:
  - **The route.** New `shadow_route.rs` holds it. Layer 3 is `UNSHADOWED_REFLECTION_LAYER`. Terrain, the terrain fixture and the sky dome gain layer 3. An unshadowed copy of the sun lives only on layer 3; `sync_unshadowed_suns` (PostUpdate) copies the source `DirectionalLight` with shadows and contact shadows forced off, plus its Transform and Visibility. The copy exists only while a routed view wants shadows Off. `route_view_shadows` switches the camera's `RenderLayers` between [0] and [3].
  - **Presets.** The baseline preset is unchanged. The water preset is 512x288, every other frame, priority -100, shadows Off, no `OcclusionCulling`, `Msaa::Off`, layer [3].
  - **Switches.** `--water-reflection off` clears the demand, and the image stays black. `--shadows off` sets the main sun's `shadow_maps_enabled` to false.
- Bevy 0.19 sources: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_light, bevy_pbr, bevy_camera).

# What to check
1. **Baseline unchanged.** With the defaults, the main view, the sun, its shadows and the reflection are exactly as before. Pay particular attention to these:
   - Does adding layer 3 to terrain and sky change anything for the main camera or for the shadow casters?
   - Does a light on layer 3 affect a camera on layer 0 in Bevy 0.19? Check how Bevy filters directional lights by `RenderLayers`, both for lighting and for shadow-caster visibility, and cite the line.
2. **The water preset.**
   - The reflection on layer 3 sees terrain and sky lit by the unshadowed sun and NOT by the shadowed source sun.
   - The reflection camera really renders no shadow cascades of its own: check how Bevy builds cascades per view and per light, and whether a light that a view cannot see still gets cascades for that view.
   - Placed objects and water stay out of the reflection.
3. **Sync.** Every light property that matters is copied: colour, illuminance, direction, Visibility, and the shadow settings forced off. A disabled or despawned source removes the copy. No recursion.
4. **Switches and config.**
   - Parsing, HELP and defaults are right.
   - `--water-reflection off` cannot leave a stale image visible.
   - Black at fresnel times reflectivity is acceptable for a measurement switch, but say if it is misleading.
5. **Tests.** They exercise the real systems, and no test was weakened. Look specifically at the coder's change to the baseline-camera Msaa assertion.

# Scope
Read only. Change nothing.

# Report
- A verdict line: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - fix`. At most 10.
- What you verified as correct.
- What you could not check.
- Under 700 words.

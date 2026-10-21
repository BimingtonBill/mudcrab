# impl-196-portal-ring-follows-worldspace (track: Portal)

## Goal
Finish impl-190 (terrain ring off where the worldspace has LOD, merged into portal as ad1c60e2) so that every
reader of the ring follows the worldspace the player is in, not the starting one. Both independent reviews said
"fix first": local/agents/runs/review-1066-portal-merge-190.muse.1/report.md and
local/agents/runs/review-1068-portal-merges-all.glm.1/report.md (section impl-190). Read both first.

## Context (portal branch head, crates/engine/src)
- Resolver: `EngineConfig::effective_terrain_radius(has_lod)` (config.rs ~523); `WorldDatabase::has_lod_for(worldspace)`
  (world/database.rs ~455-466, ~775-797). Per-frame readers already re-resolve: plan (streaming.rs ~1123), unload
  (~5152), fog (atmosphere.rs ~723-731).
- `camera_far_plane(stream_radius, terrain_radius)` (app.rs ~2055) is applied once in `setup_world` (app.rs ~2287)
  from the starting worldspace and never updated; `update_atmosphere` (atmosphere.rs ~732) rewrites fog/sky on a
  space change but not `Projection::far`.
- Doorway preview fog: portal.rs ~2632-2639 passes raw `config.terrain_radius` to `atmosphere_fog` for the
  destination space.
- Plugin install: app.rs ~339-356 decides AtmospherePlugin + SkyDomePlugin once with the resolved ring of the
  starting worldspace (`atmosphere_applies(interactive, effective radius, stream_radius)`, atmosphere.rs ~69).

## Fixes (decided)
1. Far plane follows the space: when the active exterior worldspace changes (same trigger `update_atmosphere`
   uses), recompute the far plane from the resolved ring and write it to the streaming camera(s)' perspective
   projections. While there, check whether the far plane covers the terrain LOD (`LodHorizon` reach) when the
   ring is off: if LOD beyond the far plane is clipped today, make the far plane cover max(ring, LOD reach) and
   say so in the report; if LOD is drawn by other means, say how.
2. Doorway preview fog uses the resolver for the destination worldspace (`has_lod_for(destination)`), the same
   way the main view does after arrival, so the doorway and the arrived view agree. First check what the doorway
   view actually draws for an exterior destination (ring cells? LOD chunks? neither) and fix the comment at
   portal.rs ~2632 if it is wrong; report what you found.
3. Plugin install no longer depends on the starting worldspace's LOD: decide it from the RAW configured ring,
   `atmosphere_applies(interactive, config.terrain_radius, config.stream_radius)` (benchmark runs set the raw ring
   to stream_radius, so their behaviour is unchanged), so a tour/shots run that starts with LOD and crosses into a
   worldspace without LOD has fog, sky and lighting. Update the doc comments that describe the old rule.
4. Test: a worldspace switch re-resolves the ring. Extend the plan test fixture (`write_plan_database`,
   streaming.rs ~5970) or add one with `lod_chunks` rows for one worldspace only, and assert the wanted-cell set
   differs across the switch (ring vs no ring), plus a test for fix 1 (far plane after a switch) and fix 3 (install
   rule). Name the tests after the behaviour.
Also document (no code) in `has_lod_for`'s doc that it is worldspace-granular: one LOD chunk turns the ring off for
the whole worldspace (Muse finding 4).

Foundations check for the report: no shortcut (view distance unchanged), data-driven per worldspace, mod
worldspaces without LOD keep ring + fog + far plane.

## Scope (owned files)
crates/engine/src/app.rs, crates/engine/src/atmosphere.rs, crates/engine/src/portal.rs,
crates/engine/src/streaming.rs, crates/engine/src/world/database.rs (doc only unless a fixture helper must live there).
Nothing else. Never git stash, amend, rebase or push. Commit only these paths, subject prefix `[deepseek] impl-196:`.

## Done when
All through the build gate (BUILD_GATE_OWNER=portal); check `df -h /c` before each build, below 15 GB stop and report:
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
Keep the work small: about 60 steps. If you run long, commit what passes and report what is left.

## Report
Per fix: file:line of the change and the test that pins it; what the doorway view draws (fix 2) and the far-plane/LOD
finding (fix 1); check results (last lines); anything you could not do.

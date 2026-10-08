# impl-1094-portal-reflection-layer-coverage (track: Portal; queue B3)

Base: c4eb07b6 (phase2/view-policy merged into portal, not yet in portal; worktree local/impl/portal-merge-view-policy).

## Problem (review-1089 Muse + DeepSeek, both CONFIRMED)
Reports: local/agents/runs/review-1089-portal-batch2.muse.1/report.md, local/agents/runs/review-1089-portal-batch2.ds.1/report.md.
Under the water view-policy preset, the shadowless reflection route draws only UNSHADOWED_REFLECTION_LAYER (layer 4;
render.rs ~389, ~746). In a portal run almost nothing carries layer 4: portal stamps every cell with
root_layers(CellRole::Active) = layer 0 only (portal.rs ~2979-2986, applied ~3016, ~3405), so streaming.rs ~2356-2360 /
~2588-2592 adds the reflection layer only to un-stamped terrain; placed objects, sky domes, door-layer doors
(door_layer.rs ~74) and LOD lack it too. A cheaper reflection that draws less is a shortcut; the preset cannot be priced.

## Goal
Under the water preset, the shadowless reflection draws the same content as the normal reflection (everything the
world layer shows that a reflection should show), with only the shadows removed. Default path (no preset) unchanged.

## Work
1. Find every producer of world-layer RenderLayers in portal's code (portal.rs root_layers/stamping, streaming.rs,
   door_layer.rs, sky dome, LOD in streaming/lod.rs, water, lights) and make the reflection layer follow the world layer
   wherever the reflection route needs it: preferably one place (the stamping function / a single helper that every
   producer calls) so a new producer cannot forget it. Keep portal's role layers (door layer 3, destination layers)
   intact: a cell drawn only through the portal must not leak into the main reflection.
2. Tests: a parity test that, for each CellRole and each producer kind, an entity on the world layer also carries the
   reflection layer when the water preset's route is active (and not when it is off); a test that portal-only content
   stays out of the main reflection.
3. Docs: view_policy.rs water preset doc (drop the "mostly empty in portal runs" caveat once true), render.rs.
Foundations: no content dropped, data-driven per entity, no scene knowledge; the preset stays opt-in.

## Scope (owned files)
crates/engine/src/portal.rs, crates/engine/src/streaming.rs, crates/engine/src/streaming/lod.rs,
crates/engine/src/door_layer.rs, crates/engine/src/render.rs, crates/engine/src/view_policy.rs, crates/engine/src/sky.rs,
crates/engine/src/sky_dome.rs. Never stash, amend, rebase or push. Commit only these paths, subject `[deepseek] impl-1094: ...`.

## Done when
Builds must run. Through the gate (BUILD_GATE_OWNER=portal); `df -h /c` first, below 15 GB stop:
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 100 steps; if longer, commit what passes and report what is left.
## Report
Producers found (file:line) and how each now gets the layer; tests; check results; anything still not covered.

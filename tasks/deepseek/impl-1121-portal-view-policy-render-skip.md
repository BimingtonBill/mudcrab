# impl-1121-portal-view-policy-render-skip (track: Portal; queue B3 step 2)

Base: portal head (includes B3 = view-policy merge e1c4d77e + impl-1094).

## Goal
The water view-policy preset renders the reflection on a cadence by toggling Camera::is_active
(crates/engine/src/view_policy.rs ~396, ~585-589). Measured 2026-10-09 (local/review/portal-b3-window-2026-10-09/,
portal-heavy x4 interleaved): the preset makes the render thread 5.8 ms SLOWER (specialize_and_views +1.37,
queue +0.68, prepare +0.73, p95 +7.6 ms) while the graph saves only 0.85 ms, because an inactive camera's
view is torn down and rebuilt every other frame. Portal already solved this for its own camera: read
crates/engine/src/portal.rs ~5190-5400 (module `cadence`: "Why not Camera::is_active" and "The skip: the
camera's render schedule, not its extraction"; drop_skipped_portal_view removes the camera's entry from
SortedCameras after sort_cameras and prepare_lights, before render_system).

Make view-policy cadence skips use the same mechanism: the camera stays active; on a skipped frame its
SortedCameras entry is dropped, so its Core3d schedule does not run and its target keeps the last image.
Generalize portal's skip into one shared mechanism (a render-world marker or set of entities to skip this
frame) that both the portal cadence and the view policy use, so there is one implementation. View policy keeps
writing is_active only for views that are really off (policy says not rendered at all), not for cadence skips.

## Constraints
- Default path unchanged (baseline policy = every frame, no skip).
- Portal cadence behaviour unchanged (its tests must still pass; keep its rule: first frame of a doorway never stale).
- The skipped view's target keeps its image and its cached textures stay requested, as portal's doc explains.
- Foundations: no content dropped, no look change beyond what the preset already does; data-driven per view.

## Scope (owned files)
crates/engine/src/view_policy.rs, crates/engine/src/portal.rs (cadence module only, plus moving the skip to the
shared place), crates/engine/src/render.rs (registration), a new crates/engine/src/render_skip.rs if you need
one, crates/engine/src/lib.rs (module line). Never stash, amend, rebase or push. Commit only these paths,
subject `[deepseek] impl-1121: ...`.

## Tests
A cadence-skipped view policy camera stays is_active=true on skipped frames; its SortedCameras entry is absent on
skipped frames and present on rendered ones; portal cadence tests unchanged and passing; a fully-off policy still
writes is_active=false.

## Done when
Builds must run. Through the gate (BUILD_GATE_OWNER=portal); `df -h /c` first, below 15 GB stop. If a timing hold is
set the gate waits: keep working and retry.
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 100 steps; if longer, commit what passes and report what is left.
## Report
Design (file:line), how portal and view policy share it, tests, check results.

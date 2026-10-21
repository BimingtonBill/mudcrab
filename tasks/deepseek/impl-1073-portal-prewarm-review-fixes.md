# impl-1073-portal-prewarm-review-fixes (track: Portal)

## Goal
Answer the second review of impl-286 (prestream pre-warm, merged into portal 36d4fff3):
local/agents/runs/review-1064-portal-merge-286.muse.1/report.md. Read it first, and the GLM view in
local/agents/runs/review-1068-portal-merges-all.glm.1/report.md (section impl-286).

## Context (crates/engine/src/portal.rs, portal head)
- System chain: `update_portal -> prewarm_destination_view -> skip_occluded_doorway -> ... -> isolate_cells`
  (portal.rs ~514-561). `update_portal` writes `camera.is_active` on every path (~4470, ~4484, ~4614).
- Prewarm: warming sets `camera.is_active = true` (~4256); `go_idle()` exits at ~4100-4124, ~4147-4151, ~4211-4215,
  ~4229-4233; readiness settle ~4289-4300; WaitingLoad -> Warm gate ~4175 uses `destination_is_loaded`
  (transition.rs ~1168-1178, ALL destination keys); the landing-cell check is `destination_is_resident`
  (transition.rs ~1124).

## Work
1. Muse finding 1 (warm camera left active on idle exits) is believed to be a FALSE POSITIVE: `update_portal`
   runs first each frame and sets the portal camera's `is_active` on every path, so a prewarm `go_idle` exit leaves
   the portal's own value. Confirm with a test: a warm in progress, then the window empties (or the door leaves the
   candidates) mid-warm; over the next frames the camera's `is_active` follows the portal's decision (inactive when the
   portal has no doorway). If the test shows the camera stays active, fix it (deactivate on those exits, except the
   real-doorway branch) instead, and say so.
2. Muse finding 2: gate WaitingLoad -> Warming on the LANDING cell being resident (`destination_is_resident`), not
   on every destination key, so an exterior destination warms as soon as its landing cell is in, and later cells
   re-open the warm through the existing `created`-count settle (~4290). Test: an exterior destination whose
   landing cell is resident while other grid keys are still loading goes to Warming. Keep the 20 s tour cap and
   the readiness settle as they are. Update the doc comments; if docs/handoff text describes the gate, correct
   the impl-286 handoff note in docs/handoff/ only if it names the old behaviour wrongly.
3. Muse finding 3 (readiness uses global pipeline counts): doc comment only - name it next to the settle check.

Foundations check for the report: no frame or load caps added, nothing scene- or vanilla-specific; it warms more
of what the crossing needs, earlier.

## Scope (owned files)
crates/engine/src/portal.rs, crates/engine/src/transition.rs (only if a test helper must live there),
docs/handoff/impl-286-*.md (only for the gate wording). Nothing else. Never git stash, amend, rebase or push.
Commit only these paths, subject prefix `[deepseek] impl-1073:`.

## Done when
Through the build gate (BUILD_GATE_OWNER=portal); check `df -h /c` before each build, below 15 GB stop and report:
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 60 steps.

## Report
Finding 1 verdict with the test name and what it showed; finding 2 change (file:line) and its test; doc lines;
check results.

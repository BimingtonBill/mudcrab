# impl-225 - No churn when the player turns: keep predicted destinations and the destination layer for a short while

Owned files: crates/engine/src/transition.rs (door prediction only),crates/engine/src/portal.rs (destination hold only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. impl-223's measurement (team log 2026-09-26, no commit: destinations are already shared per interior) found two kinds of churn at the Riverwood Trader bench:
1. **Prestream churn:** interiors 78792 and 117636 were despawned and streamed again with new roots at every "open-behind" state (151 and 236 nodes). impl-215's door prediction (`predicted_doors`, top-2 doors ahead within 1500) drops a door the moment the camera turns away, so turning round releases and re-requests whole interiors.
2. **Re-layer churn:** `update_portal` clears `state.destination` whenever no door is shown, so each time the doorway leaves the view (impl-188 off-screen skip) or the portal drops, the interior's root goes destination -> hidden and back (879 nodes walked each way at the Trader).

## What to build
- Prediction hysteresis: a door that was predicted stays requested for a hold time (e.g. 8 s, constant with a comment) or until the player is more than the prediction radius + margin from it, even if it drops out of the top-2; re-ranking does not release it early.
- Destination hold: when the portal stops showing a door because it went off screen, occluded or behind (not because the door closed or the far side unloaded), keep `state.destination` and the cells' destination role for a short hold (e.g. 2 s) so turning back does not re-layer; a closed door or a different destination ends the hold at once. Keep impl-191's re-extraction and impl-195's change-driven isolation working (they rely on real role changes).
- Log (debug) how many re-layer walks and cell respawns happen per bench run.

## Checks
Unit tests for both holds (kept while turning, released after the hold or when the door closes). Before/after counts from a `--portal-bench` run at the Trader doors (make a small doors file as impl-223 did: `0x0001341F`, `0x00070E69`) and the full doors file: respawns and re-layer walks per run. Short tour passes; shadows after a quick round trip still hold (`--tour-dwell 2`, 3 runs, roof shadow on Sven's porch). Read only `local/t225*` under `local/`.

## Report
Changes, constants, before/after counts, test names, tours' last lines.

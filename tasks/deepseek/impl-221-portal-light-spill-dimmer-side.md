# impl-221 - The light spill only brightens the dimmer side, so it holds on doorways it was not fitted on

Owned files: crates/engine/src/portal_spill.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. impl-210 (`crates/engine/src/portal_spill.rs`, opt-in `--portal-light-spill`, commit 61dbb3d) softens the colour seam at doorways (fitted 4.40 -> 2.87, held-out 4.02 -> 2.82) but raises the brightness step where the interior is already brighter than the porch (held-out Gerdur close-up 1.63x -> 2.51x, Trader 1.31x -> 1.60x), so it stays off by default. Read impl-210's commit message and its shots/score method (the worker's scripts were in its worktree, now gone; re-create the score from the description: per shot, log of the brightness step + colour difference of two patches either side of the doorway plane).

## What to build
Change the spill so it narrows both gaps instead of adding light to both sides:
- add light only on the side that is dimmer at the doorway (measure each side's brightness near the threshold from the lighting inputs the spill already reads: ambient + nearby LIGH + sun share), scaled by the gap, so equal sides get nothing;
- keep the colour tint part (it is what worked), applied as a tint of the added light, and consider a small tint without added brightness on the brighter side;
- keep the fade with the door, the swap at a crossing, and the layer rules.
Fit on the same four fitting shots impl-210 used (Sven's House captures 01/03 and square views; the user's captures `local/captures/2026-09-25_10-27-59/`), check on the same held-out set (Gerdur close-up and square, Trader square, Honningbrew 04, an Alftand door). The brightness step must not get worse on any held-out doorway; say per doorway.

## Checks
- Unit tests: equal sides add nothing; the dimmer side gets the light; the fade and swap still hold.
- `cargo build --profile quick --bin engine`; before/after renders (`--shots` with `open_door`, `--portal-light-spill`), the score table fitted vs held out; the short tour passes with the flag.
- If it holds on every held-out doorway, say so: the lead will turn it on by default. Read only `local/t221*` and the capture folders under `local/`.

## Report
The rule, constants and what they were fitted on, the per-doorway before/after table (fitted and held out), test names, tour line.

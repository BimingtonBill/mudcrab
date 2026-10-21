# impl-232 - Light spill measured at the threshold floor; fix the Trader and Honningbrew

Owned files: crates/engine/src/portal_spill.rs,local/t232/ (outputs)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user affirmed this queue item on 2026-09-26.

## Background
The light spill (`--portal-light-spill`, opt-in, `portal_spill.rs`) lights each side of a doorway with the other side's light, so the seam at the doorframe matches.
- impl-221 (`git log --grep impl-221`) changed it to brighten only the dimmer side and tint both by the hue gap.
- Totals improved, but **the Trader (Riverwood Trader) and Honningbrew doorways still look worse with spill than without**, so it stays opt-in.

Suspected cause: the brightness the spill compares is measured in the wrong place, for example the mean over the whole doorway image or an ambient value. It should be measured **at the threshold floor on each side**, where the seam is seen.

## What to do
1. Read `portal_spill.rs` and impl-221's commit. Say what is measured and where.
2. Measure each side's luminance at the threshold floor: a small screen or world patch just inside and just outside the threshold, from the doorway's anchored threshold (portal.rs `anchored_threshold`, read-only). Drive the spill from the difference.
3. Render with and without spill, at the Trader, Honningbrew, Sven's House and Gerdur's House. The poses are in `tools/reference/doorway_shots.json` or `local/captures/`; open the door in the shot (see shots.rs).
4. Score each doorway by the luminance step across the threshold line: a 10 px band either side, in the rendered image.
   - With spill, the step must be smaller than without, at all four doors.
   - Tune on Trader and Sven. Check on Honningbrew and Gerdur, which you never tune on.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` with `CARGO_TARGET_DIR=D:/Mudcrab/portal-quick-target` and `CARGO_BUILD_JOBS=6`.
  - Other portal coders share that target dir, so cargo's lock runs one build at a time; the machine's memory commit only fits one.
  - Straight after each build, copy `engine.exe` to `local/t232/engine.exe` and run the copy, because the next coder's build overwrites the shared one.
- Tests and clippy use `--target-dir D:/Mudcrab/shared-check-target`.
- Assets: `C:\Modding\Mudcrab\SkyrimConverted`.
- No timing or benchmark runs.

## Commits
Subjects start `[claude] impl-232:`; trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No amend, rebase or force-push.

## Report
- What was measured before and after;
- the step table (4 doors × with/without spill);
- crops;
- whether the spill can become the default;
- the commit hashes.

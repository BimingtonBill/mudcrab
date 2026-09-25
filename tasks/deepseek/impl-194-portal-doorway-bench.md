# impl-194 - A doorway benchmark: frame times with a door closed, open in view, and open behind the player

Owned files: crates/engine/src/demo_tour.rs,crates/engine/src/config.rs (only the new flag),docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user wants the portal made faster (the doorway view today is a full-window second render with its own four shadow cascades: `local/agents/research-175.out` section 3, `local/research/frame-cost-audit-2026-09-25.md`). impl-188, impl-189 and impl-195 change that cost. Each needs a before/after number that means the same thing every time, and today there is none: the tour logs no frame times, and `--benchmark-only` flies a fixed path with no doors.

## What to build

`--tour-bench <file.csv>` (config.rs), used together with `--demo-tour` and `--tour-doors N`. At each outside door of the route, after the tour has walked up to it (before it presses E), hold the camera still and measure frame times for `BENCH_SECONDS` (3 s, after a 1 s settle) in three states:
1. **closed**: the door is closed, facing it;
2. **open-in-view**: the door is fully open, facing it, the doorway on screen;
3. **open-behind**: the door is still open, but the camera has turned 180 degrees, so the doorway is off screen.

Then carry on with the tour as usual.

Write one CSV row per door and state: `door, state, frames, mean_ms, p50_ms, p95_ms, p99_ms`. Reuse `crate::metrics`'s percentile helper; don't write a second one. At the end, print one summary line per state, averaged over the doors.

Keep the tour's verdict line unchanged. A bench run is not a timing run unless it is run alone; the README says so.

## Checks

- Unit tests: the percentile and CSV row formatting, and the state sequencing, as pure logic.
- `cargo build --profile quick --bin engine`. Then `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t194 --tour-doors 2 --tour-bench local/t194/bench.csv` must still end `tour PASSED after 2 crossings (short tour)` and write the CSV. Show the CSV.
- Under `local/`, read only `local/t194*` and the two research files named above.

## Report

What changed; test names; the CSV and summary lines; the tour's last line.

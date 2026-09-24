# research-157 - Make the automated checks faster and cheaper to run

## Goal

Measure where the time goes in the project's automated checks, and propose concrete changes that cut
it, ranked by minutes saved per day against effort. Candidates to measure or assess (not a limit):
cold vs warm `cargo test -p engine --lib` and which tests dominate; clippy/test/build sharing work
(profiles, `--all-targets` duplication, the dev profile's opt-level); whether a tour needs a full
release build or could use a faster profile; a single engine run doing both tours; shorter tour
settles/timeouts; running tours in parallel on one GPU; a smoke tour (one door) for quick iteration;
headless or smaller-window tours; cached shaders.

Give measured numbers with the exact commands, and for each proposal: the change (file and setting),
expected saving, risk, and how to verify it does not weaken the checks.

## Context

Rust workspace, Bevy 0.19 engine (`crates/engine`), converter (`crates/converter`), shared contracts.
Read `AGENTS.md` ("While working", the parallel-engine rules) and `docs/demo/README.md` first.
Automated testing today:
- `cargo test -p engine --lib` (~344 unit tests), plus converter/shared tests; CI runs
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, tests.
- Demo tours: `target/release/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour <dir>`
  (8 door crossings, then a walk test) and `--demo alftand` (4 crossings). About 5 min each, run
  one after another after a ~4 min release build. `crates/engine/src/demo_tour.rs`.
- Reference shots: `--shots tools/reference/*.json --shots-out <dir>` and `tools/compare_shots.py`
  (`crates/engine/src/shots.rs`, `docs/design/reference-shots.md`); door shots with `open_door`.
- Python tests under `tests/` (`python -m unittest discover -s tests`).

You may run `cargo test`, `cargo check`, `cargo build --release -p engine` and the release engine
(assets are read-only; write output only under `local/<your task name>/`). Do not edit tracked files;
this is research - your report is the deliverable. Keep to about 80 tool calls.

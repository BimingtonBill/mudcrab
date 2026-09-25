# impl-213 - The doorway benchmark records GPU and CPU time, which V-Sync cannot hide

Owned files: crates/engine/src/demo_tour.rs (bench code only),crates/engine/src/app.rs (plugin registration only, and only for bench runs),docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's machine forces V-Sync in the NVIDIA global settings, so every frame time is at least 16.67 ms whatever the engine asks for. impl-194's `--tour-bench` therefore reads 16.67 ms in every state; see `local/bench/*.csv` and the team log 2026-09-25 21:30. We need numbers V-Sync cannot hide.

## What to build

When `--tour-bench` is on, also record per state and per door:
1. **GPU time** of the frame's render passes, from Bevy 0.19's render diagnostics (`bevy_render::diagnostic::RenderDiagnosticsPlugin`; impl-189 read per-pass GPU times this way, see its note in the team log). Record:
   - the total GPU time;
   - the main opaque pass;
   - the portal camera's passes, if they can be told apart. Check how passes are labelled per camera; if they cannot be split, record the total and say so.
2. **CPU time**: the main-world update (Bevy's frame time excluding the present wait, if available, e.g. `FrameTimeDiagnosticsPlugin` against a system-time measure), or the time from frame start to the end of render-graph submission. Pick what is measurable and say what it means.

Add the columns to the CSV (`gpu_total_ms`, `gpu_opaque_ms`, `gpu_portal_ms`, `cpu_ms`: mean and p95) and the summary lines. Register the diagnostics plugin only in bench runs, so play is unaffected.

## Checks

- Unit tests for the new aggregation and CSV columns.
- `cargo build --profile quick --bin engine`. Then `--demo riverwood --walk --demo-tour local/t213 --tour-doors 4 --tour-bench local/t213/bench.csv` must pass and write the new columns.
- Show that `open-in-view` has a higher GPU time than `closed`, and that `open-behind` is back near `closed`. The numbers can be rough; nothing needs to run alone for this check.
- Under `local/`, read only `local/t213*` and `local/bench/`.

## Report

What each column measures and how; test names; the CSV; the tour's last line.

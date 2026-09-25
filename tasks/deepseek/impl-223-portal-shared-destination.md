# impl-223 - One destination instance per interior, however many doors lead into it

Owned files: crates/engine/src/portal.rs (destination instancing only),crates/engine/src/transition.rs (prestream keys only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Brainstorm item 5 (`local/research/brainstorm-portal-summary-2026-09-25.md`, `brainstorm-208-architecture-performance.md` idea 2): 334 of 663 interiors have more than one door in (`PortalGraph::doors_into`, impl-212). Check today's behaviour first: when two doors into the same interior are near (the Riverwood Trader's `0001341F` and `00070E69`), is the interior streamed/instanced once or twice, are its cells isolated onto layer 2 once, does switching the portal between the two doors re-stream or re-layer anything? Measure with a tour or `--portal-bench`-style run (log cell spawns and isolation walks). If it already shares, report and stop. If not, make destination cells keyed by space (not by door) so every door into a space reuses the same resident cells and layer assignment; keep impl-195's change-driven isolation and impl-191's re-extraction working.

## Checks
Unit tests for the sharing; a run at the Trader (both doors) showing one set of destination cells; short tour and full Riverwood tour pass. Read only `local/t223*` and the research files under `local/`.

## Report
What it did before (numbers), the change, test names, tours' last lines.

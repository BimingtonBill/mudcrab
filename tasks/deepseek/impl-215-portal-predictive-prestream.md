# impl-215 - Stream the far side of the doors the player is heading for, not only the one nearest

Owned files: crates/engine/src/transition.rs (plan_door_prestream and its helpers only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Brainstorm item 6 (`local/research/brainstorm-portal-summary-2026-09-25.md`, `brainstorm-208-architecture-performance.md` idea 3). Today `plan_door_prestream` pre-streams the destination of every door within `DOOR_PRESTREAM_RADIUS` (800) by plain distance, plus the far side of open/closing doors (impl-179/185). `crate::portal_graph::PortalGraph` (impl-212) can rank doors by likely next use: `rank_next_doors(space, pos, forward_xy, radius)`.

## What to build
- Use the graph to rank the doors near the player by `rank_next_doors` (heading from the camera), and pre-stream the top K (constant, e.g. 2) at a longer reach (e.g. 1500 units) in addition to today's 800-unit rule, so a door the player is walking towards is resident before they reach it; doors behind the player beyond 800 are not streamed. Keep every current guarantee (held-open doors keep their destination, impl-185's wider hold).
- Rate-limit re-ranking (the heading changes every frame): re-rank when the player has moved or turned past a threshold.
- Log when a predicted door's destination becomes resident and how far the player was from the door then.

## Checks
- Unit tests: ranking picks the door ahead, the top-K cap, doors behind outside 800 not requested, held-open doors unchanged.
- `cargo build --profile quick --bin engine`; the short tour and `--tour-doors 4` pass; from the log, the distance at which each tour door's destination became resident, before and after (it should grow).
- Read only `local/t215*` and the research files under `local/`.

## Report
Changes, constants and why, test names, before/after residency distances, tours' last lines.

# impl-195 - The portal's cell isolation walks a cell's descendants only when its role changed

Owned files: crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Starts after impl-188 and impl-189 have merged (same file).

From research-190 (`local/research/frame-cost-audit-2026-09-25.md`, CPU item 5): `isolate_cells` (portal.rs ~1867-1948) runs every frame over every cell root, about 289 of them in Riverwood. For each root it calls `iter_descendants` to rewrite roles and `RenderLayers`, even when nothing changed.

## What to build

- Recompute each root's role every frame (cheap: about 289 items), but walk a root's descendants only when:
  - its role changed this frame; or
  - it is new, or has new descendants (a scene that finished spawning adds children later: find how that is detected today, e.g. `Changed<Children>` or the streaming "instance ready" signal); or
  - it still carries `PortalOriginalLayers` that need restoring.
- Keep every current behaviour: destination cells on layer 2, hidden cells hidden, restored layers exact.
- impl-191 found that Bevy drops a shadow entity whose layers change at the wrong moment. So when you do re-layer a subtree, do it in one frame, as now.

## Checks

- Unit tests: an unchanged root is not walked; a changed role is walked once; a root whose scene gains children later is walked.
- `cargo build --profile quick --bin engine`; the short tour passes.
- With impl-194's `--tour-bench` (if merged): before/after CSVs. Otherwise: the per-frame time of `isolate_cells` from the profiler (`ProfilingState` / `record_elapsed`), before and after.
- Under `local/`, read only `local/t195*` and the research file named above.

## Report

What changed; test names; the before/after numbers; the tour's last line.

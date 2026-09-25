# impl-227 - The depth composite as the default doorway: backing cards hidden, a slab per door

Owned files: crates/engine/src/portal.rs,crates/engine/src/shaders/portal.wgsl,crates/engine/src/config.rs (flag only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. impl-211's spike (`--portal-depth-composite`, commit a5b5f1b; crops `local/research/impl-211/`) works: the doorway writes the destination's depth, so the real frame occludes interior beams (user capture 03). It clamps the written depth to an 8-unit slab because FarmhouseLDoor01's black backing card 8-24 units behind the doorway plane blacks the doorway out at larger slabs. Bench (`local/research/portal-bench-2026-09-26.md`): it costs nothing measurable.
Make it production:
1. While a door's portal is drawn, hide the source door's backing card (find the node: the card behind the doorway plane in the door model; general rule, not a model name list: e.g. source-side geometry of the door model behind the plane and inside the doorway rectangle) so the slab can grow to the frame's real depth; or a per-door slab from the door model's own depth behind the plane. Pick one, justify.
2. Remove the env knob, make the composite the default path, keep `--portal-depth-composite=off` (or a similar flag) as an escape hatch for comparison.
3. Check alignment (Sven's pixel-exact, `docs/research/portal-frames.md` §1 method), the user's poses 03/04 (`local/captures/2026-09-25_10-27-59/shots.json`, door open) and Gerdur/Chillfurrow/Trader doorways; the short tour and full Riverwood tour pass.
Report: the backing-card rule or slab rule, crops, alignment, tours.

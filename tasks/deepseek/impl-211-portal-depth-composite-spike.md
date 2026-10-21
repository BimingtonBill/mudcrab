# impl-211 (spike) - Can the doorway be composited by depth instead of a flat quad with a clip plane?

Owned files: crates/engine/src/portal.rs,crates/engine/src/shaders/portal.wgsl

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. This is a spike: prove or kill the idea with a working prototype behind a flag, `--portal-depth-composite`. The default behaviour must be unchanged, so nothing regresses if the answer is no.

## The idea

Today:
- The portal camera renders the destination through an oblique clip plane at the destination doorway.
- A flat opaque quad on the source doorway plane shows that image, and writes the plane's own depth.

research-205 and the brainstorm (`local/research/brainstorm-207-seamless-visuals.md` idea 1; `brainstorm-portal-summary-2026-09-25.md` item 3) point out that the destination's geometry leaks into the frame at grazing angles. The user's captures `local/captures/2026-09-25_10-27-59/03.png` and `04.png` show interior beams crossing the exterior frame.

The quad's rectangle is only an approximation of the real opening. Since impl-189, the portal camera is the main camera carried through the rigid door map, with the same projection cropped by `sub_camera_view`. So a destination fragment's depth in the portal view equals the depth that fragment would have in the main view, if the destination stood physically behind the doorway.

Therefore the composite can be done by depth:
- The doorway pass writes the destination's own depth (`frag_depth`, sampled from the portal camera's depth, which has a `DepthPrepass`) wherever the destination is visible through the opening.
- The real source geometry in front of it (the frame, the wall, the leaf) then occludes it naturally, at whatever shape the opening really has.

## What to find out

1. Can `portal.wgsl` read the portal camera's depth texture (Bevy 0.19 `ViewPrepassTextures`, or a copy into a texture the material binds) and write `frag_depth` with the same depth convention (reverse-Z, the same near plane)? Check `bevy_pbr`'s prepass and depth bindings.
2. How to bound the pass so it only covers "through the opening" without the rectangle's overshoot. For example:
   - draw a slightly larger quad, but discard where the destination depth is nearer than the doorway plane (that is what the clip plane removes today);
   - or keep the rectangle and rely on depth for the frame occlusion.
3. Prototype it behind the flag. Compare the user's poses 03 and 04 (`--shots local/captures/2026-09-25_10-27-59/shots.json`, door open) against the default path, and check Sven's House alignment (`docs/research/portal-frames.md` section 1 method) stays pixel-exact.

## Report

Feasible or not, with the reason; if feasible, the prototype's commit, before/after crops for 03/04, the alignment result, the cost (bench or frame time), and what a production version needs. Keep the default path untouched.

# impl-189 - The doorway view renders only the doorway's rectangle of the screen

Owned files: crates/engine/src/portal.rs,crates/engine/src/shaders/portal.wgsl

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Second half of the user's "portals should only render at the size of the portal and only render what the player sees through them". Needs impl-188 (`doorway_screen_rect`, off-screen skip) merged first. Read `local/agents/research-175.out` section 3 items 3 and 4.

## What to build

1. Each frame the doorway is drawn, take impl-188's screen rectangle of the doorway (clamped to the window), round it **out** to a multiple of 64 px (so the target is reallocated rarely, not every frame; keep the 2560x1440 ceiling), and:
   - size the portal target to the quantised rectangle (`resize_portal_target` / `portal_target_size`);
   - set the portal camera's `sub_camera_view = SubCameraView { full_size: <main target size>, offset: rect.min, size: rect.size }`, so its frustum is the cone through that rectangle (Bevy's `get_clip_from_view_for_sub` keeps the oblique doorway clip plane - check it in `bevy_camera-0.19.*/src/projection.rs`);
   - pass the rectangle to the shader: a `vec4` uniform on `PortalExtension`, and `portal.wgsl` maps `(in.position.xy - rect.xy) / rect.zw` instead of the main viewport (keep the clamp).
2. The image through the doorway must stay pixel-aligned with the world around it: `docs/research/portal-frames.md` section 1 describes the alignment check; run it (or its nearest equivalent) before and after.

## Checks

- Unit tests: the quantisation (rounds out, clamps to the window, keeps the ceiling), the sub-view numbers for a known rectangle.
- `cargo build --profile quick --bin engine`; the short tour `--demo riverwood --walk --demo-tour local/t189 --tour-doors 2` passes, and its door screenshots show the far side lined up with the doorway (compare with the same shots from before your change).
- A rough cost check (not a timing run; say so) with a doorway on screen, before and after. Read only `local/t189*` and `local/agents/research-175.out` under `local/`.

## Report

What changed; the alignment check's result; test names; the tour's last line; the before/after numbers and how they were measured.

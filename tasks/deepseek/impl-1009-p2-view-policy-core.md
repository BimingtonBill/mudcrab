# Goal
First slice of upstream issue #187 (Phase 2 track, engine foundations): a `ViewPolicy` layer that bounds the cost of the water reflection camera (target size, update rate, staleness) and refreshes it at once on discontinuities (teleport, render-origin rebase, interior/exterior switch, large camera jumps), on stock Bevy 0.19. Commits 1 and 3 of the design study; the shadowless route and telemetry come later.

# Context
- Worktree and branch: see "This run" below (cut from upstream main). Read the design study first, sections 3, 4 and "Proposed implementation" (commits 1 and 3): C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/research-1002-secondary-view-policy.md. Its file:line references were taken from upstream main; re-locate them here.
- Code: the reflection camera is in crates/engine/src/render.rs (`update_water_reflection_camera`, `surface_in_view`, `WaterSurface`, camera order -1); cameras and the streaming fixture (teleports) in crates/engine/src/app.rs; `RenderOrigin` and `update_render_origin` in crates/engine/src/streaming.rs; `CameraSpace` in crates/engine/src/sky.rs; the player camera follow in crates/engine/src/physics.rs.
- Bevy 0.19 sources: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_camera, bevy_render, bevy_transform `TransformHelper`).

- **Generic, not water-only.** The policy must drive ANY secondary camera: portal views (another session's feature) will use it next. Keep `view_policy.rs` free of water types: the policy, state, demand, invalidation message and `apply_view_policies` know only cameras, image targets, observers and demand. Water is one adapter in render.rs that fills `ViewDemand` (and the desired pose/projection) for its camera. Add one test that drives a non-water camera through the policy with a hand-filled `ViewDemand`.

# Scope (owned files: only these)
New crates/engine/src/view_policy.rs; crates/engine/src/lib.rs (module registration only); crates/engine/src/render.rs; crates/engine/src/app.rs (plugin registration and the fixture's explicit invalidation call only). Do not touch streaming.rs, sky.rs or physics.rs: read `RenderOrigin` and `CameraSpace` values from them. No new crate dependencies.

# Work (two items, one commit each)
1. **Policy core.** `ViewPolicy` (target_size, update_rate EveryFrame/EveryOtherFrame/OnDemand, priority, max_staleness, jump_distance, jump_angle_radians), `ViewPolicyState` (last scheduled frame/time/pose, dirty reasons, counters requested/rendered/skipped/forced), `ViewDemand`. One `apply_view_policies` system is the only writer of the managed cameras' `Camera::is_active`; it keeps the persistent image target (same handle) and resizes it only when target_size changes. Presets: baseline (today's behaviour: 1024x576, every frame) and water (512x288, every other frame, 100 ms max staleness, 256 units / 10 degrees jump). Keep baseline as the default for now so behaviour does not change until the preset is chosen; add a config/CLI switch only if config.rs is not needed (it is not owned; otherwise leave a TODO naming the switch). Move reflection preparation into PostUpdate before transform propagation and camera updates, as the study says, using current transforms (`TransformHelper`) and the current observer projection. No demand (no visible water, no observer): deactivate and keep the image.
2. **Invalidation contract.** An `InvalidateSecondaryViews` message with a reason and an optional target. The policy also detects, by comparing values: render-origin change, `CameraSpace` change, projection/target change, water height change, demand reopening, and accumulated observer pose jumps against the LAST SCHEDULED pose. A dirty demanded view renders this frame; a dirty view without demand stays dirty until demand returns. The streaming fixture's teleport sends the message. Document (doc comment) that future door/teleport code must send it after committing the destination pose and space.

# Tests
Deterministic frame sequences: every-frame, every-other-frame, on-demand; staleness deadline forces a render; demand reopening forces a render; no water / no observer deactivates; image handle unchanged across frames; resize only on policy change; teleport on a would-skip frame renders at once; origin change while hidden stays dirty until demand returns; space change; angular jump; accumulated small moves cross the threshold; explicit invalidation with an unchanged pose. Existing render.rs water tests stay green.

# Rules
- Never use `git stash` (the stash list is shared by all worktrees). Path-only commits (`git add <owned files>`), no amend, rebase or push.
- Commits: upstream style `feat(engine): ...`, each ending `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`.
- Before every build run `df -h /d /c`; below 10 GB free on either, stop and report. Use the CARGO_TARGET_DIR given under "This run" on every cargo command (pass `--target-dir`; exports do not persist between calls).
- Do not run the engine for timing. You may not run benchmarks.

# Done when
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo nextest run -p engine --all-targets` pass, with the new tests.

# Report
Status; commits; per item what and where (file:line); the schedule position of the systems; which study recommendations you did not follow and why; test counts.

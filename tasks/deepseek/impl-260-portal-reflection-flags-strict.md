# impl-260 - Reflection flags refuse bad values; the cascade-limit test checks what matters

Owned files: crates/engine/src/config.rs,crates/engine/src/render.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user approved this batch on 2026-09-27. Source: review-258 items 2 and 6 (`local/research/review-258-portal-demo-2026-09-26.md`).

## Goal
1. **Bad reflection flag values are errors (item 2).** `ReflectionOptions::parse_flag` (config.rs ~750-790) silently ignores `--reflection-cascades 5`, `--reflection-cascades two`, `--reflection-scale 0` / `1.5`, and a bare `--reflection-prepass` or `--reflection-occlusion` (no `=on`/`=off`), leaving the default. Every other knob in the project is fatal on a bad value with a message (see how `GraphicsSettings::set` returns `Err` and how `from_config` aborts, graphics_settings.rs ~412-516, and how EngineConfig reports errors). Make these errors the same way: the run stops at start with a message naming the flag, the bad value and what is accepted. Update `parses_the_reflection_cost_flags` (it currently asserts the swallowing as intended) and add tests for each rejected form.
2. **The cascade-limit test tests the design (item 6).** `the_reflection_view_keeps_only_the_cascades_asked_for` (render.rs ~1273-1320) runs the limiter in `Update` on hand-built `Cascades` and checks only truncation. The design depends on (a) the per-view frusta (`CascadesFrusta`) being left full, because Bevy's `check_dir_light_mesh_visibility` indexes per-thread lists by the previous view's cascade count and panics otherwise (bevy_light-0.19 lib.rs ~403-407, 466-476; the comment above the system explains), and (b) the system running in `PostUpdate` after `SimulationLightSystems::UpdateLightFrusta`. Make the test fail if either is lost: assert the frusta are untouched, and test the real registration (for example run Bevy's cascade and frusta systems around the limiter in the plugin's own schedule, or assert the system's ordering). Keep it a unit test (no GPU).

## Checks
- `cargo test -p engine --lib` in your worktree; check that your new or renamed tests appear.
- A run with a bad flag shows the error and exits non-zero: `<exe> --assets C:\Modding\SkyrimConverted --reflection-cascades 5` (quick build). A run with defaults still starts (let it run a few seconds, then stop it).

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree with its own target, `CARGO_BUILD_JOBS=6`; check `quick/engine.d` names your worktree.
- No timing runs. If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Handoff note in `docs/handoff/impl-260.md`.

## Report
The error messages, test names, the bad-flag run's output line and exit code.

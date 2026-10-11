# Goal
Fix the two failing tests in your impl-1077 work. The lead ran the acceptance for you: clippy is clean, and engine nextest ran 1150 tests with these 2 failing:
- profiling::tests::preupdate_asset_markers_run_around_bevy_asset_systems
- streaming::tests::a_crossing_frame_records_the_space_change_unload_when_profiling

# Scope
Same owned files as your brief: profiling.rs, streaming.rs, app.rs, render.rs. Measurement code and its tests only.

# Done when
Run each failing test alone to read its panic:
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --no-fail-fast -E "test(preupdate_asset_markers) or test(space_change_unload)"`
- BUILD_GATE_OWNER=p2 and CARGO_TARGET_DIR are already set in your environment.
- If the test's expectation was wrong, fix the test. If the measurement is wrong, fix the code. Never weaken what a test checks.
- Then run the clippy and nextest commands from your brief, and report their Summary lines.

# Report
Status / Summary / Left / Next. Then: each failure's cause, the fix, and the Summary lines.

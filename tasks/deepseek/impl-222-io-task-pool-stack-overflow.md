# impl-222 - "IO Task Pool has overflowed its stack" at dense city doors

Owned files: crates/engine/src/main.rs,crates/engine/src/app.rs (task pool setup only),crates/engine/src/streaming.rs (only if the recursion is there),crates/engine/src/render.rs (only if the recursion is in the glTF extension handler)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The overnight bench (`tools/bench/portal-bench.ps1`'s doors, release build) crashed in 3 of 4 runs with `thread 'IO Task Pool (N)' has overflowed its stack` (`local/bench/night-default.log`, `night-depth.log`, `night-bevy.log`; one run completed). It is pre-existing (impl-213 and impl-219 saw it too) and gets frequent at the dense city doors (Markarth, Solitude, Whiterun, Windhelm, Riften).

Phase 2 Dev's lead: upstream's model-bounds check had a recursive hierarchy walk (`accumulate_relative_bounds`) that overflowed the IO pool's smaller stack; fixed upstream as an explicit Vec stack (commit 0dfde9c, on PR #34). Portal does not have that function, so something else recurses on the IO pool: candidates are bevy_gltf's scene/node loading of deeply nested models, our glTF extension handler (render.rs `SkyrimMaterialHandler`), or any recursive walk run inside an asset loader.

## What to do
1. Find the recursion. Reproduce with `--portal-bench tools/bench/portal_bench_doors.json` (release or quick; `timeout 900`), with `RUST_BACKTRACE=1`; if the overflow gives no backtrace, run a debug build or set a small stack (e.g. env `RUST_MIN_STACK=262144`) to make it fail fast and find the frame, or bisect the door list to the model that triggers it and inspect that GLB's node depth.
2. Fix the cause if it is ours (explicit stack instead of recursion). If it is in Bevy (bevy_gltf node recursion), raise the IO task pool's thread stack: check Bevy 0.19's `TaskPoolPlugin`/`TaskPoolOptions` for a stack-size setting; if there is none, set the default spawned-thread stack at startup before any thread exists (`RUST_MIN_STACK`, e.g. 16 MiB, set at the top of `main` - note it must happen before Bevy spawns its pools) and document why.
3. Show the bench completes: 3 consecutive `--portal-bench` runs with no overflow (not a timing run; timing waits for the lead's quiet window).

## Report
Where the recursion was (with the backtrace or bisected model and its depth), the fix, test names, the 3 runs' exit codes.

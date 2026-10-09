# Goal
Prove in a real walked or flying run that impl-1059's paced unload never shows a retiring cell while it drains. This is the condition for 1059 going upstream. Build flag-gated instrumentation and run it once. Track: Phase 2.

# Context
- impl-1059 (unload pacer, in portal since e2038dc1): cells leaving the window become `CellStatus::Retiring { root, detail }` (crates/engine/src/streaming.rs ~908; see `last_retire_backlog` ~309, `retire_backlog_drains`/`retire_backlog_overflows` ~752-757). They are hidden at once and despawned over several frames under `--max-unload-ms-per-frame`. Unit test from f9d8f2b2: search streaming.rs tests for "drains invisible". Read that test first; the probe checks the same property in a real run.
- portal's head is ded5e573. Engine flags: crates/engine/src/config.rs. `--shots` and the screenshot path: crates/engine/src/shots.rs (see how it captures a frame).

# What to build
1. A flag, `--probe-retire-visibility <dir>` (name it to match the existing flag style), off by default, with zero cost when off: no system added, the same rule as the profiling-gated systems.
2. When on, on every frame with a nonzero retiring backlog:
   - count the retiring roots (and their descendants with a Mesh) that are in view and not hidden. Use the main world's `InheritedVisibility` / `ViewVisibility`: any retiring entity with `ViewVisibility::get() == true` counts.
   - Record per frame: frame index, backlog size, visible-retiring count.
   - Capture the frame as a PNG for the first N (about 5) backlog frames, plus every frame where the count is nonzero (cap the total). Use the engine's existing screenshot mechanism.
   - At exit, write `retire-visibility.json` in the dir: frames probed, max backlog, total visible-retiring hits (must be 0), the hit frames, and the PNG names.
   - The run fails (non-zero exit) if any hit was found.
3. Tests: off means no systems; a retiring root that is left visible is counted (inject one by hand); a correctly hidden one is not.
4. **The run:** use the quick build. Run the engine once with this flag on a route that retires cells under the pacer, `--max-unload-ms-per-frame 3`:
   - the lod-flythrough scenario, or a `--demo` walk in an exterior (find which; scripts/phase2-baseline.ps1 shows how lod-flythrough is launched);
   - assets: C:/Modding/Mudcrab/SkyrimConverted-portal-rooms.
   Output to D:/Mudcrab/evidence/p2-drain-probe-2026-10-09/ (create it with an OWNER.txt: owner Phase 2 Dev, class B, keep-until date:2026-10-30, links no).
   Before the run, check `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`. If any hold is listed, wait, and recheck every few minutes. Never run the engine during a hold.
   Look at 2 of the captured PNGs yourself and say what they show.

# Scope
Owned files: crates/engine/src/streaming.rs, crates/engine/src/config.rs, crates/engine/src/shots.rs, crates/engine/src/app.rs
- streaming.rs: the probe system and tests only, with no change to retire behaviour. config.rs: the flag. shots.rs or app.rs: only if reusing the capture needs a small hook.
- No new dependencies. No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Also the quick build for the run: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --profile quick --bin engine`.
- Set BUILD_GATE_OWNER=p2. Run `df -h /c /d` first; stop if either has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
Measurement only, off by default, no behaviour change. It works for any worldspace.

# Report
- The flag and the files changed (file:line).
- The run command, frames probed, max backlog and visible-retiring hits.
- The PNG paths you looked at, and what they show.
- The test names, the commands and their Summary lines.

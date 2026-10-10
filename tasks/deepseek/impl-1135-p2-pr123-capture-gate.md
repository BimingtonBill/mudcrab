# Goal
Answer Chef's CHANGES_REQUESTED on upstream PR #123 (dev console, branch phase2/dev-console): the acceptance screenshot can capture the open console. Gate the capture system on the console being closed and add the schedule regression test he specifies. Track: Phase 2 (upstream PR fix; Upstream Dev pushes it under the standing yes). Base: 3a90e53e (Chef merged main into the branch). Upstream commit style (`fix(engine): ...`), no `[claude]` prefix.

# Chef's review (verbatim, the contract)
@BimingtonBill — confirmed the remaining screenshot finding at 3a90e53: `capture_acceptance_screenshot` is ungated in `Update` and can enqueue a primary-window screenshot containing the open console; acceptance checks only that the file exists.

Please register it as `capture_acceptance_screenshot.run_if(console_closed)` in `crates/engine/src/app.rs`. This is sufficient for the reported open-console/closing-frame case: input updates state in `PreUpdate`, the predicate rejects both frames, and `refresh_console_ui` hides the overlay in the closing frame before capture resumes on the next update. No additional ordering between the two `Update` systems is needed for that case.

Add a focused schedule regression test using the real capture system with otherwise-ready window/renderer/assets and satisfied frame/time warmup: no `Screenshot` entity is queued while open or on the closing frame; after closing, exactly one is queued, with no duplicates on subsequent updates. Exercise the real console close transition rather than only testing the predicate; seed elapsed warmup in the test to avoid a wall-clock sleep. Include a closed-console baseline.

Keep `collect_and_finish` running: its existing 10-second wall-clock wait must still fail a missing screenshot rather than hang or falsely pass. The capture gate pauses its local frame counter and cannot cancel an already queued capture, so validate a normal acceptance run still writes its PNG and succeeds, and the deferred case resumes within that timeout. No timeout extension is requested.

# Context
- crates/engine/src/app.rs:219 registers `capture_acceptance_screenshot` ungated in Update next to `fly_camera.run_if(console_closed)` (:218). The system is at app.rs:2132. `console_closed` is crates/engine/src/console.rs:146 (true without the resource).
- crates/engine/src/metrics.rs holds `collect_and_finish` and its 10 s wait for the screenshot (around :150-225). Do not change its timeout.
- Existing console tests in console.rs (~:816) show how the console state and its close transition are driven; reuse the real input path or the real state change the console uses to close (not just setting the predicate's input), as Chef asks.

# Steps
1. app.rs: `capture_acceptance_screenshot.run_if(console_closed)`. Nothing else in the schedule.
2. A focused schedule test (in app.rs's tests, or where the capture system's other tests live): the real capture system with otherwise-ready window, renderer and assets, frame and time warmup satisfied by seeding elapsed time (no sleep). Cases:
   - closed-console baseline: exactly one `Screenshot` entity queued, none on later updates;
   - console open: none queued over several updates; the closing frame (drive the real close transition): none; the next update: exactly one; later updates: no duplicates.
3. Keep `collect_and_finish`'s 10 s wait failing a missing screenshot; if an existing test covers it, say which; otherwise add one that does not sleep for real (seed time).
4. Validate that a normal acceptance run still writes its PNG: if a test drives the acceptance path end to end without a GPU, use it; otherwise say so plainly in the report (the lead will run a real acceptance run).
5. Checks, all gated: `cargo fmt --all -- --check`; `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy --workspace --all-targets --all-features -- -D warnings`; `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`. Check `df -h /d` before each build; below 15 GB, stop.

# Foundations check
No shortcut: the screenshot is deferred, never skipped; the timeout still fails a missing capture; no weakened test.

# Scope
Owned files: crates/engine/src/app.rs, crates/engine/src/metrics.rs (tests only, if needed), crates/engine/src/console.rs (tests only, if a test helper is needed there). One commit, upstream style, e.g. `fix(engine): defer the acceptance screenshot while the console is open`.

# Report
Status / Summary / Left / Next, the commit, the tests added (names) and what each asserts, and the check results.

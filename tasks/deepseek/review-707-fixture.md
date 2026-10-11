# Goal
Pre-open review of a small test-only change for the Mudcrab project (an open Skyrim engine, Rust, Bevy 0.19): it makes the test `app::tests::streaming_fixture_loads_its_interior_by_id_and_keeps_it_while_the_camera_crosses_exteriors` independent of machine speed. It opens upstream as a DRAFT PR. Decide whether it is correct and whether it weakens what the test checks.

# Context
- Your working folder is the review folder. `delta.diff` is the change from upstream main 1b70b9b0 to the head 274a0da1 (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- The cause, per the author: the streaming fixture driver moves the camera on frame counts (around app.rs:470-495, frames 4 to 30), while the world-database worker thread answers requests in real time. On a busy CPU the camera returns to the start before any far cell is resident, so `unloaded_cells` stays 0 and the validator gives up at frame 300 (around app.rs:555-595).
- The fix: after each `app.update()`, the test waits until `requests_submitted <= responses_received + responses_waiting` (a new `#[cfg(test)]` accessor on `WorldDatabase`), with a 120 s deadline that only catches a dead worker. The old 16 ms sleep per frame is removed. The fixture driver, the validator and the `--streaming-fixture` run mode are not changed.
- Measured by the author: old test failed 3 of 3 under 64 busy-loop processes on 16 cores; new test passed 3 of 3 under that load and 3 of 3 quiet (about 21 s).

# Scope
Read only. Change nothing.

# Done when
Both touched files are read against `before/`. Check: can the wait condition hang or pass too early (are all submitted requests always answered, including stale or cancelled ones; can `responses_received` count something that is not a request answer; can the counts overflow or race)? Does waiting on every answer remove a behaviour the test should still exercise (stale responses, unloads, in-flight requests across frames)? Is the `#[cfg(test)]` accessor sound? Are the comments accurate?

# Report
Status, Verdict (accept / small fixes / fix first), one-paragraph summary, then findings as `path:line - problem - suggested fix`, most severe first. End with what you could not check.

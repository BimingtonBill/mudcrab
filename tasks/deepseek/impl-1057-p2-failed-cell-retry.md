# Goal
Make a failed exterior cell retry loading instead of staying failed forever. Today `CellStatus::Failed` is sticky while the camera stays near: the cell is never requested again, so its ground and every object on it stay missing for the rest of the visit. Fix it at the root with a bounded, logged retry.

# Context
- Mudcrab engine, Rust, Bevy 0.19. File: crates/engine/src/streaming.rs.
- Where a cell becomes Failed: around streaming.rs:802-817 (terrain snapshot or edge validation error), :842-855 (collider build error) and :860 (an error response from the database). All of them insert `CellStatus::Failed` with no reason kept.
- Why it is sticky: `request_cell` returns early for any key already in `streaming.cells` (around :138), and the plan pass (around :531-555) removes a `Failed` entry only once the key has left the unload radius.
- Analysis (read it first): local/research/engine-foundations/analysis-1055-terrain-missing-cells.md, section "Cause 3".
- Some failures are transient (a database or IO error, a collider build that failed because of load order). Others are deterministic data problems (a LAND grid that is not 33 wide, a non-finite neighbour edge): retrying those can never succeed.

# What to build
1. `CellStatus::Failed` keeps the attempt count, the reason (a short string or an enum) and when the next attempt is allowed.
2. The plan pass re-requests a failed cell inside the stream window once its backoff has passed (for example 1 s, 2 s, 4 s, measured on the real clock the streamer already uses, not frame counts). Use at most N attempts in total, with N a named constant and a comment saying why.
3. Each failure logs one `warn!` with the cell key, cell id, attempt number and reason. After the last attempt, one `error!` says the cell is given up, with the cell key, cell id, the reason and how many attempts were made. Nothing loops silently, and a failure is never hidden or counted as success.
4. A failure the code knows is deterministic (the data itself is invalid) may skip straight to the final state with the `error!`, so a bad cell is not retried pointlessly. Say in the report which cases you treat as deterministic and why.
5. A cell that leaves the unload radius and comes back later starts again with a fresh attempt count. That keeps today's behaviour.
6. Metrics: keep `failed_cells` and `terrain_validation_failures` meaning what they mean now. Add a counter for retries and one for given-up cells next to them in `StreamingMetrics`.

# Scope
Owned files: crates/engine/src/streaming.rs
- Change only streaming.rs and its in-file tests. If a test needs a helper that does not exist, put it in the test module.
- No new dependencies. No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --no-fail-fast -E "test(streaming)", python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings
- Before each acceptance command, run `df -h /c` and stop if C: has less than 15 GB free.
- Tests, written to catch the change being wrong:
  (a) a cell whose first load fails (a transient error) is retried after its backoff and ends Resident;
  (b) a cell that keeps failing stops after N attempts, ends in the given-up state with the reason, and is not requested again while it stays in the window;
  (c) a deterministic data failure is not retried;
  (d) a given-up cell that leaves the unload radius and comes back is requested again with a fresh count.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
What shortcut could a quick fix take here (an endless retry, a retry every frame, swallowing the error), and how does yours avoid it? What happens for a mod whose LAND grid has a different size, and in another worldspace?

# Report
- What you changed, and the decisions that were not obvious: N, the backoff and the deterministic cases.
- The commands you ran and the Summary lines they printed.
- What you assumed, and the cheapest check that would show the change is wrong.

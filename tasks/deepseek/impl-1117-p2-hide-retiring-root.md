# Goal
impl-1059's unload pacer must look identical to an unpaced unload. A cell that leaves the keep-set by distance is hidden the frame it retires, then despawned when the budget reaches it. Today it stays drawn until its turn. Team Lead's decision (option b), and the condition for 1059 going upstream. Track: Phase 2.

# Context
- Branch `phase2/despawn-time-budget`, head f9d8f2b2: impl-1059, already merged into portal.
- crates/engine/src/streaming.rs:
  - `plan_cells`' keep/retire match (~1325-1380 on portal; search "paces_its_unload" and `CellStatus::Retiring`);
  - the `Retiring` doc (~930-945), which today says "A retiring cell is still drawn until its turn; that is harmless". That is now wrong;
  - the revive path (`Retiring` back to `Resident` when the cell re-enters the keep-set).
- The drain probe found it: impl-1102, branch `phase2/drain-visibility-probe` bbeead81, flag `--probe-retire-visibility <dir>`. With `--max-unload-ms-per-frame 0` (count budget, 2 per frame) on lod-flythrough it found 64 visible-retiring hits over 8 frames. Evidence: D:/Mudcrab/evidence/p2-drain-probe-2026-10-09/count-budget-2/.
- Portal already hides retiring cells during a door crossing through `isolate_cells` (CellRole::Hidden; test `a_retiring_cell_drains_invisible_off_every_layer`). Do not fight it: the two must agree.

# What to change
1. When a Resident cell (or a Loading/Failed cell with a `replaced` root) becomes `Retiring` by distance, set the root's `Visibility` to `Hidden` that frame.
2. When a Retiring cell is revived to Resident, restore the visibility it had. Find what roots normally carry (`Visibility::Inherited`?) and whether portal isolation or room culling also writes root visibility; choose a restore that cannot override them (for example, a marker that the hide set, so only that hide is undone). Say what you chose.
3. Fix the `Retiring` doc.
4. Tests:
   - a distance-retired root is Hidden in the same frame;
   - a revived root draws again;
   - with a count budget of 1 and 3 retirees, every retiree is Hidden on every frame until despawned;
   - the portal test still passes.
5. Do not change the pacing, the budgets or the space-change path.

# Scope
Owned files: crates/engine/src/streaming.rs
- No git commands. No new dependencies.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Set BUILD_GATE_OWNER=p2. A timing hold is active now: the gate waits, which is expected. Write the code meanwhile.
- `df -h /c` first; stop if C: has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
Pacing hides nothing that unpaced would show, and shows nothing that unpaced would hide. No behaviour change except the visibility.

# Report
What changed (file:line); the restore choice; the test names; the commands and their Summary lines.

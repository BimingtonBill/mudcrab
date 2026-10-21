# Goal
Make the terrain-merge GPU probe's two lit cases actually test lighting. Today they pass however lighting behaves. Track: Phase 2, issue #186. The worktree already holds the shader fix for the probe's main failure (bacf8cc2, "merged terrain shader honours the unlit flag"). The 8 blend cases now match exactly.

# Context
- Worktree: C:/Modding/Mudcrab/worktrees/p2-terrain, branch `phase2/terrain-merge`, head bacf8cc2. Commit new commits in upstream style (`test(engine): ... (#186)`) and never amend.
- Probe: crates/engine/examples/terrain_merge_probe.rs. Last output: C:/Users/Brannock/AppData/Local/Temp/p2t-probe-unlit/probe.json, `passed: true`.
- Why the two lit cases are vacuous (found by debug-1076, its report is at C:/Users/Brannock/.claude-deepseek/agents/runs/debug-1075-p2-terrain-probe-offset.1/report.md):
  1. "front face, lit": the cell's albedo is full-bright blue, so it reads [0,0,255] with or without the light. It cannot fail.
  2. "back face, lit from above, turned over": the cell spans z -12288..-16384, entirely beyond the camera's far plane (12000, probe ~line 525). The sampled pixel is empty black space, so "must not be bright" passes without the cell being drawn at all.

# What to change
1. "front face, lit": give it an albedo below full brightness, for example mid grey, so that lighting visibly changes it. Assert both directions: lit by the sun it is clearly brighter than the same material with the sun off, or brighter than a stated lower bound; and it is not white-clipped. Pick a check that fails if the merged shader skips lighting for a lit material (the inverse of the bug just fixed), and also fails if it lights an unlit one.
2. "back face": move the cell inside the camera's near and far planes. Add an assert that the sampled pixel is covered by the cell, for example a second unlit sample of the same cell, so the case can't pass on empty space again.
3. Add a case for the bug just fixed: an `unlit: true` merged-terrain cell next to the sun reads its albedo exactly, within tolerance. Today's 8 blend cases cover this implicitly; make it explicit and named.
- Keep `tolerance_u8 = 3`. Never widen a tolerance to make a case pass.

# Rules
- Commands, with `BUILD_GATE_OWNER=p2` and `CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-terrain-target` already set in your environment. Run them exactly like this, with no prefix:
  - `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo run --example terrain_merge_probe -p engine -- --output C:/Users/Brannock/AppData/Local/Temp/p2t-probe-1078`
  - `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast`
- Run `df -h /d` first; stop if D: has less than 15 GB free. Run `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status` before a probe run; if a hold is listed, wait or report.
- Change only crates/engine/examples/terrain_merge_probe.rs. If the probe proves an engine bug, stop and report it: do not fix it.

# Done when
The probe passes with all its cases. Show, by a temporary local experiment that you revert, that each lit case now fails when lighting is wrong; for example, temporarily force the unlit path for the lit cell. Report the experiment's numbers. Clippy and engine nextest are green.

# Report
Status / Summary / Left / Next. Then: the new cases and what each asserts, the experiment numbers, the final probe.json summary, the clippy and nextest Summary lines, and the commits.

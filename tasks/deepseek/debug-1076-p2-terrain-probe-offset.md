# Goal
Find the root cause of the terrain-merge GPU probe failure, and fix it. Each blend quadrant renders brighter than its reference, with a whitish offset of up to 190/255. Track: Phase 2, issue #186 (the engine half of the terrain merge). This is behaviour nothing documents: prove the cause with a measurement before you change code.

# Context
- Worktree: C:/Modding/Mudcrab/worktrees/p2-terrain, branch `phase2/terrain-merge`, head 0d1d3bc3 (clean). Commit your fixes there as new commits, in upstream style (`fix(engine): ... (#186)`), and never amend.
- The probe: crates/engine/examples/terrain_merge_probe.rs. It runs `cargo run --example terrain_merge_probe -p engine -- --output <dir>` and writes probe.json and probe.png. The last outputs are in C:/Users/Brannock/AppData/Local/Temp/p2t-probe/.
- The last result, before your fixes:
  - The back-face case passes, and the lit front-face case is correct: blue [0,0,255].
  - The 8 blend quadrants fail with max error up to 190 against a tolerance of 3. For example, stride8 quadrant 0 is [255,255,190] against reference [187,188,0]; quadrant 1 is [190,255,190] against [0,255,0]; quadrant 2 is [37,37,255] against [0,0,255].
  - A whitish offset is added everywhere, and red and green are lifted the most.
- Candidate causes, none tested yet:
  - ambient, environment or other light left in the probe scene (the probe sets `GlobalAmbientLight` to 0, but StandardMaterial may still get light from elsewhere);
  - sRGB versus linear on the probe's array textures or reference colours;
  - KTX2 array colour space;
  - weight normalisation (sum(tex*w)/sum(w));
  - an array layer lookup that picks the wrong layer;
  - the shader blend itself.
  
  Note that the offset is not uniform: reference [0,0,255] gives [37,37,255], while [0,255,0] gives [190,255,190]. That may point to a weight or layer mix-up, not light.
- Design references:
  - C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/analysis-1034-terrain-path-map.md (today's terrain shader and blend);
  - analysis-1037-bevy-bindless-terrain.md (same folder);
  - C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/format-proposal.md (the agreed format: layers, weights, colour space).
- Engine files: crates/engine/src/terrain_merge*.rs or wherever the merged-terrain material and shader live (search for `terrain_merge`), the merged WGSL shader under crates/engine/src/shaders/, and render.rs.

# Method
1. Measure. Change one thing at a time in the probe, not the engine: unlit output, a single layer at weight 1, the raw sampled texel written out, the weights written as colour. Find which stage adds the offset. Record each experiment and its numbers.
2. Fix the cause at the right layer. If it is a probe bug (scene light, reference maths), fix the probe. If it is engine code (shader, colour space, weights, layer index), fix the engine and add a unit test that would have caught it. Never widen the tolerance, and never special-case the probe.
3. Rerun the probe until every case passes. That includes the RENDER_WORLD-only array images.
4. Rerun the checks the last change (0d1d3bc3, `naga_oil` without default features) has not had: `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo nextest run -p engine --all-targets`, including the `shader_check.rs` tests.

# Rules
- Every cargo command goes through the gate, with the build on D: and BUILD_GATE_OWNER=p2. Use this form:
  `CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-terrain-target python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo <...>`
  Run `df -h /d` first, and stop if D: has less than 15 GB free. The first build is cold, about 15 min.
- The probe uses the GPU. Before each probe run, run `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`; if a hold is listed, stop and report.
- Probe output goes to C:/Users/Brannock/AppData/Local/Temp/p2t-probe-<n>/.

# Report
Status / Summary / Left / Next. Then:
- the experiments with their numbers;
- the root cause, with file:line;
- the fix commits;
- the probe's final probe.json summary;
- the clippy and nextest Summary lines;
- what you could not verify.
Foundations check: is the fix a root-cause fix? Does it hold for any land texture set, including a mod's textures in another colour space or format? Is anything hard-coded?

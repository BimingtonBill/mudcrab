# Goal
Compare Chef's upstream work on streaming/missing ground with our Phase 2 work, so Team Lead and the user can decide how to coordinate. Analysis only, read only; nothing is posted anywhere. Track: Phase 2.

# Upstream items (Mudcrab-Team/mudcrab; read with `gh pr view N --repo Mudcrab-Team/mudcrab`, `gh pr diff N --repo Mudcrab-Team/mudcrab`, `gh issue view 202 --repo Mudcrab-Team/mudcrab`; read-only gh only, never comment)
- PR #201 fix(engine): retry meshes delayed by the GPU upload budget
- issue #202 Plan: adaptive asset admission, startup loading and smooth world streaming
- PR #206 bounded frame traces for streaming diagnostics
- PR #210 prioritize nearby demand and bound shared scene loads
- PR #211 opt-in streaming reservations and adaptive pacing
- PR #212 compare static and adaptive streaming on real-time routes

# Our work (branch portal in C:/Modding/Mudcrab/mudcrab; read with git show / git log)
- The drawn bridge for #1055 missing ground: crates/engine/src/swap_bridge.rs (SwapWaiting, SwapPrepared, SwapDrawn, SwapInCameraView, stand-in, settles), design tasks/deepseek/design-1091-p2-terrain-holes-prepared-bridge.md, analysis local/research/engine-foundations/analysis-1055-terrain-missing-cells.md.
- Pacing: crates/engine/src/pacing.rs and pacing/startup.rs (unpaced first load, impl-1124), the cell retire pacer in streaming.rs (CellStatus::Retiring, RetireHidden, --max-unload-ms-per-frame), the model spawn cap, RenderAssetBytesPerFrame upload budget, the failed-cell retry (impl-1057).
- Our upstream issue #117 (read it with gh) is the area Chef's #202 implements.

# Questions
1. #201: what exactly does it fix (mechanism, file:line in its diff)? Is it the same root cause as our #1055 analysis (render assets deferred by the upload budget, CPU-side ready flag vs GPU prepare)? Does our drawn bridge make it unnecessary, does it make our bridge unnecessary, or are they complementary? What happens if both land (conflicts, double handling)?
2. #210/#211/#212 vs our pacing work (impl-1124 unpaced startup, retire pacer, spawn cap, upload budget): overlap, conflicts, and what each does that the other does not.
3. A table: our piece | Chef's piece | same problem? | relation (duplicate / complementary / conflicting) | suggested action (drop ours, rebase ours on his, offer ours to him, keep separate).
4. Main-based vs portal-based: which of our pieces exist on main (upstream) vs only on portal?

# Report
Status / Summary / Left / Next, then answers 1-4 with file:line evidence. Write the note to local/research/engine-foundations/analysis-1137-chef-201-vs-drawn-bridge-2026-10-10.md if you can (else put it in the report) and say so.

# Goal
Propose ONE consolidated fix for the water reflection's cost, replacing four overlapping efforts, and say which branches it replaces. Track: Phase 2 (steady-state 160 fps). Proposal only: read only, change nothing.

# Context
Checkout: C:/Modding/Mudcrab/mudcrab (git repo). Use git read commands only.
- The efforts to reconcile:
  - `phase2/water-reflection-gating` (6dc630f1): render the water reflection only while water can be seen;
  - `phase2/reflection-land-only`;
  - `phase2/reflection-shadowless-sun`;
  - `phase2/reflection-no-prepass`;
  - upstream issue #187 slice 2. Find its branch or PR; grep the research index local/research/INDEX.md and local/team/*.md for "187". Portal Dev is merging #187 into portal now.
- Steady-state render-thread evidence: local/research/portal-heavy-render-thread-2026-10-08.md (§4 ranks the levers; find the reflection's share).
- Water code: crates/engine/src/render.rs (the reflection camera, `WaterReflectionTexture`, the water material) and the reflection pass setup; app.rs.

# Done when
1. For each effort: what it changes (files, mechanism), its measured or claimed gain with the source, its state (merged where, reviewed or not), and how it overlaps or conflicts with the others.
2. One consolidated design:
   - what the reflection should render (which layers or geometry, shadows yes or no, prepass yes or no);
   - when it renders (the visibility gate);
   - at what cost.
   Put each piece in the right layer, data-driven, with nothing that hides a visual difference without the user's yes. Say plainly which visual changes it makes, against today's reflection, for example "land only" drops objects from the reflection, a fidelity cut that needs the user's yes.
3. Which branches it replaces, what it keeps from each, and what must land first (#187).
4. How to measure it: which runs and spans, before and after.

# Foundations check (answer explicitly)
Which parts are root-cause cost fixes, and which are fidelity cuts (shortcuts unless the user agrees)? What breaks for a mod (custom water, other worldspaces)?

# Report
Status / Summary / Left / Next, then the per-effort table, the design, the replacement map and the measurement plan. Write it also to local/research/phase2-water-reflection-consolidation-2026-10-08.md.

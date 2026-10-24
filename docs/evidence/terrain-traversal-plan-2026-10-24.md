# Terrain traversal implementation plan

Goal: remove traversal-sized terrain surface rebuilds, then evaluate safe source handover.

Base: `91c23bbd`, branch `phase2/terrain-handoff-traversal`. No publishing or engine runs.

1. Pin incremental extraction with an actual ExtractSchedule fixture comparing each mutation against a fresh full build. Preserve unaffected surface allocations. Track identity deltas at their owner, refresh changed batch roots and affected surfaces only, retain reverse named-hold dependencies, increment revision once per changed map. Keep initial/package/worldspace and expired-journal full recovery, with `terrain_handoff/extract_full_rebuild` counter. Check the revision-keyed gate ownership cache and the live-surface readers in the post-queue filter and late clipping.
2. Trace cluster plan qualification, actual drawing and withdrawal ownership. Hide stock in the main world only if every terrain-capable view is covered and drawing is proven. Retain cluster drawing through stock restoration until current view/pass StockDrawable proof. If actual draw ownership cannot retain the old plan safely, document the blocker in a separate commit instead of hiding stock unsafely.
3. Per code commit: gated fmt check, engine all-target/all-feature Clippy with warnings denied, engine all-target nextest. Independent Codex review. Report exact proof limits; no timing/visual claims without engine execution.

Review focus: identity ancestry and default disablers; merged component removal; batch mask/mesh/root withdrawal; named stand-in hold removal; package/worldspace changes and skipped extraction windows.

Research indices and local plans are absent in this new worktree. Existing source and supplied Tracy evidence establish the target; actual GPU execution remains outside this task.

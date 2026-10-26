# GPU reuse review corrections (#2153.1)

Goal: retain exact GPU reuse while closing the independent review findings.

The accepted specification is `D:/Mudcrab/review/drafts/REVIEW-2153/review.md` plus
this task thread. Bevy stays at 0.19.0. No push, content shortcut or quality change.

1. Merge integration (done: e9b2d6cf8). Assign disjoint renderer, PBR and engine edits.
2. Give each camera independent directional cascade slices. Gate stock caster indexing
   on directional views only. Test two cameras and many point/spot views.
3. Restore production phase skip semantics. Feature-only audits record missing draws.
4. Compile out GPU readback, replay, safety validation and CLI paths without the
   default-off `gpu-reuse-verify` feature. Extract large new test modules.
5. Production Cargo resolves registry core_pipeline. Cargo cannot condition `[patch]`
   on features, so native raster verification requires an explicit development-only
   overlay as well as the engine feature. No automatic/default Cargo config is added.
6. Extract scene-state inventory diagnostics in a dedicated commit. Retain exact
   resource accounting and never scan entity inventory except on explicit requests.
7. Run default and verification Clippy/nextest through the shared gate, review full
   diffs, and run one owned quiet capture campaign after coordinating window order.
8. Record production/verification/test footprints, current GPU evidence and limits;
   commit with the required GPT Sol trailer. Do not publish.

Review focus: camera slice bases match shader sampling; spot allocation remains above
all cascades; point/spot views never enter retained caster reverse indexes; missing
commands do not abort ordinary phases; default builds contain no diagnostic resources,
GPU COPY usages, lane loop or thread-local validator calls. Poisoned cache state must
never become reusable. Global asset revisions remain conservative unless narrowing
can preserve the same producer boundary without settled population scans.

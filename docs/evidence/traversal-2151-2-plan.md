# Traversal catalog and frame band implementation plan (#2151.2)

**Goal:** Remove catalog hard hitches and reduce the measured 33–100 ms traversal
band without reducing content, spreading work with throttles, or weakening proof.

**Spec:** The lead's #2151.2 instructions and N29; previous measured evidence is
`docs/evidence/traversal-2151-1.md`. Start at integration `ec2596e8d`.

**Architecture:** The certified scene loader publishes an immutable complete
instance index after final ordering, sharing the lazy batch layout scope. The
readiness producer maintains batch contributions using semantic refusal/runtime
deltas. GPU/material/view/admission changes still refresh aggregate proof. The
existing package/layout/generation/frame publication checks remain authoritative.

**Stack:** Rust, Bevy 0.19.0, Vulkan, existing certified cluster package and RTX 3090.

## Constraints and ownership

- Existing isolated traversal worktree and target; owner `sol-2151`; all heavy
  Cargo and engine jobs through the build gate. No pushes.
- Add counters before changing behavior. Preserve the counter-only binary and
  native sample, then record expected red failures before implementing reuse.
- Use the same full-content package, routes, settings and flags. No caps,
  shortened reach, lowered detail, hidden traversal loading or owned stand-ins.
- The lead approved the narrow `ClusterScene.inventory` field/constructor hunk
  in `render.rs`, plus `package.rs` publication after final ordering. Keep GPU
  reuse/preparation hunks owned by #2153 separate. Coordinate any other overlap.
- Wait for the lead's explicit `2153 WINDOW COMPLETE` message before arming.
  Individual peer hold clears do not end its multi-capture window. One bounded
  read-only watcher, one campaign.
- Check actual native view at the end. If the old peer security modal remains,
  record blocked visual proof; do not ask again or retry indefinitely.

## Review focus

- Pending/unuploaded instances must remain inventory dependencies.
- A batch can span worlds and terrain/object domains; refusal/runtime invalidity
  must neither leak between groups nor survive removal of the final contributor.
- Package/layout replacement cannot reuse numeric record keys from old inventory.
  Structural scene replacement must publish a matching rebuilt index.
- Absent semantic resources differ from revision zero; permanent and reversible
  runtime suppression retain their existing meanings.
- Withdrawal/recovery, missing consumers, current materials, shadow policy and
  admission generation must never replay stale positive readiness.

## Steps

- [x] Add per-field catalog input counters and old/new-key event. Build and freeze
  a counter-only release, then run a gated diagnostic sample before behavior edits.
- [x] Attribute every complete post-ready frame >33 ms from both round-one after
  captures using exact frame/cycle/Tracy joins. Retain counts, durations, causal
  overlaps and every unresolved interval; choose top additional fixes from evidence.
- [x] Observe the GPU-replacement regression fail on real `Producer.observe`:
  aggregate readiness must change while catalog reconstruction remains unused.
- [x] Add loader-owned `InstanceInventory` with all batch indices and inverse
  record/reference batch membership. Build after final instance ordering; retain
  certificate and layout provenance. Update every scene constructor/fixture.
- [x] Add real producer-fixture tests for one-refusal and runtime deltas, unknown
  references, world/domain separation, last-contributor removal, package/layout
  replacement, and zero settled instance visits. The observed GPU regression
  proves redundant baseline catalog calls; native changed-key counters prove
  unrelated runtime changes caused the full walks. New delta tests exercise the
  resulting inventory API rather than pretending that API existed on the base.
- [x] Maintain needed contributions for the union of changed record/reference
  batches; rebuild each touched batch once. Reuse static inventory across GPU
  changes while preserving fresh aggregate evaluation and publication checks.
- [x] Implement additional top-band fixes only after joined spans identify their
  responsible layer. Pre-stage independent work instead of hiding it in budgets.
- [x] Run fmt, strict engine/workspace all-target/all-feature Clippy, engine and
  workspace nextest, workspace doctests, and a frozen release build through gate.
- [x] After the ordered window clears, run discarded warm-up plus both scored
  TEST routes under one drained hold and owner guard; finally-clear immediately.
  Attribute every remaining >33 ms frame; require zero >100 ms frames.
- [x] Check native visual once at the end, retain its actual state, finish evidence
  and handoff, update N29, and commit with the requested co-author trailer.

The lead supplied and approved the execution method and producer-index design;
implementation proceeds within that authorization.

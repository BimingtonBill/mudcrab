# Incremental readiness aggregation implementation plan

> **For agentic workers:** Use superpowers:executing-plans. Tests precede retained behavior changes.

**Goal:** Replace readiness aggregate population walks with exact batch deltas while preserving loading and drawing proof.
**Architecture:** Producers accumulate exact source, material-binding, prepared-material and view-readiness changes. The readiness bridge retains consumer/batch/domain contributions and replaces only affected rows. Certificate, inventory, layout, admission and publication validation remain independent prerequisites.
**Tech Stack:** Rust; project Bevy 0.19; existing native Tracy and profiler-free TEST tooling.
**Spec:** User #2151.4 brief; base `5719b67f3`; preceding evidence `traversal-2151-3.md` sections Remaining readiness aggregate and final work counts.

## Constraints and review focus

- No source query/admission, origin or previous-frame history changes. No cap, throttle, hidden loading, frozen view or content substitution.
- No push. Local commit carries `Co-Authored-By: GPT Sol <noreply@openai.com>`.
- Gate jobs wait for explicit 2145.4 WINDOW COMPLETE (received). Announce own campaign; one finally-cleared hold, maximum 25 minutes.
- Preserve world/domain invalid-survivor OR, missing consumers on empty catalogs, conservative missing-material shadow policy, GPU-complete proof and optional-resource presence.
- Exact mutation readers must accumulate multiple writes, isolate readers, detect same-revision resource replacement and retain removal IDs.
- Reject stale certificate/layout/inventory/admission/frame publication; recovery republishes current proof even with identical keys.

## Tasks

- [x] Trace all authoritative dependency writers; audit fallback-only transitions and distinguish aggregate from selection counters.
- [x] Prove RED: one fallback-only batch update visits one row; generation replay visits none; one consumer bit changes one row. Prove material-reader REDs using existing native collection.
- [x] Publish source tuples and actual GPU binding deltas; independently observe prepared material mutation IDs; publish view-bit changes after fresh proof.
- [x] Retain aggregate contributions, update affected rows, test OR survivors, resource withdrawal/replacement, layer/role/world changes and full-oracle equivalence over multiple deltas.
- [x] Focused tests, strict engine/workspace Clippy, workspace nextest/doctests, release Tracy/plain builds; review complete diff.
- [ ] One bounded quiet campaign: discarded warm-ups, matching moving diagnostics and profiler-free TEST. Join crossings and report aggregate visits, selection evaluations and own projection work separately. Narrow selection evaluation only with crossing evidence and safe exact dependencies.
- [x] Record results, limits and next root plan; retain local changes in the requested commit, no push. Native measurement remains open under the refusal ruling below.

## Rulings

- Existing 12.28M count belongs to fresh selection readiness, not bridge aggregation. Lead confirmed bridge delta first; selection narrowing remains conditional on exact crossing attribution and safe producer reuse.
- Independent material mutation reader extends the native prepared-asset owner. Never drain shadow cache's existing delta stream or use per-asset versions to bypass material-address checks.

- Chose native `Messages` double-buffer cursors and a separate collection lifetime ID. This expresses equal-revision replacement baselines; Arc/weak per-reader queues are unnecessary. The empty path visits no events or assets. The log rotates once per erased collection before preparation, with an independent installation marker even for preinitialized resources.
- Existing selection material-address checks remain authoritative. Collection replacement also revokes retained queue plans, rather than accepting a restarted numeric revision.
- R4 native campaign will reuse the prior accepted R3 attribution as historical context, with its exact older source boundary noted. The new integration includes #2145.4 step 1; no isolated overall-FPS effect will be claimed from that comparison.

## Execution result

All frozen checks and both releases pass. One announced campaign was attempted after the prior jobs cleared. The unchanged guard refused its first discarded warm-up before engine launch: `not_quiet_before_start`, exit 6, final GPU sample 17% against a less-than-15% limit. One attempt, zero retries. The owned hold finally-cleared after 316.792 seconds at 2026-10-26 21:09:22 Adelaide; `FINAL COMPLETE` was relayed so other work could resume.

The native-measurement task above remains unchecked. The user explicitly instructed stopping after a refusal and retaining frozen binaries for a later quiet campaign. Both aggregate and selection route counts, crossing wall time and FPS after R4 therefore remain unmeasured. No reduction of fresh selection evaluation is claimed. Evidence and pending root plan are recorded in `traversal-2151-4.md`; local commit retains the implementation and its verified tests without claiming native performance acceptance.

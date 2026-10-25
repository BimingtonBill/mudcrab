# Standing CPU round #2145.3 implementation plan

**Goal:** Reduce standing CPU render cost below 8.33 ms, targeting 7.5 ms, without changing content or invalidation.

**Architecture:** Keep the existing source index, native query domains and renderer ownership. First distinguish uploads from command recording/finishing/submission and determine whether native table membership can bound copied-tick polling. Assess upstream ECS summary-tick backports independently, without linking them.

**Tech stack:** Locked Bevy 0.19.0, wgpu 29.0.4, native Vulkan Tracy diagnostics, shared build gate owner `sol-2145`.

**Spec:** User's #2145.3 task; prior evidence `postready-cost-2145-2.md`.

## Constraints and review focus

- Base `551909a16`; no push, no ECS linking, no content/settings caps or skipped mutable invalidation.
- Preserve exact system tick windows, query disablers, structural recovery, source discovery, terrain removal and independent asset changes.
- Preserve transparent order, mesh/material/allocator readiness, indirect offsets and multi-view buffer lifetimes.
- All heavy Cargo/engine work uses the gate. One bounded capture hold at a time, cleared in `finally`; discard first post-build run.
- Keep timing claims diagnostic, with guard verdict, binary/source identity and content comparison. Zero native query admissions do not imply zero tick polling.

## Tasks

- [x] Merge integration and launch frozen-source baseline release build.
- [x] Capture baseline after ordered #2151.1 and #2153 windows; inspect exact post-ready spans and work counters.
- [x] Trace upload and sorted-preparation owners; reject reuse unless complete invalidation and frame-local output contracts can be proved.
- [x] Write semantic/work regression before each retained engine change; observe red, implement at responsible owner, verify green.
- [x] Backport exactly upstream #25157 and #25946 in owned scratch, count pristine deltas/conflicts/prerequisites, and document renderer/PBR compatibility and unmeasured expected saving in `ecs-backport-assessment.md`.
- [x] Review retained implementation, run fmt/strict Clippy/nextest and required workspace checks through gate, build frozen candidate.
- [x] Run warm-up and retained guarded capture with identical scene, compare exact post-ready attribution/content, record GPU outlook and missed targets.
- [x] Update evidence and handoff LOG; commit kept changes with required GPT Sol trailer. Verify clean tree; never push.

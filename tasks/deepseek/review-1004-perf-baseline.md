# Goal
Review of a local branch for the Mudcrab project (open Skyrim engine, Rust, Bevy 0.19, wgpu 29): `phase2/perf-baseline`, the performance and resource baseline for upstream issue #184. It is not opened upstream yet. Decide what is wrong, misleading, or would break normal play before it is used to publish a baseline.

# Context
- Your working folder is the review folder. `delta.diff` is the change from 2433ef4f (upstream main plus PR #122, the pacing measurement tools) to the head a3a8465d (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- What it claims to add (benchmark and profile runs only; normal play must be unchanged): a run fingerprint (commit, backend, adapter, driver, schemas, settings), per-render-pass GPU time from Bevy's RenderDiagnosticsPlugin (`render/<pass>/elapsed_gpu`) with an honest `unavailable` when timestamps are missing, GPU memory counters from Bevy's MeshAllocatorDiagnosticPlugin and RenderAssetDiagnosticPlugin (counts, not VRAM bytes), CPU asset bytes, scene counts (entities, vertex invocations on Vulkan, draw calls null), CPU time per main schedule and per recorded span, report format_version 10, and a new script `scripts/phase2-baseline.ps1` that reuses `scripts/phase2-profile.ps1` (which was refactored to be dot-sourced as a library with `-AsLibrary`) to run scenes with repeats and write baseline.json and baseline.md.
- Bevy 0.19 sources: C:/Users/Brannock/.cargo/registry/src/ (bevy_render-0.19.0 etc.) if you are allowed to read them; otherwise mark Bevy claims UNVERIFIED.

# Scope
Read only. Change nothing.

# Done when
Every file in the delta has been read against `before/`. Check especially: does normal play install anything new; are the statistics right (percentiles, medians across repeats, min-max spread, units ms vs s, bytes vs counts); can a missing diagnostic produce zeros instead of null; does the profile script still behave exactly as before when run directly (not as a library); does the baseline script handle a failing engine run, a missing report, and the comparison refusal (different backend or adapter); do the docs describe what the code does.

# Report
Status, Verdict (accept / small fixes / fix first), one-paragraph summary, then findings as `path:line - problem - suggested fix`, most severe first. End with what you could not check.

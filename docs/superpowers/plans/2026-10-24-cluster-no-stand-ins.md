# Cluster Without Stand-ins Implementation Plan

> For agentic workers: execute the user-provided #2143 design continuously; no further design approval is required.

**Goal:** Cluster-owned primitives never instantiate stock render bodies. Loading releases only after all required views can draw their needed clusters.

**Architecture:** Freeze ownership after initial source/material qualification. Filter reference-specific WorldAsset templates before instancing, while retaining source geometry for validation and physics. Publish renderer readiness changes to a common loading gate. Keep refused stock and terrain handoff independent.

**Tech stack:** Rust, project-pinned Bevy 0.19.0 and Rapier 0.36, existing vendored render/PBR crates.

**Spec:** User decision and implementation brief dated 2026-10-24 in this task thread.

## Constraints

- No engine/GPU runs or publication. All heavy Cargo uses build gate owner sol-2143 and D:/Mudcrab/builds/no-stand-ins-target.
- Keep C: and D: above 15 GB. Delete only task-created folders. Keep assets/dependencies/format versions unchanged.
- Refused source rendering is permanent coverage, never transient replacement of owned clusters.
- No caps, lower settings, hidden loading, distance cutoffs or silent fallback.

## Review focus

- Full-source bounds and RenderProxy collision retain geometry without Mesh3d components.
- Missing view state, shared shadow consumers, and shader reload never produce Ready.
- Scene/material/pipeline invalidation cannot silently reclassify an owned primitive as stock.
- Startup timeout and every teleport entry point preserve a visible loading cover until Ready.
- Unchanged readiness avoids per-frame batch/pipeline scans; native terrain handoff still has a job.

## Tasks

- [x] Add regression tests proving frozen ownership, loaded/distant absent owned render bodies, and preserved collision; observe red, implement pre-instancing filtering, verify green.
- [x] Replace object complement selection with owned selection. Delete dead object ClassFilter application, visibility restore, joint intersection and BatchDrawability lifetime seam. Retain ClassFilter used by native/terrain handoff.
- [x] Add mutation-owned renderer/material/pipeline readiness epochs, cache exact consumer readiness, publish changed-only all-view needed-batch observations and owned_not_drawable diagnostics.
- [x] Integrate startup/coc/load-door/common teleport loading gates; require exact current scope, all camera/prepass/shadow dependencies, and no timeout-based uncovering.
- [x] Recompute worldspace 60, center (5,-12), radius 2 structural entity projection from the same package/source scene data; record baked LOD and launcher/converter findings without changing those subsystems.
- [x] Run fmt, engine strict all-target/all-feature Clippy and all-target nextest; workspace strict Clippy, nextest and doctests. Review full diff, commit with GPT Sol co-author and dated handoff.

## Decisions

Initial unsupported material features may permanently refuse a source. Later loader, GPU, shader or source failures do not revoke ownership and cannot create stock. Source-aware physics reads retained source handles, independently of the render component query. ClassFilter remains for native and terrain surfaces because those callers still use it; the dead owned-object consumer is removed.

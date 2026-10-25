# bevy_pbr 0.19.0: shadow render diagnostics and material readiness revisions

This is the complete published bevy_pbr 0.19.0 crate, used by the root
`[patch.crates-io]` entry. The patches below describe the source differences.
The full crate is 4.19 MB. Cargo cannot replace its private light module with a
standalone file; preserving the module tree also preserves embedded shader and
LUT asset paths. No dependency version or engine feature is changed.

## Provenance and license

- Source: published `bevy_pbr` 0.19.0, `registry+https://github.com/rust-lang/crates.io-index`.
- Crate archive SHA-256 (the original Cargo.lock checksum):
  `38ebc777de2d7f45888c68e8e04949bfb8a40aee62f700283fc340f525e67f52`.
- Published upstream revision: `c6f634ca9f406d68ba5109d921247b654cb42c10`, `crates/bevy_pbr`.
- All 153 published files are retained. LICENSE-MIT and LICENSE-APACHE are unchanged;
  the upstream license is MIT OR Apache-2.0. `.cargo_vcs_info.json` records provenance.
  Registry-local bookkeeping `.cargo-ok` and `.cargo-checksum.json` is not included.
- Task: Sol implementation #2138, 2026-10-24. This README is local integration metadata.

## Patch

The shared early/late shadow helper obtains its optional diagnostics recorder
before borrowing the render pass. It pairs `begin_pass_span` and `end_pass_span`
around the existing shadow phase and handled-error path. Its names append `_early`
or `_late` to Bevy's existing shadow view name, covering directional cascades,
point-light faces and spot lights. Pass descriptors, draw order and occlusion
behavior remain unchanged.

Direct paired calls avoid Bevy 0.19's `PassSpanGuard::end`, which forgets the guard
and leaks an owned dynamic label. No label is allocated when diagnostics are absent.
The recorder owns the label in frame data and releases it on frame-data reuse.

The recorder provides CPU time, GPU elapsed time and pipeline statistics where
supported. Unsupported measurements remain absent. Existing per-frame query limits
(256 timestamps and 128 statistics queries) can omit observations when exhausted.
Names may aggregate cameras; samples are pass observations, not per-frame totals.
The engine adds `gpu_passes.pipeline_statistics` and
`scene.fragment_shader_invocations`; `gpu_passes.passes` remains elapsed-GPU-only.

## Material readiness revisions (#2143)

Task #2143 adds `MaterialBindGroupAllocator::readiness_revision()`, an owner-local
monotonic revision for available material bind groups. Both allocator variants
record available bind-group creation, invalidation, removal, and replacement.
Bindless allocation/free includes the slab's otherwise silent cached-bind-group
clearing. Unprepared non-bindless allocation/free, unchanged preparation, and
buffer writes do not advance the revision. Reading it takes constant time;
tracking changes adds no slab scan. The public allocator enum remains unchanged.

GPU-free tests cover the availability accounting and actual non-bindless
unprepared allocation/free. They do not establish GPU correctness or timing.
The cluster readiness consumer and live acceptance belong to engine integration.

## Exact directional shadow retention (#2153)

`src/shadow_cache.rs` owns retained directional texture slices, producer-driven
caster/material revisions, current-frame external command certificates and
physical slice ownership. `src/render/light.rs` skips attachment acquisition,
clear and raster only after the complete exact key matches. Stock caster queue,
mesh extraction, skin and morph producers publish their existing changes from
`material.rs`, `render/mesh.rs`, `render/skin.rs` and `render/morph.rs`.
`lib.rs` exports the integration module. Unknown materials, commands, incomplete
draws and untracked writable texture bindings refuse reuse. Material type names
are diagnostic metadata and do not grant eligibility.

`src/shadow_cache_parity.rs` supplies opt-in same-frame native map byte checks,
including maps that normal policy already renders. The fresh and retained paths
both preserve real command/bind safety. Diagnostic refusal receipts are separate
from map equality. Ordinary rendering performs no diagnostic copies, dependency
walks or readbacks with verification disabled.

The texture writer contract, invalidation list, engine integration, native audit
boundaries and validation evidence are documented in
`../../docs/design/cluster-gpu-exact-reuse.md`. Asset-backed texture updates must
publish through `Assets<Image>` or a fresh `RenderAssets<GpuImage>::get_mut`.
Raw writes through escaped handles are outside that cooperative contract.

## Workspace and removal

PBR stays an excluded dependency, so workspace `--all-features` does not activate
unrelated optional PBR processors. The engine's existing feature set still compiles
this patch, and dependency `opt-level = 3` remains in force.

To restore the registry crate, remove only the root PBR patch and workspace exclusion
and this vendor directory, then regenerate the lockfile through the build gate.
Migrate callers of `readiness_revision()` when removing this patch. The report
fields remain useful for other instrumented passes. Re-evaluate or retire
the patch on the next Bevy upgrade. Runtime GPU and performance acceptance belongs
to the lead's subsequent timing run.

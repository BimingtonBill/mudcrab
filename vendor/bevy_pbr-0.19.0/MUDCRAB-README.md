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

The default-off `gpu-reuse-verify` feature forwards to
`bevy_render/gpu-reuse-verify`. It compiles `src/shadow_cache_parity.rs` and its
same-frame native map byte checks,
including maps that normal policy already renders. The fresh and retained paths
both preserve real command/bind safety. Diagnostic refusal receipts are separate
from map equality. Ordinary rendering performs no diagnostic copies, dependency
walks or readbacks with verification disabled. The default shadow path emits
one native pass body; the replay loop, `ShadowReuseVerification`, texture-view
diagnostics and audit counters do not compile into that path. Production
cumulative and per-view reuse windows remain available, including an
`audit_forced=0` telemetry field without stored audit state.

The texture writer contract, invalidation list, engine integration, native audit
boundaries and validation evidence are documented in
`../../docs/design/cluster-gpu-exact-reuse.md`. Asset-backed texture updates must
publish through `Assets<Image>` or a fresh `RenderAssets<GpuImage>::get_mut`.
Raw writes through escaped handles are outside that cooperative contract.

### Review corrections (#2153.1)

Directional cameras reserve distinct physical slices. A fitting array sums
camera cascades, with spots following the complete directional allocation. If
that sum exceeds the device limit, retained per-camera atlases preserve the
original per-camera/spot budget. Shared spots raster once; whole depth layers copy
to the extra atlases at identical shader indices before camera sampling. The
production overflow path alone adds COPY_SRC/COPY_DST and copy encoding; diagnostic
COPY usages remain feature-gated independently. Camera topology changes invalidate
owners, refresh unchanged spot attachments and prune removed atlas keys. Memory
grows with active camera maps; no shadow content/settings are reduced.

Only views with `DirectionalShadowMapInputs` enter stock begin/remove/set/pending
bookkeeping. Point/spot faces keep their native pending queues but do not create
stock-cache caster/material indices. Their draw wrappers also skip cache locks.
The bounded `many_point_views_do_zero_stock_bookkeeping_cpu_diagnostic` test runs
30 point lights x 6 faces x 10,000 casters plus four directional views through the
actual helpers, comparing ungated calls with the production admission gate. It
prints mutation-lock/index counts, retained memberships and elapsed CPU time;
the elapsed values are diagnostics, not renderer timings or acceptance bars.

Geometry, mesh allocation, image and pipeline revisions remain global conservative
counters: any relevant mutation anywhere invalidates every map. Narrowing these
requires complete per-caster producer dependency indices. `invalidated` therefore
does not imply that a cascade's own casters changed. Static shader declarations
remain supported; the unused unbounded per-asset `set_revision` API was removed.
Animated material inputs still publish prepared-material changes.

Poisoned dependency locks refuse certificates. Dependency mutation access discards
poisoned state without clearing poison. The map owner lock instead recovers its
per-frame early-skip flags, withdraws all owner/key certificates and permanently
disables retention until resource recreation. Fresh early passes still receive
late occlusion work; an already reused early pass skips late appending for that
frame only. Recovery never re-enables retention. A late producer change after early reuse also skips the late draw,
revokes ownership, and requires a cleared render next frame. That rare frame
retains the previous complete map rather than combining old and new casters.
Private unit tests now live in separate sibling modules.

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

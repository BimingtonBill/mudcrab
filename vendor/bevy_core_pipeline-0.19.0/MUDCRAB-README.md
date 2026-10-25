# Native raster byte parity (#2153)

This is the exact installed `bevy_core_pipeline` **0.19.0** registry package,
copied from
`C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_core_pipeline-0.19.0`.
No dependency version or upstream shader was changed. Upstream's
`.cargo_vcs_info.json` identifies commit
`c6f634ca9f406d68ba5109d921247b654cb42c10`, directory
`crates/bevy_core_pipeline`. `LICENSE-MIT` and `LICENSE-APACHE` are preserved.
Registry `.cargo-ok` and the package-local cache `Cargo.lock` are omitted.
The normalized `Cargo.toml` and original upstream manifest are preserved.

The parent owns the root Cargo patch and lockfile integration. The renderer-side
implementation is `bevy_render::render_parity`; this crate owns the real native
emission hooks. The hooks do not replace shaders or reconstruct draws.

## Integration API

```rust,ignore
use bevy::render::render_parity::{RasterParity, raster_parity_forced_lane};

// RenderGraph Begin, before native camera passes. `requested` is eligibility
// only. The producer first applies its shared, frame-latched shadow/cull gate.
// Actual retained snapshots and an actual forced production cull must already
// be encoded before registration. Each tuple is (live, retained, forced).
parity.register_view(frame_count.0, render_camera_entity, triples)?;

// In DrawClusterIndirect, use an independent ledger for B; preserve the normal
// ledger for A. Both ledgers retain ordinary early/late ONCE suppression.
let forced_audit_lane = raster_parity_forced_lane(render_camera_entity);
```

`RasterParity` is a render-world resource installed by `RasterParityPlugin`,
which `CorePipelinePlugin` installs. Its concrete methods are:

- `requested(&self, frame: u32) -> bool`: diagnostic eligibility only; does not
  choose a frame. The engine may combine this with the shadow audit's shared
  frame gate. Do not re-read the sentinel separately for each view.
- `register_view(&self, frame: u32, view: Entity, buffers: Vec<(Buffer, Buffer, Buffer)>) -> Result<(), String>`:
  validates distinct storage and copy capabilities, then arms the actual frame.
  Each view registers once. Supply every writable cluster output. Registration
  establishes handle routing, not proof of how the snapshots were produced;
  the independent native cull receipts establish that provenance.
- `is_armed(&self, frame: u32) -> bool`: exact-frame predicate used by emitters.
- `fail(&self, message: &str)`: records a loud diagnostic failure.

`raster_parity_forced_lane(view: Entity) -> bool` is scoped to the current thread
and view. It is true only inside that camera's B emission. A guard restores the
previous value even during unwinding. The more explicit
`raster_parity_lane(view) -> Option<RasterParityLane>` distinguishes A and B from
ordinary rendering.

## Request and output contract

`MUDCRAB_RASTER_PARITY_DIR` enables the diagnostic. Optional
`MUDCRAB_GPU_PARITY_REQUEST` names the controller's owned sentinel;
`MUDCRAB_RASTER_PARITY_MIN_FRAME` supplies an optional minimum extracted frame.
Environment is read once. The sentinel is polled only through diagnostic
eligibility. Camera-side file polling never arms an earlier frame without cull
buffer pairs. The parent controller creates the sentinel only after its exact
112278/35713/27106 ready-and-settled inventory, and requires cull, raster and
shadow receipts to name the same actual `FrameCount`.

For each native pass, the attachment getters run once. Before A, every live
cluster buffer receives its retained snapshot. Before B, every live buffer
receives its forced snapshot. A writes the original native attachments. B writes
new scratch attachments with identical dimensions, formats, sample counts,
viewport, clear/load/store operations and the same native emission body. Loaded
scratch attachments receive the original **pre-A** bytes before either render.
The retained buffers are restored after B. Clear/load atomics are never acquired
again for B; neither a cloned `ViewTarget` nor a cloned attachment is used as an
independent first-use state.

The prepass body includes stock opaque and alpha-mask phases, cluster draws and
background motion vectors. The opaque body includes stock/cluster phases,
alpha-mask phases and skybox. The transparent body includes the actual sorted
transparent phase. An empty transparent population produces an empty paired
native pass in audit mode, so it still records the output boundary. The original
no-item behavior remains unchanged outside the armed frame. Late prepass retains
the native occlusion/no-indirect predicate and records whether late completion
was required.

Per-attachment staging copies are encoded immediately after the paired pass.
`RenderGraphSystems::Finish`, after command submission, starts asynchronous maps.
`.bin` files contain tightly packed, native logical texel bytes: packed
`Rgb10a2Unorm` normals, `Rg16Float` motion, `Depth32Float` depth and native linear
`Rgba16Float` HDR color. Only staging row padding is removed. Floating-point
values, NaNs and signed zero are never decoded or normalized. Receipts retain
native dimensions/format, copy stride, exact operations and viewport, selected
frame/view/pass, byte counts, first difference and zero tolerance. The frame
receipt lists observed passes/attachments and is `incomplete` until every map
and required pass completes. Missing/duplicate registration, missing pass,
readback error, mismatch or 30-second map timeout fails loudly.

The named color boundary is the native opaque or transparent emitter's output,
**before temporal postprocessing**. It does not certify presented screenshots,
TAA/history or arbitrary custom writers after the transparent emitter. Normal
and motion receipts exist when those native prepass attachments are enabled;
their absence is visible in the attachment inventory. Settings are never changed
to obtain a result. Inter-pass dependencies shared by B are conditional on all
earlier attachment comparisons passing; a single mismatch fails the whole frame.

Shadow maps are deliberately not declared identical here. Both raster emissions
use the same currently bound native maps. The separate same-frame native shadow
audit must prove retained-versus-forced map identity before the controller may
accept the combined output proof. The frame/attachment receipts explicitly say
`shadow_texture_identity_proven: false`.

## Unsupported state and disabled cost

MSAA, resolves, stencil/discard, layered/mipped/non-2D attachments, deferred
prepass, OIT and writable pass queries are refused. Multisampled attachments do
not receive invalid diagnostic copy flags. A diagnostic scan records ready cached
render pipelines with explicit read-only layouts. Two small tracked-pass hooks validate the pipeline actually bound and
reject unknown/writable/implicit pipelines or raw-pass escape. Unused pipelines
do not affect the verdict. Arbitrary CPU side effects inside custom draw commands
remain subject to the parent's native-producer validation. No settings
are silently weakened. Ordinary rendering still executes once after an audit
refusal; the receipt remains failed.

With the directory variable unset, emitters execute their original body once.
No per-frame copies, allocations, attachment/pipeline scans or filesystem calls
are added. Startup installs a small resource/system and reads environment once.
Diagnostic copy flags are added at native attachment creation only when enabled
and sample-preserving copies are supported. Scratch and staging storage exist
only for the registered audit frame.

## Patch ownership and verification

Changed upstream files: `src/lib.rs`, `src/prepass/node.rs`,
`src/core_3d/main_opaque_pass_3d_node.rs`,
`src/core_3d/main_transparent_pass_3d_node.rs`, `src/core_3d/mod.rs`.
The accompanying renderer changes are the new `src/render_parity.rs`, its module
export, one main-attachment copy-usage expression in `src/view/mod.rs`, and the
two validation hooks in `src/render_phase/draw_state.rs`.
No engine or PBR files belong to this patch.

The child runs source/format/provenance checks only. Parent owns compilation,
unit tests, native GPU execution, input/provenance validation and acceptance.
Source existence and successful formatting are not a byte-parity result.

Parent unit-test filter: `render_parity::tests::` in package `bevy_render` (four
CPU tests: row packing, scoped lane/view restoration, receipt escaping, and
unknown/raw pipeline refusal). These are not GPU/provenance acceptance tests.

## Stable receipt schema

All names below are part of this snapshot's controller contract:

- `frame-{frame}.json`: `kind: "raster_frame"`, `frame` (registered cull frame),
  `render_frame` (actual submitted `FrameCount`), `status` (`"incomplete"`,
  `"failed"`, or `"equal"`), `completed_attachment_pairs`,
  `outstanding_attachment_pairs`, `views`, `error` (null or the first failure),
  `tolerance: 0`, `shadow_texture_identity_proven: false`.
- Each view has `view` (render entity bits), `buffer_triples`, `buffers`
  (`live`, `retained`, `forced` handle IDs and `bytes`), `passes`, `attachments`
  (`pass/attachment` names), and `late_required`.
- `frame-{frame}-view-{view}-{pass}-{attachment}.json`:
  `kind: "raster_attachment"`, `frame`, `view`, `pass`, `attachment`, `format`,
  `width`, `height`, `samples: 1`, `layers: 1`, `mip: 0`, `row_bytes`,
  `copy_row_bytes`, `operations` (native operations plus viewport), `boundary`,
  `retained_file`, `forced_file`, `bytes_per_lane`, `different_bytes`,
  `first_difference` (null or tightly packed byte offset), `equal`,
  `tolerance: 0`, `shadow_texture_identity_proven: false`.
- Per-attachment raw files use the same stem plus `-retained.bin` and
  `-forced.bin`. These filenames are also in the attachment receipt. Raw files
  have `row_bytes * height` bytes, without `copy_row_bytes` staging padding.
  Compare the entire raw file without float conversion or tolerance.
- Pass names are `"early prepass"`, `"late prepass"`,
  `"main_opaque_pass_3d"`, `"main_transparent_pass_3d"`; spaces become hyphens
  only in filenames. Attachments are `"normal"`, `"motion"`, `"depth"`,
  `"linear-hdr-color"`. Normal/motion appear only when enabled natively.

A controller must classify JSON by `kind` (attachment files also match a
`frame-*.json` glob). Accept only a terminal `"equal"` frame, zero outstanding
pairs, a nonzero complete pair inventory, matching raw lengths/bytes, every
expected enabled attachment and required early/late/main pass, and
`frame == render_frame == cull_frame == shadow_frame`. A missing receipt, failed
or incomplete state, absent required attachment, late omission or unmatched raw
file is not acceptance. The separate cull/shadow receipts remain mandatory for
the combined proof; this module never upgrades their status.

The vendor-local `.gitattributes` preserves upstream whitespace in five pristine
archive files. It does not alter shader bytes or exempt local audit patches from
normal whitespace checks.

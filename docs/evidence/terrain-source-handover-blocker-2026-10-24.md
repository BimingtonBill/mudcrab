# Source terrain handover requires GPU plan retention

## Decision

Do not hide stock terrain in MainWorld at the current coverage boundary. The requested withdrawal hold cannot preserve drawing there: destructive GPU queue-plan replacement runs before coverage publication. A retained coverage entry would hide stock after its replacement had stopped drawing. Keep the existing current-frame per-view gate until draw ownership can retain the actual plan and resources.

This is the explicitly permitted blocker report for the second traversal change, not an implementation or performance acceptance claim. No handover/withdrawal counters or simulated passing ownership tests were added for behavior that does not exist.

All Native, Batch and Source surfaces remain on the render-side gate, including its existing settled `ClassFilter` replay. No source-hide subset is proven safe: every eligible surface depends on current per-view compute/pipeline/material/binding readiness, and this layer owns no durable lease that preserves those dependencies through readiness loss. Detail, ring, protected and partially covered surfaces retain their existing rules. No MainWorld visibility or extraction marker is added.

## Evidence

References below use base `91c23bbd296074a1516e3de11c94849de1ec66c6`; cluster source is unchanged by this task.

- `crates/engine/src/cluster/render.rs:2763`: queue planning drains the previous `plan_items` and removes their phase representatives. At 2795, `groups.apply` replaces the view draw groups/table. At 2812, the plan epoch advances. Keeping last frame's cells cannot restore these draws.
- `crates/engine/src/cluster/handoff.rs:2433`: `select_instances` runs after the cluster forward, prepass and shadow queue systems. This is already too late to hold a removed queue plan.
- `crates/engine/src/cluster/handoff.rs:3224`: changed batch states clear previous instance eligibility and terrain block counts. At 3281, current drawability/readiness is required to enable them again. At 4068, the resulting complementary mask is uploaded. Remembering coverage cannot preserve those instance bits.
- `crates/engine/src/cluster/handoff.rs:3963`: coverage is a cached projection of the current selection's terrain blocks, keyed by generation/layout/plan. Absent views are removed at 4099; missing GPU state clears all coverage at 3564. This is selection qualification, not a durable ownership lease. The terrain module's older comment that coverage was cleared every successful invocation was inaccurate; the incremental extraction commit corrects it.
- `crates/engine/src/cluster/render.rs:3117`: every cull invocation advances the epoch even when a required pipeline is missing. `DrawClusterIndirect` at 3765 skips a view without current cull output. Preserving an obsolete mask or readiness bit cannot keep an unavailable draw path alive.
- `crates/engine/src/cluster/stock_drawable.rs:1`: `StockDrawable` certifies prepared payload/pipelines, not phase membership or later mesh bind preparation. Restoration must combine exact mesh identity, current visibility, current queue membership and per-pass drawable proof. `finalize_terrain_clips` already does these checks independently for forward and depth/shadow at `cluster/handoff.rs:2527`, after bind-group preparation.

## Failure sequence

In frame N, a source has already become hidden in MainWorld. In frame N+1, queue planning removes the old cluster representative and selection clears the cell's instance eligibility. Revealing stock through MainWorld still needs visibility propagation, extraction, specialization and queueing before current stock draw proof can exist. During that gap neither path draws the cell. Retaining the old coverage entry only preserves a suppression claim; it cannot restore the removed draw plan. The hold must therefore precede destructive plan changes.

## Main-world visibility and readiness loss

The user's follow-up constraint is decisive even for a surface whose coverage never changes: MainWorld `Visibility::Hidden` takes effect only on a later frame and drops Bevy's `RenderMeshInstances` for that mesh. An unhide cannot restore its stock draw in the frame that a cluster pipeline or material becomes unready in any view.

Keeping a remembered plan or coverage set cannot solve this case when its required draw dependency has already become unusable. The code explicitly rechecks current materials because a bindless slab can lose its bind group without its own event (`cluster/handoff.rs:3633`); missing required cull pipelines also invalidate current drawing (`cluster/render.rs:3117`). There is no surface-specific exception backed by a lifetime guarantee here.

A safe source hide would therefore also need advance notification or a retained usable pipeline/material/binding lease, with stock revealed before that lease can be lost. Otherwise the source must remain extracted and use the render-side gate, which can stop suppressing current stock without waiting for a MainWorld round trip. This task chooses that render-side path for every eligible surface. It preserves the existing stock availability; it does not claim to repair an unrelated simultaneous loss of stock's own dependencies.

## Draw proof and acquisition

Current coverage proves that the selected cluster blocks have current compute, material and view/pass qualification. It does not report completed drawing of a particular source surface. The draw command checks current cull epoch and issues indirect draws; the existing far diagnostic explicitly describes command continuity, not nonzero GPU survivors (`cluster/far.rs:108`). No stronger pixel or GPU readback requirement is introduced here.

The existing per-view gate already removes fully cluster-owned, unprotected sources before stock queueing and late clipping; a follow-up should reuse that acquisition path. Its ownership acknowledgement must name the resulting current plan/pass/clip state. Other drawable stock detail can still clip cluster terrain, so a whole-group command diagnostic alone is not an exact source-surface receipt. This is a proof-binding requirement, not a demand for pixels or GPU readback; the blocking defect remains withdrawal after destructive plan replacement.

## Required owning layer

A safe follow-up belongs beside `ClusterGpu`/`ViewGpu` and queue planning, before the destructive operations above:

1. Keep a visibility-independent source inventory with generational entity ID, immutable mesh, exact world/cell/quadrant coverage, and protected own-draw/named-hold obligations. Hidden sources must remain discoverable for restoration.
2. Enumerate all active terrain-capable consumers independently of successful cluster views. A missing coverage/GPU entry must block acquisition, not disappear from the intersection. Require a nonempty relevant consumer set, with each camera/shadow consumer's world, layers and pass obligations.
3. Acquire only when all consumers qualify and the agreed draw evidence names the current package/GPU generation, plan, pass and clip state. Publish source visibility changes across the render/main-world boundary with matching identity.
4. Before a plan or dependency withdraws, reveal stock in MainWorld and retain the old usable cluster plan, phase representatives, instance eligibility, bindings and buffers through visibility propagation, extraction, specialization and queueing. This may span multiple frames.
5. Accept restoration only from current exact stock visibility, mesh, queue and drawable facts for every required consumer/pass. Late per-pass clipping can yield restored quadrants while the held cluster instance remains eligible. Only then release its plan/resources.
6. View addition revokes global hiding until that consumer qualifies; view removal releases only that consumer's obligation. Preserve original world identity across worldspace changes. Controlled package replacement must retain the old usable GPU dependencies until stock acknowledgement; an already-lost GPU resource cannot be resurrected by a coverage hold.
7. Pipeline/material/binding withdrawal in any view must announce itself early enough to restore and draw stock before losing the retained cluster dependency. A surface without that guarantee stays on the render-side gate permanently; stable coverage or a settled replay alone never proves source hiding safe.

The existing per-view gate must exclude withdrawing sources from its hide decision, or it would prevent stock from ever reaching the acknowledgement queues. Settled ownership must avoid scanning all surfaces; dirty source/plan/consumer dependencies drive updates, while pending acquisition/restoration alone may retry.

## Acceptance still required

A production follow-up needs frame-sequence tests across all owning layers: all-view acquisition, missing/new consumer refusal, plan withdrawal held until stock acknowledgement, independent forward/prepass/shadow restoration, package/worldspace replacement, and settled-frame zero ownership work. Every simulated frame must preserve at least one usable representation for each required consumer/pass. Add `terrain_handoff/handover` and `terrain_handoff/withdrawal` counters only with that implementation. Native rendered/GPU and portal-heavy checks remain necessary before integration; this task was explicitly forbidden from running the engine.

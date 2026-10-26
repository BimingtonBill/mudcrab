# Standing CPU round #2145.4

**Goal:** Reduce the standing CPU render schedule toward 7.0 ms while preserving content, mutable invalidation and rendering semantics.

**Base:** `94d8722a0`. Locked Bevy 0.19.0 and wgpu 29.0.4. No ECS dependency patch, push, caps, quality reduction or settled-state assumption.

**Method:** Use the previous exact standing attribution as the hypothesis source, then capture frozen baseline and candidate binaries in one quiet window. Read the actual command-submission owner before treating submission as uploads. Compare main-world, extraction, render schedule and wait in their pipelined cycles; never add overlapping spans.

Research read: `analysis-1009-baseline-cpu-breakdown.md` establishes the pipelined ownership model but uses old content. `analysis-1035-static-churn-indirect.md` distinguishes actual writers from read-only scans. `analysis-1131-parallel-encoder-finish.md` explains the existing parallel finish backport. Current source and `postready-cost-2145-3.md` supersede their numerical results. Remaining questions are current thread ownership, settled submitted work and safe engine-only source scope.

## Proof and review boundaries

- Keep native query filters, exact tick windows, disablers, source ancestry/history, discovery and full recovery authoritative. Count copied rows separately from native admissions.
- Preserve transparent order, draw readiness, per-frame indirect/output indices and multi-view ownership. Reuse requires complete owner invalidation, not an unchanged camera alone.
- Write a semantic/work regression, observe red, implement the smallest responsible-layer correction, then verify green and independently review.
- Respect #2151 pose/rebase/traversal ownership and #2153 GPU ownership. No changes to their semantics without coordination.
- All heavy Cargo and engine runs use owner `sol-2145` and the existing isolated target. No compilation or heavy export inside a timing hold.
- Capture admission requires the lead's explicit `2145.4 WINDOW GO`. The latest lead rule awards the window to whichever task freezes both binaries first, subject to #2151's requested or active window. Hold observations alone do not authorize capture.
- One own bounded hold covers all Step 1 captures and is cleared after verified process cleanup. The exact GO subsequently requires a 20-minute drain plus the 1,410-second capture budget; its authorized 45-minute expiry supersedes the earlier 25-minute total cap for this campaign. Discard first run of each binary. Preserve pose, manifests, package, settings and content fingerprints.
- Include a non-Tracy release standing TEST at the same pose. Quote its FPS separately from diagnostic attribution.
- Include paired unchanged fly-fast route TESTs with the frozen binaries. Join crossing-frame source-query spans to recorded origin changes, and report per-new-entity work separately from admission polling.

## Milestones

- [x] Merge integration and start frozen baseline Tracy release build.
- [x] Attribute current settled work and main/render overlap; select a source-supported correction.
- [x] Observe focused red and green, inspect full diff, obtain independent source review, and pass production extraction runtime proof.
- [x] Freeze candidate Tracy and non-Tracy binaries.
- [x] Complete required workspace nextest and doctest gates; fmt and strict workspace Clippy passed.
- [x] Capture paired quiet diagnostics plus non-Tracy FPS; inspect logs, grader and content identity.
- [x] Record Step 1 measured deltas, remaining work and risks; update handoff and commit with required trailer.
- [x] After the Step 1 commit, freeze fresh Tracy baseline/candidate with identical journal/semantic/origin/index-update spans; restore production source byte-exact.
- [ ] Measure the later crossing split. BLOCKED: first window and one authorized retry refused pre-start quiet admission; GPU 53% / 25%. Both holds finally-cleared; no accepted Step 2 timings. A new quiet authorization is required.

## Selected bounded slice

The current hot and terrain groups independently poll `Visibility` and `InheritedVisibility`. Share only component admission within one immutable extraction and retain each native query. The work regression requires two columns times 1,024 new quiet entities to add 2,048 copied rows rather than 4,096. Count two reused probes only when the terrain consumer reaches admission; do not claim avoided rows when captured native populations differ.

Use Bevy 0.19's existing `Query::contiguous_iter()` and `ContiguousRef::changed_ticks_slice()` instead of walking all world tables. The original conjunctive `Changed<T>` query owns the captured domain and matched tables. Borrow its native read-only lens as `Ref<T>` for admission, then retain the original query for entity selection. Compilation exposed that `Allow<T>` is archetypal but lacks the `ArchetypeFilter` marker required by direct contiguous iteration. The bounded native alternative converts a read-only query to a filter-erased lens, retaining native matched storage, captured policy, density and tick window. Refuse erasure for a non-archetypal filter. Lens metadata cloning occurs per probe and must be included in paired attribution; it is not a retained cache. Runtime non-contiguous queries conservatively admit the original query, with zero copied rows. This handles sparse components, sparse predicates and sparse default disablers without reconstructing policy or retaining row locations. Remove the broad read-all World parameter when its four probe uses disappear; the surrounding Extract still owns shared MainWorld access.

Within each native chunk, test a minimum wrapping-age reduction over the safe tick slice. Preserve the native first-row positive shortcut. Native `Changed` is `W > min(A, MAX)` where `W = min(now - last, MAX)` and `A = now - changed`, with wrapping subtraction. Since `W <= MAX`, comparing `W` to the minimum unclamped row age preserves the existential result. This retains polling and may read more rows on a late positive mutation; it introduces no retained tick summary. Replace the old broad helper and its implementation-specific tests with native differential tests covering persistent default-filter capture, late registration, sparse fallback, independent windows, fresh tables, migration, swap removal, wrap and safe tick rollback. Preserve the original implementation and observed reds in target-local proof receipts.

Hot's visibility domain permits a superset of Terrain's disablers and adds no extra visibility predicate. A negative Hot component probe therefore excludes Terrain changes; a positive only admits Terrain's own query. This implication depends on the declarations and their joint initialization. It does not authorize sharing hot entities or semantic `hot_changed`.

**Prediction and rejection:** Sharing removes exactly the duplicate visibility reads on quiet consumed calls. Bulk reduction should lower the source-query zone by replacing per-row branching/clamping, but no engine saving is established. Reject it if the native differential tests fail or paired attribution shows no useful net benefit. The prior 0.7674 ms zone is a stale upper bound on this slice, not a promised saving. Use the same main28 package and standing pose for baseline/candidate; do not infer a render-schedule improvement from extraction alone.

Current ownership review rejects whole-frame submission skipping and unchanged-camera transparent-preparation skipping. Native sorted preparation still owns current instance/material/allocator metadata and frame-local offsets. Queue-time asynchronous encoder finishing could overlap subsequent views and deferred application, but not the same camera's already-recorded passes. No such scheduling change is selected without a measured pre-submit window.

The complete source index is a defensible candidate domain only with all lifecycle dirties and full recovery. It intentionally includes uncertified roots, every role-visible mesh, ancestry and retained history. The retained earlier scene already indexed 98,038 nodes among 112,278 entities; per-entity lookup is not evidence of fewer or cheaper reads. No index-scoped rewrite is selected without a measured advantage over contiguous column reads.

Separate command/count captures enable `RUST_LOG=info,wgpu_core=info,wgpu_hal=info`. Bevy's more specific default `wgpu=error` directive explained the earlier empty native queue-scope export despite global `info`. Keep the primary pair's original logging and report native API counts separately; these scopes do not expose upload payload bytes.


## Step 2 authorization and final state

The later `2151.3 FINAL COMPLETE` relay authorized an announced Step 2 window, with one hold no longer than 25 minutes and finally-clear. A subsequent explicit instruction authorized one fresh retry with the same binaries and a short quiet check first. Both windows refused engine admission under unchanged guards. The first and retry hold durations were 71.722926 and 71.473542 seconds. Source is restored to committed Step 1; diagnostic spans remain only in retained build artifacts. See the evidence document for exact guard and cleanup receipts. No further retry is authorized.

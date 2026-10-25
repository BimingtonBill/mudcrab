# #2145.3 track B: upstream ECS summary-tick backport assessment

2026-10-26. Static source work only, based on Mudcrab integration head
`551909a16dd6ea618662033d6cabc915a038a3ba` in
`D:/Mudcrab/worktrees/cluster-postready`. Parent owns integration, staging and builds.

**Verdict: the exact two-PR source backport is preserved, but cannot compile as-is.**
PR #25946 depends on the intervening #25429 API change. No Cargo command, build,
engine run, commit, publication, dependency wiring or registry/vendor source edit
was performed. No performance improvement is established.

## Primary sources and exact patch identity

| Item | Exact revision | Status |
| --- | --- | --- |
| Pristine registry release, all four crates | `c6f634ca9f406d68ba5109d921247b654cb42c10` from each `.cargo_vcs_info.json` | 0.19.0 crates.io archives |
| [PR #25157](https://github.com/bevyengine/bevy/pull/25157), optional per-column summary ticks | [`4dc818e5d901fe2af13e5572080adfe39bbac48a`](https://github.com/bevyengine/bevy/commit/4dc818e5d901fe2af13e5572080adfe39bbac48a) | Applied source delta first; merged 2026-08-13 |
| #25157 first parent | `5208068fe2e432fbe943904458fa76309c078c9d` | Exact before-image for its patch |
| [PR #25946](https://github.com/bevyengine/bevy/pull/25946), table filtering for Added/Changed | [`7756db4a5b73387f2986153a0bec7ce109716c4b`](https://github.com/bevyengine/bevy/commit/7756db4a5b73387f2986153a0bec7ce109716c4b) | Applied source delta second; merged 2026-09-29 |
| #25946 first parent | `066d241a575d6bd405e50d52de70f096beb46522` | Exact before-image for its patch |
| [PR #25429](https://github.com/bevyengine/bevy/pull/25429), static summary-tick hint | [`b9182578165b6a7d9a2bcf62c727d616c7ccc96a`](https://github.com/bevyengine/bevy/commit/b9182578165b6a7d9a2bcf62c727d616c7ccc96a) | Required prerequisite, **not applied**; merged 2026-08-18 |

Applied identities above are the actual merged/squashed commits, not the PR's
unmerged development commits and not mutable `main`. Original receipt
`C:/MudcrabBuilds/cluster-postready-target/r2-upstream-capability.json` identified
these PRs. Its `upstream-*` files describe newer main and were not copied over
0.19.0. The backport uses downloaded full commit patches, GitHub commit/PR API
responses, and raw before/after files at the exact hashes above.

The complete upstream patches cover 31 files (+751/-39) and 11 files (+469/-47),
respectively. Only registry crate source hunks are applied here: `bevy_ecs`, its
two macro companion crates, and `bevy_asset`. The original patches retain the
release notes, root examples and root benchmark changes as provenance; those
files are absent from these registry crates and were not invented in the
backport tree. This is a scoped source backport, not a copy of upstream main.

## Retained artifacts and reproduction

Scratch root: `D:/Mudcrab/builds/cluster-postready-ecs-upstream-backport`.
Every directory under it has an `OWNER.txt`.

- `pristine/crates/bevy_ecs`: exact downloaded 0.19.0 release, with macro crates
  extracted into upstream-shaped `macros` and `macro_logic` subdirectories.
- `pristine/crates/bevy_asset`: exact 0.19.0 asset release.
- `backport/crates/...`: those same sources plus only the two PRs' source hunks
  and the version-context resolutions below. This is the preserved **blocked**
  candidate; #25429 has not been silently folded into it.
- `two-pr-backport.patch`: portable LF-normalized diff against `pristine`.
  SHA-256: `a1b0875db12d277568a0a1644769b1217c677d2d64230f69a3bbb076e0c10043`.
- `receipts/merge-<sha>.patch`, `merge-<sha>.json`, `pr-*.json`,
  `component-history.json`, and `upstream/<PR>/{parent,merged}/...`: pinned
  upstream evidence, including the unapplied prerequisite patch.
- `receipts/apply-25157.log`, `apply-25946.log`, `rejects/`,
  `conflict-resolutions.json`: original application results and exact manual
  replacements. #25946 applied without rejected hunks after partial #25157.
- `receipts/diff-stats.json`: per-file git numstat and normalized before/after
  SHA-256 hashes. `receipts/static-verification.json`: fresh reproduction result.
- `reapply-verification`: independent pristine copy to which the portable patch
  was actually applied, then compared against the candidate.
- `fetch.py`, `reconstruct.py`, `resolve.py`, `verify_source.py`,
  `prove_reapply.py`: source reconstruction/check scripts. They contain no Cargo
  or engine invocation. The proof script deliberately refuses to reuse its
  already-existing verification directory.

To validate patch applicability without changing any source:

```powershell
Set-Location D:/Mudcrab/builds/cluster-postready-ecs-upstream-backport/pristine
git -c core.autocrlf=false apply --check ../two-pr-backport.patch
```

Archive SHA-256 receipts (downloaded from `https://static.crates.io/crates/`):

| Registry archive | SHA-256 |
| --- | --- |
| `bevy_ecs-0.19.0.crate` | `4654b9b3535fa7813d2878a50e1b5ad80a361dc1c913b96ded57667f65d70a01` |
| `bevy_ecs_macros-0.19.0.crate` | `4dc958a194d226d618404e0fea99a3c8b4146207a00a3ba07fa712bf71607240` |
| `bevy_ecs_macro_logic-0.19.0.crate` | `ee0b98a74141ce8dce4654c60020ebbaefab32646f42af6bbae55f282ae65ff7` |
| `bevy_asset-0.19.0.crate` | `43a05907de6a362d2c609a7a1d1b4dca3b58b768f746b719ca1a3c7dfa87d391` |

## Version conflicts resolved; prerequisite left explicit

The first patch rejected **11 hunks across six files**. These are version-context
resolutions, with no imported prerequisite feature:

| File under `crates/bevy_ecs` | Rejected hunks | Resolution |
| --- | ---: | --- |
| `macros/src/lib.rs` | 1 | Insert original summary-tick derive documentation at the 0.19 documentation boundary; newer link context is absent. |
| `src/bundle/insert.rs` | 1 | Pass `self.change_tick` into `before_insert` in the 0.19 unsafe function body; newer explicit unsafe-block context is absent. |
| `src/bundle/remove.rs` | 4 | Add the original Tick import, constructor argument, documentation and initializer field while retaining 0.19 imports/event definitions. |
| `src/bundle/spawner.rs` | 1 | Preserve the original whitespace-only hunk at 0.19's location. |
| `src/storage/table/column.rs` | 3 | Preserve 0.19's `other`/`other_last_element_index` names instead of newer `src` names; add the exact replacement, move-summary comparison and current-system tick semantics. |
| `src/world/unsafe_world_cell.rs` | 1 | Add the original table summary reference to `ComponentTickCells` at 0.19's indentation/unsafe-expression layout. |

All other relevant hunks applied with normal line offsets. Initial live progress
counted these rejects incorrectly; the retained application log establishes the
11/six count above.

The unresolved blocker is independent of those context resolutions:

- `backport/crates/bevy_ecs/src/component/mod.rs:692` declares
  `fn has_summary_tick() -> bool`; #25157's macro emits that function and
  `ComponentDescriptor::new` calls it.
- `src/query/filter.rs:887,925,1135,1173`, added by #25946, instead reference
  `T::HAS_SUMMARY_TICK`. That associated constant is absent from this trait.
- #25429 changes the trait, derive output and descriptor to the associated
  constant, and adds the typed mutable-fetch static hint. It changes four ECS
  source files (+12/-10), plus its release note (+3/-3).

Thus an unmodified composition of exactly #25157 and #25946 cannot type-check.
This is a source-demonstrated missing API, **not an observed compiler diagnostic**.
Replacing the constant with function calls would depart from the merged #25946
implementation and omit #25429's static fetch optimization; adding only a
compatibility constant would leave derive opt-ins inconsistent. Neither shortcut
was taken. Parent must explicitly expand the patch set to #25429 (or select a
separately reviewed compatibility design) before requesting a bounded compile.
No assertion is made that #25429 is the only remaining issue a compiler will find.

## Companion crates and renderer compatibility

`bevy_ecs_macro_logic/src/component.rs` parses `#[component(summary_tick)]`,
rejects sparse-set opt-in, and generates the component implementation. It is a
separate published dependency of `bevy_ecs_macros`, not optional documentation.
`bevy_ecs_macros/src/query_filter.rs` generates the newly required `filter_table`
method for derived filters. All three ECS packages need to resolve consistently;
patching `bevy_ecs` alone leaves old derives unable to accept the attribute or
implement the expanded filter interface. Copied manifests still reference the
registry dependencies; they deliberately do **not** wire this scratch tree.

#25946 also adds `filter_table` to `bevy_asset::AssetChanged<A>` (and accompanying
comments/spacing). The trait method has no default implementation. Leaving
registry `bevy_asset` untouched would cause another missing-trait-method compile
failure in an engine dependency graph even after #25429. Its source companion
patch is included in this scratch diff.

Neither PR changes `bevy_render` or `bevy_pbr`. A read-only search of the existing
vendored 0.19 sources found no custom QueryFilter implementations, affected
ComponentTickCells literals, ComponentDescriptor uses, affected public constructor
calls or direct tick-slice mutation calls. **No matching renderer/PBR source
change is identified as necessary for compatibility**, conditional on a single
consistent ECS/macro/asset dependency graph. This has not been compiled.

Enabling summaries for `MeshMaterial3d<M>` would, however, require changing its
component implementation in the current **PBR vendor** (or a new ECS design).
That is a separate opt-in change, not a required renderer compatibility patch.
No renderer or PBR source was edited here.

## Behavior, late enable and external components

#25157 adds an optional per-column AtomicTick, carried through typed/untyped
mutable access and contiguous ticks. Insert/replace updates it; table migration
merges moved row ticks relative to the current system tick; periodic tick aging
clamps the summary as well as row ticks. Sparse storage has no summary. #25946
uses summaries to reject entire tables before dense query data fetch, retaining
native row Added/Changed checks for admitted tables. Tuples use AND and Or filters
use OR; derived filters delegate to their fields.

This is conservative table admission, not a dirty-entity journal: any relevant
write can reopen an entire table. No all-world zero-scan claim follows from it.
The original non-dense/archetype-fold exclusions in `query/iter.rs` remain: their
comments explain that summaries can travel backwards when other systems access
shared tables. They must not be broadened into unconditional table skipping.

There is **no runtime/late-enable API** in these PRs. Typed registration reads
`Component::has_summary_tick()` (the constant after #25429), and column creation
allocates the summary from that descriptor. It cannot be enabled later on an
existing untracked typed column. `ComponentDescriptor::new_with_layout` gains a
boolean for dynamic table components at registration, not a world method to
retrofit typed external components or initialize all existing table summaries.

Mudcrab can annotate its own component definitions. The current declaration in
`crates/engine/src/cluster/handoff.rs:914` also observes external types whose
implementations remain untracked after these exact two PRs:

| External inputs | Defining crate requiring separate opt-in |
| --- | --- |
| `WorldAssetRoot` | `bevy_world_serialization` |
| `Mesh3d` | `bevy_mesh` |
| `GlobalTransform`, `Transform` | `bevy_transform` |
| `GltfExtras` | `bevy_gltf` |
| `ChildOf` | `bevy_ecs` hierarchy source, not changed by these PRs |
| `Visibility`, `InheritedVisibility`, `RenderLayers` | `bevy_camera` |
| `MeshMaterial3d<StandardMaterial/SnowMaterial>` | current `bevy_pbr` vendor |

Rust's orphan rule prevents Mudcrab from replacing these external Component
implementations. Newtype wrappers would change component identity and do not
opt the existing writers/queries in. A general late-enable extension would be
new behavior outside this exact backport and would need initialization, existing
borrows/escapes, future tables, dynamic IDs, safety and rollback proof.

## Tick mutation escapes and safety assessment

The required invariant is: a summary must not allow rejection of a table that
contains a row whose Added/Changed tick matches the consumer's window.
The normal mutating paths inspected in the patch carry summary updates:
Mut/MutUntyped `set_changed`/`set_added`, typed mutable dereference, contiguous
`mark_all_as_changed`, insert/replace, table moves and tick aging. #25946 repairs
MutUntyped summary updates and guards manual `set_last_changed`/`set_last_added`
updates rather than blindly copying an arbitrary older tick into the summary.
These are source observations, not executed semantic tests or a proof that
all safe combinations preserve admission. Independent review identified the
rollback-and-migration counterexample below.

Important limits remain:

- **Safe rollback setter followed by migration:** `set_last_changed` updates
  only the changed row tick. `Column::initialize_from_unchecked` preserves both
  row ticks but merges only the changed tick into the destination summary.
  Insert at tick 8, safely roll changed back to 5, then move into a new table at
  tick 9: added remains 8 while changed and the destination summary become 5.
  For reader window (7, 10), row-level Added matches while table-summary
  admission rejects it. This uses neither raw cells nor a tick-slice escape.
  The conflict resolution faithfully preserves the upstream implementation;
  this is a source-derived semantic counterexample requiring an executable
  test after prerequisite integration, not a backport transcription finding
  or an observed upstream regression. Relevant scratch paths are
  `change_detection/traits.rs:452`, `storage/table/column.rs:273` and
  `query/filter.rs:925`.
- **Safe mutable tick-slice escape:** in `change_detection/params.rs:369,374`,
  `ContiguousComponentTicksMut::added_mut` and `changed_mut` still return bare
  mutable slices without updating or invalidating the summary. They are
  unchanged by either PR. A caller can use `ContiguousMut::bypass_change_detection_split`
  to obtain values and ticks, change one value, and deliberately stamp its row
  tick through these slices. Row filtering sees the tick; a stale summary can
  reject the table first. For example, summary 5 is old for reader window (7, 9),
  while a manually stamped row tick 8 is new for that same window. The setters
  have no guard/drop invalidation or permanent
  escaped flag. This is a source-derived semantic counterexample, **not a run
  regression** and not a memory-unsafety claim. A mere direct payload write that
  intentionally bypasses both row and summary tracking is the existing bypass
  contract; manually maintaining row ticks exposes the additional mismatch.
- **Raw tick cells:** Column tick-slice/cell getters and UnsafeWorldCell's
  ComponentTickCells can expose the row UnsafeCells. Borrow rules and unsafe
  caller obligations still matter. Raw/manual writes must maintain the summary
  invariant too; the exact backport does not detect or conservatively disable
  tracking after such exposure, including hypothetical exposure before a future
  late-enable extension. Copied ComponentTicks do not escape mutable storage.
- **Atomics are not a journal or synchronization primitive:** AtomicTick uses
  relaxed loads/stores with a same-value fast path, not an atomic monotonic max.
  The upstream docs require separate synchronization for other memory. Preserve
  scheduler access rules and dense-only exclusions; no new cross-thread or
  retained-pointer guarantee is established by adding an atomic.
- No memory-safety defect is established by this bounded source review. New
  summary references retain the storage lifetimes, sparse opt-in is rejected,
  and wraparound uses existing tick comparisons/clamping. Unsafe code, aliasing,
  tick wrap/aging and concurrent filter behavior still require compiled tests
  before integration. Passing syntax and patch checks does not certify soundness.

## Performance bound and immediate consumer

The accepted prior observation in `docs/evidence/postready-cost-2145-2.md` is
**0.8120 ms** for the nested source-query zones, with **1,300,211 copied tick rows
per extraction** and zero admitted native query groups after ready. It is one
paired diagnostic capture, not a profiler-free acceptance run.

**Bounded inference:** for that measured zone alone, the recoverable baseline
cost is between zero and 0.8120 ms; perfect removal cannot save more than the
whole zone. The approximately 0.8 ms target would require removing about 98.5%
of it. Added write/summary/table traversal cost can make net savings negative.
No positive lower bound, expected point estimate, FPS gain or transferable
upstream benchmark multiplier is justified.

Merely linking this ECS backport would leave the current
`source_tick_probe::component_changes` copying every stored row: it calls
`Column::get_ticks_unchecked` directly, so native `Changed` table filtering does
not replace that work. After prerequisite/companion validation, the immediate
consumer would need to remove/replace that copied-row admission, opt in all
relevant component types, and retain native domains/default disablers,
structural/removal/asset recovery and independent differential oracles.
Untracked external inputs or sparse/non-dense queries still need correct
row-level behavior. The amount of the 0.8120 ms attributable to trackable owned
versus external inputs is not established, so no partial saving is asserted.

## What this round proves and leaves open

Fresh static checks:

- Downloaded 0.19.0 files match 233 installed-registry release files after LF
  normalization; the installed registry was read only.
- The final portable patch passes `git apply --check` on pristine and actually
  reapplies to an independent copy. All 233 files compare equal to the candidate
  afterward, after LF normalization.
- Rustfmt check passes for all 27 changed Rust files using edition 2024 and
  `skip_children=true,style_edition=2021`. This checks parsing/formatting, not
  name resolution, typing, macro expansion or compilation.
- All eight Cargo.toml/Cargo.toml.orig copies are byte-for-byte unchanged;
  manifests/lockfiles outside scratch were not edited. Every scratch directory
  has OWNER.txt. No engine/vendor/registry source or dependency link was changed.

Unproved: compilation (known #25429 blocker), expanded dependency integration,
semantic/property/Miri tests, the tick-slice and rollback/migration counterexamples as executable
regression, performance, scheduling cost, platform coverage and native renderer
acceptance. The six upstream summary-tick tests were retained, not run. No build
gate slot was consumed. Source review should settle the prerequisite and the
external-component/tick-escape policy before spending a compile slot.

## Exact delta against pristine (normalized LF)

Counts use `git -c core.autocrlf=false diff --no-index --numstat` on normalized
before/after source bytes. OWNER files, provenance downloads and scripts are
excluded. These are **blocked two-PR candidate** counts, not a claimed buildable
three-PR result.

| Scope | Changed files | Added | Deleted |
| --- | ---: | ---: | ---: |
| bevy_ecs source (excluding companion crates) | 23 | 815 | 78 |
| bevy_ecs_macro_logic source | 1 | 27 | 0 |
| bevy_ecs_macros source | 2 | 16 | 0 |
| ECS family subtotal | 26 | 858 | 78 |
| bevy_asset companion source | 1 | 10 | 2 |
| **All applied source** | **27** | **868** | **80** |
| Cargo manifests, separately (8 copied files) | **0** | **0** | **0** |
| Other metadata/lockfiles | **0** | **0** | **0** |

| Source file | Added | Deleted |
| --- | ---: | ---: |
| `crates/bevy_asset/src/asset_changed.rs` | 10 | 2 |
| `crates/bevy_ecs/macro_logic/src/component.rs` | 27 | 0 |
| `crates/bevy_ecs/macros/src/lib.rs` | 7 | 0 |
| `crates/bevy_ecs/macros/src/query_filter.rs` | 9 | 0 |
| `crates/bevy_ecs/src/bundle/insert.rs` | 3 | 0 |
| `crates/bevy_ecs/src/bundle/remove.rs` | 13 | 2 |
| `crates/bevy_ecs/src/bundle/spawner.rs` | 1 | 0 |
| `crates/bevy_ecs/src/change_detection/mod.rs` | 14 | 2 |
| `crates/bevy_ecs/src/change_detection/params.rs` | 97 | 3 |
| `crates/bevy_ecs/src/change_detection/tick.rs` | 63 | 1 |
| `crates/bevy_ecs/src/change_detection/traits.rs` | 17 | 0 |
| `crates/bevy_ecs/src/component/info.rs` | 33 | 0 |
| `crates/bevy_ecs/src/component/mod.rs` | 38 | 0 |
| `crates/bevy_ecs/src/entity/clone_entities.rs` | 1 | 0 |
| `crates/bevy_ecs/src/observer/mod.rs` | 4 | 0 |
| `crates/bevy_ecs/src/query/fetch.rs` | 12 | 5 |
| `crates/bevy_ecs/src/query/filter.rs` | 102 | 0 |
| `crates/bevy_ecs/src/query/iter.rs` | 113 | 40 |
| `crates/bevy_ecs/src/query/mod.rs` | 143 | 1 |
| `crates/bevy_ecs/src/storage/non_send.rs` | 1 | 0 |
| `crates/bevy_ecs/src/storage/sparse_set.rs` | 1 | 0 |
| `crates/bevy_ecs/src/storage/table/column.rs` | 70 | 2 |
| `crates/bevy_ecs/src/storage/table/mod.rs` | 19 | 2 |
| `crates/bevy_ecs/src/system/system_param.rs` | 1 | 0 |
| `crates/bevy_ecs/src/world/entity_access/world_mut.rs` | 45 | 7 |
| `crates/bevy_ecs/src/world/mod.rs` | 2 | 0 |
| `crates/bevy_ecs/src/world/unsafe_world_cell.rs` | 22 | 13 |

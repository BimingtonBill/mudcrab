# impl-560 — distant LOD, step 4a: object blocks, per-cell hiding, the exclusion filter

Branch `impl/560-lod-object-blocks`, cut from `phase2/lod-engine-terrain`.

## What changed and the approach

`crates/engine/src/lod.rs` now plans, streams and renders `LodBlockKind::Objects` blocks
alongside terrain, on the rules the original brief set instead of `local/research-557/DESIGN.md`'s
per-kind-band design, plus a review round's worth of correctness fixes (seamless handover, F2-F5)
described in their own section below.

- **New planner, `plan_object_blocks`.** Terrain's `plan_blocks` nests (a coarse block stays
  resident under a finer one); objects can't, or every building draws twice. The new function
  selects an **exclusive quadtree**: start from the coarsest band level that actually has object
  rows, split a block into its four children while the *parent's* rectangle is within the next
  finer band's distance, and draw only the leaves. It shares `rect_distance_units`, `block_cells`
  and `block_is_covered` with terrain, and uses the same `unload_scale` hysteresis pattern.
- **`plan_lod_blocks` now runs both planners** each frame (terrain on `config.lod_bands`, objects
  on the same list unless `--lod-objects off`, which passes an empty band slice — `plan_object_blocks`
  handles that by wanting nothing and unloading every resident object block) and merges their
  `wanted`/`unloaded` lists into the one request/unload loop that already existed.
- **Segment ownership and per-cell hiding, not the terrain shader clip.** `LodTerrainMaterial`'s
  fragment discard is a per-pixel test; `docs/research/lod-hiding-under-loaded-cells.md`
  (research done for the original brief) measured that 19-47% of a Tamriel LOD4 object shape's
  triangles reach outside their owner cell, so a spatial clip cuts or overhangs wrongly. Instead, a
  new `LodObjectSegment { owner_cell }` component goes on a primitive whose `GltfExtras` carries
  `openSkyrim.lodSegment` (`segment_owner_cell`: `dx = segment/4, dy = segment%4` from the block's
  anchor — the converter contract, task impl-558), and `update_lod_object_visibility` sets
  `Visibility::Hidden` on it while its owner cell is loaded. A primitive with no `lodSegment` extra
  never gets the component and is never hidden by a cell.
- **The "loaded" signal is shared, not duplicated.** A `LoadedCells` resource, filled once a frame
  by `collect_loaded_cells` from the same `TerrainPatch` + `ExteriorCellGrid` query `update_lod_clip`
  used to run inline, feeds both the terrain clip and the object-visibility system.
- **Material and shadow policy is per-kind.** `track_lod_readiness`'s descendant loop matches on
  `pending.key.kind`: `Terrain` keeps the existing rewrite into `LodTerrainMaterial`; `Objects`
  leaves the GLB's own `StandardMaterial` untouched (culling, normal map, roughness) and only
  attaches `LodObjectSegment` when the extra is present. Both kinds get `NotShadowCaster`.
- **The `materialExclusion` filter runs on both kinds.** research-557 finding 9.1 found the LOD
  spawn path never applied the reference path's `has_explicit_material_exclusion` filter, so a
  terrain block's non-rendering placeholder primitives (an `EditorMarker` shape, the `WATER` node)
  were drawing. The check is now `pub(crate)` in `streaming.rs` and shared; a matched primitive is
  hidden and counted (`StreamingMetrics::lod_excluded_primitives`) for both kinds.
- **`crates/engine/src/config.rs`** — `lod_objects_enabled: bool`, default `true` (objects are on
  whenever `--lod` is), and `--lod-objects on|off`, parsed the same way `--lod` is.
- **`crates/engine/src/streaming.rs`** — `has_explicit_material_exclusion` is `pub(crate)`;
  `StreamingMetrics` gained `lod_object_blocks_resident` and `lod_excluded_primitives`. Neither is a
  failure signal, so `metrics.rs` needed no change — the existing generic `lod_asset_failures` /
  `lod_validation_failures` / `lod_invariant_failures` terms already cover both kinds.
- **`world/database.rs` and `app.rs` remain untouched.** `LodBlockTable` and `load_lod_block` were
  already kind-generic, so no query change was needed. See "Left out" for the `--lod-fixture`
  decision.

## Review round: seamless handover, and F2-F5

A first review found the core sound but flagged one required fix and four smaller ones, all in
`crates/engine/src/lod.rs`. In order:

### 1. Seamless split/merge (required)

**The bug.** `plan_lod_blocks` computed `desired` as either the parent *or* its children, decided
purely by distance. The instant distance justified a split, the parent left `desired` and the
children entered it — but a child takes many frames to load, so the area between "parent removed"
and "children ready" drew nothing. Merging had the same hole in reverse.

**The fix: a sibling-group handover.** `LodPlanning` gained two new fields, `ready` and `failed`
— both subsets of `resident`, computed once in `plan_lod_blocks` from `LodWorld` plus a new
`pending: Query<(), With<PendingLodBlock>>` parameter (`ready` = `Resident` and no longer
`PendingLodBlock`; `failed` = `LodBlockStatus::Failed`). `plan_object_blocks` uses them like this,
at each split decision:

- **Split direction.** Once distance justifies a split, check `group_complete`: every one of the
  parent's *table-listed* children must be in `ready` (an unlisted child is still simply absent and
  never blocks it). If the group is **not** complete, the parent is kept in `desired` as a
  fallback, and every listed child is requested/kept as a flat leaf **without recursing into it
  further this frame** — a child that itself wants to split again gets its own turn at this same
  handover on a later frame's fresh pass, once it has settled. Any child that is already `ready` is
  pushed onto the new `LodPlan::hold_hidden` list, so the group reveals together instead of one
  quadrant popping in at a time. Once the group **is** complete, the parent is left off `desired`
  (so it unloads) and every listed child goes onto `LodPlan::release_hidden`.
- **Merge direction.** When distance says not to split but children are still resident from an
  earlier split, and the parent is not yet `ready`, the children stay in `desired` exactly as they
  already are (no visibility change needed — they were never hidden) while the parent is
  requested. Once the parent becomes `ready`, the children simply fall out of `desired` and unload,
  the same frame the parent shows.
- **Applying it.** `LodPlan` gained `hold_hidden`/`release_hidden: Vec<LodBlockKey>` (terrain never
  populates either). `plan_lod_blocks` applies them with plain `Commands` — `Visibility::Hidden` /
  `Visibility::Inherited` — read *after* this frame's unload loop, so a key that a group-complete
  release immediately replaces with finer children again (rare, only on a cascading multi-level
  settle) is simply absent from `world.blocks` by then and skipped rather than double-commanded.
  This is safe against `track_lod_readiness`'s own `Visibility::Inherited` write because `ready` is
  always one frame stale relative to it (`plan_lod_blocks` runs *before* `track_lod_readiness` in
  the chain): a child that finishes loading this exact frame is planned as "not yet ready" and
  picked up on the next pass, so the two writers never touch the same entity the same frame.

**A known limitation, not fixed.** The "don't recurse into an incomplete group's children" rule
means a *cascading* multi-level settle (say level 16 into level 8 into level 4, all at once) takes
more than one frame to fully resolve even when every block involved loads instantly — a level-8
child must itself become `ready` as a level-8 block before its own level-4 split gets a turn. I
judged this an acceptable, self-correcting trade against the alternative (recursing into
not-yet-graduated children risks a level-8 key that is *never* individually requested if it always
prefers to split further, which would leave the level-16 parent's group permanently incomplete —
a subtler version of the same hole this fix exists to close). `object_quadtree_drops_the_parent_and_releases_children_once_the_group_completes`
and its sibling tests below all exercise one split boundary at a time for this reason; nothing here
exercises the two-boundary cascade end to end.

### 2. F2 — a coarsest band with no object rows no longer disables the tier

`plan_object_blocks` used to seed its walk from `bands`' single coarsest level unconditionally. A
band list whose coarsest level has no `objects` rows (object LOD only ever has levels 4/8/16; a
horizon-only `32:...` entry has none) found zero root candidates and planned nothing, silently.
`plan_object_blocks` now walks `usable_object_levels(bands)` (coarsest first) and starts from the
first level with rows in the table (`level_has_rows`). A new `Startup` system,
`check_object_band_levels`, warns once if *no* usable level has any object rows at all.

### 3. F4 — a permanently `Failed` child no longer leaves a hole

Hysteresis (`is_tracked`, the split/merge "is a child currently being tracked" check) now excludes
`failed` explicitly: `resident.contains(key) && !failed.contains(key)`. Separately — and this is
what actually closes the hole — a `Failed` key can never enter `ready` (by construction), so
`group_complete` can never become true for a split blocked by one: the parent is kept in `desired`
*indefinitely* for that area instead of leaving a gap once the other three children arrive. The
three good children still show (each independently reaches `ready` and is released once its own
group condition holds, which does not require the failed sibling), so the result is a permanent
overlap in the failed quadrant rather than a permanent hole — exactly what the review asked for.
The failed key itself is still tracked in `desired` (so it stays in `world.blocks`, unretried, per
the existing `LodBlockStatus::Failed` contract) but never re-requested.

### 4. F5 — a non-dividing band level is dropped, not silently mis-subdivided

New `usable_object_levels(bands)`: sorts descending, then keeps a level only if it divides evenly
into the next coarser **kept** level (a level that fails the check is dropped and does not become
the reference for the next comparison, so one bad level doesn't cascade into dropping everything
below it). `quadtree_children`'s ratio (`level / child_level`) is otherwise silently truncated by
integer division for a non-dividing pair — `16 / 6 = 2`, covering only a 12x12 corner of a 16x16
parent. `plan_object_blocks` and `object_root_distance_limit` both call it; `check_object_band_levels`
warns once, naming the dropped level(s), if it drops anything. Every shipped preset (4/8/16) and
every band list the parser accepts by default passes through unchanged.

### 5. F3 — the object-root distance bound the validator actually guarantees

`validate_lod_lifecycle`'s `out_of_range_roots` check used `max(band) * unload_scale` for every
object level, which is only coincidentally loose enough for the shipped presets (all three within
a factor of 5 of each other). New `object_root_distance_limit(level, bands, unload_scale)`: for the
coarsest usable level, its own relaxed band (no parent to overshoot from); for any finer level, its
own relaxed band **plus the diagonal of the block one level coarser** — the actual bound the
quadtree's decision 4 ("whatever their own distance") guarantees, since a leaf's rectangle is a
subset of the parent rectangle that justified splitting into it, and the parent-to-camera distance
is therefore a lower bound on the leaf's own. `object_root_distance_limit_adds_the_coarser_blocks_diagonal`
tests this directly with the review's own example list, `8:32000,4:20000` (asserts the new level-4
bound exceeds the old formula's `32000 * 1.1`, i.e. that the old bound really would have
false-positived a legitimate leaf there).

## The quadtree and its hysteresis, in three lines

Starting from the coarsest band level that has object rows, a block splits into its four children
of the next finer level while **the block's own rectangle** — not each child's — is within that
finer band's distance, with the relaxed `distance * unload_scale` threshold used only when the
block is already tracked as split (any child resident and not failed), so a camera near a boundary
does not flap. A split does not finish in one frame: the parent stays resident and every listed
child is requested as a flat leaf, held hidden once individually ready, until every listed child is
ready — then the parent unloads and the children release together; a merge mirrors this, keeping
the already-visible children resident until the coarser parent is ready. An unlisted child is
simply absent and never blocks its siblings; a permanently failed one is a required member that can
never complete, so its parent is kept for that area rather than leaving a hole.

## Tests: name and what it proves

Pure (`lod::tests`, no `App`, no assets), object planner:

- `object_quadtree_keeps_the_parent_while_a_child_is_still_loading` — one level-8 parent (already
  resident and `ready`) and four level-4 children in a mix of states (one `ready`, one resident but
  still loading, two not yet requested). Asserts the parent is not unloaded, only the two
  never-requested children are newly `wanted`, and `hold_hidden == [the one ready child]` (the
  still-loading one needs no override — it starts hidden anyway). This is the review's required
  handover test, the "keeps the parent" half.
- `object_quadtree_drops_the_parent_and_releases_children_once_the_group_completes` — the same
  shape with every child `ready`. Asserts `unloaded == [parent]`, `wanted` empty, and
  `release_hidden` is exactly the four children. The "drops it once all children are ready" half.
- `object_quadtree_split_survives_small_camera_moves_within_the_relaxed_band` — four children
  already resident and `ready`, camera at 20,480 units from the level-8 rectangle (past the raw
  20,000 level-4 band, inside the 22,000 relaxed one). Asserts nothing is newly wanted or unloaded
  — the settled split holds through the hysteresis window, the case the first review round's
  now-removed `object_quadtree_split_survives_small_camera_moves_hysteresis` covered.
- `object_quadtree_keeps_children_until_a_merging_parent_is_ready` — children resident and `ready`,
  camera receded past even the relaxed band (24,576 units), parent not yet resident. Asserts the
  parent is `wanted`, nothing is `unloaded` (children stay exactly as visible as they were), and
  both hold/release lists are empty (no visibility change needed on the merge side).
- `object_quadtree_unloads_children_once_the_merged_parent_is_ready` — same, but the parent is now
  `ready`. Asserts the four children are `unloaded` and nothing is `wanted`.
- `object_quadtree_never_completes_a_split_blocked_by_a_failed_child` (F4) — three children ready,
  one `failed`, parent resident. Asserts `unloaded` and `wanted` are both empty: the parent is kept
  forever and the good children stay put, with no retry churn on the failed one.
- `object_quadtree_treats_an_unlisted_child_as_absent` — rewritten for the handover: cold start
  (nothing resident) with one of four children missing from the table asserts the parent *and* the
  three listed children are all `wanted` (the fallback applies from the first frame); a second phase
  with the three children `ready` asserts the parent drops anyway — proving an absent child, unlike
  a failed one, does not block completion.
- `object_quadtree_off_unloads_every_resident_object_block` — unchanged from the first round; an
  empty band slice (`--lod-objects off`) unloads a resident object block and wants nothing.
- `object_quadtree_starts_from_the_coarsest_level_that_has_object_rows` (F2) — a two-band list
  where only the finer level has table rows; asserts the walk starts there instead of finding zero
  root candidates at the coarser, row-less level.
- `usable_object_levels_drops_a_level_that_does_not_evenly_divide_the_next_coarser_kept_one` (F5) —
  a level 6 between 4 and 16 (16 % 6 != 0) is dropped, and does not itself become the reference the
  next level is checked against (4 still divides 16, not the dropped 6).
- `object_root_distance_limit_adds_the_coarser_blocks_diagonal` (F3) — the review's own
  `8:32000,4:20000` list: level 8 (coarsest usable) gets its own relaxed band; level 4 gets its
  relaxed band plus the level-8 block's diagonal, and that sum is asserted to exceed the old
  `max(band) * unload_scale` formula.

Pure, object segments/extras (unchanged from the first round):

- `segment_ownership_maps_index_to_its_offset_cell`, `lod_segment_extra_reads_the_converters_marker`,
  `a_loaded_owner_cell_hides_only_its_segment_and_unhides_when_it_unloads` (minimal `App`, no
  assets), `the_exclusion_check_recognises_the_converters_extras_shape` (in `streaming::tests`),
  `lod_objects_off_disables_only_the_object_tier` (in `config::tests`).

Every pre-existing terrain-only LOD test in `lod::tests` and `app::tests` still passes unchanged —
terrain calls `plan_blocks`, which never reads `ready`/`failed` or populates `hold_hidden`/
`release_hidden`.

## Left out, and why

- **`--lod-fixture` still has no object rows or GLBs.** Same reasoning as the first round: nothing
  here needs a full asset round-trip, and the fixture's terrain-tuned assertions
  (`lod_blocks_covered_skipped % 4 == 0`, the settle/unload timing) are a needless risk to disturb
  for no test that needs it. The real-data run against `C:/Modding/SkyrimConverted-upstream15/` is
  what exercises GLB loading, segment extras and the handover's actual frame timing end to end.
- **The cascading two-level settle is not covered end to end** (see the handover section above) —
  only that it converges over multiple frames, not that any single frame of it looks correct. A
  real-data run with `--lod-fixture`-style logging of `lod_object_blocks_resident` across a few
  hundred frames while flying toward a level-16 block would be the cheapest way to see it settle.
- **A pre-existing gap, not touched:** `track_lod_readiness`'s bounds-mismatch path
  (`commands.entity(entity).remove::<PendingLodBlock>()` without also flipping `world.blocks[key]`
  to `Failed`) leaves that key `Resident` with `Visibility::Hidden` forever, but — unlike a true
  `Failed` block — it now reads as `ready` under this round's definition (`Resident` and no longer
  `PendingLodBlock`). A sibling group blocked only by a bounds-validation failure would therefore
  wrongly read as "complete" and release its siblings over a permanently-hidden, uncovered
  quadrant — the F4 hole, reappearing through a different door. This is pre-existing behaviour the
  review did not ask me to touch and is not exercised by any test here; flagging it for whoever
  picks up validation failures next, or for a one-line `*status = LodBlockStatus::Failed` fix
  alongside that path if it turns out to matter on real data.
- No converter changes, no shader changes, no schema bump, no trees — all still out of scope.

## Checks run

All three from the worktree root (`D:/wkj-impl/impl-560`):

- `cargo fmt --all -- --check` — clean (after one `cargo fmt --all` pass to settle line-wrapping;
  re-checked clean afterward, twice — once per round of fixes).
- `cargo clippy -p engine --all-targets -- -D warnings` — `Finished \`dev\` profile [optimized +
  debuginfo] target(s) in 3.32s`, no diagnostics. Two lints were caught and fixed along the way:
  `unnecessary_sort_by` (first round, `levels.sort_by_key(|band| std::cmp::Reverse(band.level))`)
  and `manual_is_multiple_of` (this round, `coarser.level.is_multiple_of(band.level)`).
- `cargo test -p engine` — lib: `test result: ok. 94 passed; 0 failed; 0 ignored; 0 measured; 0
  filtered out; finished in 0.81s`, plus `world-inspect` bin `3 passed`, `main` bin `0 passed`,
  doc-tests `0 passed`. 94 is 16 more than the branch point (`d42df56`) had (78): the first round's
  nine plus this round's net seven (eleven object-planner tests now stand where two,
  `object_quadtree_draws_each_area_exactly_once` and
  `object_quadtree_split_survives_small_camera_moves_hysteresis`, stood before, both rewritten
  against the handover semantics rather than kept as-is). Every terrain and segment/extras test
  stayed green throughout both rounds.

Not run here (no GPU in this worktree): the engine binary, `--lod --lod-objects` on real assets,
and the visual double-render/overhang/pop-in checks this handover and
`docs/research/lod-hiding-under-loaded-cells.md` both predict.

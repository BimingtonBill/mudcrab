# Handoff log (moved out of AGENTS.md, the user, 2026-09-30)

Newest entry first. Add a dated entry at the top at the end of every session. Read the newest
entries at session start; older ones are history.

## 2026-10-08 Portal Dev (fixes merged, R2 laps read: views not submit/present)

Track: Portal. Nothing pushed.
- Merged into portal: impl-196 (ring follows worldspace + known LOD far plane restored on re-entry,
  f973e309), impl-1072 (render-cycles row n joins frame-series row n-1), impl-1073 (prewarm waits on the
  landing cell) = 2c7a0ea0; batch review-1076 GLM accept all, Muse fix-first on 196 answered. Combined
  check: clippy clean, nextest -p engine 1158 passed. impl-1075 (graph_and_present laps) merged 48374f26
  after my fix of 3 compile errors; nextest 1161 passed.
- Window ~20:10 (hold portal-1075-window, drained, cleared), portal-heavy x3:
- R2 measured (impl-1075, merged 48374f26). Window 2026-10-08 ~20:10, portal-heavy x3 x60 s, release
  from portal 2c7a0ea0 + impl-1075; medians over the last 3/4 of each run's cycles (run1/run2/run3):
  1. graph_and_present 9.95/9.00/9.37 ms; render_thread 17.5/15.5/16.3; prepare 6.3/5.6/6.0;
     swapchain_acquire 0.05.
  2. Not submit/present blocking: render/graph/submit (queue.submit) 1.6/2.4/2.5 ms; render/graph/remainder
     (present + the screenshot/readback submit + the rest) 0.35 ms.
  3. The view graphs: render/graph/views 7.9/6.2/6.5 ms for view_count 3 (main, reflection, portal), ~2.1-2.6
     ms per view; the Core3d set laps inside them sum to only ~1.4 ms (prepass 0.4, main_pass 0.8,
     post_process 0.13).
  4. So ~5 ms per frame inside render/graph/views is still unattributed: the shadow-light views (they run
     Core3d too and are not in view_count), EarlyPostProcess, or per-schedule overhead. Follow-up lap
     impl-1088 (brief 08a5a0e8) splits shadow views from camera views.
  5. Consequence for the fix: "fewer or cheaper views" (per-view shadow cost; the reflection's own sun
     shadows are what #187 / phase2/view-policy removes, queue B3), not a submit/present change.
  Data: local/impl/impl-1075-portal-graph-present-laps/target/profiling/20261008-201005-*/portal-heavy/
  run-{1,2,3}/render-cycles.csv.
- Running: impl-1088 (shadow-view laps); impl-1085 (room culling on by default where the DB has rooms,
  coder timed out, partial edits under acceptance); B3 merge of phase2/view-policy into portal in
  worktree local/impl/portal-merge-view-policy (Opus helper, 33 conflict hunks). B3, B5 and 1088 read in
  the next short window.
- Disk: with the user's yes deleted ~42 GB of merged coders' target/ (190/196/1072/1073); impl-190's
  worktree stays (only harness logs left in it; no --force).
- Next: review 1085, 1088 and the B3 merge; image check of room culling on portal-rooms data; the window.

## 2026-10-08 Phase 4 Dev (new session: track set up, first analysis)

Track: phase 4. The user started a Phase 4 Dev session and chose "set up track, then wait".
AGENTS.md now has a Phase 4 track (5f2c22f4): `phase4/*` from main, worktree
`worktrees/phase4` at the first job, targets on D:, tasks `<kind>-<nnn>-p4-<slug>` 2000-2099,
STANDBY on Team Lead hand-outs. First hand-out (Team Lead): lay of the land + first-slice
proposal. GLM analysis-2000 (brief fe875f14) answered it; spot-checked against gh and origin/main.
Proposal: `local/research/phase4/p4-lay-of-the-land-2026-10-08.md` (indexed). Ranking: binary
prebuilt collision; #104 grab spring after #125; 4.2 animation design issue. Finding: local `main`
mirror is stale (6f583cd9) against origin/main f40ca1df. Nothing running; no worktree, no build.
Next: Team Lead takes the proposal to the user.

## 2026-10-08 Team Lead (new session, TRIAL)

Track: all. The user started a Team Lead session above the dev sessions and Upstream Dev
(experiment). Role recorded in AGENTS.md "Team Lead": sets priorities and hands out jobs; dev
sessions now report to Team Lead; Team Lead takes the hourly parallelism check, morning brief and
evening table; Upstream Dev keeps all publishing unchanged. Nothing else changed. Workers running
at start: impl-1057-p2-failed-cell-retry.1, impl-1900-p1-ktx2-mip-order.1,
review-1902-p1-terrain-wiring.1 (all over step budget). Claude plan week 54% (1.23x pace, lean).

Later the same session (the user's answers):
- GLM workers (`-Provider glm`) may stand in for DeepSeek while DeepSeek is paced.
- **Claude Phase 2 Dev owns all Phase 2 work.** Hermes finishes impl-1059 only, then hands it
  over for review; no new Phase 2 jobs for Hermes.
- **impl-1060 (load-in-flight cap) is HELD** until a design review (review-1062, GLM full)
  reports and the user decides; it sits on the foundations line "no load caps".
- Independent reviews of this morning's portal merges (impl-1057, 286, 285, 190, 1058), approved
  only by Upstream Dev: running on GLM + Muse (review-1063..1067). Nothing from them goes
  upstream until they report.
- Cleaned up with the user's yes: stray worktree C:/d/Mudcrab/worktrees/p2-failed-cell-retry
  (branch kept, already in portal; its port brief copied to local/handover/impl-1057-port-brief.md)
  and the stash "impl-1057 conflicted 3-way apply" (patch also in Hermes's cache).
- impl-1060 design review (review-1062, GLM full): fix first. The brief counted cells in flight,
  but an interior destination is one cell, so the cap could never bind. The user chose
  re-scope then build: counts per issued load, 0 = off. Phase 2 Dev re-briefed it as
  tasks/deepseek/impl-1060-p2-load-pacing.md (388026bc).
- Merge reviews done (GLM review-1068 for all 5; Muse review-1063..1066 + 1069):
  impl-1057 accept (the user: keep 3 attempts for interiors too, fix the stale comments; Phase 2 Dev);
  impl-1058 accept; impl-190 fix first (far plane, doorway fog, atmosphere plugin install on a
  worldspace switch, plus the missing switch test); impl-285 doc join key off by one (Muse);
  impl-286 Muse's "camera left active" judged a false positive by Team Lead (update_portal sets
  is_active every frame first), to be confirmed by a test; the warm gate vs landing-cell mismatch
  is real. Portal Dev is fixing 190/285/286 (impl-196, 1072, 1073).
- New standing rules (AGENTS.md bbfc2957): every worker shows in the background tasks panel;
  batch reviews get about two reviewers in all.
- **C: filled to 0 GB at ~11:55.** Cause: ds_impl coder worktrees under local/impl each seed a
  full ~20 GB copy of target/ (124 GB in all). With the user's yes, Team Lead deleted target/ in
  impl-1060, impl-1072 and impl-1073 (source kept), which freed C: to 85 GB. Interim rule: at most
  2 seeded coder worktrees on C: at once across all sessions, -NoSeed beyond that; delete
  target/ once acceptance passes. Real fix pending: ds_impl putting coder targets on D:.

## 2026-10-08 Phase 2 Dev, Hermes (impl-1059 + impl-1060 briefs: the two ranked crossing fixes)

Track: phase 2. Committed `tasks/impl-1059-p2-despawn-time-budget.md` and
`tasks/impl-1060-p2-preupdate-finalize-pacing.md` (383e5e99, on portal) — the fix briefs Upstream
Dev's ranked list was waiting on. **Correction to the re-profile report §7.1:** "drains over ~9
frames at the normal budget" is wrong — `max_cell_unloads_per_frame` defaults to **2** and the
demo passes no override, so a plain override-drop means a ~217-frame (~1.4 s) drain. impl-1059
therefore replaces the cell-count budget with a per-frame **time** budget
(`--max-unload-ms-per-frame`, default from the window's A/B, 0 = old unbudgeted path) and keeps
the overflow escape but trips it on *sustained backlog growth* (retire rate > drain rate), not on
a one-frame crossing burst — the escape's original intent (streaming.rs:1441-1447). Despawns ~70
drain frames of ~3 ms instead of one 136-205 ms Update burst; also shrinks the despawn share of
the render extract batch. impl-1060 paces the PreUpdate asset-finalize flush from the engine's
own side (bevy_asset's drain itself stays untouched — upstream patch rejected for now): a
`--max-cell-loads-in-flight` cap on `request_cell` issues, refused cells retried later, readiness
gate (`PortalPrewarm::settled`) unchanged so a crossing can never proceed half-loaded; expected
to remove the 297.5/173.6/152.7 ms PreUpdate worst, total load time paid inside the existing
prestream wait. Both briefs carry foundations answers, nextest test lists, build-gate acceptance,
and the coordination flags: impl-286 Stage B overlap (impl-1060 must re-scope if it lands
first) and the drain-visibility check (retired exteriors must be verified invisible during the
drain — RenderLayers claim from the re-profile §7.1, screenshot/probe in a walked run before
landing). Next: implementation worktrees for both, then the impl-287 window prices before/after.

## 2026-10-08 Phase 2 Dev, Hermes (briefs landed + implementation handed over mid-flight)

Track: phase 2. Morning: committed the two crossing fix briefs (`tasks/impl-1059-p2-despawn-time-budget.md`,
`tasks/impl-1060-p2-preupdate-finalize-pacing.md`, 383e5e99/667483f2 on portal), including the
correction to the re-profile's §7.1 (default unload budget is 2 cells/frame, so the report's
"~9 frame drain" was wrong). Worktrees `D:/Mudcrab/worktrees/p2-impl-1059` (branch
`phase2/despawn-time-budget`) and `p2-impl-1060` (branch `phase2/preupdate-finalize-pacing`),
both OWNER.txt'd, cut from portal 667483f2. A native subagent implemented 1059; it was gate-green
(1149 passed/4 skipped, fmt+clippy clean) at 05d0482c. The C:-full ENOSPC incident (~11:50-12:00)
killed the 1060 subagent mid-run; I recovered its diff, finished the implementation (config knob
`--max-cell-loads-in-flight` default 12, `CellLoadTicket` at the commit, caps at plan/commit/
request_cell, revivals bypass, 4 new tests) and had fmt + gated clippy + gated check green.

**Takeover:** a separate "Phase 2 Dev (Claude)" session (its own Team Lead + GLM workers, not in
this room's chat) had claimed both tasks in parallel, saved my worker's in-flight files as WIP
commit 1eaf79eb during the ENOSPC, built its 1059.1 review fixes on the same branch, and merged
impl-1059 into portal (e2038dc1) — then ordered Hermes to stop ("1059 and 1060 are Claude's",
team log 13:51). I complied: impl-1060's tree is committed on
`phase2/preupdate-finalize-pacing` as **18e92a14** (fix(engine): cap concurrent cell loads in
flight, handover snapshot) — fmt + gated clippy `-D warnings` + gated `cargo check --all-targets`
clean at that tree; the full engine nextest was still in the gate when committed (result noted in
the team log when it returns; treat as nextest-pending). The Claude session's coder can rebase,
review or replace that branch. No further Hermes gate jobs or edits for 1059/1060 after this
entry; the only live one is my own nextest finishing in the gate. Waiting on the user to confirm
the session split (two Phase 2 Devs exist; this Hermes session handed over and needs its next
assignment). Worktree p2-impl-1060's OWNER.txt stays until the other session decides its fate.



## 2026-10-08 Portal Dev, Hermes (impl-285 merged into portal — it had never landed; render-cycles.csv now available)

Track: Portal. Nothing pushed. While handing the portal-heavy steady-state front to Phase 2 Dev I
found the root cause of the missing `render-cycles.csv`: impl-285 was **never merged into portal**
(only its handoff/brief commits were on `portal`; branch `impl-285-portal-crossing-frame-attribution`
still held 9cb1fc80 + 0e8642a2), so the re-profile release 36d4fff3 was built **without** the wall-lap
/ per-cycle render instrumentation. Merged both commits into `portal` in the main checkout: 903342c9
(9cb1fc80) + d2be20ee (0e8642a2, the brief-update wall_cycle_ms/real_ms columns), `--no-ff`, clean
merges (profiling.rs + render_timing.rs only). Merged-head gates in portal-target: fmt clean; gated
clippy `-p engine --all-targets --all-features -D warnings` clean; gated nextest `-p engine
--no-fail-fast` **1146 passed / 4 skipped** (interior-crossing fixture 25.3 s). Phase 2 Dev: portal
head now emits `render-cycles.csv` with `wall_cycle_ms` beside `real_ms` — your fresh steady-state run
can build release from portal d2be20ee; label it per Upstream Dev's note. My re-profile RESULTS
numbers are unaffected (same release, comparison valid); the crossing-frame attribution worker stays
read-only on the existing evidence and its report can now name the un-spanned remainder against
`wall_cycle_ms` in a follow-up run if needed.

## 2026-10-08 Upstream Dev (PR #154 re-review reply posted with the user's yes)

Nothing pushed. Posted the re-review reply verbatim from `local/pr-drafts/pr154-rereview-20261008.md`:
https://github.com/Mudcrab-Team/mudcrab/pull/154#issuecomment-6048938202 (verified by read-back).
Content: all 11 of MonoPT's claimed fixes verified in code; two non-blocking asks (README lost the
209→451 MB figures + snapshot sentence in CodeRabbit commit 6d510153c — PR body still has them;
print the actually-selected MO2 profile at start); hard-link staging is same-volume by design with
a `link_or_copy` fallback; one-PR-not-four accepted, CI + fixture coverage reasonable for merge.
The PR waits on the author + CI. Full report stays in the draft file; nothing else published.

## 2026-10-08 Portal Dev, Hermes (re-profile window RAN: impl-1057 + impl-286 merged into portal, priced against the Oct-8 baseline)

Track: Portal. Nothing pushed. Both accepted branches merged into `portal` in the main checkout:
7360b936 (phase2/failed-cell-retry b2f2bea6, impl-1057) and 36d4fff3 (impl-286
portal-prestream-prewarm 6925dd4c); both cut from c9f2ffa7, `--no-ff`, messages name the approvals.
Merged-head gates: fmt clean; gated clippy `-p engine --all-targets --all-features -D warnings`
clean; gated nextest `-p engine --no-fail-fast` **1138 passed / 4 skipped** (includes 1057's four
acceptance tests; the interior-crossing streaming fixture passed in 25.6 s).

- **Re-profile window 09:17-09:34** (hold `timing` set with `--drain`, GPU guard checked free,
  cleared after): `phase2-baseline.ps1 -Scenarios portal -Repeats 3 -DurationSeconds 60
  -PortalTourSeconds 40`, portal-rooms data (unchanged), release 36d4fff3 in
  portal-release-target, `-CompareTo` the Oct-8 scenes-3 baseline **ACCEPTED** (same
  machine/resolution/schema/inputs; only the commit differs). Raw
  `D:/Mudcrab/evidence/reprofile-portal-2026-10-08/scenes-1` (OWNER.txt class B);
  results `local/reference/reprofile-2026-10-08/RESULTS.md`; bench
  `local/bench/portal-bench-2026-10-08_09-34-47`.
- **Numbers:** tour 81.9 -> 87.7 fps (+7%), lod-flythrough streaming 70.0 -> 73.3 (+4.75%), standing
  84.2 -> 88.2; portal-heavy 60.2 -> 60.4, lod-far 66.9 -> 67.3, ring-off 122.7 -> 123.2 (flat).
  Every compared metric pass (no regression). 160 bar: 0/3 runs meet it anywhere (unchanged);
  over-budget frames still owned by the render thread in every scene. Bench: closed 5.84 -> 5.61,
  open states 7.90-8.42 -> 7.21-7.50 render ms; GPU 0.81-1.08.
- **impl-285 attribution payoff:** the two worst tour frames (1256: 546.5 ms, 2891: 342.4 ms) carry
  ~3-5 ms TOTAL in the spanned main-world phases — the crossing spike lives outside the spanned
  CPU systems (render thread / pipeline prep / uploads on first interior sight).
- **Next:** worker to name the owner of the crossing frames (render-thread timings for 1256/2891);
  portal-heavy steady state (~60 fps, p50 16 ms, 100% over the 6.25 ms budget, render thread) is
  the second front. Phase2-dev note: your impl-1057 LOG correction was in-flight uncommitted during
  the merges — I carried it through the impl-286 conflict resolution and committed it inside
  36d4fff3, content unchanged.

## 2026-10-08 Portal Dev, Hermes (impl-286 Stage B reviewed by the lead)

Track: Portal. Nothing pushed. Lead review of the Stage B commits below (full diff read:
`portal.rs` +718, `transition.rs` +38 tests-only, `demo_tour.rs` +36, handoff entries). Verified
in the worktree rather than taken on report: the reveal reuses the portal's own
destination-set → `isolate_cells` stamping path (no second stamping mechanism to drift); the
warm view is render-only — `apply_collider_role` keeps `ColliderDisabled` for `Destination` and
`Hidden` alike, so nothing physical wakes behind the closed door during warm frames; restoration
is a per-frame take-back of exactly the published keys (never the portal's own hold), which the
`the_prewarm_reveals_the_destination_cell_and_takes_it_back_without_a_crossing` test pins both
ways; streaming.rs diff is empty. Fixed here: the Stage A report was cited by a name never
filed (`t286-…` → `portal-prestream-prewarm-mechanism-2026-10-08.md`) in the `PortalPrewarm` doc
and the handoff entry. Known review points for Upstream Dev: the window is read from
`PrestreamCells` directly (includes predicted beyond-radius doors, not radius-only); the warm-up
is readiness-bounded (cache settle, 2 flat frames, nothing waiting) and the tour's wait is held
adaptively with a 20 s cap; cells are revealed only on warm frames; one door at a time (nearest).
Reviewer's own gate run below.

## 2026-10-08 Portal Dev, Hermes (impl-286 Stage B: prestream pre-warm implemented)

Track: Portal. Nothing pushed. Branch `impl-286-portal-prestream-prewarm` commits d51773f0,
d79833b1, 90155163, worktree `D:/Mudcrab/worktrees/portal-impl-286`, on the portal lineage
(c9f2ffa7, includes impl-1058's spawn-time layer stamps). Implements Stage A's recommendation
(`local/research/portal-prestream-prewarm-mechanism-2026-10-08.md`): the pre-warm is the existing
portal camera activated behind the prestream window.

- **Mechanism.** `portal.rs` gains `PortalPrewarm` (resource, inited with the portal's own
  build) and `prewarm_destination_view`, chained in `PortalFrame` after `update_portal` and
  before the occlusion test/resize/leaf/isolation systems. Per frame: a doorway `update_portal`
  is really drawing marks its destination warmed; otherwise the nearest active-space door whose
  destination `plan_door_prestream` is streaming and not yet warmed becomes the target. While
  that destination's cells are still streaming the phase is `WaitingLoad` (the wait holds); once
  the landing cell is resident the phase is `Warming`: the portal camera is placed exactly where
  the doorway render places it (door_map pose, destination clip plane, main projection, full
  view, `sub_camera_view` cleared) and activated, spending the wait's frames on the 44-pipeline
  specialization burst and the destination's paced uploads. Deactivation is readiness-gated:
  `PipelineCounts` (the shared render-world bridge) read each frame; the view goes off the frame
  the created count has held flat `PREWARM_SETTLE_FRAMES` (2) with an empty waiting set. Warmed
  keys are remembered run-wide (a doorway the portal drew counts too), so re-approaches and
  windows that persist re-warm nothing.
- **Adaptive wait.** `demo_tour.rs` `Phase::Prestream` keeps its 6 s floor but holds past it
  while `PortalPrewarm::settled()` is false (destination still streaming in, or warm view
  running), capped at `PRESTREAM_WAIT_MAX_SECONDS` (20 s) like `Phase::Settle`'s ceiling; one
  log line names what held the wait when it ran >1 s past the floor. Walked runs need no timer:
  their window is `plan_door_prestream`'s own, and the warm view simply runs in it.
- **Foundations check.** No shortcuts: nothing loads faster or hides loading - the wait already
  exists and the work moves into it; `RenderAssetBytesPerFrame`/`upload_budget` untouched
  (tours run it unpaced today; noted, unchanged here). No scene/route knowledge: doors, cells
  and keys only, any worldspace. Destination sun off during warm-up (illuminance 0 while
  `update_destination_atmosphere` has no open doorway; no shadow cascades warmed); the warm view
  costs ~+1.6-2.5 ms render CPU per frame while the wait is open, named in the code. No
  `streaming.rs` touches; no timing run (AFTER numbers come from the booked re-profile window
  with impl-1058 and impl-285).
- **Tests** (headless, no GPU): `the_prewarm_draws_the_destination_behind_the_prestream_wait`
  (warm view runs behind the wait while the door is closed, publishes no doorway, deactivates on
  readiness, warmed destination never re-warmed); `the_prewarm_holds_the_prestream_wait_while_
  the_destination_loads` (WaitingLoad holds with an empty StreamingWorld, then warms when loads
  land); `a_run_without_a_prestream_window_is_never_warmed` (empty window and no-portal camera
  both idle, camera untouched). Three more were recovered from the worker's leftover working
  copy by the lead and committed as 5f3717a6: `the_prewarm_reveals_the_destination_cell_and_
  takes_it_back_without_a_crossing` (while the warm view runs the cell is stamped
  Destination/layer 2/visible - the crossing's own stamps - and every write is taken back when
  the wait ends without a crossing); `a_doorway_the_portal_is_drawing_takes_the_warm_state_down`
  (a really-drawn doorway stands the warm view down, clears the viewport, keeps the portal's
  own activation); `a_run_without_the_portal_prestreams_but_never_warms` (transition.rs:
  TransitionPlugin alone never gains PortalPrewarm). Existing transition/portal/tour tests
  unchanged and green.
- **Gated** (own build folder): fmt clean; gated clippy --workspace --all-targets --all-features
  -D warnings clean; gated nextest -p engine --all-targets 1131/1131 on the worker's four
  commits; the lead's watched re-run on the recovered tree (5f3717a6) in the same folder: fmt
  clean, gated clippy clean, gated nextest -p engine --all-targets **1134 passed / 4 skipped**.
- **Incident:** one stray `Viewport` import from outside this branch's work arrived in the
  working copy mid-run and was swept into d51773f0; removed in d79833b1. The lead saw the
  second occurrence: a botched re-indent of a transition test appeared in the working copy
  between two reads and was cleaned up again minutes later (stable since 05:22, sha-verified).
  Check concurrent writers before trusting a shared worktree's diff; no other session claimed
  this worktree.
- **Next:** hand to Upstream Dev for review; then the booked re-profile window prices impl-285 +
  impl-1058 + this together on the door-tour baseline.

## 2026-10-08 Phase 2 Dev, Hermes (impl-1057 ported onto the detail-tiered streamer: failed-cell retry with backoff)

Track: Phase 2. Nothing pushed. Branch `phase2/failed-cell-retry` commit **b2f2bea6** (from
portal c9f2ffa7), worktree `C:/d/Mudcrab/worktrees/p2-failed-cell-retry` (OWNER.txt; note the
`C:/d` path is an MSYS mangling slip - move/remove needs the user's yes), streaming.rs only,
+958/-95. Original GLM worker diff (detached at old base 6f583cd9) was partly superseded - portal
since landed the `full_failed` terrain rescue and the interior retry budget - so the leftover
apply was stashed (patch kept in the phase2-dev profile scratch) and the remaining gaps were
rebuilt per the revised brief `IMPL-1057-PORT-BRIEF.md` in the worktree, by a gated native
subagent, then re-verified by this session.

- `CellStatus::Failed` = `{ detail, attempts, reason, retry_at: Option<Instant>, replaced }`;
  `retry_at: None` = given up, never asked again while the entry lives; fresh entry on return
  (plan retain drops it, as before).
- `full_failed`: `HashSet<CellKey>` -> `HashMap<CellKey, FullFailure>` `{ attempts, reason,
  retry_at }` with `retryable(now)`; plan_cells re-asks Full once the entry is retryable
  (wall-clock backoff 1 s doubling to 2 s, `MAX_CELL_LOAD_ATTEMPTS` = 3), keeping the terrain
  rescue root drawn as `replaced` the whole time (no ground hole; the tier-swap machinery takes
  the root over); downgrade-to-terrain stays for pending/given-up entries.
- `record_failed_cell` (shared exteriors/interiors via `next_full_attempt`): one `warn!` per
  failure (key, cell id, attempt, reason); deterministic (LAND strict-validation at Full) or
  budget-spent goes straight to given up with one `error!` + `failed_cells_given_up`; else backoff.
  Ring/Terrain branches untouched (still empty-space accounting). Metrics: `failed_cell_retries`,
  `failed_cells_given_up` added beside `failed_cells`.
- Interior retry: budget unchanged (`INTERIOR_FAILED_RETRIES`), now additionally gated on the same
  wall-clock backoff; a given-up interior is never re-asked. NOTE: interiors effectively cap at
  `MAX_CELL_LOAD_ATTEMPTS` (3) via the shared give-up rule - one load TIGHTER than the old
  budget (old was 1 + 3 re-asks = 4; conservative direction, per Upstream Dev's impl-1057
  review), flag to review whether interiors should keep a separate give-up rule.
- Leak fix riding along: `unload_all_cells_now` and `validate_streaming_lifecycle` now account for
  `Failed { replaced }` roots (despawn on space change / counted as plan-accounted, not orphan).
- Deviation recorded: collider-build failure was NOT converted - at c9f2ffa7 it no longer fails
  the cell at all (`spawn_cell` draws the quadrant without a collider and commits), so there is no
  site; converting it would change behaviour the brief said to keep.
- Gated (own build folder `C:/Modding/Mudcrab/builds/p2-failed-cell-retry`, OWNER.txt): fmt clean;
  gated clippy `-p engine --all-targets --all-features -D warnings` clean (worker x5 + this
  session fresh); gated nextest `-p engine --no-fail-fast -E "test(streaming)"` **122/122** and
  full `-p engine --no-fail-fast` **1132 passed / 4 skipped** (worker) - this session re-ran
  streaming **122/122** fresh on the commit. Four acceptance tests (a)-(d) adapted to the tiered
  architecture plus the existing suite passing unchanged.
- Next: hand to Upstream Dev for review against `IMPL-1057-PORT-BRIEF.md`. When the user is
  awake: move/remove the `C:/d/...` worktree (needs consent), and the booked re-profile window
  still waits for impl-1058 + prestream pre-warm + impl-285 together.

## 2026-10-08 Upstream Dev, Hermes (night: PR #154 re-review done, verdict ready, nothing posted)

Track: upstream. Nothing pushed, nothing posted — the re-review reply waits for the user's yes.

- **Sweep (04:18Z):** all our open PRs (121-128) have every review thread answered as of
  10-06; PRs 154/163/161/166/177/190-192 (external + CI PRs) have no comment newer than the
  last activity; issue #112's open maintainer comment is answered by PR #128. Nothing to
  fix-push; we wait on the maintainer.
- **PR #154 (MonoPT, MO2-Support) re-review DONE.** The author pushed fixes for our review
  (0efdcf9a1/95a1c26dc, agent commit 6d510153c, two main merges) and asked for re-review
  10-06 16:39Z. Full report: `local/pr-drafts/pr154-rereview-20261008.md` (read-only worker;
  checklist = `local/pr-drafts/pr154-mono-review-20261006.md`). Verdict: **all 11 claimed
  fixes verified in code with file:line evidence** (spot-verified myself: identity-only
  configuration hash at cache.rs:275-305, staged-pack `finalize_world_database_with_sources`
  at repair.rs:557-568, `selected_profile` default at mo2_settings.rs:29-42); CodeRabbit
  commit weakens no checks; the two #105 merges resolve cleanly (BSTriShape conflict
  test-guarded). One regression: the agent commit stripped the 209→451 MB / snapshot
  explanation from `README.md` (numbers survive in the PR body). One-PR-instead-of-four:
  accepted, the pieces are now dependency-interleaved.
- **Ready to post (needs the user's yes):** AI-labelled reply comment on #154 — verdict +
  two non-blocking asks (restore README figures; log the effective profile on fallback) +
  confirmation ask on cross-volume hard-link staging. Draft is the last section of the
  re-review report. Merge recommendation after those: fine.
- Next for me: nothing else pending on GitHub; awaiting Phase 2's door-activation-contract
  branch and the #1055 render-prepared design for review; PR #177 merge stays the user's.

## 2026-10-08 Phase 2 Dev, Hermes (impl-1058 recovered and landed: pre-stamp streamed cells' render layers)

Track: Phase 2. Nothing pushed. The impl-1058 worker dispatched at 02:47 was interrupted
mid-run (no subagent was left alive), but its acceptance sequence had kept going at the gate:
gated `cargo check` -> clippy -> nextest all ran in its build folder
`C:/Modding/Mudcrab/builds/p2-propagate-amortise` (OWNER.txt) before it died uncommitted.

- **Recovered.** Read the full uncommitted diff (~500 lines, portal.rs + streaming.rs): the
  spawn-stamp mechanism per the brief was complete, the parity/first-frame/one-frame/no-portal
  test set complete. One failure left: the worker's own new streaming test
  `a_committed_cell_is_born_on_the_layers_its_role_asks` asserted a concrete `RenderLayers` on
  the cell root - but a source root carries `Propagate` + `Inherited` only; propagation never
  writes a source's concrete layers (bevy propagate.rs relations system). The worker had left a
  debug `eprintln!` mid-investigation. Fixed the test (root exempt from the concrete assert,
  `plain` count adjusted), removed the debug print.
- **Gated** (own build folder, fresh runs by this session): fmt clean; gated clippy --workspace
  --all-targets --all-features -D warnings clean; gated nextest -p engine --all-targets
  **1128 passed / 4 skipped** (the known-flaky fixture test passed).
- **Committed 8fafcd5b** on `phase2/propagate-amortise` (from main 99d8a737), conventional
  commit style, worktree `D:/Mudcrab/worktrees/p2-propagate-amortise`. Timing note
  `local/research/t1058-propagate-amortise-2026-10-08.md` (INDEX row added): before numbers
  30.9/37.7/25.6 ms (portal/propagate, one frame before crossings); AFTER measurement waits for
  the booked re-profile window together with prestream pre-warm.
- **Next:** hand to Upstream Dev for review; the booked re-profile window measures impl-1058 +
  prestream pre-warm + impl-285 instrumentation on the same baseline. My other parked item: the
  GLM impl-1057 failed-cell-retry worktree (`local/impl/impl-1057-p2-failed-cell-retry`,
  streaming.rs +571/-20 uncommitted) is still unresumed.

## 2026-10-08 Portal Dev, Hermes (impl-285 review follow-up: wall_cycle_ms/real_ms columns)

Track: Portal. Nothing pushed. Upstream Dev reviewed impl-285 (9cb1fc80): faithful, one
recorded deviation — the brief-update's `wall_cycle_ms`/`real_ms` were not separate CSV
columns — acceptable, and left them as an optional same-branch add before the re-profile.
Done: branch `impl-285-portal-crossing-frame-attribution` commit **0e8642a2**, worktree
`D:\Mudcrab\worktrees\portal-impl-285`, on top of the cherry-picked brief-update
(bf598688 = portal's 6f467d53), so the branch carries the brief it now satisfies.

- `CycleSides` gains `wall_ms` and `real_ms`, both closed by the next cycle's `start_main`:
  `wall_ms` is a main-thread `Instant` lap `First` → next `First` (its own lap, not a sum of
  sides); `real_ms` is the `Time<Real>` delta that `First` read. `start_main_world` now reads
  `Time<Real>` and is ordered after `TimeSystems`, so the column is the same reading
  `frame-series.csv` and the frame ledger read that frame — the files join clock for clock.
- `render-cycles.csv` header is now `cycle,wall_cycle_ms,real_ms,main_world,extract,
  wait_for_render_thread,frame/remainder,render_thread,<seven phase columns>` (row `n` ↔
  frame `n`, n → n+1 render offset unchanged). The run's last cycle keeps neither clock.
- Identity test `the_cycle_sides_account_for_the_whole_cycle` asserts
  `main_world + extract + wait + frame/remainder ≈ wall_cycle_ms` against the row's own lap,
  plus wall lap ≈ main + raw window; the CSV header test names the new columns.
- Gated (`BUILD_GATE_OWNER=portal`): fmt clean, clippy `-p engine --all-targets
  --all-features -- -D warnings` clean, nextest `-p engine --all-targets` 1131 passed / 4
  skipped. Measurement only; no behaviour change (the one ordering change,
  `start_main_world` after `TimeSystems`, is measurement alignment).
- Next: unchanged — prestream pre-warm + impl-1058 land, then the booked re-profile; the CSV
  now reads wall time and clock skew directly.

## 2026-10-08 Portal Dev, Hermes (impl-285 implemented: crossing-frame attribution)

Track: Portal. Nothing pushed. Job: Upstream Dev's brief
`tasks/deepseek/impl-285-portal-crossing-frame-attribution.md` (84fd7f29), handed to this
session in the group chat. Implemented directly (not delegated): bounded, two owned files.

- **Done.** Branch `impl-285-portal-crossing-frame-attribution`, commit **9cb1fc80**, worktree
  `D:\Mudcrab\worktrees\portal-impl-285` (OWNER.txt), cut from portal (84fd7f29). Measurement
  only; no behaviour change.
- **`frame/remainder`** (`render_timing.rs:69` const, window in `end_main`:480 /
  `start_main`:465, `remainder_ms`:635): an `Instant` lap opened at the end of `Last`, closed
  at the next `First`, reported **minus the extract and the wait when both ran inside the
  window** (they are accumulated while the window is open, in `extracted`/`handed_over`), never
  negative. Written into `CycleSides.remainder_ms` and sampled as `frame/remainder` while
  recording. Gated on `track_cycles` (benchmark runs); zero cost otherwise.
- **Per-cycle render phases** (`CycleSides`: the six new phase fields; `lap`:510 writes each
  phase into the cycle the render frame belongs to via `phase_writer`:641): the seven phases are
  one value per cycle each. `render-cycles.csv` (`profiling.rs:531 render_cycles_csv`, written
  in `write_bundle`:830 beside `frame-series.csv`, bounded `MAX_RENDER_CYCLE_ROWS` = 100 000),
  header, in order:
  `cycle,main_world,extract,wait_for_render_thread,frame/remainder,render_thread,render/extract_commands_and_assets,render/specialize_and_views,render/queue,render/prepare,render/graph_and_present,render/cleanup,render/swapchain_acquire`
  **Join:** `render-cycles.csv` row `n` ↔ `frame-series.csv` frame `n` (same number). Row `n`'s
  `render_thread`/phases are the render schedule that ran *during* cycle `n` — fed by extract
  `n−1` when pipelined (the n → n+1 offset) — so a crossing frame's render time is explained by
  its own row's phases. `frame-series.csv` is byte-for-byte unchanged.
- **Row plumbing:** `RenderTimings` mirrors each cycle write into `cycle_rows` (retention keeps
  only cycles that can still be written); `take_completed_cycles`:387 hands over completed rows
  once (watermark so a late write cannot duplicate); a new `drain_render_cycles` system
  (`profiling.rs:261`, installed only when the run profiles, ordered after
  `start_main_world`, which became `pub(crate)`) moves them into `ProfilingState`.
  The run's last cycle or two have no row: their remainder window never closed.
- **Tests** (all new, `render_timing.rs` unless noted): `the_remainder_excludes_the_extract_and_wait_that_ran_inside_the_window` (pure),
  `the_cycle_remainder_excludes_the_extract_and_wait_that_ran_in_its_window` (record),
  `the_cycle_sides_account_for_the_whole_cycle` (identity, sleep-based),
  `the_remainder_is_kept_only_when_cycles_are_tracked`, `laps_write_their_phase_into_the_cycle_record` (both pipelined settings),
  `completed_cycle_rows_are_handed_over_once_once_they_are_complete`,
  `the_render_cycles_csv_names_every_column` and
  `the_render_cycles_series_keeps_its_most_recent_rows` (`profiling.rs`);
  `writes_complete_profile_bundle` now also asserts `render-cycles.csv`.
- **Checks** (own build folder `C:/Modding/Mudcrab/builds/portal-impl-285-target`, OWNER.txt):
  `cargo fmt --all -- --check` clean; gated `cargo clippy -p engine --all-targets
  --all-features -- -D warnings` clean; gated `cargo nextest run -p engine --all-targets`
  **1131 passed / 4 skipped**.
- **Deviations from the brief:** `write_cycle`'s writer changed `FnOnce`→`Fn` (it now writes two
  records); the rows drain runs in `First` after the main frame's start (a row is complete only
  once the next `First` closed its remainder) instead of inside the ledger's take — the ledger's
  records are untouched; the remainder is sampled into `cpu-spans.json` as `frame/remainder` on
  top of the CSV column (distribution for free, same gate).
- **Foundations check:** no shortcuts (measurement only, nothing hidden, no behaviour change);
  foundationally optimised (zero cost when not tracking/profiling, bounded stores); flexible
  (no worldspace/scene assumptions — the window is measured for any route); moddable (nothing
  vanilla-specific); aligned (the 160 fps goal's measurement prerequisite — the ~80 ms crossing
  remainder becomes attributable, and the per-cycle phases make the pipeline-activity frames
  comparable, which will price the prestream pre-warm's buy-back). Shortcut question: the only
  thing this takes short is the run's final row (window never closes) — documented, and it
  cannot hide a crossing (crossings are mid-run).
- Nothing running; no timing run (the verifying re-profile is a booked window after this lands
  together with the prestream pre-warm).

## 2026-10-08 Portal Dev, Hermes (impl-190 merged into portal)

Track: Portal. Nothing pushed. Job: the user's "keep working on 160 fps in portal demo,
organise the work into prepped drafts/PRs". Took Upstream Dev's prepped impl-190 draft (its
message in the group chat): the merge toward portal is Portal's to do.

- **Merged `impl-190-portal-ring-off-with-lod` (e00f7100, cut from 0fda5620) into `portal`
  with `--no-ff`.** Auto-merge clean, no conflicts (app.rs auto-merged over the water-merge
  content; the water tests still pass). Merge commit **ad1c60e2**.
- **Checks** in `C:/Modding/Mudcrab/builds/portal-hermes-target` (this profile's own folder,
  OWNER.txt reused, same branch as its last build): `cargo fmt --all -- --check` clean; gated
  `cargo clippy -p engine --all-targets --all-features -- -D warnings` clean (23.8 s); gated
  `cargo nextest run -p engine --all-targets` **1123 passed / 4 skipped** (24.9 s, includes the
  streaming-fixture interior-crossing test).
- Foundations check: no shortcuts — the ring turns off only where a worldspace has a LOD
  manifest (data-driven, matches vanilla), never a draw-distance cap; right layer (streaming
  plan reads the world DB per frame, the documented deviation); flexible/moddable — any
  worldspace with LOD benefits, others unchanged; aligned — ring-off is the exact
  lod-far-ring-off scene from the crossing-frames note (p50 8.22 ms vs 13.71 with the ring),
  the biggest steady-state lever toward 160 fps. Caveat carried from the timing note: the
  13.71/8.22 pair was not a clean A/B (data changed too).
- Next: implement fix #2 from `local/research/portal-crossing-frames-895-2666-2026-10-08.md`
  (extend portal/prestream to pre-warm interior pipeline specialisation during approach) as a
  prepped portal draft; its measurement waits on the #1 instrumentation brief (Upstream Dev).

## 2026-10-08 Hermes trial (water merge into portal)

Track: Portal. Nothing pushed. Job: the user's yes in the Hermes session ("start on water merge"); the morning item from Portal Dev's handoff aa395beb.

- **Water merge done.** `git merge --no-ff 8acbdb7c` on `portal`: rerere replayed both recorded conflicts (app.rs, render.rs) from the saved resolution; verified the merged result against `local/plans/portal-water-merge-resolution-2026-10-08.patch` by reading every hunk: app.rs fixture carries `.with_pinned_clock(1.0)`; render.rs has portal's layer block (moved lower) + the branch's TEMPORARY 12 h wrap + both embedded shaders + shared-clock system list + all four water tests (`advancing_time_leaves_water_materials_unmodified`, `a_still_view_does_not_rewrite_the_reflection_camera`, `water_settings_fields_match_the_shader_struct`, `only_a_pinned_water_clock_reads_the_material_time`); water.wgsl reads `globals.time` with the pinned-clock select; physics.rs / coordinates.rs / CI yaml / SPEC.md / dummy-content perf test from the branch. Merge commit **77bb484b** on `portal`. No `animate_water_materials` remains.
- **Checks, in a fresh build folder** `C:/Modding/Mudcrab/builds/portal-hermes-target` (OWNER.txt written, class A, this session's own — never shared): gated `cargo clippy -p engine --all-targets --all-features` clean (53 s); gated `cargo nextest run -p engine --all-targets` **1121/1121 passed, 4 skipped** (includes the streaming-fixture test the handoff log called flaky — passed in 25 s on a quiet machine). fmt check not run (merge applied recorded resolutions, no hand edits).
- Foundations check: no shortcuts (the 12 h wrap stays TEMPORARY-labelled with its follow-up, as agreed); right layer (shared clock removes a per-frame material rewrite); data-driven; no test-data special cases added by this merge.
- Next: the crossing-frame analysis (door tour frames 895/2666, ~320-360 ms) and the 37.7 ms `portal/propagate` peak; impl-190 resume; then the user plays the merged portal before anything public.

## 2026-10-08 Portal Dev (timing window after the isolation rewrite; stopped at wrap-up)

Track: Portal. Nothing pushed. Jobs from Upstream Dev (the user's plan for 10-08) and the user's own "match vanilla" decision (10-07 night).

- **Release build** of portal through the gate (C:/Modding/Mudcrab/builds/portal-release-target, full rebuild after the folder move, 7 min).
- **Timing window** 00:52-01:12, hold "timing" set and cleared, announced to all sessions. 5 portal scenes x3 + portal bench on
  portal-rooms. Results: `local/reference/baseline-2026-10-08/RESULTS.md` (raw D:/Mudcrab/evidence/baseline-portal-2026-10-08, OWNER.txt, class B).
  Isolation rewrite works (portal/isolate max 0.4 ms); door tour p99 49.9 -> 42.4 ms, p99.9 75.9 -> 67.7 ms; but the two crossing frames
  still take 318-358 ms (now real time, no longer capped at 250), with only ~2 ms in spanned main-world systems. portal/propagate peaks at
  37.7 ms once. Render thread owns the over-budget frames everywhere. Not a clean A/B: the data changed too.
  Two false starts: a pre-hold cargo check overlapped (restarted; the gate now has `hold set ... --drain`), and `-CompareTo` refuses a
  baseline on different assets (dropped; compared by hand).
- **portal-bench.ps1** default assets -> portal-rooms (911210b3).
- **Water merge 8acbdb7c: NOT done** (Upstream Dev: morning item). Conflicts were resolved, recorded with `git rerere`, then the merge
  was aborted; a re-run of `git merge --no-ff 8acbdb7c` re-applies them. Copy of the resolution:
  `local/plans/portal-water-merge-resolution-2026-10-08.patch`. Choices: app.rs test conflict = portal's side (main's #172 legacy-schema
  tests use main's schema numbers); render.rs = portal's layer block (moved lower in portal) + the water branch's TEMPORARY 12 h wrap
  period + both embedded shaders. Then build, clippy, nextest engine, commit.
- **impl-190 (terrain ring off where the worldspace has LOD, "match vanilla")**: DeepSeek coder hit max turns (121) in 8 min with an
  unbuilt, unreviewed change in 5 files (app, atmosphere, config, streaming, world/database; +379/-69) in
  `local/impl/impl-190-portal-ring-off-with-lod` (session e7d3a720-6e04-4728-80ac-f645aab2893d). Its gated acceptance was stopped
  (no runs while the user is away). Next: resume it with a narrow "finish and report" or split the brief (database check + resolver first,
  then the readers), then review and accept.
- Nothing of mine is running: no worker, build, engine, timing run.

Next (morning): water merge + checks; impl-190 resume/split + review; analysis worker on the 300 ms crossing frame and the 37 ms
propagate frame (door tour frame-series.csv, frames 895 and 2666).

## 2026-10-08 Phase 2 Dev (GLM 5.3 coder trial, stopped at wrap-up)

Track: Phase 2. Nothing pushed. Job from Upstream Dev (the user's yes, 2026-10-08).

- **GLM trial impl-1057, failed-cell retry** (brief `tasks/deepseek/impl-1057-p2-failed-cell-retry.md`,
  #1055 cause 3: `CellStatus::Failed` sticky at `streaming.rs:815`): GLM 5.3 coder, effort high, base
  origin/main f40ca1df, worktree `local/impl/impl-1057-p2-failed-cell-retry`. Stopped at 01:10 for the
  night after 67 steps / 30 min (GLM cost unclear: the spend status showed $1.10 this week mid-run, then $0.59 after the stop; check `ds_spend.py` tomorrow), while it waited for the gate (Portal's timing hold). Its work so
  far, `streaming.rs` +571/-20, is uncommitted in the worktree and has not been compiled or reviewed yet.
  Resume tomorrow: worker session `9ec2f73a-bc8e-4503-a4e1-15df893d64bc` (or `ds_impl.ps1 -Post` on the
  worktree to run its checks), then a DeepSeek review, then report to Upstream Dev: steps, wall time
  (the gate wait counted separately), cost, findings, fixes needed, verdict vs a DeepSeek coder.
- Still in today's plan as separate items: the door activation contract, the #1055 render "prepared"
  signal design, water-shared-clock into portal after Portal's timing window, the scenario smoke runs,
  and the terrain-merge GPU probe offset.

## 2026-10-08 (night) Phase 1 Dev: GLM coder trial started, terrain wiring written

All local, nothing published. Phase 1 task numbers continue at 1900-1999.

- **GLM trial (Upstream Dev's job, the user's yes):** GLM impl #1900, the KTX2 mip-order fix (`tasks/deepseek/impl-1900-p1-ktx2-mip-order.md`), stopped at the user's wrap-up. Worktree `local/impl/impl-1900-p1-ktx2-mip-order`: texture.rs +338/-64, uncommitted, never built. Resume with ds_impl/ds-agent `-Resume aadb9cee-afaf-41b8-9c8d-3289c942d737`, then DeepSeek review #1901 (brief ready), then cut `phase1/ktx2-mip-order`. Progress so far: `local/review/p1-glm-trial-1900-2026-10-08.md` (30 min, about 22 of them waiting at the gate for Portal's timing hold; 133 steps; at most $0.51).
- **#186 terrain wiring:** DeepSeek #896.1 committed 9b013eb7 on 20ac4831 (detached, worktree `local/impl/impl-896.1-p1-terrain-pipeline-wiring`, +1269 lines: terrain_pipeline.rs, the `--merged-terrain` and `--terrain-default-texture` options, a fixture test, key tests including the load-order shift). Written but unbuilt (gate hold). Next: run gated clippy and the terrain tests in that worktree, then rerun DeepSeek review #1902 (`tasks/deepseek/review-1902-p1-terrain-wiring.md`; stopped before it reported). Fixes I already want:
  - the metadata-rebuild route (`--reuse-assets`) ignores `--merged-terrain` silently;
  - progress reuses `ProgressStage::LodChunks`;
  - the merged record map is held through the texture stage (memory);
  - the stage may block the async runtime.
  After that: merge into `phase1/terrain-merge`, image-diff evidence, workspace checks.
- **Gate:** a gated build that took its slot 21 s before Portal's timing hold kept running into it. I killed mine. Upstream Dev added `hold set --drain`.
- **Still pending from yesterday:** the d4793105 port and the glb_bounds-skips-excluded-geometry drafts (the user's yes, batched with the schema bump); tell Upstream Dev if `SkyrimConverted-p1rooms` can go; the load bench full run.
- No worker running.

## 2026-10-08 Upstream Dev (night: folder move finished, GLM trial started, build gate fixes)

Track: all (Upstream Dev). Day logs: `local/team/2026-10-07.md`, `local/team/2026-10-08.md`.

- **Folder move DONE** (2026-10-07 21:45): main checkout `C:/Modding/Mudcrab/mudcrab`, worktrees/builds/data under
  `C:/Modding/Mudcrab` and `D:/Mudcrab`. The script stopped on a harmless git repair note; finished by hand: all 47
  worktrees resolve, paths rewritten (4759a8be; upstream test example paths kept, 0fda5620), 85 OWNER notes, memory
  copied to `C--Modding-Mudcrab-mudcrab`, providers.json Muse opt-in updated (user yes, .bak kept), RESOURCES.md line,
  `.claude/settings.local.json` cleaned to one rule (user's ask). Still open: `local\impl` as a junction to D: (coder
  worktrees still land on C:; C: 168 GB free), and the old memory key can be removed once sessions use the new one.
- **Dynamic linking trial (user ask):** no gain (36-37 s vs 37-38 s per one-line edit); not adopted. Lever: split the
  engine crate (next plan).
- **GLM 5.3 trial (user ask, provider `glm`):** Phase 1 GLM impl #1900 (KTX2 mip order) and Phase 2 GLM impl-1057
  (Failed-cell retry), each with a DeepSeek review; Phase 1's DeepSeek #896.1 (terrain wiring) is the comparison. All
  stopped at a clean point for the night (resume by session id). Report to the user when they land: steps, time, cost,
  findings, fixes, verdict vs DeepSeek.
- **Build gate:** owner detection reads the track token in worker labels (8375c62a); `hold set` lists running jobs and
  `--drain` waits for them (d387c468); AGENTS.md updated. Phase 1 numbers continue at 1900-1999 (eeb41240).
- **Portal:** timing window ran 00:49-~01:30 (portal 0fda5620 on portal-rooms); results with Portal Dev. Water merge is
  a morning item.
- **Morning:** GLM trial results; Portal's timing numbers; water merge into portal; door activation contract and
  missing-ground (render-prepared signal) designs; Phase 1's three converter drafts; PR #177 merge is the user's (ready).
- No worker of mine is running.

## 2026-10-07 Upstream Dev (evening: decisions, deletions, xEdit posts, #167 review, folder move prepared)

Track: all (Upstream Dev). Day log: `local/team/2026-10-07.md`; evening table: `local/team/evening-table-2026-10-07.md`.

- **User decisions (evening table):** 1 folder move now; 2 Upstream Dev deletes the listed old data; 3 three converter
  fixes (KTX2 mip order, NIF root transform port, marker bounds skip excluded geometry) prepared as upstream DRAFTS by
  Phase 1 (show the user before opening); 4 PR #177 re-read: only a main merge, changed lines identical to the reviewed
  commit, merges clean: ready for the user's merge (keep co-author trailers); 5 water 12 h wrap yes (TEMPORARY);
  6 launcher builds paused while the GPL question is open (Phase 3 told); 7 xEdit comments posted on #158, #185, #106;
  8 Bevy binding-array contribution later.
- **Posted today (user's yes):** review comment on Chef's PR #167; xEdit comments #158/#185/#106 (links in the day log).
- **Deleted (user's yes, links checked):** p2main-v24.assets-cache 21 GB, scratch p2-scenarios-backup, builds/p2-water-base,
  D: engine-before-water.exe, duplicate report. KEPT: D: SkyrimConverted-p2main (pr:125 open; p2main-doors junctions for
  open PR #128) and SkyrimConverted-p1rooms (Phase 1, while-open): ask the owners again.
- **Build gate** (tools/build_gate.py, runnable copy `C:/Users/Brannock/.mudcrab/build-gate/`): moved out of AppData
  (MSIX virtualization hid it), 2 slots per session, 15 GB free-disk floor. Hold `move` set until 22:00: clear it after
  the move (`... hold clear move`).
- **Folder move: NOT run yet.** Script `local/disk/move-2026-10-07.ps1` (re-runnable, logs to C:/Modding/move-2026-10-07.log,
  exit file C:/Modding/move-2026-10-07.exit); path rewrite `local/disk/rewrite_paths_1007.py` (dry run: ~3,000 paths in
  742 live files). The user closes every Mudcrab session, runs the script in a standalone PowerShell, then reopens the
  sessions in `C:/Modding/Mudcrab/mudcrab`. The script copies the Claude memory to the new key. `.cargo` stays at
  `C:/Modding` (still an ancestor). Not touched (user's own, needs their yes): ~/.claude/settings.json,
  .claude/settings.local.json, ~/.claude-deepseek/providers.json (Muse opt-in path), the backup scheduled task, and our
  line in D:/VFXEnhance/coordination/RESOURCES.md. After reopening: commit the rewritten tracked files, check
  `git worktree list` (no prunable), clear the `move` hold, tell every session and the DAO/VFX sessions.
- **Tomorrow's plan items:** door activation contract (Phase 2 + Portal); missing-ground fix design (render-to-main
  prepared signal, Chef's LOD area: announce first); Portal's timing window, then water merge into portal;
  terrain-merge wiring (DeepSeek, target on D:); coder worktrees on D: (local/impl junction after the move);
  world-DB stable IDs; rolled rows in the evening table file.
- No worker of mine is running.

## 2026-10-07 Phase 2 Dev (doors scene, water clock, terrain merge engine, scenario runner, terrain holes)

Track: Phase 2. All local; nothing pushed. Hand-offs to Upstream Dev, Portal Dev and `local/team/2026-10-07.md`.

- **Doors scene** `phase2/perf-doors` f3daf559 (worktree C:/Modding/wkj-p2-doors): route door steps cross
  through the player's activation (#128), per-crossing windows, aim at the door's bounds centre or the
  activation's fallback box. fmt/clippy green, route 27/27, workspace 1223/1224 (the app streaming-fixture
  test flakes under load, pre-existing). Smoke on `C:/Modding/SkyrimConverted-p2main-v24-doors`: 10/10
  crossings. Portal declined it tonight (portal crosses with its own doors.rs): tomorrow's door activation
  contract (shared request + crossing start/end events in the engine crate, one path for player, routes,
  scenarios, console, Luau).
- **Water shared clock** `phase2/water-shared-clock` 8acbdb7c (C:/Modding/worktrees/p2-water): water reads
  `globals.time`, no per-frame material rewrite; reviews DeepSeek + Muse #1041 caught a uniform field-order
  bug (fixed, layout test added). All checks green (workspace 1088/1088). Parity: identical to main whenever
  terrain draws. Approved for portal by Upstream Dev; merges tomorrow AFTER Portal's timing window. The 12 h
  `Time<Virtual>` wrap is TEMPORARY, pending the user's evening-table answer.
- **Terrain holes (#1055, user's yes):** analysis `local/research/engine-foundations/analysis-1055-terrain-missing-cells.md`:
  LOD tiles hide on the CPU "terrain ready" flag while `RenderAssetBytesPerFrame` still defers the GPU
  prepare, so neither draws; `--shots` settle ignores LOD and GPU prepare; `CellStatus::Failed` is sticky.
  Design step tomorrow (Chef's LOD area: owner check and announce first; general render-to-main "prepared"
  signal; websearch-1056 checks Bevy 0.20 for one).
- **Terrain merge engine half** `phase2/terrain-merge` (C:/Modding/worktrees/p2-terrain): reviews DeepSeek +
  Muse #1042, 7 fixes in (weld fallback, setup stall/timeout, back faces, RENDER_WORLD arrays, real KTX2
  colour-space check, no data asserts, documented differences); Phase 1's writers merged (880363fa). Head
  0d1d3bc3: clippy, workspace nextest 1173/1173, doctests and the converter-chunk test green; the GPU probe
  runs but FAILS (blend quadrants brighter than reference, a whitish offset up to 190/255; the back-face case
  passes), so the shader is not device-verified. Tomorrow: rerun clippy and engine tests for the naga_oil
  default-features change, then find the probe's offset.
- **Scenario runner v1** `phase2/scenarios` (C:/Modding/worktrees/p2-scenarios; spec
  `docs/specs/engine/scenarios.md`, posted on #193): runner drives the real controller; reviews #1043
  (DeepSeek accept, Muse fix first: input gate) fixed; head 878fa1f5, workspace 1275/1276 (its own test, fixed;
  scenario+door 75/75). Not yet run on real data; smoke and re-picks tomorrow. Bevy has no render-to-main
  "prepared" signal (websearch-1056: in 0.20 no), so #1055 needs our own ExtractSchedule bridge.
- **Disk:** C: filled to 0.5 GB at 18:46 (other sessions' coder worktrees in `local/impl`). Deleted, with the
  user's yes: `C:/Modding/wkj-p2-routes-target` (20 GB). `SkyrimConverted-p2main-v24.assets-cache` (21 GB)
  kept, on the can-go list. Owner notes for converted data now sit beside the folder.
- **Next:** door contract and #1055 design (morning plan), water into portal after the timing window,
  scenario smoke runs, terrain checks plus identity shots once Phase 1's pipeline writes real terrain data.

## 2026-10-07 Portal Dev (portal baseline, portal-rooms data, foundations merged, isolation rewrite)

Track: Portal. All local; nothing pushed. Hand-offs to Upstream Dev and `local/team/2026-10-07.md`.

- **Portal "before" baseline** (#184 demo): 5 portal scenes, release, 11:12-11:31. Results and the
  first 160 fps gap list: `local/reference/baseline-2026-10-07/RESULTS.md`. The render-thread CPU owns
  every frame (the GPU is nearly idle); the terrain ring doubles the render thread (lod-far vs
  lod-far-ring-off). Door tour: entering an interior costs one frame of about 340 ms (the CSV capped it
  at 250, a Time<Virtual> cap; Phase 2 fixed it, 36a09ba8), and leaving is followed by a 34-39 frame
  burst. Code traces: DeepSeek #640, #641, #642.
- **New portal data** `C:\Modding\SkyrimConverted-portal-rooms` (owner note beside it): a full
  reconversion with the room map (b7488c1c, XPOD) and fresh BC7 textures. 15 UASTC left (from 19,605);
  room_graph matches p1rooms. It was resumed once (the console was closed at 15:47; --resume-staging
  worked). The load check passed. AGENTS.md now points at it. Old portal-lod and C:/Modding/SkyrimConverted
  plus the D: copies were deleted with the user's yes (logged).
- **portal** now at 4eb2407d (merge of `portal-rooms`, worktree `D:/Modding-overflow/wt-portal-main`):
  room-culling #188, room-graph, perf-routes (route runner, frame budget), the world-ready fix
  (adapted to portal's wanted_cells), portal spans (#644), the per-frame series `frame-series.csv`
  (#645), isolate_cells via Bevy RenderLayers propagation (#649, review #650 fixes; research #646),
  and the editor-marker gate (#651, review #652 accept). Checks: fmt and clippy clean, engine plus
  converter nextest 1778/1778.
- **Correction:** the #651 commit (0d9d2d02) claims visible FX drips were missing. That is wrong:
  FXDrips01 sets the BSX marker bit (its drips are particles). Only 17 models change (coin bags,
  cameras, motes): `local/research/editor-marker-bsx-models-2026-10-07.md`. No reconversion for it
  alone; batch it with the next portal converter change and bump to 102_402 then (review #652).
- **Play build:** `D:\Modding-overflow\OpenSkyrimProject-target\quick\engine.exe --assets
  C:\Modding\SkyrimConverted-portal-rooms --demo riverwood --walk [--room-culling]`. Release build:
  `C:/Modding/builds/portal-release-target`.
- **Deferred:** phase2/perf-doors (f3daf559). Its route door steps drive #128's door_crossing, which
  portal does not run. Direction from Upstream Dev: one "activate this door" request that portal's
  doors.rs answers. DeepSeek research first, Phase 2 reviews. Water-shared-clock is on Phase 2's side.
- **Next (morning):** the timing window on portal-rooms with the per-frame series (before vs after the
  isolation rewrite), then the door-step design research. Plan:
  `local/plans/portal-rooms-rebuild-2026-10-07.md`.
- No worker running; no coder worktrees left on C:.

## 2026-10-07 Phase 1 Dev (#188 fix, door_links, #186 terrain merge converter half)

All local; no PRs opened. Hand-offs went to Upstream Dev, Phase 2 Dev and `local/team/2026-10-07.md`.

- **#188 room graph:** `phase1/room-graph-pr` head b7488c1c (worktree `D:/Modding-overflow/wt/p1-main-ro`). The room_portals migration savepoint now closes when RELEASE fails (busy lock). It does a ROLLBACK if the savepoint opened the transaction, else ROLLBACK TO + RELEASE. A new busy-lock test fails without the fix. Workspace nextest 1090/1090 + doctests (evidence `D:/Modding-overflow/p1-evidence/p1-room-graph-workspace-b7488c1c.txt`).
- **door_links (#112 / Phase 2's crossing):** `phase1/door-links-export` 723c0c39 (worktree `D:/Modding-overflow/wt/up-doorlinks`, now Phase 1's). XTEL is read per xEdit 4.1.5f defs `wbDefinitionsTES5.pas:12344-12350`, 32 bytes only, and non-finite values drop the link. No schema bump. Reviews DeepSeek #889 and Sol #889.1; fixes in. Built `C:/Modding/SkyrimConverted-p2main-v24-doors` (metadata rebuild reusing p2main-v24): 2,204 door_links, 0 unresolved, the same as portal-rooms. Phase 2's Riverwood and Whiterun pairs check out.
- **UASTC cause found:** portal-lod reused pre-#114 textures. The fresh portal-rooms conversion has 15 UASTC files (cubemaps only).
- **#186 terrain merge, converter half:** `phase1/terrain-merge` (worktree `C:/Modding/worktrees/p1-terrain-merge`, off main f40ca1df), head 20ac4831. Format v3 is agreed with Phase 2 (`local/research/terrain-merge/format-proposal.md`). Commits:
  - shared types and paths;
  - `build_cell_mesh`;
  - land texture arrays (`terrain_arrays.rs`: full mips, UASTC to BC7, KTX2 data smallest-first, staged publish);
  - chunk files plus the #185 key (`terrain_chunks.rs`).
  Checks: clippy clean, terrain tests 42/42, converter+shared green except the known flaky `extraction_does_not_wait_for_a_front_end_that_stopped_reading`. Upstream Dev confirmed the key changes on a load-order shift (remapped FormIDs are hashed). A pinning test is due in the wiring round. World-DB-wide stable IDs are a design candidate for the next plan.
- **Not done:** the pipeline wiring, DeepSeek impl #896.1 (brief `tasks/deepseek/impl-896.1-p1-terrain-pipeline-wiring.md`). It was stopped at disk-c before writing any code. Relaunch it after the move with its target on D:. After that: image-diff evidence, then workspace nextest + doctests on the head.
- **Disk:** the three coder worktrees `local/impl/impl-895-*`, `impl-896-*` and `impl-896.1-*` (~43 GB on C:) are for the user to `git worktree remove --force` (the classifier denied mine). Their leftovers are copied to `local/archive/p1-coder-worktree-leftovers-2026-10-07`. Throwaway `D:/Modding-overflow/p1-tmp-doorlinks-target` (class A, keep-until 2026-10-09).
- **Evening table items:** a converter heartbeat line; the .opensky-srgb alias dedupe; the KTX2 mip-order fix (batched with the schema bump, `local/research/converter/ktx2-mip-order-2026-10-07.md`); porting d4793105's root transform upstream; main's glb_bounds should skip excluded geometry by exclusion data (#84); portal's name-only marker drop (Portal's).
- **Also pending:** the load bench full run (`D:/Modding-overflow/scratch/p1-load-bench/run-full.ps1`) in a quiet slot, then the collision v3 decision; the p1rooms rebuild from fresh main data.
- **After tonight's move:** CARGO_TARGET_DIR is `D:\Mudcrab\builds\p1-target`; update paths in `local/handover/phase1-dev.md` and memory. No worker running.

## 2026-10-06 Upstream Dev (engine foundations, portal catch-up, publishing)

- **Published (the user's yes each):** issues #183 (tracking) and #184-#188 (baseline, derived-data contract, static-rendering experiment, secondary views, interior room visibility); a comment on Chef's #106 offering our .lst/.btt/.bto LOD notes. Under the standing yes: CodeRabbit round-2 fix pushes #121 -> 2167650d, #124 -> 1fd11141, #125 -> 3734ee9e, bot threads answered.
- **Hand-outs:** Phase 2 Dev: #184 baseline (main: rural 195 fps, render-thread bound; scenes fixed, dense = Riverwood), #187 view policy (slices 1+2, ca9fd9ca green), #188 engine half (fdf2a9b1, off by default), interior shots on #128 (01b2a732). Phase 1 Dev: #188 converter (phase1/room-graph-pr 0f2645b0, real data 4,246 rooms), #185 key-cost probe. Portal Dev: catch-up with main (portal = main 2c11897b + portal), LOD data at 102_401/100_701, #184 + portal scenes, LOD visible at defaults (#128 merged into portal; fog to LOD horizon).
- **Queued, local (docs/pr/upstream-queue.md):** #188 converter draft (previewed; the user: not yet), #187 draft (preview next), #184 + scene set (after #122), #188 engine (after image test). #185 comment held by the user (local/pr-drafts/engine-foundations/185-comment-input-keys.md, contract 185-contract-v1.md).
- **Research filed + indexed:** #993-#997, #999 (baseline design), #1008 (Bevy 0.20 gaps: none fully fixed), analysis #1009 (CPU breakdown).
- **Rules added (AGENTS.md / memory):** PR cap 5-6 ready + 7-8 drafts; check Bevy 0.20 before building around a 0.19 gap; acceptance tests may change; name GitHub items with what they are; no unattended runs.
- **Disk:** ~220 GB freed with the user's yes (logged); throwaway C:/Modding/tmp-up-drafts-check-target (12 GB) kept, awaiting the user's yes. Parked worktree D:/Modding-overflow/wt/up-doorlinks (phase1/door-links-export) holds uncommitted WIP for #112's door_links export: keep until a decision.
- **Crash + PC restart** handled; all sessions resumed with their state.
- **Next:** morning brief from local/team/2026-10-06-evening-table.md (unanswered items 1-4 and 8, plus the roll-over list); portal "before" baseline booked for the morning; #187 draft preview.

## 2026-10-06 (evening) Portal Dev: portal caught up with main, #184 tool, #105 LOD visible

- **Portal = c175eff8..28ce63dc:** origin/main 2c11897b merged (#105 LOD, #118, #127 rename, #142, #136/#176, #170, #178; Claude #714); portal schema 102_401 / world 100_701 (LOD_CONVERTER_SCHEMA_VERSION follows); rename applied to portal's text; portal's converter `lod.rs` is now `portal_lod`. Then #184 perf-baseline 111ca6be (+#122) with 4 portal scenes (portal-door-tour, portal-heavy, lod-far, lod-flythrough; Claude #719). Then #128 386267dc (portal's doors.rs stays the door system; #128's crossing compiled, not registered) with portal's ActiveCell driving ActiveSpace so LOD stops in interiors, and the exterior fog ending at the LOD horizon when LOD data exists (ring kept) (Claude #721).
- **Data:** C:/Modding/SkyrimConverted-portal-lod (24 GB, class C) from a `--reuse-assets` metadata rebuild of D:/Modding-overflow/SkyrimConverted-portal (101_601): 34 min, LOD chunk stage 27.4 min, 4,639 chunks. Portal-only converter changes: 86265e04 (accept portal-numbered sources, config hash by main version), 5971a599 (regenerate portal tree billboards in the rebuild).
- **Tested:** door tour (--tour-doors 2) PASSED on the new data and after the LOD fix; far view at defaults shows LOD to the horizon (local/reference/lod-check/after-defaults/); log shows 0 LOD patches inside Sven's house, 32 back outside; fmt/clippy clean, nextest 1887/1887, doctests ok. Notes: local/research/portal-lod-check-2026-10-06.md, portal-lod-data-options-2026-10-06.md.
- **Decisions (the user):** no distant objects/trees on portal (Chef's #106 / tree phase; Upstream Dev offered our .bto/.lst notes on #106). Deleted (the user's yes, logged): experimental data + cache, wt-targets/experimental, wt-experimental worktree, OpenSkyrimProject-target/debug/incremental.
- **Next (tomorrow, with the user present):** switch the LOD baseline scenes to portal defaults (keep one ring-off comparison) in scripts/phase2-profile.ps1; release LTO build (disk_guard first); booked quiet window: `pwsh -File scripts/phase2-baseline.ps1 -Assets C:\Modding\SkyrimConverted-portal-lod -Scenarios portal -Repeats 3 -Out D:\Modding-overflow\baseline-portal-184` (~30 min + build), report split render thread / main world / GPU passes / per-camera cost, explain the door-tour 40 ms commit. Later: interior doorway views show no distant terrain; C:/Modding/SkyrimConverted (66 GB, old) on tomorrow's can-go list. No worker running.

## 2026-10-06 Phase 1 Dev (engine foundations, converter side: #188, #185)

All local; no PRs opened (cap). Hand-offs went to Upstream Dev and `local/team/2026-10-06.md`.

- **#188 converter half, ready to send:** `phase1/room-graph-pr` 0f2645b0 (worktree `D:/Modding-overflow/wt/p1-main-ro`, off main 2c11897b). The REFR XPOD/XLRM/LNAM/INAM remap fix stands alone (2b4dd7ca). New `esm/rooms.rs`: room_bounds / room_portals / room_links / reference_rooms (pivot rows at export, `contained = 0` overlap rows from model bounds at integration) and room_graph_stats; spec §15; no schema bump. Reviews: DeepSeek #878 + Sol #878.1, fixes a3cc47c3, delta checks #879 / #879.1 accept. Workspace checks green on d4f8381d (1,100 passed); later commits are docs only. XRMR flags confirmed against Skyrim.esm bytes (0x80 = LNAM lighting template, 0x40 = INAM image space; XLRM = rooms seen together without a portal). Waits for the user's call on the draft.
- **Real-data run:** metadata rebuild `D:/Modding-overflow/SkyrimConverted-p1rooms` (class C, owner note beside it; assets reused from p2main, no LOD). 292 interiors, 4,246 rooms, 3,097 portals; Ansilvund01 matches the earlier byte scan. Interior list: `D:/Modding-overflow/p1rooms-room-cells.tsv`. Phase 2's room-culling image test reads it.
- **#185:** design note plus a "Measured" section on `phase1/room-graph` e7328c00 (`docs/research/derived-data-input-keys.md`); probe on the throwaway local branch `phase1/input-key-probe` 864cc0df (never send). Keys cost about 1.7 s CPU for the whole game; Upstream Dev took the recommendation (key per cell; interiors one cell per unit, exteriors grouped on LOD chunks). Parked: following model textures and materials into the key.
- **Folders:** `wt/p1-keyprobe` and `p1-tmp-keyprobe-target` are class A, keep until 10-09; `p1-rooms-converter` until 10-10. Nothing running.
- **Evening (room culling cross-check for Phase 2):** p1rooms has 1,536 overlapping room pairs with no portal or link (about a third of all overlaps). The converter's yaw convention is confirmed by the data: portal centres sit inside both of their rooms 99.2% of the time, against 72% with the opposite sign. Tolerance agreed with Phase 2: separation of 8 Creation units or less along the separating axis (engine 8b3e3bbc). Scripts are in `local/research/rooms/`.
- **Next:** CodeRabbit round 2 on #126 when it arrives; #188 fixes after the draft opens; the `overlapping_room_pairs_without_edge` counter (eps 8, same definition) if the morning plan hands it out.

## 2026-10-06 Phase 2 Dev (engine foundations: #184, #187, #188, interior shots)

All local branches; no PRs opened (cap). Status and hand-offs went to Upstream Dev and `local/team/2026-10-06.md`.

- **#184 performance baseline, handed off:** `phase2/perf-baseline` 111ca6be (stacked on #122 2433ef4f; worktree `D:/Modding-overflow/wt/p2-perf`, build `wt-targets/p2-pacing`). Run fingerprint, measured-window GPU pass times, GPU memory counters (null until measured), scene counts, CPU by schedule/span, `scripts/phase2-baseline.ps1` (baseline.json format 3). Final baseline for main: `local/perf-baseline/2026-10-06-main-final/` (world scenes CPU/render-thread bound; GPU passes <0.5 ms). Bug found by analysis #1009: rural/dense/water all ran at 0,0; fixed (f72ccb98), and 111ca6be makes `dense` Riverwood 3,-12 with `dense-legacy` 18,-5 (meant as its own PR; squash at send time). **Owed:** a ~10 min timing rerun of dense, water and dense-legacy (quiet PC). Bevy 0.19/0.20 do not time shadow cascades (needs a RenderDoc/PIX capture).
- **#187 view policy, handed off (portal-ready):** `phase2/view-policy` ca9fd9ca off main 2c11897b (worktree `wt/p2-viewpolicy`, builds in its own `target/`). Slice 1 (generic ViewPolicy, invalidation, water adapter) and slice 2 (`shadow_route.rs` shadowless reflection route, `--water-reflection-preset`, `--water-reflection off`, `--shadows off`) reviewed (DeepSeek+Muse #1021, delta #1022) and green: workspace nextest 1143 passed, doctests clean.
- **#188 engine half (room culling), handed off:** `phase2/room-culling` 4bce5914, stacked on phase1/room-graph d4f8381d (worktree `wt/p2-roomcull`, build `wt-targets/p2-roomcull`). `--room-culling` off by default; edits the main camera's `VisibleEntities` only; actors/doors/loose items/moving bodies exempt (`RoomCullExempt`). Later work: rooms that touch or overlap (separation <= `OVERLAP_TOLERANCE` = 8 Creation units, separating-axis test) count as one space, because shipped cells have 1,894 such pairs with no portal; crossing one narrows the region to the neighbour's box only; doorway pairs get no overlap edge; the box-coverage assumption is documented. Reviews DeepSeek+Muse #1028 accept; workspace nextest 1136 passed, doctests clean. Image test (20 interior poses on p1rooms, 3 off + 3 on runs; channel diff and NVIDIA FLIP agree): 18/20 clean; margin-bleak d05/d12 26/27 px = a one-pixel edge tie on one thin line drawn in both modes (no single FormId reproduces it). Visible meshes e.g. bleak 3288 -> 1188, night-c 5908 -> 2109. Note: `local/review/188-room-culling-image-test.md`; raw data `D:/Modding-overflow/scratch/p2-roomcull-image/` (throwaway worktree `wt/p2-roomcull-image`, branch `test/room-culling-image`).
- **Interior shots, handed off:** `phase2/interior-shots` 01b2a732, stacked on #128 386267dc (worktree `wt/p2-intshots`, build `wt-targets/p2-intshots`). `--shots` renders interior poses through #128's ActiveSpace; shots.log gains `space=`; workspace nextest 1122 passed. Proof: `D:/Modding-overflow/scratch/p2-intshots-proof/out/`.
- **CodeRabbit round 2 (Claude #1025), pushed by Upstream Dev:** #121 2167650d (`configure_io_task_pool` uses the pool it gets), #124 1fd11141 (`coc` prefers a gridded exterior in the streaming worldspace), #125 3734ee9e (26 directions per capsule end).
- **Next:** #184 ~10 min timing rerun (dense = Riverwood, water, dense-legacy) on a quiet PC, then squash the three scene commits at send time; check whether #122's "world ready" pacing signal fails in interiors (interior-shots + #122).
- **Rules learnt today:** check Bevy 0.20 (rc.2) before building around a 0.19 gap; foundations also go to portal (tell Upstream Dev when a branch is portal-ready); ViewPolicy must stay generic for portal cameras. The load-sensitive fixture test (`streaming_fixture_loads_its_interior_by_id...`) fails whenever the PC is busy and passes alone.
- **Disk (the user's direct yes, logged in the team log):** deleted `C:/Modding/SkyrimConverted-p2night` (75 GB), `C:/Modding/SkyrimConverted-upstream` (73 GB), `D:/Modding-overflow/wt-targets/p2-shots` (12 GB) and the closed #99 worktree `wt/p2-prepass` (9.6 GB). A cold workspace nextest in an empty target costs ~15 GB on D:; run it yourself, one at a time.
- **No workers running.** Leftovers: empty `wt-targets/p2-viewpolicy` (only OWNER.txt), a 480 MB stray `wt/p2-intshots/target`, `scratch/p2-perf-dryrun2`, `scratch/p2-physics-evidence`, and after 10-09 the throwaway `wt/p2-roomcull-image` + `scratch/p2-roomcull-image` (all class A, need the user's yes).

## 2026-10-03..06 Portal Dev (experimental demo; LOD rebuild parked)

- **Experimental demo (10-03, Upstream Dev's hand-out), done:** local-only branch `experimental/2026-10-03` = origin/main b5d317f3 + 15 open PRs + portal e6a0bd38, head 07df8a37, worktree `D:/Modding-overflow/wt-experimental`, build `D:/Modding-overflow/wt-targets/experimental`, data `D:/Modding-overflow/SkyrimConverted-experimental` (converter 101_801, world 100_401; class C). Play: `powershell -NoProfile -ExecutionPolicy Bypass -File tools/experimental_play.ps1`. 2-door tour passed; workspace checks: 1740/1743 tests (3 snapshot tests expected), clippy one unused helper, fmt differences. Portal's doors.rs drives doors; #128's crossing compiled but not registered.
- **Scripts on portal:** `tools/experimental_build.ps1` (rebuild from scratch; rerere + reuse of the previous build's identical merges, pinned heads with -NoFetch/-PortalRef, staged fixups in `tools/experimental/fixups/`; a from-scratch replay matched 07df8a37 exactly), `tools/experimental_convert.cmd`, `tools/experimental_play.ps1`, `tools/merge_keep_both.py`.
- **#105 LOD rebuild (10-04), PARKED 10-06 by Upstream Dev:** `experimental/2026-10-04` (#105 64316271 + newer heads + #142 #152 + portal) compiles at 4559e5bb, converter 101_901 / world 100_501, portal's converter lod.rs renamed portal_lod; 49 engine lib tests fail (merge semantics), engine examples don't compile; never reconverted or played. Measurements for Chef dropped (#105 merged 10-05; LOD cost comes from Phase 2's #184 baseline). Lessons: workers in hand-made worktrees need `ds-agent -OwnedFiles` (AnyWorkers added it 10-04; never edit .deepseek-agents.json); replay a reviewed portal merge with `git cherry-pick -m 1 --no-commit <old merge>` onto the new tree instead of re-merging.
- **Open:** `D:/Modding-overflow/wt-experimental/target` (15 GB, stray duplicate build, class A) on Upstream Dev's "can go" list, waiting for the user's yes. Two project-config asks still waiting for the user: narrow implAllowTools `Bash(python *)`; drop the missing `C:/Modding/shared-check-phase2` from readOnlyDirs. No worker running.
- **Also:** Discord digest on Open Render Graph's "nanite" idea: `local/research/discord-2026-10-04-org-nanite.md` (idea only).

## 2026-10-04/05 Upstream Dev (rename night; user stopped ~01:10)

- Merged upstream: #114 (Chef, with our fixes), #118 squash cbb4fd48 and #127 rename squash 5579f007 (both on the user's explicit override of the no-self-merge rule for tonight). **Main is renamed to Mudcrab:** every open branch must merge main and re-run `scripts/rename-to-mudcrab.py` (then `check`).
- #127 before merge: re-run on main after #118, snapshot -4 bytes (shorter collision key), CodeRabbit thread fixed (reader split by rustfmt; self-test 33/33). #118 before merge: main merge with a dummy-content nif.rs conflict (vertex colours + box bodies both kept), snapshot +12 bytes.
- Posted (the user's yes): #105 review comment (four non-LOD regressions + schema heads-up).
- The user's 7 drafts updated against renamed main (Claude impl #985, every resolution reviewed by me) and pushed fast-forward: #121 #122 #123 #124 #125 #126 #128. Behaviour change to mention in #121's text: `--io-threads` now refuses bad values like main's other options. #99 not touched: looks superseded by Chef's #142 (same prepass_vertex_alpha.rs); ask the user about closing it.
- Disk (the user's yes): deleted my merged-PR worktrees and build folders, about 115 GB in total (log in local/team/2026-10-04.md and 2026-10-05.md). `--portable-textures` work kept as local branch `portable-textures-followup` (60fd0a7b). New shared disk/GPU rules with the Film VFX and DAO sessions: D:/VFXEnhance/coordination/RESOURCES.md (GPU lock file, 10 GB floor, ledger for anything >5 GB).
- Built today: research index (local/research/INDEX.md), Skyrim RE digest, roadmap page, Ghidra MCP install (Tier 3 needs the user's OK per task).
- The user wants the balance_guard hook removed; I was blocked from editing my own hooks; they will remove it from ~/.claude/settings.json themselves.
- No workers running. Next (morning brief): #99 close?; --portable-textures draft PR; #113 SemVer reply; #138 relative --assets (Phase 2); fixture-test draft; labels housekeeping; #136 now the rename is in (Chef's thread for eclepso); #105 needs Chef's rework and a schema renumber.

## 2026-10-03 Upstream Dev (user stopped ~13:40, 3 h 37 min)

- Merged upstream: #98 and #102 (by Chef); #119 squash 506852da (merged by me on the user's explicit permission).
- Fix pushes under the standing yes: #98 1c087555 (main merge); #114 5a5a7bed, 29d0f95e, 69d63d24 (on #119; CodeRabbit 24-bit pitch, GPU aligned rows, README); #118 a30701ce (4 CodeRabbit census threads). #114 and #118 marked ready (the user's ask). All threads answered and resolved.
- Posted (the user's yes): #102 review reply; reviews on #119 #136 #137 #139; aligned-24-bit note on #119.
- The user created the 10 labels from labels.yml. Trial rule changes in AGENTS.md: at most 3 goals a day that need the user's decisions (track = Phase 1/2/3/Portal only); evening table at 20:30.
- Portal Dev built the experimental demo (tools/experimental_play.ps1, branch experimental/2026-10-03, local only). Queued: #105 into experimental once Chef updates it.
- Still running at hand-off: Claude impl #968 merging main into #114 (after #119); #118 main merge d2500953 checks in D:/Modding-overflow/wt-targets/up-118. Push both as fast-forwards when green, then they are ready for Chef/Lucas.
- Next (morning brief): #136 merge after the rename; #127 re-run once #114/#118/#105 are in; #113 SemVer reply draft (onboarding); #138 relative --assets for Phase 2; #143 DX12 flashing (eclepso fixing; use WGPU_BACKEND=vulkan meanwhile); evening-table rows rolled over (fixture-test draft, disk can-go list incl. merged PRs' folders and wkj-docs-workflow, onboarding post, labels.md note + #77, labelling our PRs).

### 2026-10-02 (evening) - Claude [phase2] Portal Dev: #112 door crossing handed off

Un-parked by the user. `phase2/door-preload` frozen at **e88f9bcc** (main merged through #92) and handed to Upstream
Dev as one draft: engine door-link reader (optional `door_links` table) + E/fade/space switch/teleport crossing;
the converter `door_links` export is its own later PR (waits for eclepso's #110), preload third. Checks on the
head: fmt, clippy `--workspace --all-targets --all-features`, nextest 784/784, doctests. Real data (main conversion
+ door_links copied from the portal DB, `D:\Modding-overflow\p2main-doors`): Riverwood -> Sven's house 1,235 ms
E-to-fade-in, back 7,426 ms. Fixed on the way: fade-in waited for GPU uploads (budget lifted while black), gate
scoped to the landing cell (was global: the exterior always hit the 10 s timeout), interior restore offset.
Reviews: DeepSeek 700/703, MiMo 701/704 (Sol pace-held), final "small fixes"; worldspace-0 rejection left under
Still open. `TeleportPlayer` in physics.rs is byte-identical with Phase 2's `phase2/console-coc` (trial merge:
three keep-both conflicts). Local-only evidence driver on `local/door-evidence` (wt-697; never push). PR text
`local/pr-drafts/door-crossing.md`, media in `local/review/pr-phase2-door-preload/`. Incident: a coder's
`git stash pop` applied Phase 2's stash (stashes are shared by worktrees); nothing lost; coders now banned from
git stash. **No workers running.**

### 2026-10-02 (evening) - Claude [phase3] Phase 3 Dev (Mudcrab rename re-land prepared, handed off)

- Upstream Dev's handout: the scripted re-land of the Mudcrab rename (#66 + #68's fixes). Branch `chore/mudcrab-rename` from origin/main 1b70b9b0, frozen head 26297359 (worktree D:/Modding-overflow/wt/p3-rename). Not pushed; handed to Upstream Dev; PR text `local/pr-drafts/109-pr-mudcrab-rename.md` (re-land recipe in its Status).
- `scripts/rename-to-mudcrab.py` (apply / check / self-test). A re-run on fresh main reproduces the output commit. Old env names and the old collision key are still accepted.
- Tested: head fmt and clippy (all features) clean, nextest 761/761. Main + 9 open PRs 817/817. #98 and #114 each on main, and #105 on its own head: failures only from the flaky fixture test or ones present without the rename. Reviews: DeepSeek 946/947/949, Sol 946.1, MiMo 947.1/949.1 (accept at the end).
- Lesson: one CARGO_TARGET_DIR shared across worktrees of different branches gave false compile errors; use one target per branch.
- Workers: none running. Proof worktrees D:/Modding-overflow/wt/p3-rename-proof* and logs D:/Modding-overflow/rename-proof-logs can go once Upstream Dev is done with them.

### 2026-10-02 (evening) - Claude [phase2] Phase 2 Dev (second batch: four drafts plus coc/coe, all handed off)

- Handed to Upstream Dev, frozen, not pushed: arm-32 #117 `phase2/arm-32-models` 717f47f0; measure-v2 `phase2/pacing-measure-v2` 9a781f87; console #88 `phase2/dev-console` ee4755c0; physics #104 engine half `phase2/physics-bodies-v2` 1f8e6b5f; coc/coe `phase2/console-coc` b202ae9e (stacked on the console); #98 CodeRabbit round 2 `phase2/reference-shots-v2` bdde9c25. One review round each (DeepSeek + MiMo, Sol held); run ids and verdicts are in the hand-off messages. PR texts in `local/pr-drafts/` 83-, 104-, 105-, 106-, 107-.
- Evidence: `local/pr-evidence/console/`, `physics/` (partial: a dynamic bucket in F3 outlines, no knock-over clip yet), `coc/`.
- Physics evidence data: `D:/Modding-overflow/SkyrimConverted-p118mirror`, a hard-link copy of p2main with Phase 1's #118 clutter models laid over it (delete when no longer needed). s15 conversion deleted (the user's yes).
- Lessons: never `git stash` in coder briefs (the stash list is shared by all worktrees); name each provider's run separately (`DeepSeek review #nnn`, `MiMo review #nnn`).
- The load-flaky fixture test fails often while several sessions build; it passes alone. Its fix is a row in the user's table, not started.
- Workers: none running. Next: CodeRabbit rounds on the drafts once they open.

### 2026-10-02 (afternoon) - Claude [phase1] Phase 1 Dev (physics coverage draft; #97 CodeRabbit round)

- `phase1/physics-coverage` frozen at 87081482 and handed to Upstream Dev (draft, Merge after #118): #118 head
  1540418a and main 869df14f merged in; spheres, multi-spheres, cylinders, layers 0/5/26/28, No Collision guard,
  non-uniform sphere transforms skipped. Clutter GLBs 1,685 -> 1,771 bodies (727 -> 736 dynamic); whole install
  11,976 -> 12,807 (census). Checks: workspace clippy --all-features, nextest 778, doctests (Rust 1.98.1).
  PR text `local/pr-drafts/phase1-physics-coverage.md`.
- #97 CodeRabbit round: head e06935c6 (skip after first failure, GLB header-only read); DeepSeek + MiMo accept.
- For Phase 2 Dev: `D:/Modding-overflow/p118-clutter/` holds 1,746 clutter GLBs from p2main with #118 collision.
- No workers running.

### 2026-10-02 (day) - Claude [phase2] Phase 2 Dev (review fixes on open PRs #94 #98 #99, handed off)

Track: Phase 2. All three are fast-forwards of their upstream PR heads, not pushed; Upstream Dev has the hand-offs.
- #98 `phase2/reference-shots-v2` head 5b18ec81: failed PNG writes fail the shot (temp file, PNG check, rename), valueless `--shots`/`--shots-out` are errors, settle counts failures per shot, PrimaryWindow query. Final check review-684: DeepSeek small fixes (nits), MiMo ready. Reply to Chef: `local/pr-drafts/98-reply-chef.md`; text edits `98-text-edits.md`.
- #99 `phase2/prepass-vertex-alpha-v2` head 78afd410: anchors pinned to Bevy's source, bounded retry, column-0 directives with the anchor's indent kept. Final check review-685: DeepSeek accept, MiMo ready. Text edits `99-text-edits.md`.
- #94 `phase2/engine-help-v2` head 2bfee9c8: `--help` wins anywhere, bare `--` ignored, script scans recurse and are line-scoped, numbers only. Delta 2 review-687: both small fixes, test-scan nits only (listed in the hand-off). Text edits `94-text-edits.md`.
- Tested: fmt, clippy with --all-features, nextest --workspace --all-targets on each head (730 / 716 / 742 passed).
- Workers: none running. Launcher note: ds-agent needs `DEEPSEEK_AGENT_CLAUDE` set to the app's claude.exe (reported to AnyWorkers).
- Next: Upstream Dev pushes with the user's yes; parked jobs (console, physics engine half, #117 pair, flaky-test fix) wait for the user's batch decision. New PRs follow SEND-CHECKLIST §5 (draft-first).

### 2026-10-02 (day) - Claude [phase1] Phase 1 Dev (open-PR review fixes #93 #97 #114 #118; physics coverage parked)

- Two-provider pre-open reviews (DeepSeek + MiMo; Sol was pace-held) on the open converter PRs, every finding
  verified, fixes as local commits, delta reviews until green. Frozen heads handed to Upstream Dev, nothing
  pushed: #93 615ca2ff, #97 c6164eba, #114 e331049c, #118 1540418a. PR body fixes: `local/review/pr-<n>/pr-text-fixes.md`.
- Key fixes: #114 restores main's X8R8G8B8 fallback (blocker), reads DDPF_ALPHA alpha, bounds the packed
  decode before allocating; #118 checks bodies against the written GLB (a reused name only when the GLB keeps
  the NIF order), measured on all 1,745 clutter NIFs: 1,685 bodies, 727 dynamic kept; #97 deterministic first
  error, cpu_jobs parity, no raw NUL bytes in source; #93 linear prune-record lookup plus two tests.
- Tested: fmt, clippy -D warnings, nextest per PR on Rust 1.98.1 (299 / 298 / 312 / 370 passed).
- Parked: `phase1/physics-coverage` 64fd580e (spheres, cylinders, multi-spheres, layers 0/5/26/28, No
  Collision guard; clutter census 1,772 bodies / 736 dynamic). Left: whole-install census, workspace clippy
  --all-features and nextest --workspace for ticked boxes, merge #118's fixes, PR text
  `local/pr-drafts/phase1-physics-coverage.md`.
- No workers running.

### 2026-10-02 (night) - Claude [phase2] Phase 2 Dev (pacing rework #117: measured, first change built)

**Branches, local only** (worktree `D:/Modding-overflow/wt/p2-pacing`, base upstream main 461517bf):
`phase2/pacing-measure` (1a7f6eb7, 1d3778a2, 336f36f1, d454c3d4: time to a loaded world, `--benchmark-jump`,
fly lag, per-main-schedule timing, asset arrivals per frame, frame-time windows while loading) and
`phase2/load-faster` on top (a75bb1b4, 6d7d2ad7, d5fe5593, c0038e37: IO pool = half the hardware threads, at
most 16, `--io-threads`; 16 models armed a frame instead of 4; review fixes). The two review-fix commits also
touch measurement code, so split them if the branches go up as two PRs.
**Numbers** (Ryzen 7 5800X 16 threads, release, jump to 4,-21): world ready 2.26-2.46 s -> 1.12-1.36 s, ready
after jump 2.36-2.75 s -> 1.30-1.57 s, worst frame while loading 14.7 -> 16-34 ms (one frame over 33 ms in seven
runs), fly 5000 mean model wait 899 -> 207-295 ms; emulated 4-thread machine world ready 10 s -> 5.4 s. Upload
budget and unload rate made no difference. Run-worst 100-250 ms frames are a startup one-off (Bevy blocks on the
upscaling pipeline), not loading. Raw runs and notes: `C:/Modding/wkj-tools/pacing-baseline/notes-night.md`.
**Reviews:** DeepSeek #658 small fixes, MiMo #658 fix first (small); all valid points fixed (#659, #661).
Tested on the final head c0038e37: fmt, clippy -D warnings, nextest workspace 732/732 (15 skipped).
**Found for Phase 1 (suggestion only):** every converted texture is UASTC transcoded to BC7 at load, 56 ms mean
per texture on one thread; stored BC7 loads in under 1 ms at the same size, about half of today's load time
(`local/research/texture-load-cost.md`); tangents are generated per mesh at load (~1 s CPU per start).
PR texts: `local/pr-drafts/117a-pr-pacing-measure.md`, `117b-pr-load-faster.md` (not sent; the user's
maintainer-focus rule: nothing new upstream without their yes). Plan doc updated (Claude Docs, rev 18).
No worker running. **Next:** the user decides whether the branches go up; the converter texture idea goes to
Upstream Dev as one line.

**Update 02:00 (after Upstream Dev's review):** the first numbers used SkyrimConverted-p2night (2026-09-26), which
predates #53 (native BC textures), so the texture-transcode finding was about old data and is withdrawn. Fresh
conversion `D:/Modding-overflow/SkyrimConverted-p2main` (main 461517bf converter, complete, 36 min). On it, more IO
threads load no faster and lengthen frames; the model-arming cap was the whole bottleneck. Branch now keeps main's IO
pool size (`--io-threads` stays as an override) and arms 32 models a frame (a75bb1b4..67a1e7e8, coder #663):
ready after jump 2.36-2.80 s -> 0.82-1.04 s, worst loading frame 14.7-20.4 -> 15.2-18.4 ms (8 runs), fly model wait
692-704 -> 97-168 ms. Option for the user: Bevy's C zstd (`zstd_c` instead of `zstd_rust`; the converter already
uses C zstd): another ~25% (jump 0.63-0.70 s) for a worst loading frame of 19-24 ms. PR texts rewritten. Final DeepSeek review #664: small fixes (wording, report format 10), done in 3adba3bf;
nextest engine 255/255, clippy, fmt. Branch head **3adba3bf**.
**Packaged as two unstacked PRs (the user, 02:xx):** `phase2/arm-32-models` 872b2c27 (from main: arm 32 a frame,
`--io-threads`, docs; workspace 708/708) and `phase2/pacing-measure-v2` 51a367c5 (from main: the measurement
commits + review follow-ups, no default changes; engine 253/253). Combined scratch merge clean, workspace 732/732.
Texts `local/pr-drafts/117b-pr-load-faster.md`, `117a-pr-pacing-measure.md`. Handed to Upstream Dev; the user
expects to open them as drafts. `phase2/load-faster` and `phase2/pacing-measure` are superseded.
**Kept in the tank (the user, via Upstream Dev):** both branches frozen at those heads; nothing opens yet. Final texts
are `local/pr-drafts/105-pr-117b-load-faster.md` and `106-pr-117a-pacing-measure.md` (Upstream Dev's edits).

### 2026-10-02 - Claude [phase2] Portal Dev on #112 door preload (PARKED by the user)

Upstream Dev's hand-out: Chef's Mudcrab-Team/mudcrab#112. Main has no load doors at all (no `door_links`, XTEL never
decoded, no activation, no space switch, no fade); scope `local/research/door-preload-scope.md`, plan posted on #112
(issuecomment-5942868617): PR A = converter door_links + engine reader (optional table, no schema bump), PR B =
crossing + fade, PR C = preload. Branch `phase2/door-preload` (worktree `C:\Modding\wkj-door-preload`, from
origin/main 461517bf): 6e663fd9 engine reads door links (tests pass); b4609a15 **WIP** crossing + fade
(`door_crossing.rs`, `ActiveSpace`, `TeleportPlayer` event in physics.rs as agreed with Phase 2 Dev; headless
tests pass, fmt ok; clippy not run; never run on real data). **Open in the WIP:** arrival height (XTEL is feet,
teleport sets the body origin), activation needs `ExpectedModelBounds`, restore pose after a rebase during the
fade-out, E also reaches the debug tankard grab. Converter XTEL decode is HELD for eclepso (#110 plugin-local
form IDs). Local evidence data: `D:\Modding-overflow\p2main-doors` (Phase 2's main conversion + door_links copied
from the portal DB; local only). **Next when unparked:** clippy (`--workspace --all-targets --all-features`),
an evidence run at Riverwood, converter slice after eclepso answers, then the two-provider pre-open review
(SEND-CHECKLIST §1) per PR. Upstream Dev relays Chef's/eclepso's replies. **No workers running.**

### 2026-10-01 (day) - Claude [portal] Portal Dev (catch-up with main, steps 1-5 of 7)

**Branch `portal-merge-main`** (worktree `C:\Modding\wkj-portal-merge`, head 7bc1de29, not yet merged into
`portal`; schema converter 101_601 / DB 100_401; assets `D:\Modding-overflow\SkyrimConverted-portal`).
Done: main merged; ports of paced unloads + model arming, terrain normal maps, per-camera sky dome, rapier
colliders + player rapier probe (impl-276..280); doorway carve impl-281 (f2a5579b); **missing-objects fix**
9089fd6e: main's per-frame upload budget let `--shots`/`--demo-tour` photograph before textures/meshes reached
the GPU under CPU load (Alftand rooms without benches, or fog only). Capture runs now upload unthrottled; A/B
evidence `local/demo/r283-u0` vs `r283-u16`; checked under load, 6/6 frames identical to the good ones. Phase 2
Dev told (main's `--acceptance-screenshot` gate may share the gap; passed to Upstream Dev). Cleanup pass
impl-284 from MiMo audit #282 (0ea83c7b, 7bc1de29): log noise demoted, `converted_model_path` lowercases like
main, dead leftovers removed; main's gated `WorldPlayerPlugin`/`SkyPlugin` code kept on purpose (merge cost).
Tested: engine nextest 849/849, clippy -D warnings, fmt. Walked short tours: Riverwood PASS (2 crossings).

**Later the same day:** the walked Alftand failure was not a wall: `update_render_origin` shifted every
static collider on a rebase, but pre-streamed interior roots never move, so Alftand01's floor ended a cell
(4096) from the door and the walk in found no ground (flaky: only when the exterior walk crossed a cell
boundary first). Fixed 0c9da8fb (only bodies under an `ExteriorCellGrid` root shift; new test). Merged
origin/main #67 (2a880791). Walked short tours: Alftand 4/4 PASS, Riverwood 2/2 PASS; nextest 850/850.
Conversion compare old vs new (`local/merge/conversion-compare.json`): no file, cell or reference lost.
**Done in the evening:** showcase B (`local/showcase/b`: the A poses re-rendered, 4 Alftand interiors,
`walked-crossings.mp4` from the new portal-only `--tour-clip`, 7dd86661). **`portal-merge-main` merged into
`portal` (f6b70c14)**; crates/Cargo identical to the tested branch. The user played the merged build
(Honningbrew, Falkreath Hall of the Dead crossings fine; two doors without an open clip, some partial/skipped
authored collision warnings, e.g. Whiterun wall caps). **Next:** pacing A/B rerun (timing booking), lit-slope
check of `NORMAL_GREEN_SIGN`; worktree `C:\Modding\wkj-portal-merge` can go. **No workers running.**

### 2026-10-01 (day) - Claude [phase1] Phase 1 Dev (portal catch-up converter half; textures; #104a)

**Portal catch-up (the user's option B):** converter-state brief `local/research/converter-state-for-portal-merge.md`;
schema numbering approved, portal = 100_000 + main*100 + rev (converter 101_601, DB 100_401; `cache.rs` doc +
test). Resolved converter/dummy-content conflicts on `portal-merge-main` (386044af), merged held
publish-robustness, staging-leftovers, check-outside-data (3570acb8). Reconverted to
`D:\Modding-overflow\SkyrimConverted-portal` (44:55, complete, 0 failed, `converter check` all good).

**Texture speed-up = draft #114** (`phase1/native-rgba8` 12a257fa): uncompressed DDS to native BC1/BC7 via
intel_tex_2 instead of Basis UASTC. Quiet A/B on textures-only data: texture stage 13:27 -> 5:23, disk/GPU equal,
FLIP better on all samples (`D:/Modding-overflow/tex-ab/`). **LOD containers = draft #115** (reference for Chef;
LOD is his, #103/#105). **#104 phase (a)** `phase1/physics-bodies` 448a51c2 FROZEN pending the user's yes on the
draft; contract `local/research/p104-physics-body-contract.md`; clutter census 727/1,686/1,745 vs 754/1,781/1,746
(gap = existing skips). Convert-while-playing scoped (`local/research/convert-while-playing-scope.md`, issue #111)
and ON HOLD (Chef suggested run-time loading; one-cell spike proposed).

**Lesson (the user):** a silent runner failure plus an open-ended booking cost ~1.5 h: runners signal failure
in `finally`, waits have deadlines, disk checked first, bookings carry an END-by time (memory
`feedback-long-runs-fail-safe`). Conversions run in a visible console (memory `feedback-conversions-visible-console`).
**Waiting on the user:** #104a draft, #104 coverage follow-up. **No workers running.**

### 2026-09-30/10-01 - Claude [phase1] Phase 1 Dev (Sol-review fixes sent; overnight jobs 1-4 held)

**Sent via Upstream Dev (the user's yes each):** #69 output-dir guard, #71 restore #30 + fixes, #73 remove
resumed staging, #75 fixture player race, #93 resume keeps prune records. Main-green fixes after #66 were
local only (#74 reverted the rebrand).

**Overnight jobs (handed out by Upstream Dev, all HELD, texts in `local/pr-drafts/`, summary sent to Upstream Dev):**
| Branch | Head | What |
|---|---|---|
| `phase1/check-schema-range` | a1a3ba03 | `converter check` accepts the runtime's schema range (on #95) |
| `phase1/publish-robustness` | f1c5b906 | publish failures keep the previous output findable; any stale backup stops publish |
| `phase1/staging-leftovers` | 93c9e5d5 | resume clears own temporaries; atomic loose-override copy |
| `phase1/lod-containers` | e6972697 | .btr/.bto to GLB; all 9,584 .btr + 1,078 .bto real files parse and convert |
| `phase1/check-outside-data` | 2faea763 | shared Data-overlap check (Phase 3 uses it) |
| `phase1/parallel-validation` (#97) | 64ad53af | validation on a rayon pool (25.5 s to 3.3 s); open upstream as draft #97 |

**Lessons:** ds_impl coders fill C: (run one at a time, or Claude subagents in D: worktrees); Python
heredocs with `\`+newline break Rust string continuations; Windows `sync_all` needs a writable handle.
**Next:** LOD pieces 2, 5, 7 with Phase 2 (after the go); discard D: worktrees p1-parval, p1-checkrange,
p1-publish, p1-leftovers, p1-outside, p1-pruneresume and their targets once sent. **No workers running.**

### 2026-09-30/10-01 - Claude [phase3] Phase 3 Dev (launcher safety, reviews, overnight drop-and-console)

**Upstream:** #70 (launcher safety: refuse a non-conversion Output, no Start while the game runs) MERGED,
with a Sol review (#914) doc fix and Phase 1's #69 fix. Issue #76 posted (launcher version installs, Chef's
idea, credited). Reviewed Phase 2's launcher lines on `phase2/schema-range-shared` 9e9effe2: OK (raw manifest
read matches the engine). Held-work audit: no built Phase 3 branch still held
(`local/research/held-branches-phase3.md`); `brand/mudcrab-text` obsolete. The name stays Mudcrab (the user).

**Overnight (the user's "Go" via Upstream Dev), HELD locally:** `phase3/launcher-drop-and-console` **0761e0e**
(worktree `C:/Modding/wkj-phase3-safety`, target `D:/Modding-overflow/phase3-target-safety`), from origin/main
a896b90: no console on Windows release builds + `%TEMP%\mudcrab-launcher.log`; engine with CREATE_NO_WINDOW;
Output/Data overlap refused on drop, press, Detect (Phase 1's `phase1/check-outside-data` 2faea76 merged:
merge after it). Workspace 712/712 on 5c8447e, launcher 113/113 on 0761e0e; a real release run checked.
DeepSeek impl #915/#916, review #918 (its "println panics without a console" disproved by test). PR text
`local/pr-drafts/phase3-launcher-drop-and-console.md`; handed to Upstream Dev.
**Lesson:** a launcher test run whose working folder lacks `game_data` falls back to the real Steam
Skyrim; use a fixture-only setup. **No workers running.** **Next:** the user's yes on the branch; the
Browse picker only if handed out.

### 2026-09-28 - Claude [phase2] Phase 2 Dev (overnight + day; wrap-up)

**The user's decisions:** plans cover one day or one night at a time; the portal is PARKED (Portal Dev
idle); tonight's acceptance run CANCELLED at the project-wide wrap-up (~14:30); PR texts and hand-overs
follow `local/pr-drafts/STYLE.md`; DeepSeek split Phase 1 $3.75 / Phase 2 $3.75 / Phase 3 $1.50 /
Upstream $0.73; a new **Phase 3 Dev** owns `crates/launcher` + `crates/converter-gui`.

**Branches (from main 6f583cd, green, reviewed, all with Upstream Dev, held by the PR pacing rule):**
`schema-mismatch-error` 7cce96c (which asset check failed + the reconvert command),
`fixture-interior-crossing` 3b41d4d (M5.1), `engine-help` a57dff0 (`--help`, unknown options and bad
values refused; send after #41/#45), and the crossing-stutter set `paced-cell-unloads` 8f980f6 ->
`upload-throttle` cb917c9 -> `paced-model-spawns` c8b2801: worst streaming frame ~21-25 -> ~17 ms,
p50-p99 unchanged (`C:/Modding/wkj-tools/crossing-ab/summary.txt`; the rebase costs nothing; the spikes
were model arrival). Also: `audit-engine` onto portal (merged d940251), the fast-texture FLIP pair for
Phase 1, research-615/618/625 (in `C:/Modding/wkj-phase2/local/agents/runs/`).

**Acceptance:** `accepted` needs three campaigns (evidence, `-UpdateBaseline` with the user's signed
review, compare): `local/acceptance-plan-2026-09-28.md`; clean pre-built worktree `C:/Modding/wkj-accept`.
Not run. **No workers running.** Details: `local/handover-2026-09-26-phase2.md` (2026-09-28 section).
**Next:** the acceptance campaigns; cell-commit cost (main_world ~11-12 ms, the remaining worst frames);
M4.1/M4.2 terrain inspection after #49/#42; M8.2 plan-doc fixes.

### 2026-09-28 - Claude [phase3] Phase 3 Dev (first session: converter window folded into the launcher)

**Set-up:** the user confirmed the Phase 3 rules (see "Two tracks"): worktree `C:/Modding/wkj-phase3`,
`phase3/*`, targets on D:, workers 900-999, owns `crates/launcher` + `crates/converter-gui`, DeepSeek share
$1.50 (from Phase 1's). The user's design call: converter-gui moves into the launcher and the crate is
deleted. Upstream Dev's split: L1 resume, L2 progress, L3 merged window, L4 engine exit message.

**Branches (all green, reviewed, handed to Upstream Dev; nothing pushed):**
| Branch | Head | What |
|---|---|---|
| `phase3/launcher-resume` (L1, from main) | eb5a0d6 | the launcher resumes a leftover `<output>.staging-*` instead of starting over. **Send after #31** (its journal re-checks schema/config/hash, so an automatic resume never publishes stale or torn outputs). Text `local/pr-drafts/phase3-L1-launcher-resume.md` |
| `phase3/engine-exit-message` (L4, from main) | bd26dfc | engine stdout+stderr to a temp log (never a pipe); on an early exit the launcher shows the tail + log path; no second engine. Not yet tried against a real refusing engine |
| `phase3/launcher-conversion` (L3) | e1ffb36 | converter-gui folded into `crates/launcher/src/conversion/`, crate deleted; no auto-start; Start/Stop/Resume/Delete staging/Check/Full check; leftover staging from an earlier session offered for Resume; Stop cancels a check (phase1/output-check b99bc4b merged); 270 tests. Stacks on #51, #45, the converter bundle: held |

**Found:** on main, `--resume-staging` could reuse a torn GLB (cache-hit copy) or Luau (plain write)
after a hard kill; Phase 1 fixed it on `phase1/atomic-writes` 50f421c (one copy path left, on their list).
Screenshots: `local/research/launcher-merged-*.png`, `launcher-resume-done.png`. Fixture data for launcher
runs: `D:/Modding-overflow/phase3-scratch/game_data` (run the exe with that folder as working directory).

**Test kit for the user:** `D:\Mudcrab-Launcher-Test\` (174 MB: quick launcher + engine from local-only
`phase3/try-launcher` 734982a = L3 + L4, fake Skyrim in `game_data\Data`, Start/Reset/Make-leftover scripts,
README). Play on the fake data opens a grey engine window: the launcher passes only `--assets`, and the
fixture needs a worldspace the engine doesn't start in by default.
**Evening, the user (via Upstream Dev): get the launcher rework up.** `phase3/launcher-rework` **77b9869**
(worktree `C:/Modding/wkj-phase3-rework`) = main + `phase1/progress-on-55` 67f00ba (#55 + #45 + converter
progress + `converter::find_resumable_staging`) + launcher commit 880bad5 (L3's launcher, no auto-resume,
converter's staging finder) + #55 6788d10. nextest 257. Handed to Upstream Dev (the user allowed it to open it
once green), text `local/pr-drafts/phase3-launcher-rework.md`, evidence `local/research/rework/`. This
supersedes L1 (retired) and L3's old stack; L4 is upstream in #56. Follow-ups: merge Phase 1's next
progress-on-55 head (per-entry Stop in extraction, impl #834); "Start over" label after Delete staging.
**Parked by the user:** a folder picker (Browse) for the Data folder (`local/handover-phase3-dev.md` §7).
**Open:** L2 (progress, after #45), PR texts for L3/L4, the notice pane's mouse wheel untried by hand,
the launcher's monospace font (ask the user). **No workers running.** Project-wide wrap-up (the user).

### 2026-09-28 (day) - Claude [phase1] Phase 1 Dev (converter bundle evidence, check command, window handed to Phase 3)

**Branches (local, green; Upstream Dev holds the send queue, every send needs the user's yes):**
| Branch | Head | What |
|---|---|---|
| `phase1/quiet-basis` | 604150b | basisu's per-texture "KTX2 validator bug" printf off stdout (C stdout to null device, Rust stdout kept); Claude review #828 accept + fixes |
| `phase1/output-check` | b99bc4b | `converter check <output> [--full]` (quick check of a full install ~3 s) + `check_output_with_cancel(.., &AtomicBool)` |
| `phase1/atomic-writes` | 50f421c | cache-hit copy + Luau write via temp + rename (`atomic_file.rs`); `-zstd` aa567c6 = the ktx2-zstd resolution |
| `phase1/cheap-io-zstd` | e5a2047 | cheap-io merged with ktx2-zstd (outcome carries `(ConversionOutcome, Option<OutputStamp>)`) |
| `phase1/ktx2-zstd` / `fast-textures` | 5a490d0 / 762fe3b | #51 head f14c5a9 merged in (no longer reverts the spill fix); native usage text says RGBA8 |
| `phase1/converter-window` | a062046 | **now Phase 3 Dev's** (merged into the launcher); includes the Check buttons (Claude impl #830) |
Upstream Dev's `phase1/converter-bundle` 1746f48 = #51 + zstd + native + quiet-basis + output-check + cheap-io-zstd (320 green).

**Measured (quiet booked machine; `local/research/phase1-bundle-evidence.md`):** zstd frame time
unchanged (stress 6.20 vs 6.25 ms, textures only differ); native mode re-makes all 33,175 textures in
**4.7 min** (vs ~4.5-5.1 h UASTC; 247 fallbacks; textures 14.42 GB native vs 12.96 UASTC+zstd);
cheap-io A-B-A: texture stage 3.1 -> 1.4 min, other stages unchanged, no extraction regression.
**Extraction and validation are latency-bound** (11-15 % of 16 threads, disk ~95 % idle): issue draft
`local/pr-drafts/36-issue-latency-bound-conversion.md`, HELD for the user's decision on the reply to
Lucas (who called converter work top priority). `converter check` found main's prune pass leaves ~230
GLBs with stale manifest entries (already fixed by glue 0fdc08a, rides #30/#31).

**Done later the same day (the user's launcher push):** `phase1/converter-progress-main` 74c9a8c (on #45 970cbdd: status line, Cancellation, ProgressEstimate, `find_resumable_staging`), `phase1/progress-on-55` 67f00ba (+ #55, for Phase 3), and the #52-#54 comparison `local/research/phase1-chef-prs-comparison.md` (back each with changes; our `phase1/cache-dir` has a corrupt-blob re-link bug, archive/mod.rs:221). **Evening:** #58 = our progress branch, head 38dbb9e (`phase1/converter-progress-stop`: + Stop inside archive extraction with a deterministic test, + CodeRabbit fixes merged); #59 launcher rework has it. Frozen PR heads now. The user: no follow-up comment on #52-#54 and no reply to Lucas for now; the comparison and issue draft 36 stay in local/ for later. Background of that ask (15:20): TaylorTurnerIT opened #52 (runtime pack: publish
artifacts only, blobs to `<output>.assets-cache`, hardened staging writers), #53 (native DDS blocks in
KTX2 by default, no mode flag) and #54 (per-mip zstd, level 6), all stacked on our #51 and overlapping
cache-dir, atomic-writes, fast-textures and ktx2-zstd (ours HELD; the user decides how to cooperate).
Do a diff-level comparison (correctness, #31/resume interplay, `.partial` handling, tests) and say
whether our full-install evidence (sizes, 4.7 min native run, FLIP, timing A/Bs) would strengthen his PRs.
**Sent upstream (Upstream Dev, with the user's OK, ~15:40):** #55 contributor-experience
(`phase1/contributor-experience` e45183c = contributor-onboarding + output-check), decoder-bounds
pushed into #43 (head 0914347), and a comment on #53 backing his stack and offering our numbers. #43
and #55 are frozen PR heads. Held for the comparison: cheap-io, atomic-writes, ktx2-zstd,
fast-textures, cache-dir. converter-progress would have to be rebuilt on #45's head 970cbdd to ride #45.
**Then (not started; the user's project-wide wrap-up):** sweep stale `*.partial` files in staging on
resume + guard before publish (Upstream Dev's review of atomic-writes; main has the same gap for
texture/mesh temporaries); make the archive `copy_file`/link fallback atomic on top of #51; parallel
cache verification + validation hashing, `--io-jobs` wired (plan: see the issue draft); native mode
RGBA8 for 24-bit BGR8 and the remaining layouts; the #31 x ktx2-zstd resolution when the second lands.
**Running:** nothing. Scratch: `D:/Modding-overflow/p1-timing` (native tree + A/B results),
`wt/p1-merge`, `wt/p1-bundle`, `wt/p1-fasttex`, `shared-check-target/*` can be discarded.

### 2026-09-28 - Claude [phase2] Upstream Dev (converter priority, new dev lanes, PR style, bundle evidence)

**Upstream:** main still 6f583cd (no maintainer merge since #39 on 09-26). Ours open: #29-31, #38,
#40-41, #43-49, #51 (all CI green, all merge cleanly with main, checked 09-28). #51 (vfs hard links,
70.9 -> 45.65 GB) got its CodeRabbit follow-up (f14c5a9: NTFS link-limit spill) and a body fix
(mojibake from a cp1252 round trip; never pipe PR bodies through the console). Chef (TaylorTurnerIT)
was answered on #30: the user welcomes PRs against the fork's branches.

**The user's rules now:** all publishing goes through Upstream Dev with the user's yes per item
(AGENTS.md "Publishing rule"; fork pushes included). Plans cover one day or one night. The pacing
("new PRs wait for a maintainer merge") is LIFTED for Phase 1 converter and Phase 3 launcher work,
because Lucas (owner) said on Discord that converter work is top priority (resume/WAL, parallelism,
convert-while-playing); engine PRs keep the pacing. PR/issue text follows local/pr-drafts/STYLE.md
(short summary on top, details collapsed); old PR bodies are not edited.

**Queue** (docs/pr/upstream-queue.md has heads and order): converter send list 1 contributor-onboarding
22391dd, 2 ktx2-zstd 5a490d0, 2b atomic-writes 50f421c (+ a `.partial` sweep in progress), 3
output-check b99bc4b + quiet-basis, 4 cheap-io (clean A-B-A timing in progress), 5 fast-textures
762fe3b (issue 34 first), 6 cache-dir (issue first), 7 decoder-bounds, 8 converter-progress. Launcher:
L1 launcher-resume eb5a0d6 (after #31), L3 launcher-conversion e1ffb36 (held), L4 engine-exit-message
bd26dfc (needs text + a manual refusal check). Engine (paced): schema-mismatch-error,
fixture-interior-crossing, the crossing-stutter set c8b2801 (worst streaming frame ~21-25 -> ~17 ms;
272 green), engine-help (after #41/#45). Texts ready: local/pr-drafts/30-36.

**Evidence branch:** phase1/converter-bundle c7a79b2 (worktree C:/Modding/wkj-upstream; #51 + zstd +
native + quiet-basis + output-check b99bc4b + cheap-io-zstd; 323 green). Cheap-io A-B-A: texture
stage 3.1 -> 1.4 min, other stages unchanged. The #31 x zstd resolution is Phase 1 Dev's when the second lands.
Bundle evidence: local/research/phase1-bundle-evidence.md (native textures 4.7 min for 33,175;
extraction/validation latency-bound at ~14% CPU; the latency issue draft 36 is on hold until the user
decides on a reply to Lucas). **Running:** the 90-minute upstream check (session crons). No workers.

### 2026-09-27/28 (night) - Claude [portal] Portal Dev (audit clean-up, look-out bench, schema 28; PORTAL PARKED)

**Parked:** on 2026-09-28 09:45 the user parked portal work (relayed by Phase 2 Dev). The session is
idle: no workers, coders, engines or bookings. Portal HEAD 518022c (last code commit 73b2129).
Details, what was next and a list of disk to reclaim later: `local/handover-2026-09-26-portal.md`
("PARKED" section).

**Landed overnight (700 engine lib tests, short walked Riverwood tour PASSED):** audit leftovers
impl-265 (one doorway-plane owner, one heading helper, test-only `from_args`), impl-268 (retried
interiors not reported failed, one grid-offset owner, Full commits obey the time budget), impl-271 +
73b2129 (every grid-offset copy through streaming's helpers); Phase 2's audit-engine merge (read-only
DB opens, validate-once cell cache, safe asset paths); 1a97d0a water reflection only for water a
camera draws (it ran inside closed houses); impl-267 look-out bench states (`closed-out`,
`open-out-view`, `tools/bench/portal_bench_doors_out.json`). Phase 1's `prep/portal-zstd` merged
(2d5db56) and `C:\Modding\SkyrimConverted` reconverted to **schema 28** (hard-linked extraction,
KTX2 Zstandard; complete, integration passed); release and quick engines rebuilt for it.
Workers: DeepSeek (impl-265, 267), Claude subagents (impl-268, 271), MiMo (audits 262-264,
research 266/269/272, pitfalls digest), Muse (review 265.1).

### 2026-09-27/28 (night) - Claude [phase1] Phase 1 Dev (first session: output size, converter speed, contributor tooling)

**Set-up:** the user accepted the Phase 1 rules (worktree `C:/Modding/wkj-phase1`, `phase1/*` from main,
targets on D:, owns crates/converter + dummy-content). Plan: `local/research/phase1-plan.md` (items 1-12,
contributor experience C1-C11, upstream plan items U1-U6). Research: `local/research/phase1-*.md`.

**Branches (all green, handed to Upstream Dev; publishing only with the user's OK):**
| Branch | Head | What |
|---|---|---|
| `phase1/vfs-hard-links` = PR #51 | f14c5a9 | vfs + cache one file (-25 GB measured), NTFS 1,024-name spill |
| `phase1/ktx2-zstd` | e0496e7 | KTX2 zstd (textures 19.3 -> 13.0 GB), schema +1 with recompress-not-re-encode migration, bevy zstd_rust |
| `phase1/fast-textures` | 2f507dd | `--texture-mode native` (BCn + RGBA8; 200 tex 286.9 s -> 1.1 s; FLIP <= 0.042); issue draft `local/pr-drafts/30-...` |
| `phase1/cache-dir` | 56b6bdd | ingestion cache beside the output (`<output>.cache`, migration) - issue first |
| `phase1/cheap-io` | b583f3b | no per-file fsync in staging, extractor hashes reused, one plugin merge, GLB length check |
| `phase1/decoder-bounds` | 0914347 | on #43: DDS mips, EsmReader header, BA2/zlib ratio bounds |
| `phase1/converter-progress` | ba83c96 | live status line, ETA, Ctrl+C resume (needs #45) |
| `phase1/quick-profile` | 67e7af4 | `[profile.quick]` + CONTRIBUTING (the user said yes) |
| `prep/portal-zstd` | 28c2d18 | for Portal Dev: #51 + zstd on portal, schema 27 -> 28 (workspace 1070 green) |

**Open:** the clean zstd timing A/B at 08:30 (pair ready: D:/Modding-overflow/p1-timing/SkyrimConverted
vs -raw, same engine; Phase 2 chains a 3-min run after); a timed native reconversion for the issue draft;
Wednesday: U1-U6 and plan items 8-9. **Running:** DeepSeek impl-824 (ProgressEstimate, window step A),
MiMo research-825 (first-30-minutes guide). Lessons: never `du` a conversion folder (memory
`stray-du-blocks-publish`); coders need a private target dir (shared-check-target mixed branches).


### 2026-09-26 - Claude [phase2] Phase 2 Dev (project audit and plan; acceptance baseline; parser, cache and BRP work)

**The user's asks:** "audit and make project-wide plan for me to see" (the plan page:
https://claude.ai/artifact/2HSP4u8EVcvXrjCpu9scSm), then "yes to all and go ahead with the plan".
Done from the decisions: leftovers deleted (C: 66 -> 76 GB free), 8 stale branches kept as tags
`archive/phase2/*` (integration, -lod, -lod-shots, -shots, local-shots, render-check,
vertex-alpha-not-opacity, refr-rotation); `integration-night` is the one integration tree. Upstream
posts to realfakenerd are paused by the user (the fork is free, except the 7 open-PR head branches).

**Acceptance baseline on upstream main ddb6d70** (`C:/Modding/wkj-tools/acceptance-main-60s`,
`stress-ab`): functionally clean (preflight, asset closure, runtime audit, 15 robustness tests, 11
scenarios). On a quiet machine every scenario passes at 60 s (stress p95 ~12 ms); an earlier 18.4 ms
"fail" was cargo builds running beside it, so **a timing booking holds cargo too**. Still missing for
`accepted`: quality and release gates, 3 full-length reps (~3.4 h), a human visual sign-off. The
script needs pwsh 7 (fixed below).

**New local branches, reviewed, green, held** (Upstream Dev has the list):
`parser-never-panic` (591328a VMAD 96 GiB abort fixed, a666ebc proptest never-panic properties, fb7f928
bounded decompression), `scripts-utf8-bom` 232a286, `cell-cache-validate-once` ac91f54 (cell_commit
2.86 -> 0.37 ms), `remote-dev-flag` f8a5c03 (BRP behind feature `remote` + `--remote`, 127.0.0.1).
Tools: `C:/Modding/wkj-tools/flip_shots.py` (FLIP before/after of `--shots`; same-build noise <= 0.0054,
threshold 0.02; animated models such as the mill wheel need animation frozen in `--shots`),
`stress_ab.py`. All written by Claude subagents (impl-593..597, analysis-596) and reviewed by Claude.

**Running:** Claude impl-597, shadowless second sun for the reflection view
(`phase2/reflection-shadowless-sun`, worktree `C:/Modding/wkj-p2-refsun`), then its timing A/B.
Worktrees to discard when done: `wkj-p2-proptest`, `wkj-p2-brp` (+ `wkj-p2-brp-target`),
`wkj-p2-refsun` (+ target). **Next:** cheaper crossing frames (spread despawns, rebase without
unloads), the 8 standing audits on one day's DeepSeek budget, `--shots` animation freeze, bevy_brp_mcp
(needs the user's install and MCP config).

### 2026-09-26 - Claude [phase2] Upstream Dev (merges, lighting sent, upstream posts paused)

**Upstream:** #34 and #36 MERGED (main ddb6d70): 2 of 3 for the send hold. Open ours: #29 (4dcfb69,
realfakenerd merged main in; likely next), #30, #31, #38 (d21f357: fixed realfakenerd's broken main
merge), lighting #39 (fixture), #40 (converter, world DB 3->4, head 5a89364) and #41 (engine
`--lights`, head 358450a: XRDS cap + CodeRabbit fix). HazielMagallanes (contributor) reviewed
everything; his notes were fixed and answered. #27 (TaylorTurnerIT) is a stalled draft: the user
chose to WAIT on it for base types (Whiterun's water); ours (renderable-base-types) stays unsent.

**The user's rules now (memory `upstream-send-hold`, `feedback_public_posts`,
`feedback-self-contained-asks`, `feedback-path-only-commits`):** **posts to realfakenerd's
upstream are PAUSED** (comments, issues, PRs, bot replies; pushing to an open PR head counts).
The user's own fork may be updated freely. Every ask to the user is self-contained and labelled
🤖 AI / 👤 human / 🚫 no message. Commit named paths only in the shared checkout.

**Prepared, held:** texts 11 prepass, 12 sky-dome, 13 sky-fog, 18 LOD issue (the user said yes to
the issue before pausing); first sends when allowed: cell-name-nul, io-pool-stack, parser-never-panic,
sky-dome, sky-fog, reflection-land-only (its citations need a websearch worker, DeepSeek pacing
permitting). #30 (a) follow-up written (6bfee13). History paths: send as a fresh squashed commit.
public/portal re-exported by Portal Dev (ff96ac2). Full list: `docs/pr/upstream-queue.md`.

**Tools:** `local/pr-drafts/preview.py` (lists every post and a count at the top); shared warm
target `C:\Modding\target-upstream`; `cargo nextest` for CI-matching tests. **Running:** the
90-minute upstream check (session crons). No workers, no worktrees.

### 2026-09-26 - Claude [portal] Portal Dev (portal cost, door leaf, flames, benches)

**State:** portal HEAD c5cdad9. 637 engine lib tests pass; the full Riverwood tour passes 8/8. No workers running. The details are in `local/handover-2026-09-26-portal.md` and `local/team/2026-09-26.md`.

**Landed today (the user affirmed the whole queue):**
- impl-227: the doorway depth composite is the default.
- 4daa459: suns cast shadows only while they give light. -1.24 ms looking through an open door, measured.
- impl-231 through impl-239:
  - the bevy preset's exposure;
  - the tour's screenshot;
  - light spill measured at the threshold (still opt-in);
  - flames: palette clamp and soft fade;
  - the door leaf: one canonical leaf per pair in every state;
  - the doorway floor anchor is re-probed until stable;
  - the door model double load (partial);
  - billboard bounds validation;
  - the live AA-switch crash (a Bevy stale motion-vector pipeline; `--graphics-cycle`).
- Also: an FPS counter.

**Bench:** `tools/bench/portal-bench.ps1`:
- `-Variants 'a=','b=--flag' -Repeats 2`, interleaved, with spread, about 2.25 min a run;
- render-thread columns from Phase 2's render_timing (fe5e794);
- timing runs open centred, with `--run-label` titles;
- **every automated tool as quick as possible** (the user's project-wide rule, above).

**Next:** animated models rejected by bounds (the Riverwood mill wheel), Whiterun's double gate, frame rate toward 160 (the render thread is the limit), the user's project-plan §3 items.

### 2026-09-25 (midday) - Claude [phase2] Upstream Dev (sends, reviews, preview tool)

**Role:** the user renamed this session **Upstream Dev**. It watches upstream and prepares and
organises sends. The state lives in `docs/pr/upstream-queue.md` (open PRs, decisions, send order),
`local/pr-drafts/` (texts) and the team log.

**Upstream now:** our open PRs are #29 (dcf48c3), #30 (3cd8858), #31 (9cbeee7), #34 (issue #33,
phase2/model-bounds 157d0f8), #36 (issue #35, default-water-height 9d6508a) and #38 (issue #37,
terrain-seam-weld e91db89). Every CodeRabbit finding is answered, and all are fixed except #30 (a),
which becomes a later follow-up PR. The combined branches for whichever PR merges second are
`phase2/pr30-31-combined` and `phase2/pr34-38-combined` (fc9e834, which lacks the doc-only e91db89).
No maintainer activity since #25.

**The user's rules for public text** (memory `feedback_public_posts`): an AI label at the top of
every post; the user sees a rendered preview before anything is sent
(`python local/pr-drafts/preview.py <draft.md>...` writes the exact text to post to
`local/pr-drafts/out/` and renders `preview.html`; post from those files). Replies to bots may be
posted directly, labelled as AI. Python `markdown` is installed.

**Waiting on the user:** the preview of drafts 7-9 (reference-shots: issue + PR; lights: a comment on
#24 + a fixture PR + a converter PR, world DB 3 → 4; suggested: the comment and fixture now, the
converter held). Before sending, re-run the checks on those branches **after Portal Dev's s23
reconversion is done** (no builds until Portal Dev says "done"). Also waiting: the roadmap-page
Upstream track (yes/no), the Chef Discord reply, the fork history holds, and public/portal (after
Portal Dev's hearth check).

**Running:** the 90-minute upstream check (session cron; it can be skipped while the session is
busy, so run it by hand when that happens). No workers, no cargo.

### 2026-09-25 (morning) - Claude [phase2] Upstream session (PR watch, CodeRabbit fixes)

**New session, set up by the user: "Upstream collaboration and reviews".** It watches
Mudcrab-Team/mudcrab: PRs, reviews, CI and issues. Phase 2 Dev handed it #29/#30/#31. Memory:
`upstream-watch-session`. Its worktrees are `C:\Modding\wkj-upstream` (pr31 fixes),
`wkj-upstream-29` and `wkj-upstream-30`; each has its own `target/` and can be discarded once pushed.

**Upstream state:** #29-#31 CI green, behind main but mergeable, no maintainer comments. #26 (.lip
tools) merged, so origin/main is 417a262. #27 is still held by its author (head 6834e5e). Issue #24:
nothing new.

**CodeRabbit triage** (DeepSeek review-570, reviewed): `docs/pr/coderabbit-triage-29-31.md`. The user
said public-facing code takes priority, so Claude wrote the fixes. They are local and tested; see
the Status table in that file: pr29 9b6611d+dcf48c3, pr31 e87d894, #30 (c) 8d6a807 on the combined
tree. The glue commit does NOT fix #30 (c), so `integration-lod-shots` needs 8d6a807 as well.
`docs/pr/pr27-comparison.md`: `vertex-alpha-faithful` supersedes our older branch, and the old
SLSF1 note has been corrected.

**Waiting on the user:** permission to push the fix commits to the frozen #29/#31 heads, where the
#30 (c) fix goes, and the draft CodeRabbit replies (in the triage note; the user sends them). F30-a
needs a separate follow-up PR later.

**Running:** an upstream check every 90 min (CronCreate, this session only; it ends with the session).
No workers, no cargo.

### 2026-09-25 (morning) - Claude [portal] (team working made standard; the hearth's grey sheet found)

**The user's asks:** make the team way of working standard ("Working together" above, with the
user's later changes: resource claims by message and log, the Claude plan shared, QnA the user's own
session); distant LOD in the portal demo (yes).

**Grey sheet over the hearth flames:** a hide test (`local/gallery/glow-check-compare.jpg`) showed
it is the `Flames:0` card, not Glow. research-701 (QnA's NIF decode) plus the GLB: the converter
dropped every BSTriShape's vertex colours (the flames' alpha is the palette's alpha row) and the
`Flames` NiBillboardNode, and did not export falloff. Phase 2 Dev fixed all three plus the colour
controller (see their entry). Engine half on portal, all inert until the data arrives:
`302516e` (emissiveColor channel playback, vertex alpha in the alpha row, additive cards fade to
black in fog) and `19e5fc7` (billboard nodes, `billboard.rs`; Use_Falloff). softFalloffDepth is not
played (needs the depth prepass). Reviewed and approved Phase 2's `39c3497` (vertex-alpha-faithful).

**The bundle** (`bundle/portal-s23` in `C:\Modding\wkj-phase2`, Phase 2 Dev's) is **not ready**: it
still needs vertex-alpha-faithful, nif-billboard-nodes, 4546e6e, 227da41 and the schema bump 22->23.
When Phase 2 Dev hands it over: merge it, delete `render.rs`'s `force_opaque_vertex_alpha` **in the
same commit** (before the reconversion it would bring back see-through trees), announce the
reconversion by message and log, reconvert, then check the Sleeping Giant hearth
(`local/gallery/glow-check.json`) and the Whiterun houses' new vertex shading. The LOD engine half
merges after that (big conflicts: portal lacks upstream main since #23).

**Open (portal):** interior lighting (`local/reference/look-gaps/interior-lighting-design.md`),
imagespace colour grading (`local/research/imagespace-colour-grading.md`). The Claude week is at 84%:
subagents for short lookups only. **No workers running.**

### 2026-09-25 (morning) - Claude [phase2] (hearth-flame converter fixes; portal reconversion bundle started)

Worked with Portal Dev on the grey sheet over hearth flames (team log `local/team/2026-09-25.md`).
The upstream PRs #29-#31 moved to the "Upstream collaboration and reviews" session (heads frozen;
its CodeRabbit triage `docs/pr/coderabbit-triage-29-31.md` finds the glue 0fdc08a makes #30 (c) - a
lost prune record, output still correct - reachable; integration-lod-shots carries that too).

**Landed on local `phase2/*` branches (fmt, clippy, tests green; nothing pushed):**
- `vertex-alpha-faithful` 39c3497 (from main): the vendored exporter dropped vertex colours on every
  static BSTriShape (the flame cards' fade and palette row: the grey sheet; baked shading on SSE
  meshes). Now kept and published per the shaders' rule: no SLSF2_Vertex_Colors → dropped;
  Tree_Anim → alpha 1 (wind); else alpha is opacity. Supersedes `vertex-alpha-not-opacity`.
  Reviewed and approved by Portal Dev.
- `nif-billboard-nodes` 11b2e2b, e3d30a6 (from main): NiBillboardNode parsed and kept (the flame
  cards lay flat without its transform), marked with node extras `openSkyrim.billboard`.
- `material-publication` 4546e6e (effect falloff + soft depth in OPEN_SKYRIM_material, as stored:
  cosines) and 227da41 (colour controllers → `emissiveColor`/`specularColor` channels with
  `components: 3`; the chain walk no longer stops at a colour controller). Checked on the real
  fireplace NIF against Portal Dev's key dump.
- Portal Dev's engine half of all of this is on portal (19e5fc7, 302516e), inert until the data.

**In progress: `bundle/portal-s23`** (in `C:\Modding\wkj-phase2`, from portal 19e5fc7): the one
reconversion for portal. Done: LOD converter half merged (60e9fff, world DB schema 6; converter,
shared, dummy-content tests green). **Left:** merge `vertex-alpha-faithful`, `nif-billboard-nodes`,
cherry-pick 4546e6e + 227da41 (dummy-content nif.rs will conflict: add the colour writer to the
union), bump CONVERTER_SCHEMA_VERSION 22 → 23, full workspace tests, hand to Portal Dev (they delete
`force_opaque_vertex_alpha` in the same commit and announce the reconversion). The LOD engine half
merges later without a reconversion (portal lacks upstream main since #23: expect big conflicts).

**Also open:** distant tree billboards 2-4x too bright and grey; the vanilla DistantTree lighting
research is websearch-703 (QnA started it; report at
`C:\Users\Brannock\AppData\Local\Temp\claude\C--Modding-MudcrabProject\d9ee62b9-a4a7-4ec8-af9c-bdc53350c97e\scratchpad\treelod\websearch-703.out`),
unread. QnA is no longer the research desk. **No workers running from this session.**

### 2026-09-24/25 (night) - Claude [phase2] (look fixes, distant LOD extended, audits, timing)

**Joint night with Portal Dev** (shared log `local/overnight-2026-09-24.md`; QnA researched water,
sky, LOD hiding, refraction and terrain-LOD normals in `local/research/`). Everything is on local
`phase2/*` branches; nothing pushed, and the send hold stands. `phase2/integration-lod-shots`
(`6aa71fa`) holds all of it: converter, engine, dummy-content and vendored-parser tests green.
Morning report for the user: `local/morning-report-2026-09-25.md`.

**Landed (branch, key commit):**
- Distant LOD, terrain (`lod-engine-terrain`): clipped under loaded full-detail terrain and shaded
  from its model-space normals (e75704b); the clip is a pipeline variant used only on blocks that
  touch a loaded cell (0d71cd7).
- Distant LOD, objects (`lod-mesh-containers` 9959944: per-cell segments kept; `lod-engine-objects`:
  exclusive quadtree, seamless handover, loaded-cell segment hiding, churn fix) and trees
  (`lod-engine-trees`: billboards batched per type, hidden per loaded cell, lit evenly).
- Water (`water-look`, `water-colours`), terrain normal maps fixed (`terrain-normal-maps` ba85290),
  heat-haze/refraction exclusion and material animation export with static UV transform
  (`material-publication`; Portal Dev plays it: flames scroll), reconversion hard links for vfs
  and the ingestion cache (`staging-hard-links`), strict-bounds precision (`bounds-precision`),
  audit fixes (`audit-converter`, `audit-engine`), archive/plugin precedence (`asset-precedence`).

**Timing:** the clean re-time (03:30, alone) passes all scenarios: rural 161 fps, with all LOD kinds
134 fps; the stress fly-through with LOD is p95 15.8 ms (19.3 and failing before). Results in
`C:/Modding/wkj-tools/retime2-2026-09-25`. An earlier re-time was spoiled by four runaway
`find /` scans left by subagents (memory `subagent-midtask-messages`).

**Upstream:** PR #27 (TaylorTurnerIT) overlaps renderable-base-types, vertex-alpha-not-opacity and
the schema-15 bump: comparison and recommendation in `docs/pr/pr27-comparison.md`, for the user.

**Open:** a one-frame LOD invariant warning after a camera teleport; tree billboard brightness;
the GUI launcher lacks Skyrim.ini/Plugins.txt discovery; the sky (QnA's `sky.md`). DeepSeek coders
are held by budget pacing until Fri 22:39; Claude subagents took the coding.

**04:40 upstream:** PR #23 merged (0c4592b). Local `main` fast-forwarded; `main` merged into the
LOD chain (one spec-doc conflict) and `renderable-base-types` (#23's interior test updated for
exported DOOR models); every `phase2/*` branch merges cleanly with origin/main, integration green
(440 tests). Whether the send hold lifts is the user's call.
**06:30:** PR #25 merged too (1f0a40e; main e003f67). main merged into `integration-lod-shots`
(upstream's accepted rotation code kept; 447 tests green). The #25 follow-up is re-cut as
`phase2/refr-rotation-followup` (3566737) for the user to send or not.
**Morning:** with the user's approval QnA opened upstream #29 (`refr-rotation-followup` 3566737), #30
(`pruned-texture-complete` 3cd8858) and #31 (`resume-staging-validation` 17d892f). These branches are
live PR heads: frozen unless the user OKs a change. #30 and #31 conflict with each other (`cache.rs`,
two tests at one spot; keep both), and the second to merge also needs the glue commit 0fdc08a. Local
`phase2/pr30-31-combined` (main + #30 + #31 + glue) holds the resolution and passes fmt, clippy and
the converter tests.

**No workers running.**

### 2026-09-24 (night) - Claude [portal] (the look gaps, worked with Phase 2 Dev; morning gallery)

**The user's asks:** a ranked list of how the demo's look differs from Skyrim
(`docs/research/look-gaps-2026-09-24.md`, 61 graded reference shots), then an overnight plan with
Phase 2 Dev working as one team, progress pictures, and pretty screenshots for the morning. Shared
log of the night: `local/overnight-2026-09-24.md`.

**Fixed on portal (each measured on the reference shots, tuning fitted on half and checked on the
other half):** dark city worldspaces take their parent's lighting (`7585b7d`); the sky fill is a
tenth of the sun, not half (`880a3f4`); shadows take 18 % of the sky's hue, no longer green
(`fe7d24e`); multi-pass snow materials are not drawn as a black layer - the Alftand ice blotches
(`aac5989`); effect shaders draw their greyscale texture through their palette (`b54dced`); glows
are capped under the tonemapper's white - Blackreach (`f4c40d7`); the city LAND edges past the weld
bound are drawn as authored (`80bf6de`, mirroring Phase 2's `4fe8f6e`). From Phase 2, cherry-picked
or merged: effect-shader emissive (`b8f152a`), default water height (Morthal's lake), refraction
planes excluded (the fire heat haze), the water look (white sheet gone), staging hard links for
reconversions, scripts reading the schema from source. Audit findings CC-01/02, RM-02, HT-01/02/04/05
closed.

**Data (at 23:00):** `C:\Modding\SkyrimConverted` was converter **schema 21** (complete, integration passed;
space_lighting also carries fog far colour, fog max and light fade). `engine-portal-fit.exe` is
schema 21. Tours on it: Riverwood 8/8, Alftand 4/4, 0 FAIL. Fallback copy `SkyrimConverted-s19`
(hard links; its vfs and ingestion cache removed after they filled C: at 20:40 - see memory
`reconversion-disk`). `target/debug` and `target/release` live on `D:\Modding-overflow` behind
junctions (the user allowed D:).

**Built, not switched on:** Skyrim's `1 - (d/r)^2` point-light falloff (`light_falloff.rs`, a
load-time patch of Bevy's attenuation line, QnA's research); `lights::SKYRIM_FALLOFF` is false
because alone it did not improve interiors, whose fog fills them. Next for interiors:
`local/reference/look-gaps/interior-lighting-design.md`.

**Open:** the fire cards still read as pale sheets (atlas animation, palette orientation); sky
(Phase 2); colour grading (QnA's `local/research/imagespace-colour-grading.md`: grade after the
tonemap, per weather and cell); distant land (Phase 2's LOD branches, not merged); a portal is not
drawn past ~600 units; a shots-run door once rendered closed (flaky).

**After midnight:** interiors draw their fog in the record's far colour (`7d89ae6`, Candlehearth's
cream haze gone); merged Phase 2's `water-colours` (`eea4448`, per-water colours; the DB was
re-exported at schema 21, integration passed, tours pass) and `audit-converter` (`ba3e5d9`, resolved
per Phase 2's notes: panicking assets become failures, the schema stamp comes from the shared
constant); the engine half of animated shader values (`1bc0be1`: flame atlases scroll from
`OPEN_SKYRIM_material_animation`, inert until Phase 2's converter export lands, which needs a
schema bump and a reconversion).

**Later still (01:00-02:30):** schema **22** published: Phase 2's material-animation export
(cherry-picked 43bda16, 00743f8, bd6e439, 9ef3104: `OPEN_SKYRIM_material_animation`, static
uvOffset/uvScale, tangents [outgoing, incoming] per NifSkope) plus the engine half (`1bc0be1`):
**the hearth flames scroll** (GIF `local/gallery/sleeping-giant-fire.gif`). Effect cards are unlit
and drawn at 8x (`d7f4e5c`); a grey translucent sheet over the flames is still open (suspects: fog
on the blended cards, or the additive Glow card). Audit ES-02 closed (`try_despawn`). Tours on
schema 22: Riverwood 8/8, Alftand 4/4; `engine-portal-fit.exe` is schema 22.

**Morning gallery:** https://claude.ai/artifact/1QMmmU5jKwDhdnhKUynJpZ (private; JPEGs in
`local/gallery/final/`). **No workers running from this session.** Phase 2 Dev still has coders
and an audit-converter merge in flight; take their pre-resolved merge, not the branch.

### 2026-09-24 (overnight) - Claude [phase2] (conversion, audit, fixes, reference sheet, distant LOD)

**Upstream:** realfakenerd replied on #24 (2026-09-23 21:02): be patient, the roadmap is being
reworked, management changes and a Discord server are coming. Sends stay on hold. PR #23 got the
review fixes and a reply (sent with the user's OK); #25 has a local follow-up commit (roadmap doc,
comment, test) that is NOT pushed.

**Conversion:** `C:/Modding/SkyrimConverted-upstream` (schema 14, fresh, 20:20-07:14):
complete, 242,969 converted, 0 skipped, 511 textures pruned, integration report passed. Converter
built from `phase2/pruned-texture-complete`, so this is PR 3's real-install evidence.

**Audit and fixes:** lead-515 (12 reviewers) audited every branch
(`docs/pr/phase2-branch-audit.md`); impl-520..530 fixed every branch (I read each diff, fixed
several test/clippy slips myself). Integration branch `phase2/integration` passes all tests.

**Distant LOD:** step 1 `phase2/lod-mesh-containers`, step 2 `phase2/lod-database` (world DB
schema 4; review-535 found ~2,000 offset-grid blocks dropped, fixed), step 3
`phase2/lod-engine-terrain` (engine, `--lod`, origin-relative lookup; never run on a GPU and needs
a schema-4 database). Tree LOD decoded (`docs/research/tree-lod-layout.md`).

**Reference sheet:** `docs/research/upstream-reference-sheet-2026-09-24.md`, rendered with
`phase2/render-check` (engine-side fixes + local `--shots`). Biggest gaps: no trees/plants/Guardian
Stones and see-through Alftand ice (both fixed by the schema-15 branches, which need a schema-15
conversion: ~70 GB staging, disk is short), no sky/fog, dim light, no Blackreach glow.

**Other findings:** Dragonborn "missing" refs are a tree-LOD FormID-slot mismatch, not a bug;
the 278 "lost" records are deletions by later plugins (not a bug); `plugins.priority` is not the
FormID slot for plugins after the first light plugin (small upstream candidate).

**No workers running.** **Next:** decide with the user on disk for a schema-15 conversion (to see
trees and materials), whether to push the #25 follow-up, and what to do about the Discord.

### 2026-09-23 (night) - Claude [portal] (doorway alignment landed; PortalPlugin steps 1-5)

**Doorway alignment (the user's Sven's House report) is merged and confirmed by the user** on "a
bunch of random transitions" (`2b2687b`). impl-103 anchors the map on the two doorways; the lead's
review corrections are in (gate to the placed doorway centre, height cap = step height, destination
door hidden only outside the active space). Found only on the real assets, both fixed: an exterior
doorway's grid came from its *cell* (Riverwood's doors are in the persistent cell, grid 0,0, so
the wrong part of Skyrim streamed), and a crossing landed inside the closed far door (impl-152
opens it at the swap). Non-obvious transitions (ladders, trapdoors, cave mouths) keep the `XTEL`
landing and are **parked by the user** until they get their own design.

**PortalPlugin refactor steps 1-5 merged** (impl-153, `cdefe05`): one `add_plugins(PortalPlugin)`
in `run()`, `PortalOptions` in `config.rs`, `main.rs` byte-identical to `main`. The worker could not
run cargo (harness denied it); the lead fixed three tests and fmt. Both tours pass on the final
tree: Riverwood 8/8, Alftand 4/4, 0 panics. **Step 6** (atmosphere and terrain ring out of `app.rs`)
is next; it needs builds, so it waits for the end of Phase 2 Dev's conversion.

**Also:** `docs/roadmap/backlog-engine-and-portal.md` (every idea and note, split main engine /
portal; main-engine half sent to Phase 2 Dev); research-151 incremental conversion (handed to
Phase 2); reference sheets `local/reference/cmp-103/` and doorway shots
`local/reference/doorway-152/`. `target/release/engine.exe` in this checkout predates the refactor
merge: rebuild before the next play session.

**No workers running.** Phase 2 Dev's upstream conversion runs overnight: no builds, links or new
worktrees from this checkout until they announce the end. **Next:** step 6; then the backlog
(water white sheet first, once Phase 2 Dev says whether it is fork-only).

### 2026-09-23 (evening) - Claude [phase2] (Phase 2 track running; upstream sends on hold)

**Upstream:** issue #24 (lighting scope) and PR #25 (REFR rotation) opened; #23 still open. The user
put a **hold** on sending anything more until maintainers respond (memory + `docs/pr/two-tracks.md`).

**Ready and held** (all from `main`, CI-equivalent checks run where noted), in send order:
`phase2/pruned-texture-complete` (checks pass, body `pr-03`), `phase2/empty-model-bounds` (checks
pass, body `pr-04`), `phase2/shared-converter-schema` (checks pass), `phase2/material-publication`
(schema 15; after empty-model), `phase2/vertex-alpha-not-opacity` (after material),
`phase2/terrain-samplers-weights` and `phase2/terrain-seam-weld` (need a GPU run),
`phase2/sun-shadow-cascades` (needs a GPU run and a benchmark). Workers impl-083, 501, 502,
504-508 (DeepSeek) wrote them; Claude reviewed every diff. Audit: `docs/pr/phase2-fix-audit.md`.

**Coordination with Portal Dev:** rules in "Two tracks" and "While working" (parallel engines;
timing runs alone; no reader in a folder being published; no new builds during a conversion).
Portal reconverted `SkyrimConverted` to schema 19 (complete, 0 skipped).

**No workers running.** **Next:** the upstream-schema conversion into
`C:\Modding\SkyrimConverted-upstream` (about 70 GB; 117 GB free now) so the GPU-dependent branches
can be checked, announced to Portal Dev first; then GPU runs of the terrain, shadow and material
branches. Check #23/#24/#25 for feedback before proposing any send.

### 2026-09-23 (afternoon) - Claude [phase2] [portal] (the work split into two tracks)

**The user's ask:** keep Phase 2 contributions and the portal feature clearly separate, with
Phase 2 first and done as upstream's README describes. The user agreed to all of the plan's
decisions, including lighting going upstream (issue first) and offering the portal work later as a
Phase 4 proposal. The plan is `docs/pr/two-tracks.md`; the rules are the "Two tracks" section above.

**Done:** `deepseek-setup` renamed to `portal`; `main` fast-forwarded to `origin/main` (`2105bfd`)
and tracking it; worktree `C:\Modding\wkj-phase2` on `main`.

**Blocked on the user:** disk. `SkyrimConverted` is 73.8 GB, C: has 43 GB free, so the
upstream-schema conversion (`SkyrimConverted-upstream`) does not fit yet.

**Seen in passing, not mine:** `local/conversion-s18.out.log` ends `complete: false` with 511 skipped
inputs; `s18b` is later and small. The untracked `tasks/deepseek/*` briefs and `scratch-069/` belong
to another session. The eight "modified" engine files are line-ending noise (`git diff` is empty).

**Next:** Phase 2 - rebase PR 2 (rotation) onto `main` as `phase2/refr-rotation`; open the lighting
issue after the user confirms the draft in `two-tracks.md`. Portal - the `PortalPlugin` refactor
(`portal-` worker task).

### 2026-09-23 (day) - Claude (the Riverwood demo, and two rendering bugs it exposed)

**The user's ask:** "start on the riverwood demo", off research-055's data
(`docs/design/riverwood-demo.md`). Section 8 of that document is the part written after the demo
was built and run, and is the part to trust about behaviour.

**The demo works.** `--demo riverwood --walk` starts on the Helgen road south-west of the village;
`play-riverwood-demo.cmd` is beside the Blackreach launchers. `--demo-tour` is no longer
Alftand-only (impl-056): a route is keyed off the `--demo` name, and Riverwood's is eight doors -
four houses, each entered and left again. The passing run is `local/demo/riverwood2/tour.txt`:
**8 crossings of 8, all by walking, 0 dark frames in 2,056 captured frames.** The seamless
crossing now holds in daylight and in both directions, which the Alftand route could not show.
One tour bug fixed on the way: the walk-through's standoffs were 60 and 320 units, and inside
Alvor's House the far wall is nearer than 320; a third, 160, is tried last.

**Two rendering bugs, both dataset-wide, both found by looking at Riverwood:**

1. **The forest was invisible** (research-059, impl-063, `abaa14f`). Pines rendered as a spray of
   dots. My hypothesis - alpha-tested foliage falling through `render.rs`'s `AlphaMode::Blend`
   fallback - was **wrong**, and the worker falsified it properly: the trees convert to `MASK`
   with the NIF's own threshold. Bevy tests `vertex_color.a * texture.a` against the cutoff, and
   Skyrim's tree NIFs carry a `COLOR_0` whose alpha is a shader parameter, not opacity. Fixed
   engine-side at mesh load, no reconversion. Proof: `local/reference/_tree-before-after.png` -
   the canopies fill in and the house, thatch and wall in the same frame do not move. This also
   un-breaks `snow.wgsl`, which multiplies coverage by `in.color.a` under a comment claiming no
   converted mesh has vertex colours; 1,337 of them do.
2. **Everything read wet** (research-058, websearch-060, impl-062 `59f1f20`, impl-064 running).
   `material.rs` mapped Skyrim's Blinn-Phong glossiness *exponent* as a linear percentage. All
   59,268 published materials were wrong; 14,825 were clamped to Bevy's roughness floor. Now
   `(2/(n+2))^0.25`. The other half - the specular mask in the normal map's alpha, which the
   converter drops - is impl-064; websearch-060 settled that the mask scales intensity, not the
   exponent, so it can be a `specularTexture` pointing at the normal map with no derived textures.

**Converter schema 17 -> 18.** impl-064 rides the same bump, so one reconversion covers both. The
first reconversion was running when this was written (`local/conversion-s18.out.log`); a second is
needed after impl-064 lands. **Until a reconversion completes, the engine refuses the assets on
disk** - that is its schema check working, not a fault.

**Also landed:** impl-057, `crates/engine/src/model_animation.rs` - a model's own looping `Idle`
clip on every non-door reference. The mill wheel the Riverwood finale looks at now turns (16.67 s
a revolution, confirmed in the log). The snow catalogue read `material_object IS NOT NULL`, but
the exporter writes a plain 0 for a static with no `MATO`: 9,924 markers, fences and road chunks
against 2,506 real snow statics. Now `> 0`, with a test that fails on the old query (`ad7f138`).

**Reference shots.** `tools/reference/riverwood_shots.json`, twelve poses, ten against a UESP
reference in `local/reference/riverwood/`. All twelve render. research-055 computed the eight
exteriors at ground + 120 (eye height), but UESP's "place" screenshots are free-camera shots from
above the rooftops, so four of them could not frame their reference at any yaw. A sweep of
elevated candidates is `local/reference/rw_sweep4/`; the mill finale has been re-fitted from it.
The inn and the whole-village aerial still need one more pass - `local/reference/rw_sweep2.json`
is written and unrendered.

**Running when this was written:** impl-064 (the specular mask), doc-065 (the demo guide covering
both demos). **Next:** finish the reconversion, re-render and check the wet look is gone, land
impl-064, reconvert again, re-fit the inn and village poses, then a regression run of the Alftand
tour.


### 2026-09-23 (overnight) - Claude (visual gaps, animated doors and seamless crossing, demo for contributors, upstream PR)

**The user's five goals, and where each stands.**

1. **Visual gaps.** lead-026 (4 research + 3 websearch workers) produced `docs/research/visual-gaps-spec.md`. Landed:
   - `23adbb4` glow: HDR + bloom, `EMISSIVE_EXPOSURE = 100` applied only to materials the converter marked with an emissive strength (mushrooms, the Blackreach sun, flames), so Own_Emit surfaces (ice, snowy trees) keep Tamriel's daylight within 0.8%; `intensity_for_radius` capped at a 256-unit reference, which is bit-identical below radius 512 and removes Blackreach's orange ceiling. Claude's holdout: AlftandWorld 1.14x, Blackreach 1.26x, clip 0.66%.
   - `2b69616` empty marker models (Blackreach's two) no longer fail the bounds gate.
   - `26324d5` converter `space_lighting` + `matos` + `statics` DNAM columns, layouts settled from the real bytes (LGTM payload is DATA; inherit bit set = from template; WTHR FNAM interleaves day/night; MATO's last field is an integer flag).
   - `66ee786` engine per-space lighting: backdrop, fog, ambient hue and sun tint per space, destination atmosphere on the portal camera. Record colours are applied as **hue** with the calibrated brightness kept.
   - `cfe4893` directional snow (DNAM/MATO projected material), inert until the data exists.
   - Ice (environment maps) is still open, and so is the calibration of the last two, which needs the reconversion.
2. **Animated doors and seamless crossing.** `f6749e2` converter exports Open/Close clips (all 768 door NIFs decode; 171 animated). `50bc0f1` the door state machine (clips found by name; a leaf whose swing is under 45 degrees hides when open). `49f2df5` the seamless crossing: the player's pose is carried through the door instead of snapping. `b535a30` the two wired together, plus the review corrections. Claude's check on the integrated build: **walk tour 4/4 by walking, 0 errors, 0 dark frames in 811 captured frames**; the frames either side of a swap are the same view (`local/demo/walk-swap-fixed.jpg`). The route's Dwemer doors only twitch 5-9 degrees in the real data; their non-load twins swing 86-88. **Open question for the user:** keep the faithful twitch or borrow the twin's swing?
3. **Demo for contributors.** `cc53baf`: `docs/demo/README.md`, `tools/demo/setup-demo.ps1` (prereqs, build, Skyrim detection, conversion with resume, launcher) and `tools/demo/play-demo.ps1`. Claude ran both dry runs. Nothing published.
4. **The project as a whole.** Upstream `origin/main` (16 commits: `dummy-content`, ADRs, CI) merged locally (`8c21447`, one test-only conflict). `40e0eb5` extends `dummy-content` with an interior cell and a linked load-door pair, so upstream can test doors and portals without Skyrim. research-028: the streaming fixture never exits without a benchmark window - not a ring bug.
5. **Audit and PR design.** `docs/pr/upstream-pr-plan.md` (research-029): 9 PRs, fixes first, with exclusion and cleanup lists. Cleanups done: `410ce31` converter (ADR-0002 `MUDCRAB_SKYRIM_DATA` / `MUDCRAB_CONVERTED_DIR`, no machine paths, no private names), `62c0f7b` + `944250e` engine parts 1-2, part 3 running. Not done: re-cutting the commits into the PR series (the plan's step), which needs the user.

**Blocked on the user:** the reconversion (see the entry below). Until it runs, door clips, per-space lighting and snow have no data, and each is inert by design.

**Process notes.** Every worker patch got an independent DeepSeek review (review-035/040/041/042/045/049) or a full read by Claude; four rounds of corrections were sent back. Discard each ds_impl worktree right after integrating: they seed ~11 GB of `target/` each.

### 2026-09-22 (night) - Claude (rotation fix, distant terrain, closed doors)

- **User asks:** the Ruined Tower was rotated wrongly in the side-by-side comparison, so check rotation and placement. Also increase the render distance so distant terrain shows.
- **Rotation (`933b95f`, Claude, shared contract):** Creation `REFR` angles turn clockwise. Objects are now rotated by `Rx(-x) Ry(-y) Rz(-z)`, the inverse of the old conversion.
  - Yaw was proven from the door links: 88% of doors were consistent against 42% under the old convention.
  - Tilt and order were confirmed by research-023 (`22ad24c`): on 82,731 tilted references the median error against the terrain normal is 8.2°, and every alternative is 19-32°. Scale and position are applied correctly.
  - Objects and the arrival camera now share one convention.
- **Reference poses:** re-fitted by research-024 (`6804978`, `9d6d844`): 1 matched, 15 close, 10 area only. The Alftand exteriors are shot from the north side of the ravine. The Ruined Tower door faces east.
- **Closed doors (`f9c439e`, impl-022):** a load door draws closed unless the portal is rendering through it.
- **Distant terrain (`5211bad`, impl-021 plus a review round):** a terrain-only ring out to `--terrain-radius` (default 8), with distance fog and `NotShadowCaster` on the ring.
  - Frame rate on the Pale view: 180 FPS without the ring, about 97 FPS at radius 8 (shared GPU, rough).
  - Claude's change: benchmark runs keep the ring off unless `--terrain-radius` is given. A default benchmark passed with 0 commit-budget violations.
  - DeepSeek review-025 verdict: accept after fixes.
  - Unexplained: Claude's own `--streaming-fixture --terrain-radius 8` run did not exit within 400 s. The worker's fixture run validated, but the ring terrain was not exercised there. Check this before the Phase 2 runs.
- **Harness:** `tools/ds_impl.ps1` synced to skill v1.3 (`54645f9`).
- **Queued, not started:** impl-023 (`tasks/deepseek/impl-023-door-front-from-xtel.md`). The portal takes a door's front from the model axis, which is wrong for many door models (the tower door opens only from its back). The fix takes the front from the return link's `XTEL`.
- **Open rendering gaps:** glow/emissive, per-cell lighting templates and interior fog, the snow shader (roofs render bronze), overcast sky and weather, ice reading as rock, `BlackPlane01` voids, two Blackreach models that fail validation.
- **No workers running.**

### 2026-09-22 (late) - Claude (reference screenshots and matched renders)

- **User ask:** collect Skyrim screenshots of the demo's locations, then render the same compositions in Mudcrab for comparison.
- **References:** 26 UESP screenshots (CC BY-SA; download approved by the user) in `local/reference/uesp/` (git-ignored, `SOURCES.txt`).
- **Built:** design `docs/design/reference-shots.md` (the shots-file contract). impl-017 `--shots <file> [--shots-out <dir>]` renders exact poses once streaming settles (`5e53712`, DeepSeek, verified at runtime by Claude). research-018 wrote the first poses, `tools/compare_shots.py` and the mapping/fitting scripts (`adbea02`, `4050696`).
- **First renders:** `local/reference/cmp1/`. The Ruined Tower matches; most other poses are rough (walls, and some pitch signs flipped).
- **Found:** the Alftand01 arrival is washed out to white at eye height. The suspect is the stopgap camera lantern (3e9) in `update_atmosphere`. It showed once crossings landed at eye height (`4b1dd7a`).
- **Done:** research-020 refined all 26 poses by rendering: 1 matched, 12 close, 13 area only (`036efbe`; compare-sheet fix `64129bb`). impl-019 calibrated the lighting (`efa5351`):
  - the lantern is removed;
  - Bevy's 4π on point lights was missing from `lights.rs`;
  - the ambient levels and colours were re-measured.
  Claude's holdout check on the refined poses: Alftand02 is 1.8x too dark (it was 8x), AlftandWorld 1.2x, Blackreach 1.5x too bright; Tamriel is unchanged. `.deepseek-agents.json` now has `implAllowTools` with `cargo build` and the release engine.
- **Renders:** refined poses with the new lighting are in `local/reference/cmp-lit/` (with `_sheet.jpg`). Measure with `python tools/research/exposure_stats.py --shots <file> --render <dir>`.
- **Rendering gaps the comparison found** (in `docs/research/reference-shot-poses.md`):
  - no emissive/glow (Blackreach);
  - no per-cell lighting templates (`XCLL`/`LTMP`) or fog, so the lighting is flat;
  - ice reads as rock;
  - no snow shader or weather;
  - black voids (`BlackPlane01`?);
  - two Blackreach models fail validation (`WispAmbush`, `FrostSpiderAmbush01`);
  - one 4334-radius `FalmerCityLight02NS` floods the Blackreach ceiling orange.
- **Next:** the user picks the order. Glow first is recommended.

### 2026-09-22 (day) - Claude (terrain textures, see-through meshes, open doors with portals)

- **User asks:** work on terrain and terrain textures; doors not rendered (or open) so you can see into the next interior; fix objects that render transparent or misplaced.
- **Merged (DeepSeek, reviewed and integrated by Claude):** impl-009 portals + open load doors + pre-streamed cells kept out of the active space (`f219a05`); impl-010 terrain - root cause of the stretched textures was the ClampToEdge sampler on tiled layers, now Repeat; bilinear layer weights in the material uniform (`0ece570`); impl-012 converter - vertex alpha no longer means blend (2,632 materials were see-through), blend factors kept, EditorMarker shapes dropped (`b4ce0d9`). research-011 (`bc54a17`) diagnosed the transparency/misplacement classes.
- **Claude fixes found only at runtime:** portal startup ordering panic; WGSL u32->f32 in terrain.wgsl; portal quad was a floor-oriented `Plane3d`; every crossing arrived facing the return door (`arrival_camera_rotation`, `a49b4df`). Verified with `--demo-tour`: 4/4 crossings, walk test, and a doorway screenshot showing AlftandWorld through the Alftand02 door (`local/demo/portal5/02-door.png`); the tour now saves the portal camera's own image per door.
- **Harness:** `claude.exe` lives under the MSIX package folder, so launches need the updated skill (fixed by the DeepSeek Workers session); `tools/ds_impl.ps1` skips acceptance when no worker ran and seeds each task's `target/` from the lead's debug build (ported from the harness). Pending: sync the other tools with the harness at a clean point.
- **Later the same day:** schema-15 assets published (see-through ice/rock and editor markers gone, verified on screen); impl-013 glows/terrain bases/labels (`8d4160b`); tools synced with the harness (`c17fc48`); impl-014 converter `lights` table + `references.radius_override`, DB schema v5 (`5cd1bf4`); impl-015 engine point lights with a nearest-64 budget (`2acd409`); impl-016 auto-load doors cross on contact (`83ec4cd`). All DeepSeek, all accepted without correction.
- **Claude runtime fixes:** `ds_impl -Integrate` now stamps copied files (a 1 s "build" had reused a stale binary); lights never rendered - model-less LIGH references lacked `Visibility` (B0004) - and were ~100x too dim for the camera exposure (`EXPOSURE_CALIBRATION = 50` in `lights.rs`, the brightness knob); `update_atmosphere` never applied when starting inside Blackreach (`188e184`).
- **Data state:** `C:\Modding\SkyrimConverted` is converter schema 15 / DB schema 5 (504 lights, 15,167 lit references, 315 auto-load doors). A publish can fail with "Access is denied" if anything holds `skyrim_world.db` open (a worker script did); `converter.exe ... --resume-staging <staging dir>` republishes.
- **Verified:** `--demo-tour` 4/4 crossings + walk test on the final build; screenshots in `local/demo/lights3/`. The user is running agentic tests on this PC, so no FPS/timing numbers from today are valid.
- **Evening fix (Claude):** the black Alftand01 -> Alftand02 (and Tamriel -> Alftand01) portals were a heading-sign bug, not rendering: a Creation `XTEL` heading turns *clockwise* (forward = `(sin z, cos z)`; every route link puts the arrival in front of its destination door along that), while `arrival_camera_rotation` reused the counter-clockwise object convention plus a half turn, which only happened to fit AlftandWorld. Now `Quat::from_rotation_y(-z)`, with a regression test built from the four real links. Crossings also land the camera at eye height (XTEL is the feet point; fly mode and the tour had sat on the floor). Tour 4/4, screenshots `local/demo/portalfix2/`. Open question: objects still use the counter-clockwise `Rz` convention and look right on screen; not re-verified against the clockwise heading finding.
- **Known gaps:** lights overexpose right next to a source (inverse-square; tune `EXPOSURE_CALIBRATION` or add a tonemapping/exposure pass); GPU clustering ignores render layers, so a portal destination's lights can faintly reach the active space (impl-015's note).
- **Next:** the user's look at the lit demo; then the Phase 2 shakedown and visual review (no GPU-heavy runs while the user's agentic tests run).

### 2026-09-22 - Claude (overnight: walkable Alftand -> Blackreach demo; Phase 2 sign-off paused)

- **The user's goal for the night:** "get this cool Blackreach demo up and running, extra points if it's interactable and I can walk through the level". Done: `play-blackreach-demo.cmd` (or `target\release\engine.exe --assets C:\Modding\SkyrimConverted --demo alftand --walk`) starts on foot at the Alftand exit; four load doors (E) lead down to Blackreach with no loading screen. `--demo blackreach` (or `play-blackreach-shortcut.cmd`) starts inside Blackreach.
- **Verified:** `--demo-tour <dir>` walks the route by itself, screenshots every place and door, then holds W in Blackreach. Final run on `3de1505`: 4/4 crossings, walk test 600 units grounded, 0 errors, 170-180 FPS. Screenshots: `local/demo/final/`.
- **Built by DeepSeek workers, reviewed and integrated by Claude:** impl-005 (converter: `XTEL` -> `door_links`, DOOR/ACTI/FLOR/CONT/TREE/LIGH models, schema v4), impl-006 (engine: `ActiveCell`, interiors, rebase guard, `TransitionPlugin` pre-streams and crosses), impl-007 (`PlayerPlugin`: mouse look, walking via Bevy mesh ray casting, E to open, F to fly), impl-008 (weld vanilla one-point terrain seams instead of dropping cells; no water plane on dry cells). All four accepted without correction. Claude: design + `doors.rs` contract, CLI (`--walk`, `--demo`, `--start-position/--start-yaw`, `--demo-tour`), app wiring, demo tour, atmosphere (sky / underground ambient / lantern), objective text, launchers.
- **Data:** `C:\Modding\SkyrimConverted` reconverted at schema v4 (complete, integration passed, 2204 door links all resolved; the four route doors match research t02 exactly).
- **Harness fix:** `tools/ds_impl.ps1` now gives every checkout its own `target/`; a shared one let parallel engine workers run each other's test binaries (found by impl-007).
- **Known gaps:** the terrain layer blending is still blocky (research-004's Defect A, fix recipe in `docs/research/terrain-blending-seams.md`, not yet implemented); `LIGH` references are not real lights (interiors use ambient + a camera lantern, Blackreach reads dark blue); load doors are opened with E only (Skyrim's auto-load doors fire on contact); no skinned meshes, NPCs or physics (Phase 4).
- **Phase 2 sign-off (paused for the demo):** shakedown on real assets passed every gate except the visual review; the terrain fixes above are what the visual review needs. Test locations chosen from data: rural (0,0), dense (4,-21), water (21,-3). Next: implement the blending fix, rerun the shakedown, user reviews screenshots, then two full campaigns.

### 2026-09-21 - Claude (clone, toolchain, DeepSeek harness)

- **Cloned** `realfakenerd/Mudcrab` at `8919096` into `C:\Modding\MudcrabProject`.
- **Toolchain installed** on this machine: Visual Studio 2022 Build Tools (C++ workload) and Rust stable 1.98.1 (MSVC) via rustup. CMake and Ninja were already present.
- **DeepSeek harness** copied from the DOA Xbox360 UI project and adapted: `.deepseek-agents.json`, `tasks/deepseek/` (context, cookbook, delegation policy, reviewer, run analyst, advisors, self-test, implementation template, result schema), `tools/ds_impl.ps1` (isolated worktree per implementation task, sharing `target/`), `tools/ds_status.ps1`, `tools/run_ds_queue.ps1`, `tools/ds_report.py`, `tools/check_worker_result.py` (now recognises `cargo` commands), `tools/check_citations.py`, and their tests (`python -m unittest discover -s tests`).
- **Verified:** `cargo check --workspace` passes (5 min cold, only a future-incompat warning from the `proc-macro-error2` dependency). The harness self-test passed 6/6 (`local/agents/runs.csv`).
- **Skyrim SE** is installed at `C:\Games\Steam\steamapps\common\Skyrim Special Edition`. Workers can read it (in `readOnlyDirs` and described in `tasks/deepseek/context.md`) but never write to it.
- **Private project:** the user wants none of this public. Commit locally only, never push, never open PRs against the upstream repository.
- **Next:** run the harness self-test (`tasks/deepseek/selftest.md`), then choose the first tasks.

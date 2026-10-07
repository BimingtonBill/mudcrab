# Upstream send queue

## Waiting to send (2026-10-06; the user: soft cap 5-6 ready PRs + 7-8 drafts; now 5 ready + 2 drafts: room for about 5 drafts)

> **Gate (the user, 2026-10-07):** no engine-foundation draft or PR opens (or goes ready) until it has run in the portal demo and the user has seen it there ("Seen in portal" column: `no` / `yes (date, evidence)`). "Portal-mergeable" is the earlier step: Portal Dev may merge the branch into portal. A mergeable branch is not yet sendable. See local/plans/foundations-to-portal.md.

| Branch | Head | Issue | Checks | Next | Portal-mergeable | Seen in portal |
|---|---|---|---|---|---|---|
| phase1/room-graph-pr (wt/p1-main-ro) | b7488c1c, workspace nextest 1090 + doctests green (0f2645b0 + XPOD pairs c3802c19, in-place room_portals migration 4cf4e20f, savepoint fix b7488c1c; Upstream Dev read the diff 13:2x) on main 2c11897b | #188 converter half | workspace clippy/nextest 1,100 passed/doctests green 17:31; DeepSeek+Sol #878/#879 accept | real data (p1rooms 19:35): 4,246 rooms, 3,097 portals, 292 interiors, 2,444 links, 364,262 pivot + 63,684 overlap rows, 37 portals missing a side; Upstream Dev full diff read; 2b4dd7ca (marker FormID remap) can go alone first; decide whether docs/research notes go in the PR; no schema bump, no engine reader yet | yes (d4f8381d; needs a portal reconversion to show) | no |
| phase2/perf-baseline (wt/p2-perf) | 111ca6be on #122 head 2433ef4f (stacked) | #184 | fmt/clippy, workspace nextest 1134 passed, doctests; reviews #1004/#1008 answered | after #122 merges; baseline local/perf-baseline/2026-10-06-main-final/baseline.md (CPU-bound; ±15% noise) | yes (111ca6be) | no |
| (split from perf-baseline) scene set | squashed: phase2/perf-baseline-send be08181a (tree = 111ca6be) | acceptance scenes: dense -> Riverwood 3,-12, dense-legacy 18,-5, docs agree | thresholds unchanged; watch synthetic p95 16.15 and stress streaming one repeat 16.78 vs 16.67 | Phase 2 squashes into one commit off main when queued; before/after from the booked rerun | with perf-baseline | no |
| phase2/view-policy (#187 slice 1) | ca9fd9ca on main 2c11897b (slices 1+2) | #187 | fmt/clippy, workspace nextest 1143 on ca9fd9ca, doctests; DeepSeek+Muse #1014/#1017 answered | default behaviour unchanged (1024x576 every frame); slice 2 (shadowless reflection, presets, rural A/B) in progress | yes (49961394) | no |
| phase1/door-links-export (D:/Mudcrab/worktrees/up-doorlinks) | 723c0c39 on main 2c11897b (XTEL exactly 32 bytes, matching the remap) | #112 (door links for PR #128) | workspace nextest 1090 + doctests (ea2dc095); 70ae824c converter 595, clippy/fmt clean; DeepSeek #889 accept, Sol #889.1 (a) NaN XTEL fixed, (b) subset-export NULL worldspace = known limit; Upstream Dev read the diff | optional door_links table, same shape as portal and #128 DOOR_COLUMNS; no schema bump; then p2main-v24 DB-only rebuild after Portal's conversion | yes | no |
| phase2/room-culling (#188 engine) | 4bce5914 on phase1/room-graph (stacked) | #188 | movers exempt; workspace nextest 1125 on fdf2a9b1, doctests | image test passed (18/20 poses 0 px (FLIP 0); 2 poses a 1-px line-edge raster tie, FLIP mean 0.0001, below run-to-run noise 0.08; meshes 3288->1188 in Bleak Falls), note local/review/188-room-culling-image-test.md; DeepSeek+Muse #1028 accept; nextest 1136 | after image test | no |
| phase2/perf-routes (worktrees/p2-routes) | 36a09ba8 on perf-baseline 111ca6be (stacked) | #184 / 160 fps goal | DeepSeek review #1032 accepted, 4 findings fixed; fmt/clippy/doctests, workspace nextest 1168 (9fdecea7); 36a09ba8 real-clock frame timing, engine nextest 421 | travel + held-out check routes, frame budget, over-budget owners; doors scene (phase2/perf-doors, impl #1031) stacked on #128 next | yes | no |

**HOLD (the user, 2026-09-25): no new public PRs until at least 3 of the standing PRs (#29, #30, #31, #34, #36, #38) are merged.** Merged so far: #34, #36, #39 (3 of 3: **the hold is met** 2026-09-26; posts to upstream are separately paused by the user). Review fixes to open PRs (user OK each) and labelled bot replies continue.

Owner: **Upstream Dev**. Upstream is Mudcrab-Team/mudcrab, whose main is 417a262 as of
2026-09-25. Every new PR or issue, and any text meant for humans, needs the user's OK. Replies to
bots (CodeRabbit) may be posted by agents, labelled as AI.

Sources: QnA's handover `local/handover-upstream-2026-09-25.md`, the branch scan below, and
`docs/pr/two-tracks.md` for which work belongs upstream at all.

## Open upstream (ours)

**2026-09-25 afternoon:** HazielMagallanes (a contributor, not a maintainer) reviewed all six, each "Approve (in my opinion)". His non-blocking notes were fixed and pushed with the user's OK (heads below) and answered on each PR (`local/pr-drafts/10-haziel-replies.md`). Combined branches need the new heads merged (Phase 2 Dev).

| PR | Branch | Head | State | Next |
|----|--------|------|-------|------|
| #29 | phase2/refr-rotation-followup | 4dcfb69 (realfakenerd merged main in twice) | CI green; CodeRabbit confirmed the fix (checked 48 order/sign combinations itself) | wait for a maintainer |
| #30 | phase2/pruned-texture-complete | 3cd8858 | CodeRabbit answered; the (c) fix waits on the combination | the second of #30/#31 to merge takes `phase2/pr30-31-combined` (43cd1eb: resolution, glue, the (c) fix and 136d05a; converter 140 tests green) |
| #31 | phase2/resume-staging-validation | 136d05a | CodeRabbit second-pass Minor fixed and answered (9cbeee7 pushed with the user's OK) | as above |
| #34 (issue #33) | phase2/model-bounds | 0ebe63b | **MERGED** 2026-09-25 18:07Z by realfakenerd (squash, main 7151669) | done |
| #36 (issue #35) | phase2/default-water-height | b5fc81f | **MERGED** 2026-09-25 18:37Z by realfakenerd (main ddb6d70) | done |
| #38 (issue #37) | phase2/terrain-seam-weld | d21f357 (realfakenerd's main merge e60d6ba dropped two summary lines; the fix d21f357 pushed with the user's OK and explained on #38) | opened 2026-09-25; the issue asks weld vs reject | as above; conflicts with #34 in **profiling.rs** (report template). Resolution ready: `phase2/pr34-38-combined` 073c20b (Phase 2 Dev; includes 0ebe63b and 3e28bdd; fmt, clippy clean, 56 engine tests). The second to merge takes 073c20b's profiling.rs: #38's lines plus `- Empty model references: {}` after `- Assets ready: {}` |

Owed: a follow-up PR for CodeRabbit #30 (a) (check the source under `staging/vfs` before pruning).
The #30 reply promised it. **Written 2026-09-25 night:** `phase2/prune-keeps-failed-sources` 6bfee13 (on #30's head 3cd8858; merges cleanly with main and pr30-31-combined): a missing KTX2 whose source DDS is under `staging/vfs` is not pruned. Test `a_texture_that_failed_to_convert_is_not_pruned_as_absent` (fails without the fix: 2 pruned vs 1). fmt, clippy, converter + dummy-content tests green. Held by the send rule; no draft text yet.

## Others' PRs we track

- **#27** TaylorTurnerIT (trees/flora, cutout vertex alpha, schema 15): the author is holding it.
  It overlaps renderable-base-types, vertex-alpha-* and shared-converter-schema. See
  `pr27-comparison.md`; the recommendation is not to race it but to offer the vertex-alpha-faithful
  rule as a review note (the user's to post).
- **#32** TaylorTurnerIT (kache build docs): closed 2026-09-25.
- Issue **#24** (ours, static lighting scope): open, answered only with "be patient, roadmap rework".

## Waiting on the user's decision

1. ~~Discord "Chef"~~ settled: the user replied; Lucas said "let there be light".
2. **reference-shots** (`--shots`, engine only, merges cleanly): ask first (issue or Discord) or
   send directly? Include an example shots file? Notes in `local/pr-drafts/reference-shots.md`.
3. ~~Point lights~~ settled: sent as #39/#40/#41 (hold exception).
4. ~~History paths~~ decided 2026-09-26 (the user, 3a): when lod-engine-trees or material-publication is sent, send it as one fresh squashed commit (same tree, clean history); nothing existing is rewritten.
5. ~~public/portal~~ done 2026-09-26: re-exported by Portal Dev with the user's direct yes; head ff96ac2 (portal ae25821).

**POSTS TO UPSTREAM PAUSED (lifted for the 7 bundle PRs by the user's "open all 7", 2026-09-27; everything else still asks first) (the user, 2026-09-26: "hold off on posts to realfakenerd's upstream").** No comments, issues, PRs or bot replies on Mudcrab-Team/mudcrab until the user lifts it. The user's own fork (e.g. public/portal) is not affected. Held: the LOD issue (18), the reference-shots route decision. Pushing review fixes also waits unless the user OKs it.

## Bundles (the user, 2026-09-27: fewer, bigger PRs, each with photo/video evidence)

Built locally from main 6f583cd by Upstream Dev (merges of the component branches; conflicts noted). **All nine pass fmt, workspace clippy -D warnings and nextest --workspace (2026-09-27)**: converter-hardening a3e132f 284, engine-loading a649381 258, tooling 36fd76f 265, sky 9ddddf9 274, water 55439de 267, vertex-colours 7397f00 260, terrain-textures ab2f3e7 274, sun-shadows 5714ec6 269, housekeeping 81439cc 284. These replace sending the components one by one.

| Bundle branch | Components | Evidence needed | Status |
|---|---|---|---|
| `phase2/converter-hardening` | converter-snapshots, cell-name-nul, parser-never-panic (Cargo.toml/lock + dummy-content.md: kept both) | test output (never-panic properties, snapshot) | built, checks green |
| `phase2/engine-loading` | io-pool-stack, cell-cache-validate-once | stress A/B table (claim the cell-commit drop 2.86 -> 0.37 ms, not fps) + impl-222's portal crash log labelled as portal (the crash did not reproduce on main in 2x60 s: intermittent) | built |
| `phase2/tooling` | scripts-utf8-bom, scripts-schema-from-source, conversion-stage-times, benchmark-frame-times, render-world-timing | sample output | built |
| (onto #30) | prune-keeps-failed-sources 6bfee13 | test output | ready; a push to our own PR |
| `phase2/sky` | sky-dome, sky-fog | before/after pairs | built; **evidence** `evidence/sky/flip/flip-sheet.jpg` (Winterhold, Riften Fishery, Fort Dunstad: grey void -> blue sky + haze; strong) |
| `phase2/water` | water-look, water-colours, reflection-land-only (render.rs: kept both) | lake before/after + reflection | built; **evidence** `evidence/water` (white river at the mill -> shaded); send after sky or render with sky merged in (flat grey without a sky); reflection citations to verify |
| `phase2/vertex-colours` | prepass-vertex-alpha, vertex-alpha-faithful, vertex-alpha-flag | gravel/Whiterun before/after | built; **evidence weak on main**: `evidence/vertex-colours/sheet.jpg` (s14 main vs s15 bundle; clearest: RW-02 blue smear gone; walls barely change; engine+data both differ). The strong gravel/Whiterun pictures are portal-only; reconsider how to pitch this bundle; evidence: Phase 2 Dev reconverts a copy on D: (SkyrimConverted-upstream-s15, local evidence converter, not for upstream); needs CONVERTER_SCHEMA_VERSION 14 -> 15 + reuse-window entry right before sending (coordinate with #27's 15 and #40's world DB 3 -> 4) |
| `phase2/terrain-textures` | terrain-samplers-weights, terrain-normal-maps | ground close-ups | built; #42 covers only the Repeat sampler: drop/adapt our sampler part (c2bcb38, affbd41 fixture) to #42 or base on it if it merges first; keep the weights (17-sample field) + normal maps (engine-only: renders on today's data); expect a streaming.rs conflict; the picture shows blending + relief; **evidence** `evidence/terrain-blending/sheet.jpg` (main | samplers+weights | full; use it, not round 1's) |
| `phase2/sun-shadows` | sun-shadow-cascades, water-reflection-gating | wide shadow view; timing | built; **evidence** `C:/Modding/Mudcrab/worktrees/p2-tools/evidence/sun-shadows` (Bleak Falls Barrow, Riften Fishery: no shadows -> long shadows). Riften reflection question settled: not a gating bug (cascades-only = bundle, FLIP 0.019; the town's shadow on the water changes the read, `evidence/riften-reflection`); needs a benchmark |
| `phase2/housekeeping` | audit-converter, audit-engine, asset-precedence | test output | built |
| (later) publishing and re-conversions | publish-robustness dab401e (failed restore reports the backup path; cleanup failure after a good publish is a warning; a stale backup refused before converting, never deleted; test: no absolute paths in the published tree; 136 tests; nits: stale doc above `STAGING_INFIX`, docs/demo/README could mention the refusal), resume-publish-only, staging-hard-links, reuse-window-single-source e59baad | tests + the real 'Access is denied after 33 min' case | publish-robustness is from main and could go alone first; the rest after #30/#31 merge |
| (after #27) fire and effects | material-publication, nif-billboard-nodes, shared-converter-schema | before/after | waits on #27 |
| (after the owner answers) distant scenery x2 | lod-engine-terrain (incl. containers + database); lod-engine-trees + tree-billboard-lighting | before/after | waits on the LOD issue |
| unchanged | initially-disabled-refs (after #40), reference-shots, remote-dev-flag, renderable-base-types (#27) | | |

**PR texts prepared 2026-09-27 (preview `local/pr-drafts/preview-bundles.html`, images on the fork's pr-assets 430eb7e):** 19 converter-hardening a3e132f, 20 engine-hardening fe010d0 (replaces engine-loading; + audit-engine + texture-path-containment; 265 green), 21 tooling 36fd76f, 22 sky 9ddddf9 (replaces 12/13), 23 water 55439de (after sky), 24 terrain-textures ab2f3e7 (credits #42; drop our Repeat part if #42 merges first), 25 sun-shadows 5714ec6 (timing in: rural +0.5 ms p50, stress +6% fps). Held: `asset-precedence-audit` 5a9a88b (audit-converter + asset-precedence, 278 green; send as a fresh squashed commit: history has local paths), vertex-colours (weak evidence on main; rethink the pitch).

 
**PACING LIFTED FOR PHASE 1 (converter) WORK (the user, 2026-09-28, after Lucas said converter work is top priority):** converter PRs no longer wait for a maintainer merge. This is NOT permission to send: every PR/issue/comment still needs the user's own yes in Upstream Dev's session. Engine (Phase 2) PRs keep the pacing below.

**Phase 3 (launcher) send list (Phase 3 Dev, 2026-09-28; the user decided converter-gui folds into crates/launcher, so upstream sees one launcher; PACING LIFTED for Phase 3 launcher work too (the user, 2026-09-28: "launcher is important"); still one yes per send):** L1 launcher-resume RETIRED 2026-09-28 (superseded by phase3/launcher-rework, which offers Resume as a button; was `phase3/launcher-resume` eb5a0d6, reviewed; SEND AFTER #31 MERGES (merge main in, re-run checks): before #31, auto-resume could publish a torn or old-schema staged output; text local/pr-drafts/phase3-L1-launcher-resume.md); L2 progress (after #45 + converter-progress); L3 launcher-conversion (`phase3/launcher-conversion` e1ffb36 = converter-gui folded in + L1 + output-check b99bc4b + Resume/Stop/Check; held until #51, #45 and the converter bundle; screenshots local/research/launcher-merged-*.png; no text yet); L4 engine-exit-message (`phase3/engine-exit-message` bd26dfc, from main, independent of #31; engine stdout+stderr to temp_dir/openskyrim-engine-stderr.log, last 12 lines + path shown on a non-zero exit, Play refused while running; Upstream Dev skimmed it, fine; TODO before sending: PR text from docs/handoff/impl-903-p3-engine-exit-message.md and one manual check against a refusing engine; conflicts with L3 in handlers.rs). Parked (the user's call): a Browse folder picker.

**OVERLAP ALERT (2026-09-28 ~15:00): TaylorTurnerIT opened a stack on top of our #51 (f14c5a9):** #52 P1 runtime pack without build workspace (staging keeps vfs/ + cache; publishes only artifacts; blobs to <output>.assets-cache; hardens staging writers incl. the cache-hit copy and script writes) = overlaps OUR cache-dir + atomic-writes; #53 P2 native DDS blocks in KTX2 on desktop = overlaps OUR fast-textures; #54 P3 per-mip Zstandard (level 6, config key) = overlaps OUR ktx2-zstd. DO NOT send those four of ours as separate PRs; the user decides how to cooperate (review/support his, offer our evidence, or PR our pieces into his branches).

**2026-09-30: Lucas merged everything** via staging #65 (7dae5b8), then his rebrand #66 (f3fd0e4, main CI red; Haziel's issue #68 asks a revert: the user says stay out). Post-merge audit (helpers #909-#911, local/research/postmerge-2026-09-30-*.md) -> sent with the user's yes after SEND-CHECKLIST (combined tree: clippy clean, nextest 699/701, the 2 = main's): #69 converter output guard (stack bottom) -> #70 launcher safety; #71 restore #30 + post-merge fixes; #72 engine DB schema 3 again + lights layers; #73 remove staging after a resumed publish (credits Chef #52). Held: phase1/main-green f83819e (main's fmt + 2 tests), not sent. Open: main's engine exits on the dummy fixture (physics.rs:103, Chef's #64); launcher schema check follow-up after #72.

**LUCAS IS MERGING (Discord, relayed by the user, 2026-09-29 morning): the user asked about organising the PRs; Lucas said it's okay and he'll finish the merges tomorrow, then everything will be up to date.** So: do NOT merge main into our conflicting PRs (#40, #41, #43, #44, #45, #46-#48, #57, #58, #59) and send no new PRs (incl. the Mudcrab rename) until his merge pass is done; only CodeRabbit fixes and replies. Help if he asks.

**#58 review (2026-09-29):** HazielMagallanes requested changes (8 findings). With the user's yes: pushed Phase 1's fixes 9315f30 (verbose lines, honest summary, typed outcome/Interrupted, quiet-basis merged in, try_send extraction progress, real exe name), body API note, reply (local/pr-drafts/46), duplicate #45 metrics thread resolved. Finding 7 needed no change (#45 scripts never convert). Still conflicting via #45 until the merge pass. Mudcrab rename 4fd46e1 reviewed by both owners, held with new PRs.

**Drafts #46-#49 (2026-09-29): the user said don't touch the open drafts for now.** Local main-56f2431 merges by Phase 2 Dev, NOT pushed: phase2/sun-shadows-main 8091500 (#47, reviewed, workspace 300 passed), phase2/sky-main 570a641 (#46) and phase2/water-main a5cfa6c (#48) NOT verified (workspace run suspect; rerun clippy + nextest in D:/Mudcrab/worktrees/p2-sky, p2-water before use). v2 body edits drafted: local/pr-drafts/49-52. Chef's #61 carries an integration branch of our #43-#49.

**2026-09-29 evening: #30 MERGED (56f2431, Haziel's resolution); #50 merged. The upstream repo is now realfakenerd/mudcrab (project renamed Mudcrab, visible text only per the user).** Mudcrab text PR: Phase 3's brand/mudcrab-text 4fd46e1 (on main 56f2431, 287 green, Phase 1 OK'd converter lines; 30 files; text local/pr-drafts/brand-mudcrab-text.md), to be merged with current main before sending; propose folding #59's launcher strings into #59.

**2026-09-29 later: #29 and #31 MERGED (main 56489bb).** #30 resolved by HazielMagallanes himself (ff4ddaf: re-records pruned GLBs, carries prune records, prunes only when the source is absent, wires journal-verified staged meshes); Phase 1 reviewed it: correct. Optional perf notes to offer him later (the user's call): skinned meshes never verify on resume (composite source hash), case-sensitive vfs paths on Linux. Our #30 glue/follow-ups retired.

**2026-09-29: #27 MERGED (main a5c2215, converter schema 15).** 11 of ours conflict with main: #40, #41, #43, #44, #45, #46, #47, #48, #57, #58, #59. Clean: #29-31, #38, #49, #51, #55, #56. Main merges into our PR heads wait for the user's yes. Converter schema main+1 is now 16 (ktx2-zstd etc.).

**DRAFTED (the user, 2026-09-28 evening, to cut open-PR load: 19 of ours were open):** #46 sky, #47 sun shadows, #48 water, #49 terrain textures are now GitHub drafts ("not now"; nothing closed, no comments posted). Mark ready again with `gh pr ready <n>` when the maintainers want engine look work. Not done: folding #29 (needs a target PR; the user's call).

**Follow-ups PUSHED with the user's yes (2026-09-28 evening):** #58 -> 38dbb9e then 975601d after CodeRabbit (stop takes effect inside an archive's extraction, deterministic test); #59 -> f7ec63a then 06a5340 (Start label after Delete staging + #58's head merged; workspace 404 green). The user: no follow-up comment to TaylorTurnerIT for now (our PRs cover what matters), and no reply to Lucas.

**PENDING the user's yes:** push Phase 1's impl #834 (Stop takes effect inside an archive's extraction; per-entry stop checks; 2432421 on #58's 74c9a8c) to #58, after Phase 1 makes its pipeline test deterministic (branch phase1/converter-progress-stop, now 5fc578b: 2432421 + a deterministic test hook; converter 161 green; local converter-progress-main back at 74c9a8c). Phase 3 verified #59 77b9869 + 2432421 passes (259); then #59's "Stop waits for the archive" line can say fixed in #58 (a PR-text edit: ask the user). NOTE: #58's fork head is now ec7dd03 (CodeRabbit round: #45 7d11444 merged + fixes); stop-fix branch now a129387 (5fc578b + ec7dd03 merged; 303 green); build CodeRabbit fixes for #58 from that, not from the local branch tip.

**OPENED AUTOMATICALLY under the user's allowance (2026-09-28 ~16:40, up to 3 converter/launcher PRs; 2 used):** #58 converter progress (`phase1/converter-progress-main` 74c9a8c, stacks on #45; 293 green), #59 launcher rework (`phase3/launcher-rework` 77b9869, merge after #45/#55/#58; workspace 388 green; screenshots on pr-assets a0512f5). CodeRabbit round on #43/#55/#56/#57 handled: 9 fixed (#43 216e3e0, #55 6788d10, #56 49dfbb1, #57 4f3225c), 2 declined with evidence (UCRT close.cpp; bevy_state NextState::set). CodeRabbit is handled automatically from now on (the user).

**LAUNCHER REWORK = PRIORITY (the user, 2026-09-28 evening):** rebuild as `phase3/launcher-rework` from main, stacked only on #55 (check + cancel) + Phase 1's re-cut `phase1/converter-progress-main` (Cancellation, ProgressEstimate, format_clock); no dependency on the dropped texture/cache branches or #51. Resume is offered (a button), not automatic, until #31. Send order: progress PR, then launcher rework. PROGRESS PR = `phase1/converter-progress-main` 74c9a8c (Phase 1 Dev): 3 commits on #45's head 970cbdd (it uses #45's stage clock; no #51 dependency): live status line, byte-weighted progress, Cancellation + resumable Ctrl+C, ProgressEstimate, converter::find_resumable_staging. ~2,250 lines, so its own PR stacked on #45 ("merge after #45"), not pushed into #45. #52 needs no launcher change beyond ignoring `<output>.pack-*` and `<output>.assets-cache`.

**SENT 2026-09-28 with the user OK ("do it all"): #55 contributor-experience, #56 engine-start-errors, #57 streaming-crossings; #43 fast-forwarded to 0914347 (decoder bounds) + comment; comment to TaylorTurnerIT on #53. These branches are live PR heads now.** Bundles as prepped: B1 contributor-experience `phase1/contributor-experience` e45183c (onboarding + converter-check; 275; text 38); B3 engine-start-errors `phase2/engine-start-errors` f295721 (schema-mismatch-error + launcher engine-exit-message; 274; text 39; manual refusal check still open); B4 streaming-crossings `phase2/streaming-crossings` cabd7a9 (stutter set + interior fixture; 274; text 40); push into #43: decoder-bounds 0914347 (293; comment text 41). Comment to TaylorTurnerIT on #53: text 42. HELD: B2 speed-ups (cheap-io; overlaps #52's pipeline work, wait for Phase 1 Dev's comparison); B5 resume (atomic-writes overlaps #52; L1 after #31); converter-progress can't go into #45 as-is (built on #51's old head), so it goes back to Phase 1 Dev.

**PREPPED FOR THE USER (2026-09-28, all green on the branch that would be pushed):** converter-check `phase1/converter-check` e05b2d0 (275; text 37), ktx2-zstd 5a490d0 (279; text 35; stacks on #51), crossing-stutter `phase2/crossing-stutter` d1507fd (squash of c8b2801; 272; text 32; engine, so the pacing applies unless the user says otherwise), contributor-onboarding 22391dd (257; text 30). Preview: local/pr-drafts/preview-four.html. Nothing sent.

**Ideas saved (not started; each needs the user's go):** (a) a short comment for Lucas pointing at #31 as the resume WAL he asked for, and #51 + cheap-io as the answer to the IO-bound conversion (Makiavel: CPU 0%, mostly IO); (b) launcher auto-resume: the launcher reruns everything when a conversion didn't finish, so pass `--resume-staging` to the converter (Phase 1 Dev); (c) convert-while-playing (Lucas: prioritise the first quest's assets): a design issue for later.

**Phase 1 converter send list (reorganised 2026-09-28; each item = one yes):** 1 contributor-onboarding 22391dd (from main); 2 ktx2-zstd 5a490d0 (after #51; schema main+1); 2b atomic-writes 50f421c (from main; zstd-resolved aa567c6; cache-hit GLB copy + Luau write via temp+rename; makes main's resume safe without #31; open: stray .partial files after a kill must not be published); 3 output-check b99bc4b (cancellable check for the launcher's Stop) + quiet-basis 604150b (from main; `converter check` + Basis quiet terminal); 4 cheap-io (after #51; send the zstd-resolved head phase1/cheap-io-zstd e5a2047 if ktx2-zstd has landed, else cheap-io b583f3b); 5 fast-textures 762fe3b (issue first, after ktx2-zstd); 6 cache-dir 56b6bdd (issue first, after #51); 7 decoder-bounds 0914347 (after #43); 8 converter-progress eb6b488 (after #45). Standing: #30/#31 glue to whichever merges second; #31 x ktx2-zstd resolution (Phase 1 Dev) for whichever of those lands second.

**PACING (the user, 2026-09-27 23:36, via Phase 2 Dev's schedule): new PRs wait for the next maintainer merge; fixes to our open PRs still go (each with the user's yes here).** Plans cover one day or one night at a time (the user, 2026-09-28); this queue keeps ORDER only, not dates.
**Converter send order under the pacing (agreed with Phase 1 Dev, 2026-09-28; the user can change it):** 1 contributor onboarding (quick-profile + onboarding-docs), 2 ktx2-zstd (not before its clean timing A/B), 3 cheap-io (merge #31 in + a torn-GLB resume test once #31 is on main), 4 cache-dir (issue first), 5 fast-textures (issue first, after ktx2-zstd); converter-progress slots in after #45 merges.
**#44 note (non-blocking):** `is_safe_relative_asset_path` is defined twice (streaming.rs and world/database.rs, from audit-engine 9e40e93). Phase 2 Dev can write a one-module follow-up if a reviewer raises it or #44 needs a push anyway (push = the user's yes).

**SENT 2026-09-27 with the user's OK ("open all 7 prepped PRs"):** #43 converter-hardening a3e132f, #44 engine-hardening fe010d0, #45 tooling 36fd76f, #46 sky 9ddddf9, #47 sun-shadows 5714ec6, #48 water 55439de, #49 terrain-textures ab2f3e7 (credits #42). These branches are now live PR heads.

**CodeRabbit round 1 (2026-09-27, the user OK'd the 8 replies):** fixes pushed: #43 9adf30d (LAND proptest at valid lengths), #45 970cbdd (main_world ordering; frame-times write failure exits with an error), #46 4093038+8cc206a (sky.md fog 0.47 at the ring edge), #47 f3c70e8 (far check projects the bounds on the forward axis, plus a cell-sized test), #49 bc313d3 (a failed normal map falls back instead of hiding the quadrant). Declined with reasons: #45 last-render-frame drain (one frame; distributions) and #48 WATER_LAYER in LIGHT_LAYERS (Bevy skips directional lights per view, not per mesh). #44 had no findings. Texts: local/pr-drafts/26-coderabbit-bundles.md.

## Staged branches, in send order

Branch scan of 2026-09-25 against origin/main 417a262 ("fork" means the same commit is on
BimingtonBill/mudcrab). The "plain" column is the name used on the user's roadmap page
(`local/roadmap/phase2.json`).

| Order | Branch(es) | Plain | Ahead | Merge | Fork | Status |
|-------|-----------|-------|-------|-------|------|--------|
| 1 | empty-model-bounds + bounds-precision (one PR) | Invisible marker models don't fail the size check; long test runs no longer fail at random on correct models | 2+1 | clean | same | **SENT** as #33/#34 |
| 2 | default-water-height | Morthal's lake and the open sea come back (10,630 of 11,187 cells had no water) | 1 | clean | same | **SENT** as #35/#36 |
| 3 | terrain-seam-weld | No hairline gaps between land squares; city worldspaces no longer fail the seam check | 3 | clean | 4fe8f6e not on fork | **SENT** as #37/#38 |
| 4 | terrain-samplers-weights | Ground textures tile and blend properly | 2 | clean | same | needs a GPU check |
| 5 | sun-shadow-cascades | Sun shadows reach across the whole view | 2 | clean | same | needs a GPU check and a benchmark (timing runs go alone) |
| 6 | water-look + water-colours | Lakes and rivers no longer draw as a white sheet | 2+3 | clean | none | needs a GPU check; lakes read grey until there is a sky |
| 7 | terrain-normal-maps | Ground detail maps (pebbles, relief) | 4 | clean | same | needs a GPU check |
| 8 | reference-shots (a96d50e) | The reference-shot tool | 1 | clean | none | **PREPARED**: issue + PR text in `local/pr-drafts/7-reference-shots.md`; waiting on the user's preview OK. Checks re-run after s23: green |
| 8b | lights-fixture (dbb1cf6), then lights-converter (e4a2c18, on top) | Test data with a lamp; the converter records lights and their radius | 1+1 | clean | none | **PREPARED**: a comment on #24 + 2 PRs (`local/pr-drafts/8-lights-fixture.md`, `9-lights-converter.md`); world DB 3 → 4 in the converter PR. Suggested: the #24 comment + fixture now, hold the converter until #24 is answered. Engine PR 3 waits on the brightness decision |
| 8c | sky-dome (599d5f2, from main 417a262) | A blue sky with a hazy horizon instead of a flat grey backdrop | 1 | clean | none | Phase 2 Dev: built, reviewed, GPU-checked on SkyrimConverted-upstream (Riverwood, Guardian Stones), 56 engine tests. Engine only, no schema change. Gradient dome in SkyrimClear's day colours at the far plane; clear colour = Fog Far. Pictures `local/gallery/phase2/sky-dome-before-after.jpg`; later steps `local/research/sky-design-phase2.md`. Held by the send rule. **PREPARED** 2026-09-26: `local/pr-drafts/12-sky-dome.md`, fog `13-sky-fog.md`; renders to add. Phase 2 Dev's plan (sky-design-phase2 section 4): the dome alone first, then fog in Fog Far as a small PR 2; weather/climate records (schema bump) as PR 3 before any clouds, sun, moons or stars |
| 9 | scripts-schema-from-source | Test scripts read the version from the code | 2 | clean | same | tooling; low risk |
| after #27 | shared-converter-schema, renderable-base-types, material-publication, vertex-alpha-faithful, nif-billboard-nodes | one version number; trees/doors/objects get models; surface shine and see-through, heat haze, fire and effects, flame animation; flame cards | | clean | mixed | **each changes converter output and needs a schema bump**. Phase 2 Dev holds vertex-alpha-faithful ee0e6d7 (39c3497 + the BLEND-only fix), nif-billboard-nodes 11b2e2b+e3d30a6 and material-publication's 4546e6e/227da41 until then. vertex-alpha-faithful is also the user's call because it overlaps #27. Send one at a time, or bundle, so the schema numbers don't collide with #27's 15. material-publication also has a history-path hold (decision 4) |
| discuss first | lod-mesh-containers, lod-database, lod-engine-terrain/objects/trees, tree-billboard-lighting (fd292df, on lod-engine-trees: distant trees shaded like vanilla DistantTree, pale band -> green; `local/gallery/phase2/distant-trees-before-after.jpg`) | Distant land, steps 1-4 | 3-31 | 4 of 5 CONFLICT with main | mixed | two-tracks says **an upstream issue before any code**. Too big to just send; needs a roadmap talk, and the conflicts resolved. lod-engine-trees has a history-path hold |
| **SENT #51** | vfs-hard-links (`phase1/vfs-hard-links` f14c5a9 after the 09-28 follow-up, live PR head, frozen on main 6f583cd, reviewed, 265 green, on the fork; text local/pr-drafts/28-vfs-hard-links.md) | a converted install stores vfs/ and .ingestion-cache/ as one set of files, ~26 GB saved on EVERY install (research-800: local/research/phase1-output-size.md); fresh-install test required | 3-4 | clean | converter | independent of #30/#31; send pending the user's OK |
| next | ktx2-zstd (Phase 1 Dev, `phase1/ktx2-zstd`, impl-803; the user said yes) | Zstd supercompression of converter KTX2 (~11 GB per install; roadmap 1.4 says "supercompressed") | 2-3 | converter + engine Bevy zstd feature (Phase 2 Dev OK) | converter schema = main + 1 at send time (16 if #27 lands first, else 15 and #27 moves); reuse window must re-encode pre-zstd textures | after #51 (e0496e7 builds on #51 OLD head 85b968e: must merge f14c5a9 first, asked); 5a490d0 has #51 head merged; size measured 19.29 -> 12.96 GB textures (-33%); timing no measurable cost; engine load = Portal Dev s28 tour; text 35-ktx2-zstd.md; Upstream Dev workspace check 279 green |
| held (pacing) | quick-profile (Phase 1 Dev, `phase1/quick-profile` 67e7af4, the user OK'd writing it) | `[profile.quick]` in the root Cargo.toml + CONTRIBUTING "Fast Optimised Builds"; one-file engine rebuild 39 s vs 4 min 20 s | 1 | build config only | reviewed; doc says `--bin engine` | new PR: after the next maintainer merge, the user's yes here |
| held | cache-dir (Phase 1 Dev, `phase1/cache-dir` 56b6bdd (docs: same-volume, left behind on delete, written in place), on #51's head f14c5a9; review-816 applied) | ingestion cache moves beside the output (`<output>.cache` or `--cache-dir`): output -22.5 GB, reconversions stop re-linking 156,586 blobs; one-time migration of old outputs; no schema bump | 3 | converter | layout-contract change: issue first (draft local/research/phase1-cache-out-of-output.md §7); cache written in place, not staged | after #51 merges + pacing; Upstream Dev full diff review at send |
| held | cheap-io (Phase 1 Dev, `phase1/cheap-io` b583f3b (adds a GLB total-length check in validate_artifacts), 7 commits on #51 head f14c5a9; review-815 applied) | no per-file sync_all for rebuildable staging, extractor hashes reused, output hashing on workers, plugins parsed once. EVIDENCE (A-B-A, 2026-09-28): reconversion texture stage 3.1 -> 1.4 min (-56%, reproduced), wall -4%; extraction/meshes/validation unchanged (no regression) | 4 | converter | must keep #31's resume-time re-hash of staged bytes (what makes dropping fsync safe); textual conflicts with cache-dir/ktx2-zstd (Phase 1 resolves) | after #51 + pacing |
| held (pacing) | schema-mismatch-error (Phase 2 Dev C8, `phase2/schema-mismatch-error` 7cce96c, engine app.rs only; text local/pr-drafts/31-schema-mismatch-error.md; send squashed on a new branch) | the engine names the failed asset-set check, both versions and the fix (5 cases), 6 tests | 1 squashed | engine | reviewed, fixes in; trivial conflict with ktx2-zstd | after the next maintainer merge |
| held | decoder-bounds (Phase 1 Dev impl-810, `phase1/decoder-bounds` 0914347, 5 commits on #43 head 9adf30d; review-820 applied) | DDS mip bound, EsmReader file-size + TES4 check at open, BA2 1032:1 / 255:1 expansion limit, zlib-ratio refusal in #43's read_bounded | 4 | converter | built on #43; e593804 changes #43's read_bounded signature (a #43 review change there = re-merge); 64 MiB record cap is ESM-only (largest shipped record <100 KB) | after #43 merges + pacing |
| merged (#65) | crossing-stutter set (Phase 2 Dev: paced-cell-unloads 8f980f6 + upload-throttle cb917c9 + paced-model-spawns c8b2801, stacked, head c8b2801; text 32-paced-cell-unloads.md) | worst streaming frame ~21-25 -> ~17 ms (unloads 2/frame, GPU uploads 16 MiB/frame, 4 model spawns/frame); p50/p95/p99 unchanged | 1 PR | engine | 3 new options (0 = old behaviour); engine-help must add them; #45 touches profiling.rs; workspace checks 272 green (Upstream Dev) | after the next maintainer merge |
| merged (#65) | fixture-interior-crossing (Phase 2 Dev, `phase2/fixture-interior-crossing` 3b41d4d, engine test; text 33-fixture-interior-crossing.md) | M5.1: the streaming fixture loads an interior by id amid exteriors; one root, one reference, no duplicates/orphans | 2 squashed | engine tests | reviewed ('never unloads' is current behaviour only); plan_cells conflict with paced-cell-unloads | after the next maintainer merge |
| held | fast-textures (Phase 1 Dev C1, `phase1/fast-textures` 2f507dd, 2 commits on ktx2-zstd e0496e7; review-822 applied) | opt-in `--texture-mode native`: keep the DDS's own BC1-7 blocks (RGBA8 for 32-bit) in KTX2+zstd; default stays uastc byte-identical. 200 textures: 286.9 s -> 1.1 s, 0 fallbacks, 6% smaller; FLIP <= 0.042 over 8 Riverwood poses (evidence/native-textures/flip) | 2 | converter | ISSUE FIRST: roadmap 01 promises transcodable UASTC (mobile/web); this is a desktop/dev-only option | after ktx2-zstd + pacing |
| held (pacing) | onboarding-docs (Phase 1 Dev, `phase1/contributor-onboarding` 22391dd = onboarding-docs 8ea88d9 + quick-profile 67e7af4; text 30-contributor-onboarding.md) | real clone URLs; CONTRIBUTING check step = CI; fixture engine command (--worldspace 1); new docs/specs/meta/requirements.md (build/conversion/disk costs) | 1 | docs | reviewed + bundled; keep requirements.md's output size in step with main (70 GB -> ~45 GB after #51); trivial main.rs conflict with converter-progress | replaces quick-profile's slot 1 once bundled |
| held | engine-help (Phase 2 Dev, `phase2/engine-help` a57dff0, engine config.rs/main.rs + tests/cli.rs) | `engine --help` lists every option (exit 0); unknown options exit 2 with did-you-mean; bad values refused loudly (behaviour change); drift tests vs HELP_TEXT and scripts/phase2-*.ps1 | 4 squashed | engine | rewrites from_args: conflicts with open #41 and #45 (and held paced-cell-unloads); send AFTER they merge, their flags moved into the new parser | after #41 + #45 |
| held | converter-progress (Phase 1 Dev, `phase1/converter-progress` ba83c96) | converter stage progress; builds on #45's stage-time commit ce66d11 | ? | converter | after #45 merges | handover with timing A/B pending |
| low | staging-hard-links, audit-converter, audit-engine, asset-precedence | reconversions reuse files without copying; audit fixes; mod file and archive order | 1-4 | clean | mixed | small; bundle later. staging-hard-links' other half (a3792e6, reused outputs) needs the #30/#31 combination first |
| drop | vertex-alpha-not-opacity (superseded by vertex-alpha-faithful), refr-rotation (merged as #25); both archived as tags `archive/phase2/*` (also integration, integration-lod, integration-lod-shots, integration-shots, local-shots, render-check) | | | | | do not send |

## Unsent branches with a known dependency

- **resume-publish-only** ea757a9 (publish-only resume, not sent): once #30 is in, pruned GLBs are rewritten after their size is recorded, so its per-asset size check must skip them (portal's f19bbf2 has that guard). Add the guard before sending.

- **vertex-alpha series** (held; replaces vertex-alpha-faithful's rule): `vertex-alpha-flag` f05c1d1 (on
  vertex-alpha-faithful: lighting shapes use vertex alpha as opacity iff SLSF1_Vertex_Alpha && !Tree_Anim)
  + `prepass-vertex-alpha` 9dad025 (from main: the depth prepass tests vertex alpha like the main pass). Two PRs, prepass FIRST (latent on main, small); **prepass PREPARED** as `local/pr-drafts/11-prepass-vertex-alpha.md`.
  Needs a schema bump and Portal Dev's s27 GPU check; overlaps #27.
- **sky-fog** f32eccf (on sky-dome): distance fog fitted to SkyrimClear's fog. Sky PR 2, after the dome.

- **io-pool-stack** 1af3b95 (from main ddb6d70; port of portal impl-222): bevy_gltf loads textures in `IoTaskPool::scope`, whose `block_on` runs other queued glTF loads on the same thread (~85 KiB each); streaming Markarth reached 8,074 KiB and overflowed the 8 MiB IO stack. Fix: 128 MiB reservation + a nesting test (300 scoped loads on a one-thread pool). A real crash on main, small: a good first send when the hold lifts (issue + PR, the user's call; a Bevy issue too, the user's call).

- **water-reflection-gating** 6dc630f (from main ddb6d70, one commit, render.rs): the water reflection renders only while water can be seen. Evidence is the fork's (Riverwood, shadowed build): frame 10.9 -> 9.6 ms, GPU 1.45 -> 1.26 ms with the river out of view; upstream main shows no difference (no sun shadows, frame cap), so the PR must say the gain appears once shadows land. The 16-18 ms was a 90 s run total, not a spike: gate cost ~+0.5 us/frame (mean 1.0-1.15 us, p99 2.0-2.4 us). No main-only proof is possible: main's benchmark camera looks straight down at every river cell (water always in view; 2 views, GPU ~0.55 ms on both builds; data `C:\Modding\Mudcrab\worktrees\p2-tools\water-pose\`). **Upstream Dev's recommendation: send it with or right after `sun-shadow-cascades`**, whose 4 cascades are what it saves; alone on main it is a change with no measurable gain. Fact sheet E.

- **From Phase 2 Dev 2026-09-26, all from main, fmt/clippy/tests green (held):** `converter-snapshots` 94a091f (insta snapshot tests of converter output); `cell-name-nul` 44d8720 (on it: `cells.interior_name` lost its trailing NUL, caught by the snapshots; a real bug fix, good early send); `conversion-stage-times` ce66d11 (elapsed time per log line + per-stage times); bench tooling `benchmark-frame-times` a8560b5, `render-world-timing` 49b2d8d.
- **Lighting (hold exception, the user 2026-09-26): SENT** with the user's OK: #24 comment, **#39** fixture (4041832, **MERGED** 2026-09-26 13:19Z, main 6f583cd), **#40** converter (5a89364; main merged in after #39's squash: head **5f3ae5e** pushed with the user's OK). Engine **#41** `phase2/lights-engine` 358450a (main merged in after #39: head **3b88920** pushed with the user's OK) with the user's OK (picture hosted on the fork's `pr-assets` branch fe3cbe6). XRDS cap added to #41 (f79cf46).

- **lights-xrds-cap** f79cf46 (one commit on #41's 7410b6d): XRDS above 8,192 units ignored (data gap between 6,919 and the nine outliers 15,967..3,736,737; a ratio rule drops ordinary lamps). Test `an_override_above_the_cap_is_ignored`. **Pushed onto #41 with the user's OK (head f79cf46); #41's body updated.**
- **reflection-land-only** 925205f (from main, held): the water reflection camera draws terrain only; objects on their own layer (sun keeps both, so objects still cast). Rural p50 3.96 -> 3.71 ms (+6.5% fps). Citations in `local/pr-drafts/factsheet-reflection-land-only.md` (high confidence on LOD + sky, medium on the exclusion list; the REFR flag 0x10000000 Reflected By Auto Water and XPWR are a candidate follow-up). Honest look change to state: shore objects leave the reflection until LOD/SSR exist. Best sent after the sky PR, so the reflection has a sky. **Before quoting: the citations are a worker's, not re-fetched (Phase 2 Dev's correction); open STEP [Water] and DynDOLOD How-LOD-Works (a websearch worker), and verify or drop the bUseWaterReflectionsStatics/Actors = Oblivion/FO3 line (recollection, not research).**

- **initially-disabled-refs** a47bde9 (held): references Skyrim starts disabled (flag 0x800) are recorded and not loaded; adds `references.flags`, which also enables the reflection follow-up (honour 0x10000000 Reflected By Auto Water: flagged references join the reflection's world layer; Phase 2 Dev will give the flagged count). Bumps the world DB 3 -> 4 too, so it goes **after #40**: when #40 merges, Phase 2 Dev merges main in and resolves to 5 everywhere (shared, db-schema.md, Compatibility line, test literals). The engine's exact version check refuses a database without the column.

- **renderable-base-types** 41333e1: priority (Whiterun's water and 56% of WhiterunWorld's references have no mesh on main). NOT sent while the maintainer waits on #27 (a superset). The user chose to **wait** (2026-09-26): no comment on #27, ours not sent; let #27 finish. Draft kept (`local/pr-drafts/17-pr27-base-types.md`) in case the user changes their mind. Its LIGH arm overlaps #40 (one writer after a merge).
- **terrain-vclr-srgb** e4b6586: HOLD, no visible effect in renders; not sendable without evidence.

- **From Phase 2 Dev 2026-09-26 (all from main, reviewed, green, held):**
  - `parser-never-panic` fb7f928 (3 commits): VMAD array counts reserved up to 96 GiB on corrupt input and aborted the process; decompression no longer reserves a corrupt header's declared size; proptest never-panic properties for ESM/VMAD/cell cache/BSA/BA2/PEX/LIP/NIF. **Real crash fix: early send.**
  - `scripts-utf8-bom` 232a286: phase2-acceptance.ps1 fails to parse in Windows PowerShell 5.1 (UTF-8 without BOM + an em-dash); phase2-close.ps1 misreads accented headings. Tiny.
  - `cell-cache-validate-once` ac91f54: CellCache::terrain re-validated the whole 489 MB rkyv archive per cell commit; stress streaming/cell_commit 2.86 -> 0.37 ms mean (504 commits, interleaved 2x2; frame p95 within noise). Data `C:/Modding/Mudcrab/worktrees/p2-tools/stress-ab/`.
  - `remote-dev-flag` f8a5c03: opt-in Bevy Remote Protocol (feature `remote` + `--remote`, 127.0.0.1 only; default deps unchanged). New capability: issue first.
  - Acceptance baseline on main (for PR bodies): every scenario passes at 60 s on a quiet machine (stress p95 ~12 ms); missing for `accepted`: quality/release gates, 3 full reps, human visual sign-off. `C:/Modding/Mudcrab/worktrees/p2-tools/acceptance-main-60s/`.

- **reuse-window-single-source** e59baad (impl-608, held): one converter reuse window, pinned 0..=15; 130 converter tests. Fits the converter-hardening or the next schema-bump PR.

## Candidates with no branch yet

- **#30 (a) follow-up** (owed; see above).
- **Plugin FormID slot:** `plugins.priority` is off by one for plugins after the first light plugin
  (research-531, `docs/research/dragonborn-formid-slots.md`). A small fix.
- **Schema stamp housekeeping** (for the next schema bump): the stamp is still a literal in exporter.rs
  create_tables, the engine app.rs fixture and the world/database.rs test.
- **Visible record deletions:** an optional `deleted` column instead of dropping silently (research-533).
- **Point lights** (decision 3) and the **engine backlog** (`docs/roadmap/backlog-engine-and-portal.md`):
  check each against upstream `main` first, since some defects exist only in the fork.

## Rules for each send

**Evidence in every PR (the user, 2026-09-27):** each staged PR carries visual validation in its body: before/after screenshots (or a short video/GIF for motion, e.g. doors, water, streaming) for anything that changes what you see, labelled with how they were made (build, data, any local-only switch). Host them on the fork's `pr-assets` branch (pinned by commit, as in #41) and embed them. A PR with no visible change shows its evidence as test output or a measured table instead. Phase 2 Dev supplies the renders with each fact sheet.

From upstream's CONTRIBUTING.md and `two-tracks.md` "Rules for every Phase 2 PR":

1. **Issue: optional, the user's call per send.** CONTRIBUTING's workflow step 1 is "Find or Create
   an Issue" ("check the issue tracker to ensure no one else is working on the same feature"): a
   suggestion, not a rule. #29-#31 went up without one. Every send request lists each post
   separately (issues, comments, PRs) with a count (the user, 2026-09-25).
2. Merge origin/main in; run `cargo fmt --all -- --check`, `cargo clippy --all-targets
   --all-features -- -D warnings`, and the touched crates' tests.
3. The test needs no game data (dummy-content fixtures); no Bethesda data in the diff or body; no
   machine paths or private names (the history too: `tick.py` scans for them); no citations of
   `docs/research/` or `tools/research/`.
4. `///` doc comments on new functions; commits in upstream's style (`fix(converter): ...`).
5. The body references the planning doc it serves (`02-completion-plan.md` milestone or a
   `docs/specs/` file), uses the Mudcrab name, and ends with the attribution line. It is
   written in plain words for the user, with an AI label on kept text.
6. The user OKs the send. After opening, watch CI and CodeRabbit and answer both.

## Reference documents

- `two-tracks.md`: which work is Phase 2, the PR rules, and the send-order notes (the #30/#31 glue,
  staging-hard-links, effect emissive riding material-publication).
- `upstream-pr-plan.md` (2026-09-23): the older 13-PR plan for the whole fork, with the exclusion
  list (§4, never in a PR) and the cleanup list (§5). Superseded as a plan, still valid for §4 and §2.
- `phase2-branch-audit.md`, `phase2-fix-audit.md`: audits of each branch.
- `local/pr-drafts/`: bodies of #29-#31, the reference-shots notes, the point-lights plan.
- `local/overnight/tick.py`: read-only scan (pushable, held, history paths, GitHub activity).

## Roadmap page

Phase 2 Dev brought `phase2.json` and `docs/roadmap/phase2-track-tasks.md` (861ca29) up to date on
2026-09-25. Proposed to the user: an "Upstream" track on the roadmap page from
`local/roadmap/upstream.json` (Upstream Dev's file). If the user agrees, Phase 2 Dev drops the PR-status
items from phase2.json and keeps only build status there.

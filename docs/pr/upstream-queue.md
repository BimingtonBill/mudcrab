# Upstream send queue

**HOLD (the user, 2026-09-25): no new public PRs until at least 3 of the standing PRs (#29, #30, #31, #34, #36, #38) are merged.** Merged so far: #34, #36 (2 of 3). Review fixes to open PRs (user OK each) and labelled bot replies continue.

Owner: **Upstream Dev**. Upstream is realfakenerd/wah-krah-jol, whose main is 417a262 as of
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
| #38 (issue #37) | phase2/terrain-seam-weld | e60d6ba (realfakenerd merged main in, but dropped two summary lines: clippy fails). Fix `upstream/pr38-fix-merge` d21f357 + comment, awaiting the user's OK | opened 2026-09-25; the issue asks weld vs reject | as above; conflicts with #34 in **profiling.rs** (report template). Resolution ready: `phase2/pr34-38-combined` 073c20b (Phase 2 Dev; includes 0ebe63b and 3e28bdd; fmt, clippy clean, 56 engine tests). The second to merge takes 073c20b's profiling.rs: #38's lines plus `- Empty model references: {}` after `- Assets ready: {}` |

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

1. **Discord "Chef"** asked whether you are looking into lighting. The user writes the reply; QnA's
   talking points are in the handover. Fact to keep straight: point lights work on the fork, the
   Skyrim falloff patch is off, and the look doesn't match Skyrim yet.
2. **reference-shots** (`--shots`, engine only, merges cleanly): ask first (issue or Discord) or
   send directly? Include an example shots file? Notes in `local/pr-drafts/reference-shots.md`.
3. **Point lights**, a 3-PR plan (`local/pr-drafts/point-lights-plan.md`): ask in #24 first
   (recommended) or go. The branches are not built yet: impl-572/573 are queued for when the
   DeepSeek coders are released (~22:39). Also: a neutral documented brightness default (recommended) or a re-fit.
4. **History paths:** lod-engine-trees and material-publication commits contain `C:/Modding/...`
   paths (no names). Accept them, or rewrite before pushing to the fork.
5. **public/portal** is 14+ commits behind portal and needs a re-export
   (memory `public-fork-export`) before it is pushed again. Portal Dev recommends waiting for the
   s23 bundle reconversion and tours, when the fire fix actually shows, and will say when. HEAD is
   safe to publish now if the user wants it sooner.

## Staged branches, in send order

Branch scan of 2026-09-25 against origin/main 417a262 ("fork" means the same commit is on
BimingtonBill/wah-krah-jol). The "plain" column is the name used on the user's roadmap page
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
| low | staging-hard-links, audit-converter, audit-engine, asset-precedence | reconversions reuse files without copying; audit fixes; mod file and archive order | 1-4 | clean | mixed | small; bundle later. staging-hard-links needs the #30/#31 combination first |
| drop | vertex-alpha-not-opacity (superseded by vertex-alpha-faithful), refr-rotation (merged as #25) | | | | | do not send |

## Unsent branches with a known dependency

- **resume-publish-only** ea757a9 (publish-only resume, not sent): once #30 is in, pruned GLBs are rewritten after their size is recorded, so its per-asset size check must skip them (portal's f19bbf2 has that guard). Add the guard before sending.

- **vertex-alpha series** (held; replaces vertex-alpha-faithful's rule): `vertex-alpha-flag` f05c1d1 (on
  vertex-alpha-faithful: lighting shapes use vertex alpha as opacity iff SLSF1_Vertex_Alpha && !Tree_Anim)
  + `prepass-vertex-alpha` 9dad025 (from main: the depth prepass tests vertex alpha like the main pass). Two PRs, prepass FIRST (latent on main, small); **prepass PREPARED** as `local/pr-drafts/11-prepass-vertex-alpha.md`.
  Needs a schema bump and Portal Dev's s27 GPU check; overlaps #27.
- **sky-fog** f32eccf (on sky-dome): distance fog fitted to SkyrimClear's fog. Sky PR 2, after the dome.

- **io-pool-stack** 1af3b95 (from main ddb6d70; port of portal impl-222): bevy_gltf loads textures in `IoTaskPool::scope`, whose `block_on` runs other queued glTF loads on the same thread (~85 KiB each); streaming Markarth reached 8,074 KiB and overflowed the 8 MiB IO stack. Fix: 128 MiB reservation + a nesting test (300 scoped loads on a one-thread pool). A real crash on main, small: a good first send when the hold lifts (issue + PR, the user's call; a Bevy issue too, the user's call).

- **water-reflection-gating** 6dc630f (from main ddb6d70, one commit, render.rs): the water reflection renders only while water can be seen. Evidence is the fork's (Riverwood, shadowed build): frame 10.9 -> 9.6 ms, GPU 1.45 -> 1.26 ms with the river out of view; upstream main shows no difference (no sun shadows, frame cap), so the PR must say the gain appears once shadows land. The 16-18 ms was a 90 s run total, not a spike: gate cost ~+0.5 us/frame (mean 1.0-1.15 us, p99 2.0-2.4 us). Before sending: a main-only proof (a fixed pose facing inland at a river: occlusion_culling_views 2 -> 1 and GPU ms), Phase 2 Dev to measure. Fact sheet E.

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
   `docs/specs/` file), uses the Wah Krah Jol name, and ends with the attribution line. It is
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

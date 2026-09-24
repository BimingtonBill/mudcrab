# Upstream send queue

Owner: **Upstream Dev**. Upstream is realfakenerd/wah-krah-jol, whose main is 417a262 as of
2026-09-25. Every new PR or issue, and any text meant for humans, needs the user's OK. Replies to
bots (CodeRabbit) may be posted by agents, labelled as AI.

Sources: QnA's handover `local/handover-upstream-2026-09-25.md`, the branch scan below, and
`docs/pr/two-tracks.md` for which work belongs upstream at all.

## Open upstream (ours)

| PR | Branch | Head | State | Next |
|----|--------|------|-------|------|
| #29 | phase2/refr-rotation-followup | dcf48c3 | CI green; CodeRabbit findings answered and fixed | wait for a maintainer |
| #30 | phase2/pruned-texture-complete | 3cd8858 | CodeRabbit answered; the (c) fix waits on the combination | the second of #30/#31 to merge takes `phase2/pr30-31-combined` (eeb65b0: resolution, glue and the (c) fix) |
| #31 | phase2/resume-staging-validation | e87d894 | CI green; CodeRabbit fixed | as above |

Owed: a follow-up PR for CodeRabbit #30 (a) (check the source under `staging/vfs` before pruning).
The #30 reply promised it. Not written yet.

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
BimingtonBill/wah-krah-jol):

| Order | Branch(es) | Ahead | Merge | Fork | Status |
|-------|-----------|-------|-------|------|--------|
| 1 | empty-model-bounds + bounds-precision (one PR) | 2+1 | clean | same | ready; body `pr-04` exists; acceptance reliability |
| 2 | default-water-height | 1 | clean | same | ready |
| 3 | terrain-seam-weld | 3 | clean | same | needs a GPU check |
| 4 | terrain-samplers-weights | 2 | clean | same | needs a GPU check |
| 5 | sun-shadow-cascades | 2 | clean | same | needs a GPU check and a benchmark (timing runs go alone) |
| 6 | water-look + water-colours | 2+3 | clean | none | needs a GPU check |
| 7 | terrain-normal-maps | 4 | clean | same | needs a GPU check |
| 8 | reference-shots (a96d50e) | 1 | clean | none | **READY** (Phase 2 Dev: fmt/clippy/engine tests green, one real pose rendered; engine-only, off unless `--shots`, no schema bump). Waits on decision 2 |
| 9 | scripts-schema-from-source | 2 | clean | same | tooling; low risk |
| after #27 | shared-converter-schema, renderable-base-types, vertex-alpha-faithful, material-publication, nif-billboard-nodes | | clean | mixed | **each changes converter output and needs a schema bump** (Phase 2 Dev holds vertex-alpha-faithful 39c3497, nif-billboard-nodes 11b2e2b+e3d30a6 and material-publication's new 4546e6e/227da41 until then; vertex-alpha-faithful is also the user's call because it overlaps #27): send one at a time, or bundle, so the schema numbers don't collide with #27's 15 |
| discuss first | lod-mesh-containers, lod-database, lod-engine-terrain/objects/trees | 3-31 | 4 of 5 CONFLICT with main | mixed | too big to just send; needs a roadmap talk, and the conflicts resolved |
| low | staging-hard-links, audit-converter, audit-engine, asset-precedence | 1-4 | clean | mixed | small; bundle later |
| drop | vertex-alpha-not-opacity (superseded by vertex-alpha-faithful), refr-rotation (merged as #25) | | | | do not send |

Rules for each send: check the track (`two-tracks.md`), merge origin/main in and rerun CI's checks
(fmt, clippy, tests), write the body in plain words for the user with an AI label on kept text,
get the user's OK, then open the PR. After opening, watch CodeRabbit and CI and answer both.

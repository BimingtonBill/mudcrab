# Phase 2 track: task list

Owner: the Phase 2 Dev session. Goal: help upstream (Mudcrab) finish Phase 2, the way its
README and `docs/roadmap/02-completion-plan.md` describe. Details and rules live in
[`docs/pr/two-tracks.md`](../pr/two-tracks.md); this file is the short list.

**Rule in force (2026-09-25):** each send needs the user's OK. Upstream Dev turns ready branches into
PRs and handles reviews; its queue is [`docs/pr/upstream-queue.md`](../pr/upstream-queue.md). Phase 2
Dev builds and finishes the branches.

Status keys: ✅ done · 🔄 in progress · ⏸ waiting · ⬜ not started

## 1. On GitHub now (2026-09-25)

| | Item | Status |
|---|---|---|
| ✅ | PR #23: door test fixture | ✅ merged 2026-09-24 (0c4592b) |
| ✅ | PR #25: rotation fix | ✅ merged 2026-09-24 (1f0a40e) |
| ✅ | PR #29: rotation follow-up (roadmap note, test) | ⏸ open, review fix pushed (dcf48c3) |
| ✅ | PR #30: a conversion that prunes broken texture links can still complete | ⏸ open (3cd8858); glue + review fix ready locally |
| ✅ | PR #31: a resumed conversion doesn't keep outdated files | ⏸ open, review fixes pushed (e87d894) |
| ✅ | Issue #24: is lighting in scope? | ⏸ waiting for an answer |

Next candidates: the reference-shot tool (`phase2/reference-shots`, ready); point lights
(planned, `local/pr-drafts/point-lights-plan.md`).

## 2. Tonight

| | Task | Status |
|---|---|---|
| ✅ | Convert the game into upstream's format (`SkyrimConverted-upstream`): complete, 0 skipped, finished 07:14 on 2026-09-24 | done |

## 3. Tomorrow (needs tonight's conversion)

| | Task | Status |
|---|---|---|
| ⬜ | Short acceptance test run on upstream's code, to measure the one risky check (empty models) | |
| ⬜ | Look at the terrain blending fix on screen | |
| ⬜ | Look at the terrain seam fix on screen | |
| ⬜ | Look at the sun shadow fix on screen, and measure its frame-rate cost | |
| ⬜ | Convert one real distant-land file with step 1 of the distant-LOD work and check the result | |

## 4. Ready to send, held by the rule (in send order)

| | Fix | Branch | Before sending |
|---|---|---|---|
| ✅ | Conversions can finish "complete" | `phase2/pruned-texture-complete` | nothing |
| ✅ | Empty models are not errors | `phase2/empty-model-bounds` | nothing |
| ✅ | Converter version number in one place | `phase2/shared-converter-schema` | nothing |
| ✅ | Test scripts read the version number | `phase2/scripts-schema-from-source` | nothing |
| ✅ | Materials: wet look, see-through ice/rock, editor markers | `phase2/material-publication` | after the empty-models fix |
| ✅ | Trees, doors and other objects get models | `phase2/renderable-base-types` | agree the version number with the materials fix |
| ✅ | Tree leaves are not dots | `phase2/vertex-alpha-not-opacity` | after the materials fix |
| ✅ | Terrain blending | `phase2/terrain-samplers-weights` | on-screen check |
| ✅ | Terrain seams | `phase2/terrain-seam-weld` | on-screen check |
| ✅ | Sun shadows | `phase2/sun-shadow-cascades` | on-screen check and frame-rate check |

## 4b. Overnight 2026-09-23

| | Task | Status |
|---|---|---|
| ✅ | Local branch `phase2/integration`: every Phase 2 branch merged (schema 15 once, in `crates/shared`); all tests pass; release engine and converter in `C:/Modding/Mudcrab/worktrees/p2-tools/` | done |
| ✅ | Independent audit of all 12 branches ([`phase2-branch-audit.md`](../pr/phase2-branch-audit.md)) | done: 1 ready, 10 fix first, 1 needs discussion |
| ✅ | One fix worker per branch: all 11 fixed and committed on their branches (the rotation fix is local only; pushing it to #25 needs the user) | done |
| ✅ | Local `--shots` tool for the integration build (`phase2/local-shots`) | done, not run yet |
| ✅ | Reference sheets vs UESP ([`upstream-reference-sheet-2026-09-24.md`](../research/upstream-reference-sheet-2026-09-24.md)), engine-side fixes only | done; schema-15 fixes need a schema-15 conversion |
| ✅ | Tree LOD files decoded and validated ([`tree-lod-layout.md`](../research/tree-lod-layout.md)) | done |
| ✅ | Dragonborn "missing" references: an ID-space mismatch in tree LOD, not a converter bug; the 278 "lost" records are deletions by later plugins | done |
| ✅ | Distant land step 2: branch `phase2/lod-database` (world DB schema 4) | done, not yet reviewed by an independent worker |

## 5. Distant land (level of detail)

Design: [`docs/design/distant-lod.md`](../design/distant-lod.md).

| | Step | Status |
|---|---|---|
| ✅ | 1. Converter reads the distant land and object files | branch `phase2/lod-mesh-containers` |
| ✅ | 2. Database tables for distant blocks; tree billboards | branch `phase2/lod-database` |
| 🔄 | 3. Engine draws distant terrain | branch `phase2/lod-engine-terrain`, tests pass, never run on a GPU |
| ⬜ | 4. Engine draws distant objects and trees | |
| ⬜ | 5. Add it to upstream's acceptance tests | |
| ⬜ | 6. Regenerate distant land for mod-changed areas | later |
| ⬜ | Propose the design upstream (an issue) | held by the rule |

## 6. Later

| | Task | Notes |
|---|---|---|
| ⬜ | Triage the engine backlog ([`backlog-engine-and-portal.md`](backlog-engine-and-portal.md)): water, fire-lit rooms, rune stones, snow, ice, weather, auto exposure, fade-in | First check each on upstream's code; some may be fork-only |
| ⬜ | Lighting PRs | Only if issue #24 says yes |
| ⬜ | Crash hardening (fallible commands on streamed entities) | Optional; can't be triggered upstream today |
| ⬜ | Faster conversions | Research done on the portal side (`docs/research/incremental-conversion.md`) |

## 7. Finish line

| | Task | Needs |
|---|---|---|
| ⬜ | Two full acceptance campaigns (~5 h each) on upstream's code | The visual fixes merged upstream |
| ⬜ | Signed visual review | The user, on those screenshots |
| ⬜ | Phase 2 marked done upstream | An `accepted` verdict |

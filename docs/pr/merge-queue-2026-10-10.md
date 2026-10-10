# Upstream merge queue, 2026-10-10 (Upstream Dev)

Main is `c39449b5`. Under #192 (merged 0a781368) every PR needs an Approve from a CODEOWNERS human other than its
last pusher, plus CI and resolved threads. The user is a code owner: they can approve anyone else's PR, and
our own PRs need Chef, Lucas, MonoPT or Haziel. Each approve or merge is a separate yes from the user.

## A. Ready to merge now (approved, CI green, clean)

| # | PR | Author | Approved by | What's left | Ask |
|---|---|---|---|---|---|
| 1 | #198 fix(converter): read localized NPC names from string tables (fixes eclepso's #195) | Ggargur (new contributor) | Chef | merge only; a first-time contributor likely has no write access | the user merges (squash, keep co-author trailers) |
| 2 | #154 Mod Organizer 2 integration in launcher and converter | MonoPT | Chef | merge only; MonoPT has write access and may merge it himself | leave to MonoPT, or the user merges if he asks |

## B. Others' PRs waiting for a human approval (the user can be that approver)

In the order I'd review them: small and independent first, then the ones that touch our lanes.

| # | PR | Size and area | Why this position | Ask |
|---|---|---|---|---|
| 3 | #205 fix(engine): stop authored decals flickering | engine render, small | independent visual fix | Upstream Dev review, then the user's approve |
| 4 | #190 fix(converter): fail on input discovery errors | converter, small | turns silent skips into errors (a foundations "no silent fallback" fix) | review, then approve |
| 5 | #201 fix(engine): retry meshes delayed by the GPU upload budget | engine streaming/render | part of the missing-ground cause (#1055); complements our drawn bridge (analysis-1137) | review with Phase 2's analysis; approve if it holds |
| 6 | #167 Reuse verified terrain LOD chunks | converter LOD | our review posted 10-07; our supporting LAND comment still waits for the user's yes | approve after the comment question is settled |
| 7 | #171 Fix NPC faction remapping | converter | independent | review, then approve |
| 8 | #173 / #174 / #175 / #161 validation, Mutagen comparison, lavapipe GPU tests, corpus | tooling/CI | Chef's answer-key programme; no conflict with ours | batch review |
| - | #206, #210, #211 (draft), #212 streaming (#202) | engine streaming | in our #117 area: hold until the user decides the #202 coordination comment | after the #202 decision |

## C. Our open PRs and what each needs

| PR | State | Needs |
|---|---|---|
| #123 developer console | CHANGES_REQUESTED by Chef (screenshot gate + schedule test) | Phase 2's fix (impl-1135), then a fix push under the standing yes, then a reply to Chef (the user's yes) |
| #122 world load and frame timing | CONFLICTING with main (after #121) | merge main in, re-run checks, fix push (standing yes) |
| #128 go through load doors | BEHIND main | update branch; reads door links that the new door-links PR (D1 below) exports |
| #124 CenterOnExterior / CenterOnCell | waiting for a reviewer | an Approve from another code owner; stacked on #123's console |
| #125, #126 drafts | drafts | unchanged |

## D. Three new PRs being prepared (Phase 1 refreshes each on main; nothing opens without the user's yes)

| # | Branch | Issue | What it does | Notes |
|---|---|---|---|---|
| D1 | phase1/door-links-export (743b93a0, main merged) | #112, feeds #128 | the converter writes an optional `door_links` table from XTEL (destination door, arrival point, destination cell and worldspace); no schema bump | body: local/pr-drafts/pr-door-links-export.md |
| D2 | phase1/room-graph-pr (a9e9be15, main merged) | #188 converter half | interior room boxes, portal doorways, room links and room membership into four optional tables; fixes the room/portal marker FormID remap | body: local/pr-drafts/draft-188-room-graph-body.md (refresh numbers on the new head) |
| D3 | phase1/ktx2-mip-order (649fc45e, main merged), as a draft | none yet (spec fix) | KTX2 mip level data stored smallest-first as KTX 2.0 section 3.7 requires, in every writer | conflicts with #167's texture.rs rewrite; changes output bytes, so it may need the converter schema bump Chef is planning (E9): ask in the PR |

**Refresh (Phase 1, 2026-10-10):** main c39449b5 merged into all three, clean; no schema bump on any (door-links and room-graph add
optional tables). D1 vs D2: one trivial conflict at the end of the full-load-order export in `esm/exporter.rs` (keep both
blocks, room graph first); whichever lands second resolves it. Against open upstream PRs: D1 conflicts with #209
(exporter.rs); D2 conflicts with #209 (exporter.rs + pipeline-report snapshot) and #204 (snapshot only); D3 is textually clean
with #167/#204/#208/#209 but must rerun its KTX2 tests and snapshot after #167 merges. Workspace checks queued after Phase 1's
conversion hold (~22:58, cold builds, ~30-40 min each): D:/Mudcrab/evidence/p1-evidence/upstream-prep-checks-2026-10-10.txt.

Soft cap check: ours open = 4 ready (#122, #123, #124, #128) + 2 drafts (#125, #126). Adding D1 and D2 ready and D3 as a
draft gives 6 ready + 3 drafts, inside the 5-6 ready + 7-8 drafts cap.

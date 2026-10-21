# Two tracks: Phase 2 contributions and the portal feature

> **History (2026-10-16):** the user replaced the base/Plus split with one tree where every divergence from vanilla is an option defaulting to vanilla: `docs/process/one-tree-options-policy.md` and `AGENTS.md`, section "One tree". The one-way merge flow and the publishing rules below still stand; the "Plus never goes upstream" rule does not.

**Status:** agreed with the user 2026-09-23. Supersedes the "one branch for everything" layout of
`deepseek-setup` (now `portal`). The PR series itself is still designed in
[`upstream-pr-plan.md`](upstream-pr-plan.md); this document decides which of its PRs belong to which
track, and how the two tracks are kept apart.

**Hold (2026-09-23):** nothing more is sent to GitHub until maintainers respond to #23, #24 and #25.
Branches and PR bodies are prepared locally in the meantime.

**Maintainer reply (2026-09-23 21:02, realfakenerd on #24):** "Please be have patience, i am
reworking the roadmap, a lot of management changes are coming, and discord server is one of them",
with an invite to a new Discord server. No answer on lighting yet; the roadmap itself may change,
so re-read upstream's roadmap before sending anything more.

**Priority:** the Phase 2 track comes first. Portal work fills the time Phase 2 PRs spend waiting on
upstream review.

> **Current state (2026-10-02):** upstream is `Mudcrab-Team/mudcrab` (renamed from realfakenerd/Mudcrab); the 2026-09-23 send hold, the schema numbers, the conversion plans and the PR 1-12 list below are **history**. For live rules see AGENTS.md (tracks, publishing, checks, disk), `local/pr-drafts/SEND-CHECKLIST.md` and the send queue `docs/pr/upstream-queue.md`. Portal Dev may build `phase2/*` work for upstream (e.g. the door crossing, #128); it goes upstream through Upstream Dev like any Phase 2 branch.

## Why

Upstream (`realfakenerd/Mudcrab`, now **Mudcrab**) counts Phase 2 as implemented and waiting
on one thing, the single completion criterion of
[`02-completion-plan.md`](../roadmap/02-completion-plan.md): a recent, reproducible real-asset
bundle whose `acceptance-report.json` says exactly `accepted`, with a baseline and a signed visual
review of the rural, dense, water and stress scenarios. Our visual-fidelity fixes feed that review;
the acceptance campaign closes it.

Upstream also draws a phase boundary (`02-core-engine.md`, "Explicit phase boundary"): animation,
particles and physics are Phase 4. Door clips, model `Idle` clips and the walking player fall on the
Phase 4 side, so most of the portal feature is not Phase 2 work and must not be presented as such.

## The two tracks

| | Phase 2 track | Portal track |
|---|---|---|
| Purpose | contribute to upstream Phase 2, as its README, CONTRIBUTING.md and roadmap describe | the user's seamless-door / portal feature and demos; offered upstream later as a Phase 4 proposal |
| Branches | `main` (mirror of `origin/main`, never committed to) and `phase2/<topic>`, one per upstream PR, cut from `main` | `portal` (formerly `deepseek-setup`) |
| Worktree | `C:\Modding\wkj-phase2` | `C:\Modding\MudcrabProject` |
| Converted assets | `C:\Modding\Mudcrab\SkyrimConverted-upstream`, converted by the `phase2` worktree's converter | `C:\Modding\Mudcrab\SkyrimConverted` |
| Push target | `myfork` (`BimingtonBill/mudcrab`), PR to `realfakenerd/Mudcrab` `main` | never pushed |
| Commit style | upstream's: conventional commits (`fix(converter): ...`), no `[claude]`/`[deepseek]` prefix, no citations of fork-only files | this repo's: `[claude]` / `[deepseek]` prefixes |
| Worker task names | `<kind>-<nnn>-p2-<slug>`, numbers 500-599 | `<kind>-<nnn>-portal-<slug>`, numbers 100-199, then 600+ |

### The flow is one-way

```
origin/main ──► main ──► phase2/<topic> ──► upstream PR ──► origin/main
                  │             │
                  └─────────────┴──────────► portal   (merge only; nothing flows back)
```

- `portal` merges `main` after every upstream merge, and merges a `phase2/*` branch early when the
  portal work needs the fix before upstream takes it.
- A `phase2/*` branch that **ports a fix `portal` already has** (most of the list below) is not
  merged into `portal` early: the two versions would only conflict twice. It reaches `portal` once,
  through `main`, and the merge keeps whichever version upstream accepted. Only new Phase 2 work
  (the shared schema constant, for example) is merged early.
- Nothing is merged from `portal` into `main` or `phase2/*`. Code that exists only on `portal` reaches
  upstream through a new `phase2/*` branch built by cherry-pick or rewrite and cleaned per
  `upstream-pr-plan.md` §4-§5.
- **Upstream first:** a Phase 2 bug found while working on the portal (the glossiness exponent, the
  foliage vertex alpha, the shadow cascades) is fixed on a `phase2/*` branch against upstream code
  and assets, then merged into `portal`. It is not fixed on `portal` first.

### Assets are per track

The engine refuses assets from another schema. Upstream's Phase 2 expects database schema 3,
converter manifest 14 and cell cache 3 (`02-core-engine.md`, "Compatibility"); `portal` is at DB 5 /
converter 18. Each track converts with its own converter into its own directory. A `phase2/*` PR
that bumps a schema also updates that Compatibility section.

**Disk is the blocker.** On 2026-09-23 `C:\Modding\Mudcrab\SkyrimConverted` was 73.8 GB, the portal `target/`
19.4 GB, and C: had 43 GB free. A second full conversion does not fit beside the first. The user is
freeing 40+ GB (2026-09-23). Order: the portal session finishes its post-impl-064 reconversion
first and reports its peak staging space; only then does the Phase 2 session build a `target/` in
`wkj-phase2` and convert `SkyrimConverted-upstream`.

## Tonight's upstream conversion (planned 2026-09-23)

Converter: `C:\Modding\Mudcrab\worktrees\p2-tools\converter-upstream14-pruned.exe`, built from
`phase2/pruned-texture-complete` (upstream `main` plus PR 3, schema 14 unchanged), so the set ends
`complete` and doubles as PR 3's real-install evidence. Command:
`converter-upstream14-pruned.exe "<Skyrim SE>\Data" C:\Modding\Mudcrab\SkyrimConverted-upstream --cpu-jobs 6 --io-jobs 4`.
A fresh run re-encodes every texture (hours). Announce "converting" to Portal Dev first; no new
builds on either track until "past conversion"; nothing reads the output or staging folders;
budget about 75 GB.

Result: complete 2026-09-24 07:14, 242,969 converted, 0 skipped, integration passed.

## The schema-15 conversion (2026-09-24)

With 116 GB free, a second set goes into a **new** folder, `C:\Modding\Mudcrab\SkyrimConverted-upstream15`,
so the schema-14 set stays as PR 3's evidence. Converter `C:\Modding\Mudcrab\worktrees\p2-tools\converter-integration-lod.exe`,
built from `phase2/integration-lod` (the integration branch plus distant LOD steps 1-3): converter
schema 15, world DB schema 4. One run lets the reference sheet show trees, plants and the material
fixes, and gives LOD step 3 its first database. It carries the vertex-alpha branch that is still
waiting on discussion, so the set is for looking and testing, not PR evidence. Same command shape
and the same rules as above; Portal Dev agreed.

## Which work goes where

Numbers are the PRs of `upstream-pr-plan.md` §3. Order is the proposed submission order.

### Phase 2 track

Order revised after research-079 ([`phase2-fix-audit.md`](phase2-fix-audit.md)), which checked each
fork fix against upstream code. Items 1-2 unblock acceptance itself, so they go first.

1. **PR 1** `test(dummy-content)` interior + door pair - open upstream as #23, waiting for review.
2. **PR 2** `fix(shared): Creation reference angles turn clockwise` - opened as #25 (branch
   `phase2/refr-rotation`, body in [`pr-02-body.md`](pr-02-body.md)); CI-equivalent checks passed locally.
3. `fix(converter)`: a pruned dangling texture does not make a conversion incomplete (`23f68f6`).
   Branch `phase2/pruned-texture-complete` (impl-083), body [`pr-03-body.md`](pr-03-body.md); CI-equivalent checks pass (2026-09-23), held.
   Upstream's own `8b5bf80` records each pruned URI as a skip, so a stock install never converts
   `complete` and the acceptance preflight can never pass. No schema bump.
4. `fix(engine)`: an empty converted model is not a bounds failure (`2b69616`). No bump. Branch
   `phase2/empty-model-bounds` (impl-501), body [`pr-04-body.md`](pr-04-body.md); CI-equivalent checks pass (2026-09-23), held.
5. `refactor(shared)`: one converter manifest schema version. Upstream defines it twice
   (`converter/src/cache.rs` `CONVERTER_SCHEMA_VERSION` and a hand-synced copy in
   `engine/src/app.rs` `converter_schema_version()`); move it to `crates/shared` beside
   `WORLD_DATABASE_SCHEMA_VERSION`. Goes before the first schema bump, and makes each bump a
   one-line conflict when `portal` merges (Portal Dev's proposal A, accepted). Branch
   `phase2/shared-converter-schema` (impl-502); CI-equivalent checks pass (2026-09-23), held.
6. `fix(converter)`: material publication - glossiness exponent, vertex alpha is not blend, blend
   factors, editor markers (`59f1f20`, `b4ce0d9`). One schema bump (14 -> 15), one reconversion.
   Branch `phase2/material-publication` (impl-506, `5ea5f79`), also updating the acceptance
   preflight and spec/roadmap schema lines. **Send after item 4:** marker-only models become empty
   scenes, which upstream's bounds gate fails until item 4 lands. The published blend factors are
   not read by any runtime yet.
7. `fix(engine)`: vertex-colour alpha is not opacity (`abaa14f`), engine-side, no bump. Branch
   `phase2/vertex-alpha-not-opacity` (impl-504). **Send only after item 6 lands:** the rewrite is
   global, which is right only once `SLSF1_VERTEX_ALPHA` no longer makes a material Blend
   (`b4ce0d9`). On upstream today, those Blend materials use vertex alpha as opacity, and forcing
   it to 1.0 would change them.
8. `fix(engine)`: terrain blending - repeating samplers, bilinear weights (`0ece570`); then the seam
   weld as its own PR. No bump. Branch `phase2/terrain-samplers-weights` (impl-505). **Needs a GPU
   run before sending:** no test compiles the WGSL, so it waits for upstream-schema assets.
   The seam weld is branch `phase2/terrain-seam-weld` (impl-508): tolerance 512 units (four sample
   steps), normals of moved points recomputed. The fork's dry-cell water change was left out:
   upstream already skips cells with no water height.
9. `fix(engine)`: sun shadow cascades sized for Creation units (`8bd7a01`). Branch
   `phase2/sun-shadow-cascades` (impl-507). **Needs a GPU run and a benchmark before sending:** the
   shadow pass now draws the streamed world (and most of the 250k-instance benchmark field), and the
   acceptance camera at 20,000 units sits beyond the last cascade, so its screenshots can't show it.
9b. `feat(converter)`: catalogue six more renderable base types (DOOR/ACTI/FLOR/CONT/TREE/LIGH
    models into `statics`; the half of fork `df91524` without the XTEL door links). Branch
    `phase2/renderable-base-types` (impl-511): 125,874 TREE references gain a model, which the
    visual review needs; the PR #23 reviewer asked for it for DOOR. It bumps the converter schema
    14 -> 15, as `phase2/material-publication` does: whichever is sent second bumps to 16.
10. `fix(scripts)`: `phase2-visual-baseline.ps1:46` requires manifest schema 12 and throws on
    current assets (found by research-510, [`phase2-acceptance-runbook.md`](phase2-acceptance-runbook.md)).
    Branch `phase2/scripts-schema-from-source` (Claude): both Phase 2 scripts read
    `CONVERTER_SCHEMA_VERSION` from the Rust source instead of hard-coding it.
10b. Optional, last: fallible commands on streamed entities (`782f8dd`, 13 upstream sites). No crash
   is reachable on upstream today; send only as hardening, if at all. On `portal` it held in real
   play: 12 load-door crossings, 0 panics (2026-09-23).
11. **Lighting**, only if upstream says yes on issue #24 (see below).
12. **The acceptance campaign** (Marco 7), once the fixes above are merged: three release
    repetitions of each scenario from a clean upstream commit with upstream-schema assets, a
    baseline, and the user's signed `visual-review.json`. Only hashes and reports are posted, never
    assets. Ask the Phase 2 maintainer (MacLove13) on an issue first whether our hardware counts as
    target hardware.

### Phase 2 candidates, not started

- **Level of detail and distant terrain** (user decision 2026-09-23, recorded by Portal Dev in
  `docs/design/portal-performance.md`): Skyrim's own distant models (the converter's `lod` table),
  tree impostors and a real distant-terrain system belong to this track. Upstream's Phase 2 goal
  names "massive render distances". The existing terrain ring is a testing aid, not the design.
  Design: [`docs/design/distant-lod.md`](../design/distant-lod.md) (research-513): convert Skyrim's
  shipped LOD as-is first (the LOD textures are already converted; `.btr`/`.bto` are ordinary SSE
  NIFs missing only `BSMultiBoundNode`), then regenerate only mod-changed blocks. Six PRs; the
  first is converter-only. Needs an upstream issue before any code, under the send hold.
  Step 1 is built: branch `phase2/lod-mesh-containers` (impl-514). All 9,584 `.btr` and 1,078 `.bto`
  shipped files parse; the vendored `BSSubIndexTriShape` layout was wrong and is replaced by the
  measured one. No schema bump (new outputs only), but about 233 Creation Club meshes that use
  `BSMultiBoundNode` change hierarchy (2 change transform): a reviewer may ask for a bump. The
  design's "level-4 terrain LOD has vertex colours" is false for vanilla (0 of 11,217 shapes).

- **The main-engine backlog** the user and Portal Dev drew up (2026-09-23):
  [`docs/roadmap/backlog-engine-and-portal.md`](../roadmap/backlog-engine-and-portal.md). Water as a
  white sheet, dark firelit interiors, glowing rune stones, snow, ice (environment maps), weather,
  eye adaptation, fade-in instead of popping, faster conversions. Unscheduled. Before any of it
  becomes Phase 2 work, check whether the defect exists on upstream `main` or only in the fork's
  lighting (several may come from fork-only code such as the sky-fill ambient).

- **`plugins.priority` is not the FormID slot** for plugins after the first light plugin
  (research-531, [`dragonborn-formid-slots.md`](../research/dragonborn-formid-slots.md)): anything that
  decodes a FormID through `plugins.priority` is off by one. Small upstream fix candidate.
- ~~274 Skyrim.esm and 4 Update.esm records are lost in conversion~~ **Not a bug** (research-533):
  every one is deleted by a later official plugin (record flag 0x20) and the merge correctly drops
  it. Optional hardening only: record deletions visibly (a `deleted` column) instead of silently.

### Portal track

- Door links from `XTEL` (PR 3's other half), interiors and the active cell (PR 6), seamless
  crossing with the carried pose, doorway portal rendering (PR 7).
- Door open/close clips (PR 5's door half, PR 8) and model `Idle` clips - Phase 4 animation.
- First-person player, walking, auto-load doors (PR 12's other half) - Phase 4 gameplay.
- Demos (Alftand/Blackreach, Riverwood), `--demo-tour`, reference shots, launchers, `docs/demo/`.
- Everything on the exclusion list of `upstream-pr-plan.md` §4: `AGENTS.md`, `tasks/`, `local/`,
  the DeepSeek harness, research notes and research scripts.

Later, the portal work is offered upstream as a **Phase 4 proposal** (the user agreed): an issue with
the design and a short demo capture, then PRs cut from `portal` the same way as Phase 2 ones. PR 1
(#23) already gives upstream a way to test doors without game data.

## Keeping the code apart on `portal`

Merging `main` into `portal` stays cheap only if the portal feature touches few upstream files.
Refactor on `portal` (worker task `portal-` prefix) so that:

- all portal-only systems register through one `PortalPlugin` (doorway rendering, crossing,
  door animation, model animation, player, demos, demo tour, reference shots);
- `main.rs` stays byte-identical to upstream (it only calls `engine::run`), and `run()` in `app.rs`
  adds the portal through exactly one `add_plugins(PortalPlugin)`; `config.rs` differs by one
  `PortalOptions` block plus two lines in `from_args` (design: `docs/design/portal-plugin.md`);
- Phase 2-shaped code on `portal` moves into the modules it will go upstream as, so a new file
  carries it instead of `app.rs`: `atmosphere.rs` (sky, fog, ambient, per-space lighting; the
  lighting proposal of issue #24) and `terrain_ring.rs` (the distant terrain ring, which the user
  calls a testing aid, not the design). The sun shadow
  cascades stay in `app.rs`, because their upstream PR is a small edit there;
- streaming, rendering and material code carry no portal branches; where the portal needs a hook,
  the hook is a small, general Phase 2-shaped seam (an event, a resource) that could go upstream on
  its own.

Acceptance: `git diff main portal -- crates/engine/src/main.rs` is empty and `config.rs` shows only the
`PortalOptions` block; the Riverwood and Alftand demo tours still pass (8/8 and 4/4 crossings, 0 dark frames).

## Rules for every Phase 2 PR

From upstream's CONTRIBUTING.md and README, plus what the fork has learnt:

- an issue first (search, then open), then a `phase2/<topic>` branch;
- `///` doc comments on new functions;
- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`, and the
  tests of the touched crates;
- a test that needs no game data (dummy-content fixtures), since upstream CI has no Skyrim install;
- the description references the planning doc it serves (`02-completion-plan.md` Marco N, or a
  `docs/specs/` file), uses the Mudcrab name, and ends with the attribution line;
- no machine paths, no private names, no citations of `docs/research/` or `tools/research/`;
- no Bethesda data in the diff or the description.

## Process

- The handoff log tags each entry `[phase2]` or `[portal]`.
- Worker briefs state the track, the worktree, the assets directory and the branch.
- Engine instances run in parallel; the two exceptions (timing runs, and a conversion publishing
  into a folder an engine is reading) are in AGENTS.md, "While working".
- Opening issues and PRs upstream is publishing: the user confirms each one.

## Issue: lighting in the Phase 2 visual review (posted as Mudcrab-Team/mudcrab#24, 2026-09-23)

> **Title:** Is static lighting in scope for the Phase 2 visual review?
>
> Right now, lamps, torches and glowing materials don't light anything, and per-cell lighting
> settings aren't read. That makes interiors and dusk scenes look flat. We have a working
> implementation. Should it be part of Phase 2, or wait for a later phase?

## Send-order notes found by integration (2026-09-24 night)

- `phase2/resume-staging-validation` and `phase2/pruned-texture-complete` need a small glue commit
  when both are in (integration branch, "record a pruned mesh's new bytes"): pruning rewrites a
  mesh after its hash was recorded, and the resume check then distrusts it. Whichever lands second
  carries that commit.
- `phase2/staging-hard-links` relies on no staged output being written in place; it adds the
  unlink before conversion that makes script output safe.
- The effect-shader emissive fix rides `phase2/material-publication`'s unreleased schema 15.

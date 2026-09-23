# Upstream pull-request plan

**Status:** research note, 2026-09-23 (research-054). Full re-audit of everything this fork has done,
and the design for presenting it to `realfakenerd/OpenSkyrim` — now **Wah Krah Jol** — without
publishing anything yet. Replaces research-029's plan of the same name, which covered
`8919096..3ae5541`.
**Revision audited:** `8919096..449af90` (147 commits; 131 on this branch, 16 brought in by the
`8c21447` upstream merge). Working tree **clean** apart from one stat-only phantom entry
(`crates/engine/src/doors.rs` shows as modified in `git status`; `git diff` is empty, so the content
is identical — a stale index stat, not a change).
**Rebase target:** `origin/main` = `2105bfd`, fetched 2026-09-23 08:39 local time. **This is 7 commits
past what this branch merged** (`0ec2d36`) — see §0.1.
**Nothing here has been pushed, and no branch was created.**
**Method:** `git log`/`git diff` over `8919096..HEAD`, `8919096..origin/main`, `0ec2d36..origin/main`
and `origin/main...HEAD`; `git grep` over the tracked tree; `git ls-files`; `cargo fmt --all --
--check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test
--workspace`; `cargo test --release -p dummy-content -- --ignored performance --test-threads=1`;
direct reads of `C:\Modding\SkyrimConverted` (manifest, integration report, one door `.glb`). Windows,
warm `target/`.

---

## 0. What changed since research-029, in order of how much it matters

1. **The rebase target moved again, and it eats one of the planned PRs.** `origin/main` is now
   `2105bfd`, 7 commits past `0ec2d36`:
   - `114339c` *fix: complete Creation Club asset conversion (84 failures to zero)* **already
     implements the old PR 2** (BSA zero-padded filename tables, same archive cited, same
     zero-only semantics, plus a trailing-garbage test). **Drop our BSA change entirely** — see
     §3.0. It also adds Creation Club file flags we do not have.
   - `8b5bf80` *fix: prune dangling texture URIs from converted GLBs* rewrites 397 lines of
     `crates/converter/src/mesh.rs`, which is our material-fidelity PR's file.
   - `96a730c`/`39070d8` rework `crates/converter/src/pipeline.rs` (skip failed assets, redo list).
   - `d46a179` **rebrands the project to "Wah Krah Jol"** and adds `LEGAL.md`; `2105bfd` changes
     `FUNDING.yml`. Every PR body, title and README touch must use the new name, and our README
     edit conflicts with it.

   The complete drift, `git diff --stat 0ec2d36 origin/main` (11 files, +975 / −65), and what it
   collides with:

   | File | Drift | Collides with |
   |---|---|---|
   | `crates/converter/src/archive/bsa.rs` | +59 | **the whole of our BSA fix — dropped** (§3.0) |
   | `crates/converter/src/mesh.rs` | +411 | PR 10 (`mesh.rs` is ours too — rebase by hand) |
   | `crates/converter/src/material.rs` | +11 | PR 10 (different region, but check) |
   | `crates/converter/src/pipeline.rs` | +226 / −37 | PR 5 (our change there is **one line**) |
   | `crates/converter/src/config.rs` | +32 | nothing — we never touch it |
   | `crates/engine/src/bin/world-inspect.rs` | +22 | nothing — we never touch it |
   | `README.md`, `LEGAL.md`, `.github/FUNDING.yml`, `devenv.lock`, `vendor/**/shader_texture_set.rs` | +231 | our README bullet (excluded anyway) |

   Everything else in the fork's footprint is untouched upstream, so the rebase is mechanical apart
   from the rows above (four code files plus a README hunk that is excluded anyway).
2. **The cleanup landed in the code and is verified — but only in `crates/`.** `git grep` over
   `crates/` finds no `local/`, no `impl-NNN`, no machine path, no `SKYRIM_DATA_DIR`; the two
   fallbacks ADR-0002 forbids are gone and `OPENSKYRIM_SKYRIM_DATA` / `OPENSKYRIM_CONVERTED_DIR`
   are respected with a printed skip. **`docs/` and `tools/` were not swept** — 22 research scripts
   and 11 research/handoff notes still carry `C:/Modding/SkyrimConverted`, and production doc
   comments now cite `docs/research/*` and `tools/research/*.py` **files that the exclusion list
   keeps out of the PR** (new problem, §2.6).
3. **The reconversion happened. Almost nothing is inert any more.** `C:\Modding\SkyrimConverted` is
   **converter schema 17, `complete: true`**, integration report **schema 5**; the database carries
   `space_lighting`, `statics.material_object`, `matos`, `door_links` and animation data, and
   `meshes/dungeons/dwemer/door/dwemerlargedoorload01.glb` contains an `"Open"` clip. Door
   animation, per-space lighting and directional snow are therefore live on real data — but three
   module docs still say the opposite (§5.1).
4. **The gate that blocked everything is open.** `crates/dummy-content` now generates an interior
   cell and a reciprocal load-door pair (`40e0eb5`) and `crates/converter/tests/fixture_doors.rs`
   runs that preset through the real parser, exporter and `validate_database` **with no game
   install**. A maintainer can now review and run the door, portal and crossing work. This is the
   single most valuable PR in the series and it should go first.
5. **The work is bigger and cleaner than the last audit measured.** ≈196 files, +54,860 lines of our
   own (upstream contributed 38 files / +4,071 of the range total). `#[test]` count in `crates/`:
   **156 at the fork point → 519 at HEAD (+363)**. `cargo fmt`, the exact CI clippy command and the
   CI performance budgets all pass here, and `cargo test --workspace` is 507 passed / 29 ignored /
   0 failed.

---

## 1. Inventory

**Range `8919096..HEAD`:** 147 commits, 234 files, **+58,931 / −645**. No file was renamed or
deleted (`git diff --name-status --diff-filter=RD` over the range is empty), so nothing upstream
had is being removed outside `bsa.rs` (replaced below).

**Split:** 131 commits are on this branch's first-parent line (130 authored + the `8c21447` merge);
16 are upstream's, of which 14 are non-merge. Upstream's own tree delta is **38 files, +4,071 /
−174** (`git diff --shortstat 8919096..0ec2d36`) — a new `crates/dummy-content` crate, `docs/adr/`,
two CI jobs, `devenv.lock`, a vendored `shader_texture_set.rs` and their test suites. **Our side is
≈196 files and +54,860** (the two figures are the range total minus upstream's delta; they overlap
on the handful of files both sides changed, `bsa.rs` above all).

| Area | Files | Lines | Commits touching it (ours / upstream) | What it is |
|---|---|---|---|---|
| `crates/engine` | 21 | +23,602 / −434 | 45 / 0 | the runtime: crossings, portals, doors, player, lighting, terrain, shots, demo |
| `crates/converter` | 19 | +7,349 / −198 | 11 / 7 | plugin parsing, door links, lights, space lighting, snow, NIF animation, materials |
| `crates/dummy-content` | 15 | +3,356 | 1 / 10 | upstream's fixture generator + our interior/`XTEL` extension |
| `crates/shared` | 2 | +33 / −9 | 3 / 0 | `coordinates.rs` (rotation), `WORLD_DATABASE_SCHEMA_VERSION` 3→5 |
| `crates/launcher` | 0 | 0 | 0 / 0 | **untouched in the range** |
| `Cargo.toml`, `Cargo.lock` | 2 | +179 / −4 | 0 / 1 | upstream's own `947fc08`: `crates/dummy-content` joins the workspace (+ its lock entries). **Our side never touched a manifest — no dependency was added by this fork** |
| `tools/` | 63 | +12,998 | 26 / 0 | 6 harness tools, `compare_shots.py`, 52 `tools/research/*.py`, 2 UESP pose files, 2 demo scripts |
| `docs/` | 37 | +6,016 | 27 / 0 | ours: 4 designs, 12 handoff notes, 8 research notes + 1 data file, 1 demo guide, this plan; upstream's: the 8 ADRs, `docs/adr/README.md` and `docs/specs/converters/dummy-content.md` (10 files) |
| `tasks/` | 61 | +3,641 | 42 / 0 | the DeepSeek briefs, prompts, templates, schemas |
| `tests/` | 5 | +1,506 | 4 / 0 | Python tests for the harness and the comparison tools |
| `.github/` | 1 | +43 | 0 / 1 | upstream's new security + performance CI jobs (not ours) |
| root | 8 | +208 | 19 / 0 | `AGENTS.md` (+137), `.deepseek-agents.json`, `CLAUDE.md`, `.gitignore` (+8), `README.md` (+5), 2 `.cmd` launchers — plus upstream's `CONTRIBUTING.md` (`f6235d2`) |

Per-area `git diff --shortstat` figures sum exactly to the range total (234 files, +58,931), so
nothing is double-counted.

### 1.1 Our commits by theme

Counted by subject matter over the 130 authored commits (the same commit often writes code, a brief
and a handoff note; it is counted once, by what it changed).

| Theme | Commits (≈) | Where the proof is |
|---|---|---|
| engine features (crossings, portals, doors, player, lights, render, terrain, shots, demo tour) | 44 | `crates/engine/src/**` (45 commits touch the crate; `874b723` is counted under process) |
| converter features (door links, lights, space lighting, snow, NIF animation, materials) | 11 | `crates/converter/src/**` (18 commits touch the crate, 7 of them upstream's) |
| shared contract (rotation convention, schema constants) | 3 | `crates/shared/src/**`, `crates/engine/src/app.rs` |
| `dummy-content` extension for upstream | 1 | `40e0eb5` |
| research notes and their scripts (`docs/research/`, `tools/research/`) | 22 | 8 notes, 52 scripts |
| process: briefs, handoffs, queue, PR plan, delegation blocks, module stubs | 41 | `tasks/deepseek/**`, `AGENTS.md`, this file |
| harness/tool sync (`.deepseek-agents.json`, `tools/ds_*`, `tools/check_*.py`, `tests/test_*`) | 8 | private |

The classification is mine and is approximate at the margin (a commit that writes a stub *and* its
brief is counted once, under process), but it sums to the 130 authored commits.

### 1.2 Tests

`cargo test --workspace` on this checkout, all measured today:

| Target | ran | passed | ignored |
|---|---|---|---|
| `converter` lib | 164 | 145 | 19 |
| `converter` bins (8 binaries) | 11 | 11 | 0 |
| `crates/converter/tests/fixture_doors.rs` | 3 | 3 | 0 |
| `crates/converter/tests/fixture_round_trip.rs` | 8 | 7 | 1 |
| `dummy_content` lib + bin | 45 | 45 | 0 |
| `dummy_content/tests/performance.rs` | 4 | 0 | 4 (opt-in) |
| `engine` lib | 275 | 273 | 2 |
| `engine` bins | 2 | 2 | 0 |
| `launcher` | 3 | 3 | 0 |
| `project_wormhole_nif` (vendored) | 12 | 11 | 1 |
| `shared` | 7 | 7 | 0 |
| doctests (2 engine, rest empty) | 2 | 0 | 2 |
| **total** | **536** | **507** | **29** |

Every one of the 29 ignored tests is opt-in and says so in its `#[ignore = "…"]` text
(`requires OPENSKYRIM_SKYRIM_DATA …`, `reads the converted Skyrim door models
(OPENSKYRIM_CONVERTED_DIR)`, `performance`), and each prints `skipping: set …` and returns when the
variable is absent. **CI runs all of them as skips and is green.** The heaviest files:
`streaming.rs` 42 tests, `door_animation.rs` 39, `player.rs` 32, `mesh.rs` 21,
`world/database.rs` 21, `portal.rs` 20, `app.rs` and `render.rs` 18, `exporter.rs` and `shots.rs` 17,
`nif_animation.rs` 16, `lights.rs` and `material.rs` 13, `bsa.rs` 12, `transition.rs`,
`world/lighting.rs` and `snow.rs` 11.

The fork's only new `crates/*/tests/` integration test is `fixture_doors.rs` (3 tests, no Skyrim
needed) — plus upstream's own `fixture_round_trip.rs`, which the merge brought in. It is
upstream-style, driven by `dummy-content`, and it is the model every other PR should follow.

---

## 2. Audit, as an upstream maintainer would run it

### 2.1 The exact CI commands

`.github/workflows/run_tests.yml` (upstream's, unchanged by us) runs five jobs on `ubuntu-latest`:
`format`, `clippy`, `test` (nextest), `security` (`cargo audit`), `performance`, gated by
`tests_pass`. What happens when I run each here:

| CI job | Command | Result here |
|---|---|---|
| format | `cargo fmt --all -- --check` | **passes**, no output (research-029 could not run this) |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **passes**, exit 0. No crate declares `[features]` (grep over every `Cargo.toml`), so `--all-features` is a no-op and this *is* the CI command. The only diagnostic is the pre-existing `proc-macro-error2 v2.0.1` future-incompat warning upstream also has. |
| test | `cargo nextest run --workspace --all-targets --no-fail-fast --no-tests=warn` | `cargo test --workspace`: **536 tests, 507 pass, 29 ignored, 0 fail**, exit 0. `cargo nextest` itself: **not verified** (§8). |
| security | `cargo audit --ignore RUSTSEC-2026-0194 --ignore RUSTSEC-2026-0195` | **not run** (§8). Note the fork adds no dependency, so it cannot introduce an advisory. |
| performance | `cargo test --release -p dummy-content -- --ignored performance --test-threads=1` | **passes**, 4/4, exit 0 (0.13 s) |

Still unproven, and no PR body should claim otherwise: **nothing here has ever been built on Linux**,
which is the only platform CI uses. The one Windows-specific block
(`crates/engine/src/app.rs`, benchmark process priority via `windows_sys`) has a
`#[cfg(not(windows))]` twin, so nothing obviously breaks, but it is untested. The lead should run
the five CI commands on the rebased branch before pushing the first PR.

### 2.2 No game data, no large files

`git ls-files` contains **no** `.nif`, `.dds`, `.esm`, `.esp`, `.esl`, `.bsa`, `.ba2`, `.glb`,
`.gltf`, `.ktx2`, `.pex`, `.rkv`, archive or executable. The only binaries in the tree are the two
**pre-existing upstream** screenshots (`docs/images/phase2-milestone-1-rendering-progress.png`
2.4 MB, `docs/images/phase2-rural-skyrim-assets.png` 1.9 MB), neither touched by this fork. The
largest tracked file we added is text: `crates/engine/src/streaming.rs` (245 KB), then
`door_animation.rs` (162 KB), `player.rs` (108 KB), `portal.rs` (105 KB),
`tools/research/rotation_convention.py` (39 KB), `docs/research/data/object-rotation-audit.json`
(510 lines). Nothing needs Git LFS.

### 2.3 Private names and machine paths — verified clean in `crates/`, not in `docs/` or `tools/`

`git grep` results over the tracked tree:

| Pattern | `crates/` | `tools/` | `docs/` | `tasks/`, root |
|---|---|---|---|---|
| `brannock` (case-insensitive) | none | none | `docs/design/animated-doors-and-seamless-crossing.md:23` | `AGENTS.md:14`, `.deepseek-agents.json:3`, 8 more files under `tasks/` |
| `C:/Modding` | none | **22 scripts** | 11 notes | 38 files under `tasks/` |
| `deepseek` | none | harness tools + `check_citations.py` | `docs/handoff/**`, `docs/research/{visual-gaps-spec,transparent-and-misplaced-meshes,phase2-status}.md`, `docs/design/animated-doors-and-seamless-crossing.md:3` | `AGENTS.md`, `.deepseek-agents.json`, 42 briefs |
| `impl-0NN` / `research-0NN` / `docs-0NN` | **none** | none | none | briefs |
| `local/` | **one**: `crates/engine/src/app.rs:1533` (`local/reference/cmp-s17/SR-place-Alftand_Ruined_Tower.jpg`) | none | none | `AGENTS.md`, this file |
| `OPENSKYRIM_SKYRIM_DATA` / `OPENSKYRIM_CONVERTED_DIR` | 34 sites, all with a printed skip and no fallback | — | — | — |
| `SKYRIM_DATA_DIR` (the old invented name) | **gone** | gone | gone | gone, except as history in this file |
| `dbg!`, `todo!()`, `unimplemented!()`, `FIXME`, `TODO`, `HACK` | none (the one `XXXX` hit in `esm/extractors.rs:21` is a record-tag comparison and is upstream's file, untouched) | none | none | none |

So the four cleanup commits (`410ce31`, `62c0f7b`, `944250e`, `432c152`) did what they claimed in the
crates — with **exactly one miss** (`app.rs:1533`) and one class of reference they could not have
removed (see §2.6). The docs and scripts the brief suspected were never in their scope.

By every commit in the range: `git log --format=%ae 8919096..HEAD` is
`90541801+BimingtonBill@users.noreply.github.com` (a GitHub noreply address) for ours and the
upstream authors' own addresses for the 16 merged commits. **No private email is in the history.**

### 2.4 Code hygiene

- No `dbg!`, no new `TODO`/`FIXME`/`HACK`/`todo!`/`unimplemented!` anywhere in `crates/`.
- Magic numbers are named and *derived*: `lights.rs` derives the light exposure from
  `INTERIOR_AMBIENT_BRIGHTNESS` and documents the `4π`; `config.rs` justifies
  `DEFAULT_TERRAIN_RADIUS = 8` with a measured FPS table and keeps benchmark runs on the old read
  distance; `door_animation.rs` derives its swing threshold from `DOORWAY_CLEAR_DEGREES`. This is
  better documented than most upstream code.
- `#[allow(clippy::too_many_arguments)]` has grown with the new modules (it is the standard Bevy
  escape hatch and pre-dates the fork in `metrics.rs`/`profiling.rs`), but a reviewer will notice
  the count.
- **The duplicated schema constant is still duplicated and still untied**:
  `crates/engine/src/app.rs:1344-1348` `const fn converter_schema_version() -> u32 { 17 }` versus
  `crates/converter/src/cache.rs:11` `CONVERTER_SCHEMA_VERSION: u32 = 17`. Nothing fails when they
  drift, and the two have been bumped together by hand three times in this fork (15, 16, 17).
- Three module docs are now **stale about the data**: `world/lighting.rs:29` ("that is the state of
  the shipped assets until the schema-16 reconversion"), `snow.rs:13-15` ("every database converted
  before schema 16, which is what ships today") and `door_animation.rs:15` ("until the assets are
  reconverted none does"). The reconversion happened; the shipped data is schema 17 and has the
  tables and the clips.

### 2.5 Commit and documentation style

Our subjects are `[claude] …` / `[deepseek] …` with a `Co-Authored-By: Claude Opus 5
<noreply@anthropic.com>` trailer. Upstream's are Conventional Commits with a scope
(`fix(dummy-content): …`, `docs(adr): …`, and the newest are plain `fix:`). The message *content* is
good — it quotes measurements, names the check that was run and admits what was not — and should
survive; the framing must not. `docs/` layout: upstream has `docs/adr/` and `docs/specs/`; we added
`docs/design/`, `docs/research/` and `docs/handoff/`, plus `docs/demo/` and `docs/pr/`.

### 2.6 The new citation problem: code that points at files we will not ship

The old audit's list of 20 private references is obsolete and has been superseded by a different
one. The cleanup replaced `local/…` and `impl-NNN` citations with in-repo ones — and the in-repo
ones are `docs/research/*.md`, `docs/design/*.md` and `tools/research/*.py`, all of which are on the
exclusion list. A `git grep` for those three prefixes plus `local/` over `crates/` returns **46
sites** (one of which, `streaming.rs:1843`, is the phrase "local/global transform" and is not a
citation at all) — so **44 citations to the notes and one to a `local/` path**. Current state
(production code, not tests):

| File(s) | Cites |
|---|---|
| `crates/engine/src/door_animation.rs` (7 sites: `:38,110,186,3083,3262,3385,3453`) | `tools/research/load_door_twin_coverage.py`, `tools/research/door_animation_nif.py` |
| `crates/engine/src/world/lighting.rs:9,497`, `crates/converter/src/esm/lighting.rs:5,7,351`, `crates/converter/src/esm/directional_material.rs:6,10`, `crates/engine/src/{lights,render,portal,app,snow}.rs` (10 more) | `docs/research/visual-gaps-spec.md` |
| `crates/converter/src/mesh.rs` ×3, `material.rs` ×1 | `docs/research/transparent-and-misplaced-meshes.md` |
| `crates/converter/src/esm/exporter.rs:2337-2338`, `crates/engine/src/{transition.rs:170,config.rs:320,demo_tour.rs:7}` | `docs/research/worldspace-transition-demo.md` |
| `crates/engine/src/lights.rs:2`, `player.rs:2,20`, `transition.rs:1-2`, `world/lighting.rs`, `crates/engine/src/config.rs:58`, `shots.rs:4,680` | `docs/design/{lights-and-auto-doors,blackreach-demo,animated-doors-and-seamless-crossing,reference-shots}.md` |
| `crates/engine/src/app.rs:1533` | `local/reference/cmp-s17/SR-place-Alftand_Ruined_Tower.jpg` — **the one `local/` citation the cleanup missed** |

Two ways out, per PR: **carry the note** it points at (the design notes are 57–565 lines and are
excellent; the research notes are 111–471), or **replace the citation with the fact** (a number, a
record name, a sentence). The engine's flat `docs/design/blackreach-demo.md` (79 lines) is a genuine
design document and belongs in a PR; `docs/research/visual-gaps-spec.md` is signed as a merged
*worker* report and needs its two process paragraphs cut. This is now the main de-coupling job in
the series (§5).

---

## 3. The PR series

**Recommendation unchanged: re-cut, do not push this history.** 130 authored commits interleave
private tooling with code, 41 of them exist only to write a brief or a handoff note, and several
touch `app.rs`/`streaming.rs`/`cache.rs` in the same commit that bumps a schema constant. Branch
off `origin/main` and carry the *content* of the commits below as conventional-commit subjects with
no `[claude]`/`[deepseek]` prefix and no trailer.

**13 PRs, in sending order.** 1–5 are independent of each other and can be prepared in parallel;
6–13 are one stack in which each is developed against the previous one merged (they all touch
`app.rs`, `streaming.rs`, `config.rs` or `world/database.rs`, which together are edited by 21, 14,
6 and 8 commits respectively).

| # | Title | Commits | Depends on | Needs Skyrim to test? | Runtime evidence |
|---|---|---|---|---|---|
| **1** | `test(dummy-content): an interior cell and a linked load-door pair` | `40e0eb5` | — | no | `fixture_doors.rs` 3/3 through the real parser + exporter |
| 2 | `fix(shared): Creation reference angles turn clockwise` | `933b95f` | — | no | 82,731 tilted refs; door-tour 4/4; reference renders re-fitted |
| 3 | `feat(converter)!: door links from XTEL, six more renderable base types` | `df91524` | — | opt-in only | 2,204 door links resolved in the real DB |
| 4 | `feat(converter)!: LIGH lights and an XRDS radius override` | `5cd1bf4` | 3 (schema order) | opt-in only | 504 lights / 15,167 lit references in the real DB |
| 5 | `feat(converter)!: NIF door clips, per-space lighting and directional snow (schema 16→17)` | `f6749e2`, `26324d5`, `97eefa4`, `60a6079` | 3, 4 | opt-in only | 768 door NIFs decoded, 171 animated; lighting records cross-checked against the DB |
| 6 | `feat(engine)!: interiors, an active cell and seamless load-door crossings` | `7a155fa`, `4b1dd7a`, `7195065` | 3 | no (fixture) + yes (real) | `--demo-tour` 4/4 crossings by walking; 600 units grounded |
| 7 | `feat(engine): render a load door's destination through the doorway` | `f219a05`, `a49b4df`, `f9c439e`, `23a95a2` | 6 | no + yes | `local/demo/portal5/02-door.png`; closed doors verified after `f9c439e` |
| 8 | `feat(engine): doors open with their own model's animation` | `50bc0f1`, `49f2df5`, `b535a30`, `c02737b`, `e4c0510` | 5, 6, 7 | opt-in only + yes | walk tour 4/4, 811 frames, 0 dark frames, 0 errors (pre-`c02737b`) |
| 9 | `feat(engine): point lights from LIGH references, glow and HDR` | `2acd409`, `188e184`, `efa5351`, `23adbb4` | 4 | no + yes | `local/demo/lights3/`; `efa5351` vs 26 UESP references; `23adbb4` holdout 1.14×/1.26× |
| 10 | `fix(converter), feat(engine): NIF material fidelity` | `b4ce0d9`, `8d4160b` | — (rebase `mesh.rs`) | no (fixtures now exist) | 2,632 materials no longer see-through — **screenshot-only** |
| 11 | `feat(engine): terrain layer samplers, welded seams and a distant ring` | `0ece570`, `68c19bb`, `5211bad` | 6 (streaming) | no + yes | FPS table on a shared machine; ring unused by the Phase 2 campaign |
| 12 | `feat(engine): a first-person player, auto-load doors, empty marker models` | `be8c2dc`, `e1b334a`, `83ec4cd`, `2b69616` | 6, 7 | yes | `--demo-tour` walk test; Blackreach benchmark with `empty_model_references 2` |
| 13 | `feat(engine): per-space lighting, weather sky and directional snow` | `66ee786`, `cfe4893`, `e795350` | 5, 9 | yes | calibrated against 26 references; pooled medians in `e795350` |
| — | **dropped** | `7ee3ab9` | — | — | superseded by upstream `114339c` |
| — | **held back** | `5e53712` (`--shots`), `d50185a`+`c516a2e`+`d2ef589`+`3de1505` (demo), the UESP reference workflow, `tools/research/*` | — | — | §4 |

**How to build a branch.** All 147 commits are in this repository, so the source blobs are present
and a per-path apply is exact:

```bash
git fetch origin main                       # the rebase target is 2105bfd, not this branch
git switch -c pr/NN-slug --no-track origin/main
# (a) whole-file, when no later commit in this fork touched the file:
git checkout <sha> -- <paths>
# (b) or one commit's diff restricted to this PR's paths, in commit order:
git diff <sha>^ <sha> -- <paths> | git apply -3
git add <paths>
git commit -m "<conventional subject with no trailer>"
```

**PRs 1 and 2 are (a)** — nothing in this fork touches `crates/dummy-content/**` after `40e0eb5` or
`crates/shared/src/coordinates.rs` after `933b95f`, so taking those files whole is exact.
**PRs 3, 4 and 5 are (a) too, but only in ascending order**: all three take `exporter.rs`,
`esm/mod.rs` and `shared/lib.rs` as they stood at their own commit, so each is cut on top of the
previous one rather than independently. **PRs 6–13 need a manual port** — their diffs are against
files that a later commit in the same PR rewrote, and the intersection across PRs is `app.rs` (21
commits), `streaming.rs` (14), `world/database.rs` (8) and `config.rs` (6). Say so in the PR bodies:
upstream would rather see a clean port than a rebase artefact.

---

### 3.0 Drop PR 2 of the old plan (our BSA padding fix)

`7ee3ab9` implemented "accept zero padding after the last BSA filename", citing
`MarketplaceTextures.bsa`'s 2,272-byte declared table and 2,263 bytes of names. **Upstream's
`114339c` already does exactly this** — the same archive, the same zero-only rule, an equivalent
negative test (`accepts_zero_padded_filename_table_and_rejects_trailing_garbage`) — and it goes
further with Creation Club file flags (`0x8711_0000`). `git diff HEAD origin/main --
crates/converter/src/archive/bsa.rs` shows our version is a strict subset.

**Action:** carry no BSA change in any PR. When the lead rebases, take `origin/main`'s
`crates/converter/src/archive/bsa.rs` wholesale and let our `7ee3ab9` and its local test builder
disappear. Our own `git apply` of PR 3–5 diffs must not include that file (they already do not).

---

### 3.1 PR 1 — `test(dummy-content): an interior cell and a linked load-door pair`

> **Built and verified, 2026-09-23.** Branch `pr/01-dummy-content-interior-door` off `origin/main`
> (`2105bfd`), 3 files, +705 −9; `cargo fmt`, the CI clippy command and `cargo test --workspace`
> all pass on it. Body in `docs/pr/pr-01-body.md`. **One correction to the scope below:**
> `fixture_doors.rs`'s `generated_interior_plugin_exports_a_reciprocal_door_pair` asserts
> `door_links` rows and `DOOR` records in `statics`, which upstream's converter does not produce;
> that test fails on `origin/main` and has moved to PR 3. The branch carries the other two tests
> (195 lines of the 358).

* **Contains:** `crates/dummy-content/src/esm.rs` (+387), `src/main.rs` (+116),
  `crates/converter/tests/fixture_doors.rs` (new, 358).
* **Source commit:** `40e0eb5`. Additive API (`esm::Interior`, `esm::Door`,
  `plugin_with_interior`, preset consts) and a `gen <dir> --with-interior` CLI flag; the existing
  `plugin()` output is asserted byte-identical.
* **Build:** `git checkout 40e0eb5 -- crates/dummy-content crates/converter/tests/fixture_doors.rs`
  — no later commit touches either path, so this is exact.
* **Depends on:** nothing.
* **Test story:** all of it runs on CI with no game install. `fixture_doors.rs` calls the real
  `parse_plugin_file`/`parse_group`/`parse_record_header`, checks the `EDID`/`XTEL`/`NAME`
  subrecords, runs `exporter::validate_database`, and includes a truncation/mutation sweep. The
  `dummy-content` writer keeps its 45 tests and the 4 release performance budgets.
* **Runtime evidence:** not applicable — the test *is* the evidence.
* **Reviewer will ask:** *"Why does an unrelated fork get to extend our fixture crate?"* — answer
  with the dependency table: without an `XTEL` pair, no reviewer or CI job can exercise a load
  door, a portal, a crossing, an auto-load door or a door animation. Also expect: *"is the API
  intrusive?"* (it is additive and `plugin()` is byte-identical — say so), and *"why a `PRESET_*`
  const rather than parameters?"* (it is the smallest thing that makes the fixture stable).
* **Fit:** very high. This is the PR that makes 6, 7, 8 and 12 reviewable, and it is worth sending
  alone first (the old plan called it "PR 4a").

### 3.2 PR 2 — `fix(shared): Creation reference angles turn clockwise`

* **Contains:** `crates/shared/src/coordinates.rs` (the whole fix is the sign of the inverse in
  `conjugate_quaternion` plus the rewritten doc comment and four tests), and two test assertions in
  the engine that corroborate it (`streaming.rs`,
  `transition.rs:1197 fn the_arrival_rotation_faces_the_camera_where_the_player_should_face`).
* **Source commit:** `933b95f` — **drop its `tools/reference/uesp_shots_4x3.json` hunk** (708 lines
  of pose re-fits belonging to the private reference workflow).
* **Build:** `git checkout 933b95f -- crates/shared/src/coordinates.rs` (untouched afterwards). Then
  port the two engine test assertions by hand: `streaming.rs` and `transition.rs` have both been
  edited many times since, so an `apply -3` will conflict. The shared-crate tests alone carry the
  fix (`crates/shared/src/coordinates.rs:172
  fn a_yaw_turns_clockwise_like_a_heading`).
* **Depends on:** nothing. `crates/shared/src/coordinates.rs` is untouched upstream.
* **Test story:** 7 tests in `crates/shared`, all CI-safe; the two engine assertions are belt and
  braces.
* **Runtime evidence:** the fix was found from a wrong-looking screenshot and proved two ways — on
  63 door models with 5+ placements, 88% of `XTEL` arrivals land at the model's usual angle under
  the clockwise convention against 42% under the counter-clockwise one; on **82,731 tilted
  references** the median error against the terrain normal is 8.2° here versus 19–32° for tilt-sign
  and Euler-order alternatives. Both are re-derivable from an unmodded install, and the reference
  renders were re-fitted afterwards (`6804978`, `9d6d844`).
* **Surgery before sending:** `tools/research/rotation_convention.py` (1,018 lines) has
  `DB = "C:/Modding/SkyrimConverted/skyrim_world.db"` at module level — parameterise it (argv or
  `OPENSKYRIM_CONVERTED_DIR`) before shipping it as the proof. Also worth sending:
  `docs/research/object-rotation-convention.md` + `docs/research/data/object-rotation-audit.json`,
  which currently cite the same machine path (2 lines).
* **Reviewer will ask:** *"You flipped four test expectations — did you fix the code or the tests?"*
  (the evidence above); *"does this need a re-conversion?"* (no — the converter stores raw angles
  and this is a runtime conversion; `WORLD_DATABASE_SCHEMA_VERSION` is unaffected); *"why is
  OpenMW's yaw different?"* (this is the same convention as OpenMW's Bethesda-reference handling —
  say so).
* **Fit:** high, and it is a good first impression: a pure function, a documented archaeology
  result, and a demonstration of how this project settles conventions.

### 3.3 PR 3 — `feat(converter)!: door links from XTEL, six more renderable base types`

* **Contains:** `crates/converter/src/esm/exporter.rs` (+566), `crates/converter/src/esm/mod.rs`
  (+59), `crates/shared/src/lib.rs` (`WORLD_DATABASE_SCHEMA_VERSION` 3 → 4).
* **Source commit:** `df91524`. Related, and to be folded in: `1cd6832` (the engine's fixture
  schema literals follow, and the module stubs are rustfmt'd) — it is fixture plumbing inside PR 6's
  file, so put it wherever it applies cleanly.
* **Build:** `git checkout df91524 -- crates/converter/src/esm/exporter.rs
  crates/converter/src/esm/mod.rs crates/shared/src/lib.rs` — **but** `exporter.rs`, `mod.rs` and
  `lib.rs` are all rewritten by PR 4 and PR 5's commits, so take this commit's version *before*
  applying those (the order in this table is the order that works).
* **Depends on:** nothing. Blocks 4, 5, 6, 8, 9.
* **Test story:** 17 tests in `exporter.rs`, all CI-safe; the DB-schema assertions are in
  `world/database.rs` (engine side, PR 6).
* **Runtime evidence:** 2,204 door links resolved against the real `skyrim_world.db` (the four route
  doors match the independent research note exactly); six more base types now render
  (`ACTI`, `FLOR`, `CONT`, `TREE`, `LIGH`, `DOOR`).
* **Reviewer will ask:** *"What is `XTEL` and where is the format reference?"* — cite UESP's
  *Skyrim Mod:Mod File Format/REFR* and point at the four-link table in
  `docs/research/worldspace-transition-demo.md` §2.2 (carry that note, or inline the table);
  *"which six base types, and what changes?"* — list them in the body, "six more" is not
  reviewable; *"a shared-contract bump invalidates data people have"* — the converter rebuilds the
  world database and cell cache on every run (`cache.rs:53-80`) and the engine rejects a mismatched
  database with a clear error (`world/database.rs`); *"why is the version duplicated in the engine
  binary?"* (`app.rs:1344` answers it — add the missing test, §5.5).
* **Fit:** high, and it is a precondition for the two features upstream's Phase 2 acceptance plan is
  missing (interiors and lighting).

### 3.4 PR 4 — `feat(converter)!: LIGH lights and an XRDS radius override`

* **Contains:** `crates/converter/src/esm/exporter.rs` (+504), `crates/shared/src/lib.rs`
  (`WORLD_DATABASE_SCHEMA_VERSION` 4 → 5), `crates/engine/src/app.rs` (one fixture-schema literal,
  2 lines).
* **Source commit:** `5cd1bf4`. **Correction to the old plan:** this commit does **not** touch
  `cache.rs` — the converter schema went 14 → 15 in the material-fidelity commit (§3.10), so this
  PR is a database-contract change only, and the world DB is rebuilt on every conversion.
* **Build:** as PR 3, taking this commit's version of the same three paths.
* **Depends on:** PR 3 (same constant, next value).
* **Test story:** converter tests cover the `LIGH` record layout; the SQL assertions live in the
  engine (`world/database.rs`, PR 9). The real-plugin test is `#[ignore]`d behind
  `OPENSKYRIM_SKYRIM_DATA` and skips with a printed message.
* **Runtime evidence:** 504 lights and 15,167 lit references in the real database.
* **Reviewer will ask:** *"Why a radius override?"* — `XRDS` is in the record and the engine needs a
  forward radius for its nearest-64 budget; *"15,167 references from 504 lights?"* — say how (each
  reference to a light base type becomes a light, and the engine keeps the nearest 64).
* **Fit:** high; it is the data half of PR 9.

### 3.5 PR 5 — `feat(converter)!: NIF door clips, per-space lighting and directional snow (schema 16 → 17)`

* **Contains:** `crates/converter/src/nif_animation.rs` (new, 2,103),
  `crates/converter/src/esm/lighting.rs` (new, 672),
  `crates/converter/src/esm/directional_material.rs` (new, 213),
  `crates/converter/src/esm/{exporter,mod}.rs`, `crates/converter/src/cache.rs`,
  `crates/converter/src/pipeline.rs`.
* **Source commits, in order:** `f6749e2` (door `Open`/`Close` sequences → glTF clips),
  `26324d5` (per-space lighting `XCLL`/`LGTM`/`CLMT`/`WTHR` and STAT directional snow `DNAM`/`MATO`),
  `97eefa4` (`CONVERTER_SCHEMA_VERSION` 15 → 16, with a migration that carries schema 12–15
  manifests forward and reuses the ~33k texture encodes), `60a6079` (17: animation scale channels
  are `VEC3`, not `SCALAR`).
* **Build:** `git checkout HEAD --` for the six files whose final state is ours and which upstream
  has not touched since the merge point (`nif_animation.rs`, `esm/lighting.rs`,
  `esm/directional_material.rs`, `cache.rs`, `esm/exporter.rs`, `esm/mod.rs`), and **hand-apply one
  line** to `pipeline.rs`: `matches!(previous_manifest.schema_version, 12 | 13)` becomes `12..=16`.
  Do **not** take our `pipeline.rs` wholesale — upstream rewrote it after the merge point
  (`96a730c` +226/−37, `8b5bf80`) and our copy would revert the skip-failed-assets and
  dangling-texture work. And do not take our `esm/exporter.rs` for PRs 3 and 4: those take the same
  file at their own earlier commit, so PR 5 lands on top of both.
* **Depends on:** PR 3 and PR 4 (same `exporter.rs`, same `mod.rs`, ascending schema constants).
* **Surgery to consider:** this is the largest PR in the series — three new converter modules
  (2,988 lines, 2,103 of them `nif_animation.rs`) plus its share of `exporter.rs` — and the natural
  split is *door clips* / *lighting + snow*, one schema bump each (16 then 17). The only reason it is
  one PR here is that `97eefa4` bumps the manifest for both features at once and
  `WORLD_DATABASE_SCHEMA_VERSION` stays 5 (the new tables are additive and the engine probes for
  them). If the maintainer prefers two PRs, cut them in that order and let the second one move the
  constant from 16 to 17 by itself.
* **Test story:** `nif_animation.rs` 16 tests, `esm/lighting.rs` 11, `esm/directional_material.rs` 4,
  plus a truncation/mutation sweep; the real-asset tests are `#[ignore]`d behind
  `OPENSKYRIM_CONVERTED_DIR` and skip with a message. **This is the PR that most needs a
  `dummy-content` fixture** (a NIF with one `NiControllerSequence` exporting two clips; a `CELL` with
  an `XCLL`; a `STAT` with a `DNAM`/`MATO`), and the fixture crate can now do NIF and ESM writers.
  Add at least the door-clip one before sending: the alternative is a reviewer taking 2,103 lines of
  binary parsing on trust.
* **Runtime evidence:** 768 door NIFs decoded and 171 found animated; the lighting/snow record
  layouts were settled against real bytes (`tools/research/space_lighting_dump.py`) and the published
  database carries all four tables.
* **Reviewer will ask:** *"What are these record layouts based on?"* — the honest answer is
  partially UESP/web documentation plus dumps of our own bytes; the exporter tests pin the layouts
  and the tables cross-check against a real install. Say which is which.
* **Fit:** high for door clips (a real capability nothing else in the repo has) and medium for
  lighting/snow (new tables, additive, engine probes for them).

### 3.6 PR 6 — `feat(engine)!: interiors, an active cell and seamless load-door crossings`

* **Contains:** `crates/engine/src/transition.rs` (new, 1,629), `crates/engine/src/doors.rs` (new,
  369 minus the animation half), the active-cell and pre-stream half of `streaming.rs`,
  `world/database.rs` (the door-link query), `world/components.rs`, `lib.rs`, `app.rs`, `config.rs`.
  Fold in `7195065` (measure spawned bounds in model space) and the engine half of `1cd6832`
  (fixture schema literals).
* **Source commits:** `7a155fa` (the feature), `4b1dd7a` (`fix(engine): an XTEL arrival heading
  turns clockwise, and the camera arrives at eye height` — **keep this as its own commit**; it has a
  regression test built from the four real links and is a reviewable bug fix on its own), `7195065`.
* **Depends on:** PR 3 (schema 4) and, for a runnable demo, PR 1 (the fixture).
* **Surgery — this is where the demo must come out:** delete `crates/engine/src/demo_tour.rs` (706
  lines), `EngineConfig::demo_tour` and `demo` (`config.rs:54-56, 130-131`), the `--demo-tour` and
  `--demo` arms (`config.rs:269-279`), `DemoStart` and its two Alftand/Blackreach presets
  (`config.rs:319-355`) plus their tests (`config.rs:439-455`), the on-screen objective overlay in
  `app.rs`, `docs/demo/README.md`, `tools/demo/*.ps1`, the two `play-*.cmd` launchers and the
  README bullet that points at them. What remains is the general feature: `--walk
  --start-position … --start-yaw …`.
* **Test story:** 11 tests in `transition.rs`, 4 in `doors.rs` and 42 in `streaming.rs` at HEAD (some
  of those arrived with later PRs), and `crates/converter/tests/fixture_doors.rs` (PR 1) runs the
  fixture through the door-link path. The four real door links are additionally asserted by an
  `#[ignore]`d test.
* **Runtime evidence:** `--demo-tour` walked the route itself: **4/4 crossings, 600 units grounded,
  0 errors, 170–180 FPS** (`3de1505`, screenshots in `local/demo/final/`).
* **Reviewer will ask:** *"How do I try this?"* — PR 1 plus `--start-position`; *"I cannot
  reproduce your tour"* — the tour that produced the evidence is not shipped (§4), so point them at
  PR 1's fixture test for the data path and `--walk` for the feel, and say plainly that the
  crossing's end-to-end check is a recorded run, not a CI test; *"three thousand lines for one PR"* —
  split `transition.rs`+the `XTEL` query from the streaming half if asked; *"what if the destination
  is not resident yet?"* — `DOOR_PRESTREAM_RADIUS = 800.0` and the ordering argument in the module
  doc; *"does `--streaming-fixture` hang?"* — **not a bug in this code** (research-028: the fixture
  only exits through the benchmark path, and the campaign always passes a window), but do not mention
  the flag in this PR.
* **Fit:** this is the feature the README's "no loading screens" promise and Phase 2 acceptance are
  short of. It is the strongest thing the fork has, but only once a reviewer can run it.

### 3.7 PR 7 — `feat(engine): render a load door's destination through the doorway`

* **Contains:** `crates/engine/src/portal.rs` (new, 2,360), `shaders/portal.wgsl` (+31),
  `app.rs` wiring; `a49b4df` and `f9c439e` as their own commits.
* **Source commits:** `f219a05` (portal views, open load doors, pre-streamed cells kept off the main
  layers), `a49b4df` (the doorway quad is vertical, not floor-oriented, and arrivals face into the
  room), `f9c439e` (`fix(engine): a load door draws closed unless the portal is rendering through
  it`), `23a95a2` (the door's front comes from the return link's `XTEL`, not the model's axis).
* **Depends on:** PR 6. Send after it.
* **Test story:** 20 tests in `portal.rs`, CI-safe.
* **Runtime evidence:** the portal camera's own image per door (`local/demo/portal5/02-door.png`
  shows AlftandWorld through the Alftand02 door); the closed-door rule was checked on screen after
  `f9c439e`.
* **Reviewer will ask:** *"Why a second camera and a render-layer split rather than a stencil?"* —
  the module doc (47 lines) answers it and is the best writing in the fork; keep it; *"what is
  `DESTINATION_LAYER` and who else uses it?"* — **disclose the known limitation**: Bevy's GPU
  clustering ignores render layers, so a portal destination's lights can faintly reach the active
  space; *"does the doorway leak when the leaf is up?"* — the ordering argument in the doc.
* **Fit:** high, self-contained, and visually demonstrable the moment PR 1 lands.

### 3.8 PR 8 — `feat(engine): doors open with their own model's animation`

* **Contains:** `crates/engine/src/door_animation.rs` (new, 3,796), the `DoorState`/`DoorLeaf`/
  `mesh_is_out_of_the_way` contract in `doors.rs`, the wiring in `transition.rs`, `player.rs`,
  `portal.rs`, `world/database.rs`, `demo_tour.rs` (drop that file — see PR 6) and `app.rs`.
* **Source commits:** `50bc0f1` (the `Closed → Opening → Open → Closing` state machine, clips found
  by name), `49f2df5` (the crossing carries the player's pose through the doorway instead of
  snapping), `b535a30` (the two wired together; no blank frame before the swap), `c02737b` (a load
  door whose own `Open` clip does not clear the doorway borrows its non-load twin's clip, else
  scales its own about its rest pose), `e4c0510` (rustfmt).
* **Depends on:** PR 5 (the clips), PR 6 (the crossing), PR 7 (the leaf/portal interaction).
* **Test story:** 39 tests in `door_animation.rs` and the `doors.rs`/`portal.rs` tests are CI-safe.
  The one real-asset test is `#[ignore]`d and **skips even when the variable is set**, because a
  headless app cannot build the model through the glTF loader — say that in the PR body rather than
  letting a reviewer find it. The engine has no CI-runnable door animation test yet; a
  `dummy-content` NIF with an `Open` clip would fix that, and it is the single most valuable fixture
  to add after PR 1.
* **Runtime evidence:** the walk tour after `b535a30`: **4/4 crossings by walking, 0 errors, 0 dark
  frames in 811 captured frames**, and the frames either side of a swap are the same view
  (`local/demo/walk-swap-fixed.jpg`). `c02737b` was integrated after the last tour; **its swing
  change has not been re-verified at runtime** (the commit says the tour is the end-to-end check, and
  no tour is recorded since). Re-run the tour before sending.
* **Reviewer will ask:** *"Why is the engine inventing a swing for a door whose data says 5°?"* —
  the module doc (90 lines) answers it at length: the game's load doors only have to move before the
  loading screen comes down, and this engine has none; the twin lookup and the about-the-rest-pose
  scaling are the two documented paths, and only 2 of the install's 19 twinned pairs match by node
  name; *"does this break non-animated doors?"* — no, a door with no clip is promoted to `Open` in
  the frame it is activated, exactly as before.
* **Fit:** high for the feature; this is also the largest single file in the fork and the one where a
  reviewer will ask for the fixture test first.

### 3.9 PR 9 — `feat(engine): point lights from LIGH references, glow and HDR`

* **Contains:** `crates/engine/src/lights.rs` (new, 871), the light query in `world/database.rs`, the
  light-spawning half of `streaming.rs`, `app.rs` ambient and atmosphere, `render.rs` glow handlers.
* **Source commits:** `2acd409` (the feature, nearest-64 budget), `188e184` (visible reference
  parents, brightness calibration, atmosphere at startup — the runtime fixes without which nothing
  rendered), `efa5351` (ambient recalibrated against the references; the stopgap camera lantern
  removed), `23adbb4` (HDR + bloom, `EMISSIVE_EXPOSURE = 100` only for materials carrying
  `KHR_materials_emissive_strength` — which the converter already wrote before the fork — and
  `intensity_for_radius` capped at a 256-unit reference).
* **Depends on:** PR 4 (the `lights` table). Send after PR 7 so it does not fight over
  `streaming.rs`.
* **Test story:** 13 tests in `lights.rs`, 18 in `render.rs`, all CI-safe.
* **Runtime evidence:** `local/demo/lights3/` on screen; `efa5351` measured against the 26 UESP
  references (Alftand02 8× → 1.8×, AlftandWorld 1.2×); `23adbb4`'s holdout check on poses it was
  never tuned on: AlftandWorld 1.14×, Blackreach 1.26× (clip 0.66%), Tamriel unchanged.
* **Reviewer will ask:** *"Why 50?"* — `lights.rs` derives it: a light delivers a fixed surface
  brightness at half its own radius, so the constant is scale-free (keep that derivation, it is the
  best answer in the PR); *"Bevy already puts 4π on point lights?"* — that was the bug, and the
  module says which `PI` went missing; *"you tuned this against screenshots of a copyrighted game"* —
  the honest answer is that the *numbers* are committed and the *images* are not, and it is one
  scalar with a derivation; be ready for the maintainer to prefer a neutral default.
* **Fit:** high. Self-contained, well-derived, fills a roadmap gap, and `23adbb4` gives it a visible
  payoff.

### 3.10 PR 10 — `fix(converter), feat(engine): NIF material fidelity`

* **Contains (converter):** `crates/converter/src/material.rs` (+291), `crates/converter/src/mesh.rs`
  (+290), `crates/converter/src/cache.rs` (`CONVERTER_SCHEMA_VERSION` 14 → 15 and the schema-12..14
  migration arm). **Engine:** `crates/engine/src/render.rs` (+675), `streaming.rs` (+25),
  `world/database.rs` (+55) — additive and multiplicative glow blending, textureless terrain bases,
  clean door labels.
* **Source commits:** `b4ce0d9` (converter: vertex alpha is not blend; keep blend factors; drop
  `EditorMarker` shapes) and `8d4160b` (engine: glows, terrain bases, labels).
* **Depends on:** nothing structurally — but **this PR's `mesh.rs` is the file upstream rewrote at
  `8b5bf80` (397 lines) and `114339c`**: rebase it by hand onto `origin/main`'s `mesh.rs`, do not
  apply a diff.
* **Surgery:** the four `research-011 class A/B/C` comments are already replaced by
  `docs/research/transparent-and-misplaced-meshes.md` citations — either carry that note, or inline
  what each class is.
* **Test story:** 21 tests in `mesh.rs`, 13 in `material.rs`, all CI-safe. **The weakness is the
  evidence, not the code:** "2,632 materials changed appearance" is backed by 3D screenshots in
  `local/`. The fix is now cheap — `crates/dummy-content/src/nif.rs` already writes NIF shapes — so
  add fixture NIFs with `SLSF1_VERTEX_ALPHA`, an additive glow card and an `EditorMarker`, and
  assert on the exported glTF JSON (opaque, with these factors; marker dropped). That converts
  "trust my screenshots" into a test a reviewer can run.
* **Reviewer will ask:** *"Is dropping `EditorMarker` geometry ever wrong?"* — the test in `mesh.rs`
  covers a real object whose fifth shape is a marker and keeps its other four; say so.
* **Fit:** the converter half is high value (see-through ice and rock, invisible-body meshes) once
  the fixture exists; the engine half is a visual change with a documented reason per case.

### 3.11 PR 11 — `feat(engine): terrain layer samplers, welded seams and a distant ring`

* **Contains:** `crates/engine/src/render.rs`, `shaders/terrain.wgsl`,
  `crates/engine/src/world/{components,database}.rs`, `streaming.rs`, `app.rs`, `config.rs`.
* **Source commits:** `0ece570` (`fix(engine): repeat the tiled terrain layer samplers and blend
  layers bilinearly`), `68c19bb` (weld one-point cell-edge seams instead of rejecting the cell; no
  water plane on dry cells), `5211bad` (the terrain-only ring to 8 cells beyond the full grid, with
  distance fog and `NotShadowCaster`).
* **Depends on:** PR 6 for `streaming.rs`.
* **Test story:** CI-safe unit tests incl. the `config.rs` parser tests (7). The ring's residency
  cannot be tested by the streaming fixture at all — the fixture's cell cache is empty, so ring
  cells can never become resident (research-028 §4); say that rather than implying coverage.
* **Runtime evidence:** the Pale view: 180 FPS without the ring, ≈97 FPS at radius 8 on a **shared**
  machine (the module doc admits the table is order-of-magnitude). The benchmark path keeps the ring
  off unless asked, and a default benchmark passed with 0 commit-budget violations.
* **Reviewer will ask:** *"Your FPS table is from a busy machine"* — offer the ring **off by
  default** and let `--terrain-radius 8` opt in; the upstream default render distance is a decision
  for them, not for us. *"Does this change the acceptance numbers?"* — the benchmark exemption is
  the answer; make it prominent.
* **Fit:** the sampler fix and the seam weld are close to unarguable bug fixes; the ring is a
  preference, and it should be presented as one.

### 3.12 PR 12 — `feat(engine): a first-person player, auto-load doors, empty marker models`

* **Contains:** `crates/engine/src/player.rs` (new, 2,567), the auto-load marker half of `doors.rs`,
  `app.rs`, `config.rs` (`--walk`), `transition.rs`, `streaming.rs`, `world/database.rs`, and
  `2b69616` for the bounds gate.
* **Source commits:** `be8c2dc` (first-person controller), `e1b334a` (wire `--walk` and exact start
  placement), `83ec4cd` (auto-load doors cross on contact; `E` stays for the rest), `2b69616` (a
  reference whose converted model is an empty scene — an editor marker, e.g. Blackreach's
  `WispAmbush`/`FrostSpiderAmbush01` — is skipped and counted in `empty_model_references`, every
  other bounds failure stays fatal).
* **Depends on:** PR 6 and 7.
* **Test story:** 32 tests in `player.rs`, 4 in `doors.rs`; `--walk` cannot be combined with a
  benchmark run (`config.rs` states it).
* **Runtime evidence:** the `--demo-tour` walk test (600 units grounded, no falls); the worker's
  Blackreach benchmark for `2b69616` (`empty_model_references 2`, transform failures 0, passed).
* **Reviewer will ask:** *"Why mesh ray-casting instead of a physics engine?"* — Phase 4 owns
  Rapier; say plainly that this is a stopgap and name what it cannot do (no bodies, no slopes beyond
  the ray, no stepping); *"`--walk` in a benchmark?"* — it is rejected, repeat that.
* **Fit:** CONTRIBUTING lists character controllers under engine work, so this is welcome — but it
  is the least self-contained of the engine PRs.

### 3.13 PR 13 — `feat(engine): per-space lighting, weather sky and directional snow`

* **Contains:** `crates/engine/src/world/lighting.rs` (new, 823), `crates/engine/src/snow.rs` (new,
  929), `shaders/snow.wgsl` (+84), and `app.rs` (`space_atmosphere`, the fog/backdrop/ambient
  application, and the calibration constants), `portal.rs` (destination atmosphere).
* **Source commits:** `66ee786` (per-space lighting, fog and backdrop from the `space_lighting`
  table), `cfe4893` (directional snow from `STAT` `DNAM`/`MATO`), `e795350` (the calibration on
  schema-17 data: `SUN_TINT_STRENGTH = 0.75`, `DAY_SUN_LEVEL = 1.6`, interior ambient 800 → 480,
  outdoor fill 160 → 640).
* **Depends on:** PR 5 (the tables) and PR 9 (the ambient constants this recalibrates).
* **Test story:** 8 tests in `world/lighting.rs`, 11 in `snow.rs`, CI-safe, both written to degrade
  gracefully when the table is absent (which is what makes them safe on upstream's fixtures).
* **Surgery before sending:** update the three stale "until the reconversion" doc comments (§2.4) —
  the shipped data has the tables now; a reviewer who reads them and then runs it will be confused.
* **Runtime evidence:** the calibration set re-run on the integrated build: pooled render/reference
  medians Tamriel 0.69 → 0.96 and neutral, Alftand01 1.78 → 1.15, AlftandWorld 0.27 → 1.14,
  Alftand02 1.12 → 0.82, Blackreach 0.99 → 1.33, AlftandZCell 4.81 → 2.97. Two knowns: ZCell is
  still ≈3× (its reference is a dark room and the pose renders a lit wall) and Blackreach shares
  AlftandWorld's weather row, so the two cannot move independently.
* **Reviewer will ask:** *"This reads a table only your converter writes"* — that is why it is after
  PR 5, and why the probe-and-fall-back path matters; *"the snow formula is undocumented in the
  game"* — the module says so explicitly ("this engine's reading, fitted by eye") and it is worth
  repeating in the body; **the snow has no rendered verification recorded on real data** — it was
  written while the tables were absent. Render it before claiming it works.
* **Fit:** medium-high. The per-space lighting is a real fidelity gain; the snow is a subjective
  fit and should be presented as one.

---

## 4. Exclusion list — never in a PR

| Path | Why |
|---|---|
| `AGENTS.md` (137), `CLAUDE.md` (1) | the working agreement and the handoff log: `C:\Modding\OpenSkyrimProject`, `C:/Users/Brannock`, worker names, task ids, the user's goals in quotes |
| `.deepseek-agents.json` | `readOnlyDirs` names `C:/Users/Brannock/.cargo/registry/src` and `C:/Modding/SkyrimConverted` |
| `tasks/deepseek/**` (61 files) | briefs, prompt templates, reviewer/analyst/advisor roles, result schemas, `context.md`, `cookbook.md` |
| `tools/ds_impl.ps1`, `ds_status.ps1`, `run_ds_queue.ps1`, `ds_report.py`, `check_worker_result.py`, `check_citations.py` | the harness: `~/.claude/skills/deepseek-agents`, `local/agents`, `DS_RUN_ID` |
| `tests/test_check_citations.py`, `test_check_worker_result.py`, `test_ds_report.py` | tests for the above |
| `docs/handoff/**` (12 notes) | process records; they cite `local/`, `tasks/deepseek/` and machine paths |
| `docs/pr/upstream-pr-plan.md` (this file) | it names the harness, the machine paths and the private plan by design — it is a working document, not a PR artefact |
| `tools/research/**` (52 scripts) | one-off probes; 22 hard-code `C:/Modding/SkyrimConverted` at module level. Keep-and-parameterise `rotation_convention.py` (PR 2's proof) and, if the matching PR is sent, `auto_load_doors.py`, `space_lighting_dump.py`, `esm_route.py`, `door_animation_nif.py`, `load_door_twin_coverage.py`; leave the other ~46 private |
| `tools/compare_shots.py`, `tools/reference/uesp_shots_*.json`, `tests/test_compare_shots.py`, `tests/test_reference_shots.py` | the private UESP reference workflow (CC BY-SA images live in `local/`) |
| `crates/engine/src/shots.rs` (`--shots`), its `config.rs` arms, `docs/design/reference-shots.md` | a genuinely useful visual-regression tool, but every default points at the reference workflow and its companion tools are Windows-path scripts. Good PR *after* the paths are parameterised and a synthetic shots file is generated by `dummy-content` |
| `docs/demo/README.md`, `tools/demo/*.ps1`, `README.md`'s "Walk the demo" bullet | demo-specific; they hard-code a converted-assets path and a route |
| the demo code: `crates/engine/src/demo_tour.rs`, `config.rs`'s `demo_tour`/`demo` fields, `--demo-tour`/`--demo` arms, `DemoStart` and its presets, the objective overlay in `app.rs` | cell ids, absolute positions, route names |
| `play-blackreach-demo.cmd`, `play-blackreach-shortcut.cmd` | hard-code `C:\Modding\SkyrimConverted` |
| `.gitignore` additions: the `# AI worker harness` block, `/local/`, `tools/_dev_*.py`, `tools/tmp_*.py` | harness scratch rules. `__pycache__/` and `*.pyc` on their own are a welcome upstream contribution — send them alone if at all |
| `local/**` | worker state, checkouts, reference images, screenshots — git-ignored, never add |

Mechanical check before any push (both greps must come back with only the excluded paths, and
`docs/design/animated-doors-and-seamless-crossing.md:23` and `crates/engine/src/app.rs:1533` must be
fixed first):

```bash
git grep -n -iE "brannock|C:/Modding|deepseek|\.claude|ds_impl" <branch>
git ls-files | grep -vE '\.(rs|toml|lock|md|json|wgsl|py|ps1|cmd|yml|yaml|txt|png|jpg|gitignore)$'
```

---

## 5. Cleanup list, in the order I would do it

1. **Fix the stale data claims in three module docs** — `world/lighting.rs:29`, `snow.rs:13-15`,
   `door_animation.rs:15` all say the assets lack the tables/clips "until the reconversion". The data
   is schema 17 and has them; `docs/demo/README.md:57` likewise still lists the two Blackreach models
   as failing, which `2b69616` fixed. *Four sites, a sentence each, and they are the first thing a
   reviewer reads.*
2. **Decide each citation's fate** (§2.6) — 44 production doc comments point at `docs/research/*`,
   `docs/design/*` and `tools/research/*.py` (2 of them inside the held-back `shots.rs`, which do not
   matter). Per PR: either carry the note (cut its process paragraph: `visual-gaps-spec.md`'s opening
   two paragraphs are worker-process, not content) or inline the fact.
   `crates/engine/src/app.rs:1533`'s `local/reference/…` citation is the one outright miss of the
   previous sweep.
3. **Take upstream's `bsa.rs`** and let our `7ee3ab9` go (§3.0). Also take upstream's `README.md`,
   `LEGAL.md` and `pipeline.rs`.
4. **Strip private names from the docs that will ship** —
   `docs/design/animated-doors-and-seamless-crossing.md:3,22,23` says "DeepSeek, research-027" and
   points at `tasks/deepseek/context.md` and `C:/Users/Brannock/…`; 11 notes across
   `docs/design/`, `docs/research/` and `docs/handoff/` carry `C:/Modding/SkyrimConverted` in their
   method section, and several sign off with "(Claude)" or "Owner: Claude (lead)". Replace with the
   fact, or with "computed from a converted install".
5. **Tie the duplicated schema constant with a test** — `crates/engine/src/app.rs:1344` versus
   `crates/converter/src/cache.rs:11`. Nothing fails when they drift. A converter-side test that
   asserts the engine's literal equals the converter's constant (or a move to `crates/shared/`,
   which is the right home for a contract) ends it.
6. **Split the demo out of the features** (PR 6's surgery, §3.6) — `demo_tour.rs`, the presets, the
   overlay, `docs/demo/`, `tools/demo/`, the `.cmd` launchers and the README bullet. Without this,
   PRs 6 and 12 cannot be sent as they stand.
7. **Re-cut the commits** — 130 subjects with `[claude]`/`[deepseek]` prefixes and a
   `Co-Authored-By: Claude Opus 5` trailer become conventional-commit subjects with no trailer, one
   to three per PR. Keep the bodies: they contain the measurements, and they are better than the
   subjects.
8. *(needs work first)* **Add the generated-fixture cases** where the fork currently proves things
   with screenshots or a real install: a NIF with `Open`/`Close` sequences (PR 8), a NIF with vertex
   alpha / an additive glow / an `EditorMarker` (PR 10), a `CELL` with an `XCLL` and a `STAT` with a
   `DNAM`/`MATO` (PR 13). `dummy-content` already has NIF, ESM, BSA and DDS writers; this is the
   difference between "trust me" and "run it".

---

## 6. State of the fork

### 6.1 What works, on real data (converter schema 17, integration report schema 5)

- **`C:\Modding\SkyrimConverted` is complete**: 87,804 references, 72,964 exterior cells, 52,181
  terrain cells, 0 missing models, 0 invalid models, 0 missing textures, empty `failures` in the
  manifest, `complete: true`, integration report `passed: true`.
- **Walking the Alftand → Blackreach route with no loading screens**: four load doors, a portal view
  through each doorway, per-space lighting, first-person controls, and the player's pose carried
  through the crossing. Demonstrated by `--demo-tour`: 4/4 crossings by walking, 0 errors, 0 dark
  frames in 811 captured frames, 170–180 FPS (that figure is from a shared machine).
- **Door animation**: `Open`/`Close` clips exported from 171 of 768 door NIFs, a four-state machine
  driven by the clip's own clock, a leaf that stays drawn when it swings clear and hides when it
  does not, and a fallback that borrows a twin's swing or scales the door's own clip about its rest
  pose.
- **Lighting**: 504 `LIGH` records → 15,167 lit references with a nearest-64 budget; HDR + bloom;
  emissive scaled only where the converter marked a strength; per-space ambient, fog, backdrop and
  sun tint from the records, calibrated against 26 reference screenshots.
- **Materials and terrain**: vertex alpha no longer means blend, blend factors survive, editor
  markers are dropped, terrain layers tile and blend bilinearly, seams are welded, and a
  terrain-only ring reaches 8 cells further with distance fog.
- **The fixture path**: `dummy-content gen --with-interior` produces an exterior cell, an interior
  and a reciprocal door pair, and `fixture_doors.rs` converts it through the real parser, exporter
  and validator with no game install.

### 6.2 Written but not yet seen working

- **Directional snow** (`cfe4893`): implemented, tested, and the tables it reads are now in the
  database — but it was written while they were absent and **no rendered verification of it is
  recorded anywhere**. Its formula is admitted to be this engine's own reading, fitted by eye.
- **The load-door swing borrowed or scaled from a twin** (`c02737b`): measured on real clips and
  unit-tested, but the last recorded walk tour pre-dates it.
- **Ice and environment maps**: named as a gap repeatedly (`docs/research/visual-gaps-spec.md`,
  `reference-shot-poses.md`) and not addressed by any commit.
- **Weather beyond the first `WLST`**: the converter resolves a worldspace's climate to its first
  weather; there is no weather sequence, transition or overcast state.
- **The terrain ring**: on by default, exercised in interactive runs; the streaming fixture cannot
  exercise its residency at all (empty cell cache).

### 6.3 Known broken, limited or open

- **`--streaming-fixture` never exits unless a benchmark window is given.** Not a ring bug: the only
  exit in a fixture run is the benchmark report path (`metrics.rs:104`). The Phase 2 campaign always
  passes a duration and never turns the ring on, so it is not exposed. Anyone who runs the fixture by
  hand with no window will think it hung. **Do not mention the flag in any PR.**
- **GPU clustering ignores render layers**, so a portal destination's lights can faintly reach the
  active space (impl-015's note). Disclose it in PR 7.
- **Lights overexpose right next to a source** (inverse-square with a `4π` and a scale-free
  constant). Mitigated by the glow cap; not solved.
- **AlftandZCell is ≈3× too bright** and **Blackreach shares AlftandWorld's weather row**, so the two
  cannot be tuned independently (`e795350`).
- **Two Blackreach models** (`WispAmbush`, `FrostSpiderAmbush01`) are empty scenes after conversion;
  `2b69616` skips them and counts them instead of failing the bounds gate. Repaired, but
  `docs/demo/README.md` still calls them broken.
- **No skinned meshes, NPCs, physics or UI** — Phase 4, by design.
- **Nothing has ever been built on Linux**, which is where CI runs.
- **The reconversion's disk cost**: `C:\Modding\SkyrimConverted` was ~67 GB with ~69 GB free on the
  drive before the schema-17 run (research-031); worth knowing before anyone re-runs it.

---

## 7. Risks

| Risk | Location | Severity |
|---|---|---|
| The working agreement ships: private paths, the user's own words in the handoff log, worker names, task ids, the exact commands used to run the workers | `AGENTS.md`, `CLAUDE.md`, `.deepseek-agents.json`, `tasks/deepseek/**` | high — but they are whole new files, so excluding them is a one-line decision |
| A PR body or code comment cites a file the PR does not carry (`docs/research/*`, `docs/design/*`, `tools/research/*`) | 44 production doc comments (§2.6) | medium — looks sloppy and exposes the workflow; fixed by per-PR decisions in §5.2 |
| 41 process commits (briefs, handoffs, queue updates) interleaved with the code | all 130 authored commits | medium — the reason to re-cut rather than push |
| A CI red on the first PR: never built on Linux, `cargo nextest`/`cargo audit` never run here | the whole series | medium — run the five CI commands on the rebased branch first |
| The rebase target is 7 commits stale, two of which (BSA, `mesh.rs`) overlap our work | `114339c`, `8b5bf80`, `d46a179` (rebrand) | medium — rebase onto `2105bfd` **first**, then cut |
| Claiming features work while the streaming fixture hangs and GPU clustering ignores layers | PRs 6, 7, 11 | medium — disclose both |
| Screenshot-only evidence for the biggest visual change (2,632 materials) | PR 10 | medium — the fixture NIF closes it |
| Snow, twin-swing doors and ice presented as working when they are unverified or absent | PRs 8, 13; §6.2 | medium — say which is which in the bodies |
| An AI-attribution trailer or a `[claude]`/`[deepseek]` prefix on a commit | all 130 | low, removed by the re-cut |

**Not risks:** no Bethesda asset is tracked or ever was; the commit email is a GitHub noreply
address; no `.gitignore`d data, screenshot or converted asset has been committed; the fork's own
54,860 lines **add no dependency and never touch a manifest** (the range's only
`Cargo.toml`/`Cargo.lock` change is upstream's `dummy-content` workspace member), so `cargo audit`
cannot move because of us; the fork removed no file upstream has.

---

## 8. What I could not verify

- **`cargo nextest` and `cargo audit` availability could not be checked**: `cargo nextest
  --version`, `cargo audit --version` and a listing of `~/.cargo/bin` were all refused by this
  session's permission policy. I ran `cargo test --workspace` instead, which executes the same test
  bodies (nextest is a runner, and it adds no test). **The exact `security` job has not been run**,
  and the exact `test` job has not been run *through nextest*.
- **Nothing was built or tested on Linux**, the only platform CI uses. Every verdict in §2.1 is
  Windows.
- **No runtime check of my own.** I did not launch the engine (no GPU-heavy run while other workers
  and the user's agentic tests may be active, per the handoff log). Every claim about what a frame
  looks like is the fork's own record, not something I re-measured. The one runtime fact I checked
  directly is that the converted assets on disk are schema 17 / complete and that a door `.glb`
  contains an `Open` clip.
- **`cargo clippy` was a warm run** (6.2 s, "Checking engine", all other crates cached). That still
  replays every crate's diagnostics and exits 0 with `-D warnings`, and `--all-features` is a no-op
  because no crate declares features — but it is not a from-scratch build.
- **`git status` is not clean** in the sense that `crates/engine/src/doors.rs` is listed as modified
  with an empty `git diff` (a stale index stat), and several files in `crates/engine/src/` have
  mtimes after the last commit (`door_animation.rs`, `portal.rs`, `streaming.rs`, up to 10:17 today).
  I did not touch them; nothing in this note should be read as a statement about whoever did.
- **Line and file counts** are `git diff --shortstat` figures over the named ranges and pathspecs;
  the per-area figures sum exactly to the range total, and the range total (234 files, +58,931 /
  −645) is exact. `#[test]` counts (156 at the fork point, 519 at HEAD) are `git grep` line counts
  over `crates/` and are a few short of the 536 the runner reports — the remainder is the vendored
  `project_wormhole_nif` crate (12) and the 2 doc-tests plus a small residue I did not chase. Where
  the two disagree in this note I quote the runner.
- **`crates/launcher` was untouched in the range**: if the launcher appears in a PR diff, something
  outside this fork's history put it there.
- **Door-link, rotation and lighting statistics** (88%/42%, 82,731 references, 2,204 links, 504
  lights) are the fork's own measurements, re-derivable from an unmodded install with the scripts in
  `tools/research/`, but I did not re-run them.
- **The 7 upstream commits past our merge** were read as diffs, not as intent: `114339c`'s overlap
  with our BSA fix is unambiguous in the code, but whether its author would also want our test cases
  is a judgement call.

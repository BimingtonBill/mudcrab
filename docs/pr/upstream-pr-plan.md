# Upstream pull-request plan

**Status:** research note, 2026-09-22. Audit of everything this fork has done since it was cloned,
and a design for presenting it to `realfakenerd/OpenSkyrim` without publishing anything yet.
**Method:** `git log`/`git diff 8919096..HEAD` and `8919096..origin/main`, `git grep` over the
tracked tree, `cargo check --workspace`, `cargo test -p shared`, `cargo test -p converter --lib`,
`cargo test -p engine --lib`, `cargo clippy --workspace --all-targets -- -D warnings`. Windows,
debug profile, warm `target/`.
**Nothing here has been pushed, and no branch was created.**

---

## 0. The five things that matter

1. **Upstream has moved.** `origin/main` is 16 commits past the fork point (`0ec2d36`), all of them
   a new `crates/dummy-content` fixture crate, eight ADRs, and two new CI jobs. Every PR must be
   re-cut off `origin/main`, not off this branch.
2. **Only one file both sides touched** (`crates/converter/src/archive/bsa.rs`, and only its
   `mod tests`). A rebase is realistic; the conflict is small and has an obviously right answer.
3. **Upstream cannot run any of this.** Their fixtures generate an *exterior-only* worldspace with
   no doors. The door, portal, light and player work is therefore unreviewable as it stands. The
   single highest-value thing to add is a `dummy-content` extension that generates an interior
   plus a load door with an `XTEL` link.
4. **The code is good; the packaging is not.** 147 new unit tests, no `dbg!`, no new `TODO`s, no
   new dependencies, thorough module documentation. But 83 commits are prefixed `[claude]`/
   `[deepseek]`, signed with a `Co-Authored-By: Claude Opus 5` trailer, and 20 doc comments cite
   private task ids (`impl-021`, `research-011`) and git-ignored `local/` paths.
5. **No Bethesda data is tracked and no private email is in the history.** The embarrassing
   material is `AGENTS.md`, `.deepseek-agents.json` and `tasks/deepseek/**` — private tooling that
   must never reach a PR.

---

## 1. Inventory

83 commits, 149 files, +32,208 / −416 lines. No file was renamed or deleted in the whole range
(`git diff --name-status --diff-filter=RD` is empty), so nothing upstream had is being removed.

| Path | Files | Lines | What it is |
|---|---|---|---|
| `crates/` | 24 | +14,479 / −416 | the only upstream-worthy code |
| `tools/` | 54 | +10,059 | 6 harness tools + 46 research/measurement scripts + 2 reference JSON files |
| `tests/` + `tasks/` | 48 | +4,148 | Python tests for the harness; 42 task briefs |
| `docs/` | 18 | +3,389 | 3 designs, 8 handoff notes, 6 research notes, 1 data file |
| root | 6 | +172 | `AGENTS.md`, `CLAUDE.md`, `.deepseek-agents.json`, `.gitignore`, 2 launchers |

### 1.1 `crates/` by kind

| Change | Commit(s) | Files (changed lines) | Upstream? |
|---|---|---|---|
| **Rotation convention fix** | `933b95f` | `shared/src/coordinates.rs` (40), `engine/src/{streaming,transition}.rs` (16, tests only) | **yes — PR 1** |
| **BSA name-table zero padding** | `e6677a4` | `converter/src/archive/bsa.rs` (31) | **yes — PR 2** |
| **XTEL door links, 6 more base types (schema v4)** | `df91524` | `converter/src/esm/{exporter,mod}.rs` (625), `shared/src/lib.rs` (2) | **yes — PR 3** |
| **LIGH lights table, XRDS radius (schema v5)** | `5cd1bf4` | `converter/src/esm/exporter.rs` (504), `shared/src/lib.rs` (2), `engine/src/app.rs` (2) | **yes — PR 3** |
| **Interiors, active cell, seamless crossings** | `7a155fa`, `4b1dd7a` | `engine/src/{transition,streaming,world/database}.rs` (1,600+) | **yes — PR 4** |
| **Portals through load doors; closed-door rule** | `f219a05`, `f9c439e` | `engine/src/{portal.rs,shaders/portal.wgsl,app.rs}` (1,650) | **yes — PR 5** |
| **LIGH point lights; calibration** | `2acd409`, `efa5351`, `188e184` | `engine/src/{lights.rs,streaming,world/database,app}.rs` (950) | **yes — PR 6** |
| **NIF material fidelity, glows, markers** | `b4ce0d9`, `8d4160b` | `converter/src/{material,mesh}.rs` (581), `engine/src/{render,streaming,world/database}.rs` (755) | **yes — PR 7** |
| **Terrain blending; distant-terrain ring** | `0ece570`, `5211bad` | `engine/src/{render,streaming,app,config,world/*}` + `terrain.wgsl` (2,600) | **yes — PR 8** |
| **First-person player; auto-load doors** | `be8c2dc`, `7a155fa`, `83ec4cd`, `dc09c90` | `engine/src/{player,doors,app,config,transition}.rs` (1,950) | **yes — PR 9** |
| **`--shots` pose renderer** | `5e53712` | `engine/src/{shots,app,config,transition,lib}.rs` (1,283) | later — §4.10 |
| **`--demo-tour`, `--demo alftand\|blackreach`** | `d50185a`, `c516a2e`, `d2ef589`, `3de1505` | `engine/src/{demo_tour,config,app}.rs`, 2 `.cmd` | **no — §5** |
| **Bounds check precision** | `7195065` | `engine/src/streaming.rs` | yes (small, fold into PR 4) |
| **Converter schema plumbing** | `8e898af`, `1cd6832` | `engine/src/app.rs`, `converter/src/cache.rs` | yes, inside PR 3/7 |

### 1.2 Tests

`#[test]` count in `crates/`: **156 at the fork point → 303 at HEAD (+147)**, counted with
`git grep -c "#[test]" <rev> -- crates/`. The heaviest additions are `engine/src/streaming.rs`
(12 → 34), `engine/src/player.rs` (0 → 23), `engine/src/world/database.rs` (7 → 19),
`engine/src/shots.rs` (0 → 17), `converter/src/esm/exporter.rs` (3 → 13),
`converter/src/material.rs` (10 → 13). All are in-module `#[cfg(test)]` unit tests: the fork adds
**no** `crates/*/tests/` integration test and no bench, which is the opposite of where upstream's
newest work is going (`crates/converter/tests/fixture_round_trip.rs`,
`crates/dummy-content/tests/performance.rs`, `crates/dummy-content/benches/writers.rs`).

### 1.3 The private tooling

`.deepseek-agents.json` (32), `AGENTS.md` (117, the generated agreement plus a long handoff log),
`CLAUDE.md` (1, `@AGENTS.md`), `tasks/deepseek/` (42 files: briefs, prompts, templates, schemas),
`tools/ds_impl.ps1` (339), `tools/ds_report.py` (525), `tools/check_worker_result.py` (443),
`tools/check_citations.py` (272), `tools/ds_status.ps1` (84), `tools/run_ds_queue.ps1` (76),
`tests/test_ds_report.py` (502), `tests/test_check_worker_result.py` (536),
`tests/test_check_citations.py` (164), `docs/handoff/` (8 notes, 529 lines),
`play-blackreach-{demo,shortcut}.cmd` (14), and the 45 scripts under `tools/research/` plus
`tools/compare_shots.py` and the 2 reference JSON files (10,059 lines together, most of them
one-off probes with `C:/Modding/SkyrimConverted` hard-coded at module level). None of this belongs
upstream.

---

## 2. Audit findings

### 2.1 Does it build and test? Yes, on Windows

```
$ cargo check --workspace
    Checking shared v0.1.0 ... Checking converter v0.1.0 ... Checking engine v0.1.0 ... Checking launcher v0.1.0
    Finished `dev` profile [optimized + debuginfo] target(s) in 20.27s
    [exited with code 0]
```
(the only diagnostic is the pre-existing `proc-macro-error2 v2.0.1` future-incompat warning that
upstream also has at the fork point)

```
$ cargo test -p shared
    running 7 tests ... test result: ok. 7 passed; 0 failed; 0 ignored
$ cargo test -p converter --lib
    running 118 tests ... test result: ok. 105 passed; 0 failed; 13 ignored
$ cargo test -p engine --lib
    running 165 tests ... test result: ok. 165 passed; 0 failed; 0 ignored
$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.89s   [exited with code 0]
```

The 13 ignored converter tests are all opt-in and correctly gated — they say so in their names
(*"requires OPENSKYRIM_NIF_FIXTURE with a locally installed Skyrim NIF"*, *"reads Skyrim.esm from
the Skyrim SE install"*), and each returns early if the file is missing.

### 2.2 Upstream has moved: 16 commits since the fork point

```
$ git branch -a -v
  main          8919096 [behind 16] Merge pull request #18 ...
  remotes/origin/main  0ec2d36 Merge pull request #20 from HazielMagallanes/feat/dummy-content-nif-esm
```

The 16 commits are all one effort: a new `crates/dummy-content` crate (13 files, 2,700 lines) that
**generates** DDS, PEX, BSA, BA2, NIF and ESM fixtures procedurally and deterministically, with
`cargo run -p dummy-content -- gen Data` then `cargo run -p converter --bin converter -- Data
modern_assets` in `CONTRIBUTING.md`. Their rules, from upstream's
`docs/adr/0002-ground-truth-unmodded-install.md` (`origin/main`; neither the `crates/dummy-content`
crate nor the `docs/adr/` folder exists in this checkout):

> Real samples are extracted to `/tmp` only and never committed. Opt-in tests use
> `OPENSKYRIM_SKYRIM_DATA` and `OPENSKYRIM_*_FIXTURE` and stay `#[ignore]`d so CI never needs
> proprietary data.

Three consequences for this fork:

* **The env-var convention clashes.** Upstream already uses `OPENSKYRIM_SKYRIM_DATA`; the fork
  invented a second name, `SKYRIM_DATA_DIR`, for the same install
  (`crates/converter/src/esm/exporter.rs:1195` and `:1356`). Rename before shipping.
* **The hard-coded fallbacks are exactly what ADR-0002 forbids.**
  `crates/converter/src/mesh.rs:1576` falls back to `C:/Modding/SkyrimConverted/vfs/meshes` when
  `OPENSKYRIM_CONVERTED_NIF_ROOT` is unset;
  `crates/converter/src/esm/exporter.rs:1196` and `:1357` fall back to
  `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data`. Upstream's convention is: no
  default, skip with a message when the variable is missing.
* **Where a case can be synthesised, upstream now expects it synthesised.** The BSA padding fix in
  PR 2 is the clearest example — that case belongs in `dummy_content::bsa`.

The fork's 16 changed files and upstream's 38 intersect in exactly one place:
`crates/converter/src/archive/bsa.rs`. Upstream rewrote its `mod tests` fixture builder to call
`dummy_content::bsa::v104(...)` and added a truncation/mutation sweep; the fork extended that same
builder with a `padding` argument and added two tests. `crates/converter/src/esm/binary.rs` looks
risky (the fork's `exporter.rs` calls `parse_plugin_file`) but upstream's change there is a new
`#[cfg(test)] mod tests` only — **no API change, no compile break**. `Cargo.toml`/`Cargo.lock` are
untouched by the fork, so the rebase takes upstream's copy cleanly. That the fork added no new
dependency in 14,479 lines is a real review asset and should be said in the PR bodies.

### 2.3 What upstream CI requires

`.github/workflows/run_tests.yml`, triggered on pull requests to `main` and `develop`, ubuntu-latest:

| Job | Command |
|---|---|
| format | `cargo fmt --all -- --check` |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| test | `cargo nextest run --workspace --all-targets --no-fail-fast --no-tests=warn` |
| **security** (new) | `cargo audit --ignore RUSTSEC-2026-0194 --ignore RUSTSEC-2026-0195` |
| **performance** (new) | `cargo test --release -p dummy-content -- --ignored performance --test-threads=1` |
| gate | `tests_pass` — every job must be `success` or `skipped` |

Notes for us:

* CI is **Linux only**. This fork has never been built on Linux. Everything I verified is Windows.
  The one Windows-specific addition (`crates/engine/src/app.rs:246-267`, benchmark process
  priority via `windows_sys`) has a `#[cfg(not(windows))]` no-op twin, so nothing obviously breaks
  — but it is unproven, and a PR body should not claim otherwise.
* `--all-features` is not what I ran (I ran `--workspace --all-targets`). No crate in the workspace
  declares a feature that the engine code I read is gated on, but the lead should run the exact CI
  command before preparing anything.
* `cargo nextest` is not installed on this machine; I ran `cargo test`.
* `cargo audit` runs on every Rust PR. The fork adds no dependency, so it cannot drag in a new
  advisory — worth stating in each PR.
* The `.github/PULL_REQUEST_TEMPLATE.md` asks for Objective / Details / Verification with the five
  checkbox commands and an "Environment: OS, Backend" line. The lead should fill it in with what the
  engine actually selects on this machine — I did not run it, so I cannot say whether it is DX12 or
  Vulkan.

### 2.4 Code hygiene

* **No `dbg!` anywhere.** The only new `println!`/`eprintln!` are converter progress/warning lines
  (`crates/converter/src/esm/exporter.rs:349,353,450`) and test-only diagnostics.
* **No new `TODO`/`FIXME`/`HACK`/`todo!`/`unimplemented!`.** `git grep -c` over `crates/` finds
  exactly one, in `crates/converter/src/esm/extractors.rs`, and that file is untouched in
  `8919096..HEAD` (`git diff` for that path is empty) — so it is upstream's.
* **19 × `#[allow(clippy::too_many_arguments)]`**, all in `crates/engine/src/` (`streaming.rs` ×8,
  `app.rs` ×2, `portal.rs` ×2, `shots.rs` ×2, one each in `demo_tour.rs`, `player.rs`,
  `transition.rs`). This is the standard Bevy-system escape hatch and `metrics.rs:92`/
  `profiling.rs:210` had it before the fork, so it is consistent — but a reviewer will notice the
  count and it does mean some systems take fifteen parameters.
* **Magic numbers are named and derived.** `lights.rs:91-103` defines `LIGHT_EXPOSURE = 50.0`,
  derives `HALF_RADIUS_ILLUMINANCE` from `INTERIOR_AMBIENT_BRIGHTNESS`, and documents
  `HALF_RADIUS_WINDOW = 225.0/256.0` with a citation to `bevy_pbr/src/render/pbr_lighting.wgsl`'s
  `getRangeFalloff`. `crates/engine/src/config.rs:65-85` justifies `DEFAULT_TERRAIN_RADIUS = 8`
  with a measured FPS table and `crates/engine/src/config.rs:297-303` keeps benchmark runs on the
  old numbers. This is better than most
  upstream code and should be left alone.
* **One duplicated constant with no test holding the halves together:**
  `crates/engine/src/app.rs:1300-1304`, `const fn converter_schema_version() -> u32 { 15 }`,
  duplicating `crates/converter/src/cache.rs:11`. The comment explains why (not linking the
  converter into the runtime binary) but nothing fails when the two drift. Either move it into
  `crates/shared/` or add a converter-side test asserting the two agree.
* **`--streaming-fixture` does not terminate** with the terrain ring on (open, `research-028`);
  do not ship that flag path in a PR until it does.

### 2.5 Private references inside code and docs that would ship

These are `file:line` citations to things a reader cannot open. They are not secrets, but they
read as someone else's project notes, and they name the assistant and the worker harness.

**Production code (worse — these are in the shipped crates):**

| Location | What it says |
|---|---|
| `crates/engine/src/portal.rs:5` | module doc: *"See `tasks/deepseek/impl-009-door-portals.md`"* |
| `crates/engine/src/config.rs:16` | *"the engine as it was before impl-021"* |
| `crates/engine/src/config.rs:69` | *"(`local/impl-021/far-views.json`, pose `far-pale-yaw40`)"* |
| `crates/engine/src/streaming.rs:47` | *"(`local/impl-021/after/shots.log`)"* |
| `crates/engine/src/lights.rs:26,68,80,87,165,306,432` | `impl-014`/`impl-019`, `local/calib/calibration.md` |
| `crates/engine/src/app.rs:210,1356,1357,1359,1369,1376,1385,1386` | `impl-015`/`impl-019`, `local/calib/calibration.md` |
| `crates/engine/src/player.rs:614` | *"(Reported by `impl-006`…)"* |

**Converter tests (research-note names that are not in the repo):**
`crates/converter/src/mesh.rs:1600,1628,1657` and `crates/converter/src/material.rs:1623` cite
*"research-011 class A/B/C"*.

**Docs:** `docs/design/{reference-shots,lights-and-auto-doors,blackreach-demo}.md` are signed
*"(Claude)"* / *"Owner: Claude (lead)"*; `docs/research/transparent-and-misplaced-meshes.md:3`,
`docs/research/phase2-status.md:143` cite `tasks/deepseek/`; `tools/reference/uesp_shots_4x3.json`
carries `"reference": "local/reference/uesp/SR-place-Alftand.jpg"` and `research-020`/`research-024`
in its `note` fields (`:17-18`, `:33-34`, and 19 more).

**Test-only strings** (`local/reference/...` in `crates/engine/src/config.rs:481-501`,
`crates/engine/src/shots.rs:693,724,845,853`) are harmless — they are literals in a parser test —
and need no change.

### 2.6 Assets, licensing, and other people's data

* `git ls-files` contains **no** `.nif`, `.dds`, `.esm`, `.esp`, `.esl`, `.bsa`, `.ba2`, `.glb`,
  `.ktx2`, `.pex`, `.rkv`, or archive of any kind. The only binaries in the tree are the two
  pre-existing upstream screenshots, `docs/images/phase2-milestone-1-rendering-progress.png` and
  `docs/images/phase2-rural-skyrim-assets.png`, neither of which this fork touched.
* The largest new tracked files are text: `tools/research/rotation_convention.py` (1,018 lines),
  `crates/engine/src/player.rs` (1,862), `docs/research/data/object-rotation-audit.json` (510).
  Nothing large, nothing generated, nothing that needs Git LFS.
* The UESP screenshots the reference work is built on live in `local/reference/uesp/` and are
  git-ignored — correctly. If `tools/reference/uesp_shots_*.json` is ever shipped it should carry a
  one-line CC BY-SA attribution for UESP, or stay private; it is 424 lines of camera poses that are
  useless without the images.
* `docs/research/phase2-status.md:172` shows a `D:\SkyrimConverted` command line, matching the
  style of upstream's own `docs/roadmap/02-acceptance.md:32`. Not a leak.
* Every commit is authored by
  `BimingtonBill <90541801+BimingtonBill@users.noreply.github.com>` (`git log --format=%ae` over
  all 83 commits is that address and nothing else). No private email is in the history.

### 2.7 Commit and documentation consistency

Upstream's last 16 subjects are Conventional Commits with a scope:
`feat(dummy-content): add ESM plugin writer and generated worldspace`,
`fix(dummy-content): use pure-Rust miniz_oxide for zlib payloads`,
`test(converter): add pipeline end-to-end and parser hardening sweeps`,
`docs(adr): record dummy-content architecture decisions`.

This fork's subjects are `[claude] Object rotations turn clockwise, as Skyrim's do` and
`[deepseek] Distant terrain: terrain-only ring to 8 cells beyond the full grid, with distance fog`,
and the bodies carry a `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` trailer. The
prefixes are consistent within the fork and every commit has one, but they are not what upstream
writes, and the bodies are written for a reader who knows `impl-021`, `research-025` and the
handoff log. The *content* of the messages is good (they say why, and quote measurements) and
should be kept — the framing needs to change.

`docs/` is where the fork is weakest for upstream consumption. Upstream now has `docs/adr/` for
decisions and `docs/specs/` for formats; the fork added `docs/design/`, `docs/research/` and
`docs/handoff/`. The design and research notes are worth keeping (see §4.1 on the density of
evidence in them) but `docs/handoff/` and every "worker result" sentence inside the others are
process, not content.

---

## 3. The PR series

Nine PRs, ordered so the cheap and the independent go first. Each is one feature or one fix, and
each can be reviewed in one sitting.

**Send first (no dependency on anything, no rebase risk): PR 1 and PR 2.**

Recommendation for the whole series: **re-cut, do not push this history.** The 83 commits
interleave private tooling with code, several of them exist only to sync the harness or write a
brief, and the ones that bump `WORLD_DATABASE_SCHEMA_VERSION` and `CONVERTER_SCHEMA_VERSION` do it
across the two crates in one commit that also touches `app.rs`. Branch off `origin/main` and
cherry-pick the *content* of each commit listed below into one to three conventional-commit
commits: subject `fix(shared): ...` / `feat(converter)!: ...`, body = the existing measurements
with the private ids removed, no `[claude]`/`[deepseek]` prefix, no `Co-Authored-By` trailer.

Per PR below: **contains** (files / source commits), **depends on**, **reviewer will ask**, and any
surgery needed.

### 3.1 PR 1 — `fix(shared): Creation reference angles turn clockwise`

* **Contains:** `crates/shared/src/coordinates.rs` — the whole fix is `conjugate_quaternion(` at
  `:38` and its closing paren at `:44`, plus the rewritten doc comment at `:24-36`, three updated
  tests and one new one;
  `crates/engine/src/streaming.rs` and `crates/engine/src/transition.rs` — **test assertions only**
  (16 lines; `git show 933b95f -- crates/engine/` is all inside `mod tests`). Optional but
  recommended: `docs/research/object-rotation-convention.md` and
  `docs/research/data/object-rotation-audit.json`.
* **Source commits:** `933b95f` (drop its `tools/reference/uesp_shots_4x3.json` hunk — 708 lines of
  pose re-fits that belong to the private reference workflow) and nothing else.
* **Depends on:** nothing. `crates/shared/src/coordinates.rs` is untouched upstream.
* **Squash:** one commit. The fix is four lines; a reviewer should see them together with the tests
  and the proof.
* **Reviewer will ask:**
  1. *"Your diff flips four existing test expectations. Did you fix the code or the tests?"* — the
     answer is the evidence: on 63 door models with five or more placements, 88% of doors put their
     `XTEL` arrival point at their model's usual angle under the clockwise convention against 42%
     under the counter-clockwise one; and on 82,731 tilted references the median error against the
     terrain normal is 8.2° here versus 19–32° for every alternative (tilt signs and Euler order).
     Both are re-derivable from an unmodded install; ship the script that did it
     (`tools/research/rotation_convention.py`, 1,018 lines) or a trimmed version of it.
  2. *"Does this need a re-conversion?"* — no. The converter stores raw angles; this is a runtime
     conversion. `WORLD_DATABASE_SCHEMA_VERSION` is unaffected by this PR.
  3. *"Why is this right and OpenMW's yaw wrong?"* — it is the same convention as OpenMW's
     Bethesda-reference handling; say so in the body.
* **Fit with upstream:** high. Upstream has no reference-rotation test at all, this is a
  well-documented pure function, and it demonstrates the project's hardest problem (Babylon-era
  coordinate archaeology) being settled with data.

### 3.2 PR 2 — `fix(converter): accept zero padding after the last BSA filename`

* **Contains:** `crates/converter/src/archive/bsa.rs:164-170` (production: the exact-length
  `ensure!` becomes "everything after the last name must be NUL"), plus the two new tests
  (`accepts_zero_padding_after_the_last_filename`,
  `rejects_non_zero_bytes_after_the_last_filename`).
* **Source commits:** `e6677a4`.
* **Depends on:** nothing — but it is **the one file upstream also changed**, so rebase first.
* **Conflict resolution:** upstream's `mod tests` now builds its fixture through
  `dummy_content::bsa::v104(&[Entry::new(...)], Compression::None)`
  (`git show origin/main:crates/converter/src/archive/bsa.rs`). The production hunk applies
  cleanly. Two ways to keep the test: (a) preferred, add a padding knob to `dummy_content::bsa`
  (ADR-0001 says fixtures live in that crate) and add `padding_after_last_filename_is_accepted`
  next to their sweep; (b) keep a small local builder. Do not drop the negative test — it is the
  answer to the next question.
* **Squash:** one commit.
* **Reviewer will ask:**
  1. *"Does this loosen validation?"* — no. Only NUL bytes are accepted; any other byte still
     errors with the same message, and the second test proves it.
  2. *"Evidence?"* — `MarketplaceTextures.bsa` declares a 2,272-byte name table and uses 2,263,
     followed by nine NULs. (Name the archive in the commit body; it is a Creation Club archive that
     ships with the Anniversary install.)
* **Fit with upstream:** very high — two lines, real-world evidence, a generated fixture case, and
  a parser already under a mutation sweep.

### 3.3 PR 3 — `feat(converter)!: door links and lights from the plugin records (DB schema 3 → 5)`

* **Contains:** `crates/converter/src/esm/exporter.rs` (XTEL door-link extraction and the `LIGH`
  table, +1,068), `crates/converter/src/esm/mod.rs` (+59), `crates/converter/src/cache.rs:11`
  (`CONVERTER_SCHEMA_VERSION` 14 → 15) and `:61` (the schema-12..14 migration arm),
  `crates/shared/src/lib.rs:7` (`WORLD_DATABASE_SCHEMA_VERSION` 3 → 5),
  `crates/engine/src/app.rs:1300-1304` (the duplicate constant).
* **Source commits:** `df91524` (door links, schema v4, six more renderable base types) and
  `5cd1bf4` (lights table, `XRDS` radius override, schema v5) — two commits, in that order.
* **Depends on:** nothing. Blocks PRs 4, 6, 7.
* **Surgery before sending:** rename `SKYRIM_DATA_DIR` to upstream's `OPENSKYRIM_SKYRIM_DATA` and
  delete both path fallbacks (`crates/converter/src/esm/exporter.rs:1196` and `:1357`); add the
  missing "skip" path so the
  ignored tests are inert without the variable (they already return early — keep that).
* **Squash:** keep two commits. A reviewer benefits from seeing the two schema bumps separately,
  and the ADR-style "why six more base types" belongs in the first one's body. If the maintainer
  prefers, split into two PRs (v4 then v5) — nothing but the order of the version constant depends
  on it.
* **Reviewer will ask:**
  1. *"What is `XTEL` and where is the format reference?"* — cite UESP *Skyrim Mod:Mod File
     Format/REFR* and point at the four-link table the fork measured independently
     (`docs/research/worldspace-transition-demo.md` §2.2; the same four links are asserted by the
     `#[ignore]`d `route_doors_of_the_real_plugin_decode_through_export`).
  2. *"Which six base types, and what do they change?"* — list them explicitly in the body; "six
     more renderable base types" is not reviewable.
  3. *"A shared-contract version bump invalidates data people already have."* — say how: the
     converter rebuilds the world database and cell cache on every run
    (`crates/converter/src/cache.rs:61-72`), and the
     engine rejects a mismatched database with a clear error
     (`crates/engine/src/world/database.rs:309`).
  4. *"Why is the converter schema version duplicated in the engine binary?"* — the comment at
     `app.rs:1301` answers it; add the missing test that ties the two together (§2.4).
* **Fit with upstream:** high, and it is a precondition for the two features upstream's Phase 2
  acceptance plan is actually missing (interiors and lighting).

### 3.4 PR 4 — `feat(engine)!: seamless load-door crossings between cells and worldspaces`

* **Contains:** `crates/engine/src/transition.rs` (new, 797), `crates/engine/src/streaming.rs` (the
  active-cell/pre-stream half, several hundred lines), `crates/engine/src/world/database.rs` (the
  door-link query, ~248), `crates/engine/src/doors.rs` (new, 59),
  `crates/engine/src/world/components.rs` (+13), `crates/engine/src/lib.rs` (+7), and
  `crates/engine/src/app.rs` + `crates/engine/src/config.rs` (plugin wiring, the start-placement
  flags, and — to be removed — the demo preset; see the surgery note below). Also fold in `7195065`
  (measure spawned bounds in model space — a review comment from the player-controller work that
  belongs with this code).
* **Plus a `dummy-content` commit, and this is the important one:**
  `crates/dummy-content/src/esm.rs` + `layout.rs` currently generate an **exterior-only**
  worldspace (on `origin/main`, `crates/dummy-content/src/esm.rs` lines 28-43 are the complete cell
  spec — grid x/y and nothing else, and `git grep -n XTEL origin/main -- crates/dummy-content`
  finds nothing). None of that crate exists in this checkout. Extend it to
  emit an interior cell, a door reference with an `XTEL` link, and the door's model, then add a test
  that converts the generated plugin and crosses the door. Without this a maintainer has no way to
  run the feature: they have no Skyrim install and CI never will. If the maintainer would rather
  take it separately, send it as **PR 4a** first — it is worth a PR on its own.
* **Depends on:** PR 3 (schema v4).
* **Surgery before sending:** *this PR is where the demo leaks.* Remove
  `crates/engine/src/demo_tour.rs` (new, 324 lines), the `demo_tour`, `demo` and `DemoStart` fields
  (`crates/engine/src/config.rs:53-56`, `:130-131`, `:269`, `:272-279`, `:319-355`), the on-screen
  objective overlay
  in `app.rs`, and the two `.cmd` launchers. What remains is the general feature: walk to a door,
  cross, keep walking. `--walk --start-position … --start-yaw …` is enough to demonstrate it.
* **Keep `4b1dd7a` as its own commit** (`fix(engine): an XTEL arrival heading turns clockwise, and
  the camera arrives at eye height`). It has a regression test built from the four real links and
  it is a genuine bug fix that a reviewer can accept on its own — do not bury it in the feature
  commit.
* **Squash:** three commits — the `dummy-content` extension, the crossing feature, the arrival-heading fix.
* **Reviewer will ask:**
  1. *"How do I try this?"* — the `dummy-content` extension (above). Everything else is a
     non-answer without a game install.
  2. *"This is three thousand lines in one PR."* — split the streaming half from the transition
     half if asked; `transition.rs` and the `XTEL` query stand alone reasonably well.
  3. *"What happens if the destination is not resident yet?"* — answer with
     `DOOR_PRESTREAM_RADIUS = 800.0` (`transition.rs:27`) and the ordering argument in the module
     doc (`transition.rs:1-13`).
  4. *"Does `--streaming-fixture` still hang?"* — yes, with the terrain ring on. Do not mention the
     flag in the PR; it is not part of this feature.
* **Fit with upstream:** this is the feature the README's "Zero Loading Screens" bullet promises and
  the roadmap's Phase 2 acceptance is short of. It is the strongest thing the fork has, but only
  once it is runnable without the author's machine.

### 3.5 PR 5 — `feat(engine): render a load door's destination through the doorway`

* **Contains:** `crates/engine/src/portal.rs` (new, 1,812), `crates/engine/src/shaders/portal.wgsl`
  (+31), the `app.rs` wiring (+5), and `f9c439e` as a second commit
  (`fix(engine): a load door draws closed unless the portal is rendering through it`,
  `portal.rs` only, 245 lines).
* **Depends on:** PR 4.
* **Squash:** two commits (portal rendering; closed-door rule). The second is a distinct visual
  bug fix with its own reasoning.
* **Reviewer will ask:**
  1. *"Why is a portal camera and a render-layer split the right design rather than a stencil or a
     scissor?"* — `portal.rs:1-47` already answers this at length (pre-streamed cells appear as
     floating geometry *and* the player ray-casts against them, so they must be off the main
     camera's layers). Keep that doc comment; it is the best writing in the fork.
  2. *"What is `DESTINATION_LAYER` and who else uses it?"* — check it against the reflection camera
     and the GPU clustering caveat (lights in a portal destination can still reach the active space,
     because Bevy's clustering ignores render layers). **Disclose this in the PR body** — it is a
     known limitation and a reviewer will find it otherwise.
  3. *"Does the doorway leak when the leaf is up?"* — the ordering argument at `portal.rs:38-46`.

### 3.6 PR 6 — `feat(engine): point lights from Skyrim's LIGH references`

* **Contains:** `crates/engine/src/lights.rs` (new, 696), the `world/database.rs` light query
  (~240), `streaming.rs` light spawning (~196), `app.rs` wiring; then `efa5351` (the calibration
  and the removal of the stopgap camera lantern, `app.rs` +73/-… , `lights.rs` +177) and `188e184`
  (visible reference parents, atmosphere at startup).
* **Depends on:** PR 3 (schema v5). Send after PR 4 so it does not fight over `streaming.rs`.
* **Squash:** two commits — `feat(engine): …` and `fix(engine): calibrate interior ambient and
  remove the camera lantern`.
* **Surgery:** `lights.rs` references `local/calib/calibration.md` in three doc comments
  (`:80`, `:87`, and the `app.rs` ambient constants at `:1357`); replace with the numbers or with a
  committed research note. Keep the `4π` derivation at `lights.rs:306` — it is the actual bug and
  the reason the lights worked at all.
* **Reviewer will ask:**
  1. *"Why 50?"* — `lights.rs:77-99` derives it: `HALF_RADIUS_ILLUMINANCE` is `LIGHT_EXPOSURE`
     times the interior ambient, so a light delivers a fixed surface brightness at half its own
     radius and the constant is scale-free. Strong answer; keep it.
  2. *"Bevy already has a 4π on point lights?"* — that was the bug; `lights.rs:26` says the
     derivation lost one `PI` and `:306` says both belong. Explain in the body.
  3. *"You tuned this against screenshots of a copyrighted game."* — the honest answer is that the
     *numbers* are committed and the *images* are not, and that the tuning is a single scalar with a
     documented derivation. Be ready for the maintainer to prefer a neutral default.
* **Fit with upstream:** high — `lights.rs` is self-contained, well-derived, has 10 tests, and
  fills a gap the roadmap calls out.

### 3.7 PR 7 — `fix(converter) / feat(engine): NIF material fidelity`

* **Contains (converter half):** `crates/converter/src/material.rs` (+291),
  `crates/converter/src/mesh.rs` (+290, the vertex-alpha, blend-factor and `EditorMarker` changes),
  `crates/converter/src/cache.rs:11,61` (`CONVERTER_SCHEMA_VERSION` 14 → 15).
  **Engine half:** `crates/engine/src/render.rs` (+675), `streaming.rs` (+25),
  `world/database.rs` (+55) — additive and multiplicative glow blending, textureless terrain bases,
  clean door labels.
* **Source commits:** `b4ce0d9` and `8d4160b` (two commits).
* **Depends on:** nothing structurally, but sequence it after PR 4 so `streaming.rs` does not
  conflict.
* **Surgery:** the four `research-011 class A/B/C` comments (`mesh.rs:1600,1628,1657`,
  `material.rs:1623`) either become plain language or cite a committed note.
* **Squash:** two commits, one per side.
* **Reviewer will ask:**
  1. *"2,632 materials changed appearance. Where is the before/after?"* — **this PR is the one
     with the weakest evidence for an upstream reviewer**, because the before/after is 3D
     screenshots in `local/`. The fix: `dummy-content` already has a NIF writer
     (`crates/dummy-content/src/nif.rs`), so add fixture NIFs with `SLSF1_VERTEX_ALPHA`, an additive
     glow card and an editor marker, and assert on the exported glTF JSON. That converts "trust my
     screenshots" into "the material comes out opaque, with these factors".
  2. *"Is dropping `EditorMarker` geometry ever wrong?"* — `mesh.rs:1556-1568` already has the test
     (a real object whose fifth shape is a marker keeps its other four); say so.
* **Fit with upstream:** the converter half is high value (see-through ice, invisible-body meshes)
  once the fixture exists.

### 3.8 PR 8 — `feat(engine): terrain layer samplers, blending, and a distant-terrain ring`

* **Contains:** `crates/engine/src/render.rs` (+401), `crates/engine/src/shaders/terrain.wgsl`
  (+56), `streaming.rs` (+38) for the sampler/blending fix; then the ring — `config.rs` (+104),
  `world/components.rs` (+13), `world/database.rs` (+180), `streaming.rs` (+1,894), `app.rs` (+205).
* **Source commits:** `0ece570`, then `5211bad` (two commits).
* **Depends on:** the sampler fix stands alone; the ring conflicts heavily with PR 4/5/6 in
  `streaming.rs` — announce the order (PR 4 → 5 → 6 → 7 → 8) or send the ring last.
* **Squash:** two commits (`fix(engine): repeat the tiled terrain layer samplers and blend layers
  bilinearly`, `feat(engine): a terrain-only ring beyond the full-detail grid`).
* **Reviewer will ask:**
  1. *"Your benchmark table (`crates/engine/src/config.rs:71-84`) is from a shared machine with
     other work running."*
     The doc comment admits it ("read the table as order of magnitude rather than to the frame").
     A maintainer will want it re-measured on a quiet machine before the ring becomes a default.
     Be prepared to send the ring **off by default** and let `--terrain-radius 8` opt in.
  2. *"This changes the default render distance — does it invalidate the Phase 2 gates?"* — the
     argument at `config.rs:297-303` (benchmark runs keep the ring off unless asked) is the answer;
     make it prominent.
  3. *"`--streaming-fixture --terrain-radius 8` never exits."* — **known and unfixed.** Either fix
     it or leave the fixture flag out of the PR entirely.

### 3.9 PR 9 — `feat(engine): a first-person player and auto-load doors`

* **Contains:** `crates/engine/src/player.rs` (new, 1,862), `crates/engine/src/doors.rs` (the
  auto-load marker half), `app.rs`, `config.rs` (`--walk`), `transition.rs` (+2),
  `streaming.rs` (+65), `world/database.rs` (+172).
* **Source commits:** `be8c2dc`, `83ec4cd`, and the `--walk` wiring in `e1b334a`/`dc09c90`.
* **Depends on:** PR 4 (and PR 5 for the portal/leaf interaction it relies on).
* **Surgery:** drop the `demo_tour.rs` references again (`83ec4cd` touched it).
* **Squash:** two commits (`feat(engine): a first-person player (--walk)`,
  `feat(engine): auto-load doors cross on contact`).
* **Reviewer will ask:**
  1. *"Why mesh ray-casting instead of a physics engine?"* — because Phase 4 owns Rapier; say
     plainly that this is a stopgap and name what it cannot do (no bodies, no slopes beyond the
     ray, no stepping).
  2. *"`--walk` cannot be in a benchmark run."* — `crates/engine/src/config.rs:46-48` already states
     that; repeat it.
* **Fit with upstream:** CONTRIBUTING lists character controllers under engine work, so this is
  welcome — but it is the largest and least self-contained of the nine.

### 3.10 Hold back (not for a PR yet)

* **`--shots` (`crates/engine/src/shots.rs`, 1,114 lines; `docs/design/reference-shots.md`).** A
  genuinely useful visual-regression tool, but every default in it points at the private UESP
  reference workflow (`shots.rs:693,724,845,853`, `crates/engine/src/config.rs:481-501`) and its
  companion tools
  (`tools/compare_shots.py`, `tools/research/exposure_stats.py`, `contact_sheet.py`) are all
  Windows-path scripts. It becomes a good PR after the reference paths are parameterised and a
  synthetic shots file is generated by `dummy-content`.
* **The reference-shot workflow itself** — `tools/reference/uesp_shots_*.json`,
  `docs/research/reference-shot-poses.md` (471 lines of pose fitting). Excellent work, unshippable:
  it needs UESP's images to mean anything, and 19 of its lines cite `local/`.
* **`tools/research/*.py`** — 48 scripts, ~8.5k lines. A handful are worth keeping upstream and the
  rest are one-off probes with a hard-coded `C:/Modding/SkyrimConverted`:
  keep-and-parameterise `rotation_convention.py` (the proof for PR 1) and
  `auto_load_doors.py`/`space_lights.py`/`esm_route.py` (the measurements behind PRs 3, 4, 6);
  leave the other ~43 private.

---

## 4. Exclusion list — never in a PR

| Path | Why |
|---|---|
| `AGENTS.md` (117), `CLAUDE.md` (1) | the working agreement and the handoff log: `C:\Modding\OpenSkyrimProject`, `C:/Users/Brannock`, `C:\Modding\SkyrimConverted`, the user's goals in quotes, worker names, task ids |
| `.deepseek-agents.json` (32) | `readOnlyDirs` names `C:/Users/Brannock/.cargo/registry/src`; `allowTools` names the agent harness |
| `tasks/deepseek/**` (42 files) | briefs, prompt templates, reviewer/analyst/advisor roles, result schemas, `context.md`, `cookbook.md` |
| `tools/ds_impl.ps1`, `tools/ds_status.ps1`, `tools/run_ds_queue.ps1`, `tools/ds_report.py`, `tools/check_worker_result.py`, `tools/check_citations.py` | harness; `~/.claude/skills/deepseek-agents`, `local/agents`, `DS_RUN_ID` |
| `tests/test_check_citations.py`, `tests/test_check_worker_result.py`, `tests/test_ds_report.py` | tests for the above |
| `docs/handoff/**` (8 notes) | process records; several cite `local/` and `tasks/deepseek/` |
| `play-blackreach-demo.cmd`, `play-blackreach-shortcut.cmd` | hard-code `C:\Modding\SkyrimConverted` |
| `.gitignore` additions: the `# AI worker harness` block, `/local/`, `tools/_dev_*.py`, `tools/tmp_*.py` | harness scratch rules. `__pycache__/` and `*.pyc` on their own are a welcome upstream contribution — send them alone if at all |
| the demo code: `crates/engine/src/demo_tour.rs`, `crates/engine/src/config.rs:53-56,130-131,269,272-279,319-355` (`DemoStart::named`, the `--demo` presets and their Alftand/Blackreach coordinates), the objective overlay in `crates/engine/src/app.rs` | demo-specific hard-coding: cell ids, absolute positions, route names |
| `tools/reference/uesp_shots_*.json` (424), `tools/compare_shots.py`, `tests/test_compare_shots.py`, `tests/test_reference_shots.py` | the private UESP reference workflow, CC BY-SA images in `local/` |
| `local/**` (untracked, git-ignored) | worker state, checkouts, reference images, screenshots, calibration runs |

Two things to check mechanically before any push:
`git grep -n -iE "brannock|C:/Modding|C:\\\\Modding|deepseek|\.claude|ds_impl" <branch>` and
`git ls-files | grep -viE '\.(rs|toml|lock|md|json|wgsl|luau|py|ps1|cmd|yml|yaml|txt|gitignore)$'`.

---

## 5. Cleanup list, in the order I would do it

1. **Strip private names from production doc comments** — `crates/engine/src/portal.rs:5` (a whole
   `tasks/deepseek/…` path in a module doc), `crates/engine/src/config.rs:16,69`,
   `crates/engine/src/streaming.rs:47`, `crates/engine/src/lights.rs:26,68,80,87,165,306,432`,
   `crates/engine/src/app.rs:210,1356-1386`, `crates/engine/src/player.rs:614`,
   `crates/converter/src/mesh.rs:1600,1628,1657`, `crates/converter/src/material.rs:1623`. Replace
   with a sentence or an in-repo note. *20 sites; mostly word swaps; no behaviour change.*
2. **Make the install-dependent tests follow upstream's convention** — delete the two
   `C:/Games/Steam/...` fallbacks (`crates/converter/src/esm/exporter.rs:1196` and `:1357`), rename
   `SKYRIM_DATA_DIR` → `OPENSKYRIM_SKYRIM_DATA`, and delete the `C:/Modding/SkyrimConverted/vfs/meshes`
   fallback in `crates/converter/src/mesh.rs:1574-1576` so `OPENSKYRIM_CONVERTED_NIF_ROOT` is
   required. ADR-0002 is explicit that real samples never get committed and opt-in tests are
   env-gated.
3. **Split the demo out of the features** — `crates/engine/src/demo_tour.rs` (324 lines), the
   `DemoStart` presets, `--demo`/`--demo-tour` parsing, the objective overlay, and the two `.cmd`
   launchers. They are the reason PRs 4 and 9 cannot be sent as they stand.
4. **Re-cut the commits** — 83 subjects with `[claude]`/`[deepseek]` prefixes and a
   `Co-Authored-By: Claude Opus 5` trailer become conventional-commit subjects with no trailer, one
   to three per PR. Keep the bodies; they contain the measurements.
5. **Decide the fate of the duplicated schema constant** — `crates/engine/src/app.rs:1300-1304`
   versus `crates/converter/src/cache.rs:11`, with nothing asserting they agree. Move it to
   `crates/shared/` (it is a contract) or add a test.
6. *(lower)* **Fix or hide `--streaming-fixture` with the ring** — a maintainer who runs the fixture
   will hit the hang. Either fix it before PR 8 or remove `--streaming-fixture` from it.
7. *(lower)* **Add the generated-fixture cases** where the fork currently proves things with
   screenshots or a real install: the BSA padding case, a NIF with vertex alpha / an additive glow
   / an editor marker, and — most importantly — the interior + door worldspace in `dummy-content`.

---

## 6. Risks

**What would embarrass the user publicly, and where it is:**

| Risk | Location | Severity |
|---|---|---|
| The working agreement ships: private paths, the user's own words in the handoff log, the names of the AI workers and the task ids, the exact commands used to run them | `AGENTS.md`, `CLAUDE.md` | high — but they are whole new files, so excluding them is a one-line decision |
| The harness's read-only directories name `C:/Users/Brannock/...` | `.deepseek-agents.json`, the third `readOnlyDirs` entry | high, same reason |
| 45 task briefs containing the lead's instructions to an AI worker, with paths and process | `tasks/deepseek/**` | high |
| Doc comments that say "impl-021" or point at `local/impl-021/far-views.json`, a path that exists on one machine | 20 sites listed in §2.5 | medium — looks sloppy rather than leaky, but it exposes the workflow |
| Machine-specific defaults in test code: the author's Steam path and the author's converted-assets path | `crates/converter/src/mesh.rs:1576`, `crates/converter/src/esm/exporter.rs:1196,1357` | medium — a maintainer reads this as "I did not make it runnable for you" |
| A CI red on the first PR: this history has never been built on Linux, and `--all-features` was not exercised here | the whole series | medium — mitigate by running the exact CI commands on a `origin/main` rebase before pushing |
| Claiming the features work while `--streaming-fixture` hangs and GPU clustering ignores render layers | PR 4, 5, 8 | medium — disclose both in the PR bodies |
| An AI-attribution trailer on a `[claude]`-prefixed commit | all 83 commits | low, but re-cutting removes it |

**Not risks:** no Bethesda asset is tracked or ever was; the commit email is a GitHub noreply
address; no `.gitignore`d data file, screenshot or converted asset has been committed; no
dependency was added.

---

## 7. What I could not verify

* **`cargo fmt --all -- --check` was not run.** The command was refused by this session's
  permission policy, twice, in both spellings. It is a required CI job, and the code is heavily
  rustfmt-formatted by hand, so it is very likely to pass — but "likely" is not "ran". The lead
  should run it before preparing any branch.
* **`cargo clippy --workspace --all-targets --all-features -- -D warnings` (the exact CI command)
  was not run** — I ran it without `--all-features`, and it finished in 0.89 s with no diagnostics,
  which means it used cached results. A `--all-features` run would recompile most of the workspace.
* **Nothing was built or tested on Linux**, which is the only platform CI uses.
* **`cargo nextest`** is not installed here; I used `cargo test`.
* **No runtime check of my own.** I did not launch the engine (no GPU-heavy run while other
  workers and the user's agentic tests are active, per the handoff log). Every claim about what a
  frame looks like is the fork's own record, not something I re-measured.
* **The rotation convention's on-screen proof** rests on `local/reference/cmp-lit/` screenshots I
  did not open; what I verified directly is the code, the tests, and that the claim's supporting
  data file and script are committed.
* **The two untracked files in the working tree** (`tools/research/door_animation_nif.py`,
  `tools/research/fixture_ring_probe.py`) belong to other workers running right now; I did not read,
  touch or commit them. `git status` was not clean, so nothing in this note should be read as a
  statement about those tasks.
* **Line counts** are `git diff --stat` "changed lines" (insertions + deletions) and per-path
  `git diff --shortstat` figures; the per-path sums are 39 lines short of the range total, which is
  a pathspec/rename-detection artefact I did not chase. The range total (+32,208 / −416, 149 files,
  83 commits) is exact.

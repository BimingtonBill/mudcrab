# research-029 - upstream PR audit and plan

Deliverable: `docs/pr/upstream-pr-plan.md` (a new folder). Nothing was pushed, no branch was
created, no GitHub contact.

## Verified

* **The range:** 83 commits, 149 files, +32,208 / −416 lines from `8919096` to HEAD. No file was
  renamed or deleted.
* **It builds and tests on Windows:** `cargo check --workspace` exit 0; `cargo test -p shared`
  7 passed; `cargo test -p converter --lib` 105 passed / 13 ignored (all opt-in, env-gated);
  `cargo test -p engine --lib` 165 passed; `cargo clippy --workspace --all-targets -- -D warnings`
  exit 0. `#[test]` count in `crates/`: 156 → 303 (+147).
* **Upstream has moved 16 commits** past the fork point (`origin/main` = `0ec2d36`), all of it the
  new `crates/dummy-content` fixture crate, eight ADRs and two new CI jobs (cargo audit, release
  performance budgets). Its rule for real game data is ADR-0002: generated fixtures in CI, opt-in
  tests behind `MUDCRAB_SKYRIM_DATA`/`MUDCRAB_*_FIXTURE`, `#[ignore]`d, no defaults.
* **Only one file both sides touched:** `crates/converter/src/archive/bsa.rs`, and only its
  `mod tests` (upstream rewrote the fixture builder to `dummy_content::bsa::v104`). Upstream's
  `esm/binary.rs` change is a new `#[cfg(test)] mod tests` only, so the fork's `exporter.rs` call
  site still compiles after a rebase. `Cargo.toml`/`Cargo.lock` are untouched by the fork.
* **No Bethesda asset is tracked** (`git ls-files` has no `.nif/.dds/.esm/.esp/.bsa/.ba2/.glb/.ktx2`)
  and every commit's author is the GitHub noreply address.
* **`dummy-content` generates exterior cells only** - no door, no `XTEL`, no interior
  (`git grep XTEL origin/main -- crates/dummy-content` is empty), which is why the door, portal,
  light and player work cannot be run by an upstream reviewer as it stands.
* **The private material that must not ship:** `AGENTS.md`, `CLAUDE.md`, `.deepseek-agents.json`,
  `tasks/deepseek/**` (42 files), the `tools/ds_*` + `tools/check_*.py` harness and its three
  Python tests, `docs/handoff/**`, the two `play-blackreach-*.cmd`, the UESP reference workflow, and
  the demo code (`demo_tour.rs`, the `--demo` presets at `crates/engine/src/config.rs:319-355`).
* **20 production doc comments cite private things** (`tasks/deepseek/impl-009-door-portals.md` in
  `crates/engine/src/portal.rs:5`, `local/impl-021/...`, `local/calib/calibration.md`, `impl-0NN`,
  `research-011 class A/B/C`), and three test fallbacks hard-code this machine's paths
  (`crates/converter/src/mesh.rs:1576`, `crates/converter/src/esm/exporter.rs:1196` and `:1357`).

## Proposed series

Nine PRs, re-cut off `origin/main` rather than pushed as this history: (1) `fix(shared)` clockwise
Creation angles, (2) `fix(converter)` BSA name-table zero padding, (3) `feat(converter)!` door links
and lights, DB schema 3 → 5, (4) `feat(engine)!` seamless load-door crossings **plus a
`dummy-content` commit that generates an interior and a door**, (5) `feat(engine)` door portals,
(6) `feat(engine)` LIGH point lights, (7) NIF material fidelity, (8) terrain samplers and the
distant-terrain ring, (9) the first-person player. 1 and 2 can be sent today; 3 and 4 are worth the
most.

## Not done / unknown

* **`cargo fmt --all -- --check` was not run** - the command was refused by this session's
  permission policy, twice. It is a CI requirement, so run it before preparing a branch.
* **`cargo clippy ... --all-features`** (CI's exact form) was not run; mine finished in 0.89 s from
  cache, which means the workspace was already checked but not with `--all-features`.
* **Nothing was built or tested on Linux**, which is the only platform CI uses. The one
  Windows-specific addition (`crates/engine/src/app.rs:246-267`) has a no-op
  `#[cfg(not(windows))]` twin, but that is a code reading, not a build.
* `cargo nextest` is not installed here; I used `cargo test`.
* I ran no GPU work and opened no screenshots, so every claim about what a frame looks like is the
  fork's own record, not something I re-measured.
* `tools/research/door_animation_nif.py` and `tools/research/fixture_ring_probe.py` are untracked
  and belong to other workers running now; I did not read, touch or commit them.

## Next step

The lead decides which PRs to prepare. If only one goes upstream, make it PR 1 (the rotation
convention): smallest diff, hardest evidence, and it is the sort of fix a maintainer remembers.
Before any of it, run `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings` on a branch cut from
`origin/main`, and strip the private doc-comment references listed in §5 of the plan.

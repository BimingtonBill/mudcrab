# research-054 — full work audit and the updated PR plan

Deliverable: `docs/pr/upstream-pr-plan.md`, rewritten in place (research-029's version covered
`8919096..3ae5541`). Nothing was pushed, no branch was created, no GitHub contact. The only other
file this task touched is this note.

## What the audit found that research-029 could not

* **Upstream has moved again and it eats a whole PR.** `origin/main` is now `2105bfd`, **7 commits
  past what this branch merged** (`0ec2d36`) — fetched 2026-09-23 08:39. `114339c` *already
  implements the old PR 2*: the BSA filename-table zero-padding fix, the same archive
  (`MarketplaceTextures.bsa`), the same zero-only rule, plus Creation Club file flags we do not
  have. `git diff HEAD origin/main -- crates/converter/src/archive/bsa.rs` shows our version is a
  strict subset. **Carry no BSA change in any PR.**
* **The project was rebranded to "Mudcrab"** (`d46a179`, with `LEGAL.md`). PR titles and bodies
  must use the new name.
* **The complete upstream drift is 11 files, +975 / −65.** Four of them collide with our work:
  `bsa.rs` (dropped), `mesh.rs` +411 and `material.rs` +11 (the material-fidelity PR — rebase by
  hand onto upstream's file), `pipeline.rs` +226/−37 (our change there is **one line**, the
  `matches!` range in the schema-compatibility check — do not take our copy wholesale). The other
  seven are the rebrand, `devenv.lock`, a vendored file, and two converter files we never touch.
* **The reconversion happened, so nothing is inert any more.** `C:\Modding\SkyrimConverted` is
  converter schema 17, `complete: true`, report schema 5; the DB carries `space_lighting`,
  `statics.material_object`, `matos` and `door_links`, and
  `meshes/dungeons/dwemer/door/dwemerlargedoorload01.glb` contains an `"Open"` clip. Three module
  docs still claim the opposite (`world/lighting.rs:29`, `snow.rs:13-15`, `door_animation.rs:15`) —
  first item on the cleanup list.
* **The cleanup is verified complete in `crates/` and never touched `docs/` or `tools/`.** No
  machine path, no `local/`, no `impl-NNN`, no `SKYRIM_DATA_DIR` and no machine fallback survives in
  the shipped crates — with **exactly one miss** (`crates/engine/src/app.rs:1533`). But 22
  `tools/research/*.py` and 11 notes still hard-code `C:/Modding/SkyrimConverted`, and the bigger
  new problem is that **44 production doc comments cite `docs/research/*`, `docs/design/*` and
  `tools/research/*.py`** — files the exclusion list keeps out of every PR, so each PR must either
  carry the note it cites or inline the fact.
* **The unblocker exists now.** `40e0eb5` extends `dummy-content` with an interior cell and a
  reciprocal `XTEL` door pair, and `crates/converter/tests/fixture_doors.rs` runs it through the
  real parser, exporter and validator with no game install. That is the new PR 1 and it should be
  sent first: it is what makes the door, portal, crossing and animation PRs reviewable.
* **The work is much larger than the last count:** 147 commits in the range (130 authored, 16
  upstream's, 1 merge), 234 files, +58,931 / −645; `#[test]` in `crates/` 156 → 519.

## Verified this session

* `cargo fmt --all -- --check` — **passes** (research-029 could not run it).
* `cargo clippy --workspace --all-targets --all-features -- -D warnings` — **passes, exit 0**. No
  crate declares `[features]`, so `--all-features` is a no-op and this is exactly CI's command.
* `cargo test --workspace` — **536 tests, 507 pass, 29 ignored, 0 fail**. All 29 ignored are opt-in
  and env-gated, and each prints its skip.
* `cargo test --release -p dummy-content -- --ignored performance --test-threads=1` (CI's
  `performance` job) — **4/4, exit 0**.
* `git ls-files`: no game data and no large file. The only binaries are upstream's two screenshots.
* Greps for private names and machine paths over the tracked tree — table in §2.3 of the plan.
* The working tree was clean apart from a stat-only phantom on `crates/engine/src/doors.rs`
  (`git status` lists it, `git diff` is empty). I did not touch it.

## The series

13 PRs, re-cut off `origin/main` (never pushed as this history): (1) `dummy-content` interior +
door pair, (2) shared clockwise Creation angles, (3) converter door links + six base types,
(4) converter LIGH lights, (5) converter NIF door clips + per-space lighting + directional snow
(schema 16 → 17), (6) engine interiors + seamless crossings, (7) engine portals, (8) engine door
animation, (9) engine point lights + glow/HDR, (10) NIF material fidelity, (11) terrain samplers +
distant ring, (12) first-person player + auto-load doors + empty marker models, (13) engine
per-space lighting + snow. 1–5 are independent; 6–13 are one stack and need a hand port, not a diff
apply, because `app.rs` (21 commits), `streaming.rs` (14), `world/database.rs` (8) and `config.rs`
(6) are rewritten throughout. Held back: `--shots`, the demo (`--demo`, `--demo-tour`,
`demo_tour.rs`), the UESP reference workflow and `tools/research/*`.

## Readiness blockers, in order

1. Rebase onto `2105bfd` **first** — the branch's own merge is 7 commits stale and two of those
   commits rewrite files we also changed.
2. Take upstream's `bsa.rs` and drop our `7ee3ab9`.
3. Split the demo out of the engine PRs (`demo_tour.rs` 706 lines, the `--demo` presets, the
   overlay, `docs/demo/`, `tools/demo/`, the two `.cmd` launchers, the README bullet).
4. Add the missing `dummy-content` fixtures where the fork proves things with screenshots or a real
   install: a NIF with `Open`/`Close` clips (PR 8), a NIF with vertex alpha / an additive glow / an
   `EditorMarker` (PR 10), a `CELL` with an `XCLL` and a `STAT` with a `DNAM`/`MATO` (PR 13).
5. Re-run the walk tour before sending PR 8 — `c02737b`'s swing change landed after the last
   recorded tour, and the twin-swing door is otherwise unverified at runtime.
6. Decide each of the 44 doc-comment citations (§2.6).

## Not done / unknown

* **`cargo nextest` and `cargo audit` availability could not be checked** — `cargo nextest
  --version`, `cargo audit --version` and a listing of `~/.cargo/bin` were all refused by this
  session's permission policy. I ran `cargo test --workspace` instead (same test bodies). **CI's
  exact `test` job has not been run through nextest, and the `security` job has not been run.**
* **Nothing was built or tested on Linux**, the only platform CI uses.
* **No GPU work of my own**, per the handoff log's rule while the user's agentic tests may be
  running. Every frame-level claim in the plan is the fork's own record. What I checked directly is
  the data on disk: manifest schema 17 / `complete: true`, report schema 5 / `passed: true`, the
  four new tables present in the database, and an `Open` clip inside one door `.glb`.
* `cargo clippy` was a warm run (6.2 s, all crates cached); it still replays every crate's
  diagnostics and exits 0, but it is not from scratch.
* The 88%/42% door-link, 82,731-reference and 504-light statistics are the fork's own measurements.
  I did not re-run `tools/research/*` to reproduce them.
* Several files in `crates/engine/src/` (`door_animation.rs`, `portal.rs`, `streaming.rs`) have
  mtimes after the last commit, up to 10:17 today. `git status` shows them unmodified, so this is
  someone re-reading or checking out files, not an edit in flight. Not mine.

## Next step

The lead fetches `origin/main`, rebases nothing yet, and picks the PRs to cut. If only one goes
upstream, make it **PR 1** (the `dummy-content` interior + door pair): it is additive, tested in CI,
and it is the only thing that lets a maintainer without Skyrim run any of the rest.

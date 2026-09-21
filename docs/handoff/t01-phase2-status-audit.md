# t01 — Phase 2 status audit (handoff)

**Deliverable:** `docs/research/phase2-status.md` — one row per milestone item M0.1…M8.4 with code and
test evidence, plus the conversion recipe (task point 5), the acceptance gate checklist (point 6) and
the grid coordinates (point 7), doc/code disagreements and ranked risks.

**Scope kept:** documentation only. No file outside `docs/research/phase2-status.md` and this handoff
was written. The converter, the engine and `scripts/*.ps1` were read, never executed. The Skyrim
install was read-only (a directory listing of `Data/`).

## Verified (read in the code at commit 49b5ec8)

- Converter schema is 14 (`crates/converter/src/cache.rs:11`) and the world database schema is 3
  (`crates/shared/src/lib.rs:7`) — both match what the preflight asserts
  (`scripts/phase2-acceptance.ps1:168`, `:173`).
- Conversion CLI and output layout are as documented in the note; converted assets land at the output
  root (`meshes/`, `textures/`), raw sources under `vfs/`, which is why the preflight can pass
  `<assets>/vfs/meshes` to `asset-closure` (`scripts/phase2-acceptance.ps1:208-212`).
- The engine side (HZB/indirect proof, streaming lifecycle, terrain seams, water layer separation,
  terrain/cache/asset rejection paths) is implemented and covered by `#[test]`s, not only by docs.

## Actually run (test evidence, not claims)

- `cargo test -p converter` — all targets pass, exit code 0; 95 tests in the library target, only the
  environment-gated ones (`OPENSKYRIM_*_FIXTURE`, `OPENSKYRIM_SKYRIM_DATA`) report `ignored`.
- `cargo test -p shared` — 6 passed, 0 failed.
- `cargo test -p engine` — 42 passed, 0 failed in the library target plus 2 in `world-inspect`,
  exit code 0, including all fourteen tests the acceptance robustness block names. Cold debug
  compilation of the engine took about twenty minutes on this machine.
- `python tools/check_citations.py docs/research/phase2-status.md docs/handoff/t01-phase2-status-audit.md`
  — 0 broken citations; the single "check by hand" line is the Phase 2 pending marker in the
  repository-root README (line 55), which the tool flags only because three files share that name.

## Unknown / needs the lead

- Whether the real asset set reaches `manifest.complete` and `integration.passed` — needs the
  conversion to be run; the note says which two gates block first.
- Whether any `STAT`/`MSTT`/`FURN`-reachable NIF uses `NiTriStripsData` (unsupported, no parser
  anywhere in the tree). `nif-audit` over `vfs\meshes` answers it in minutes.
- The true dense/water cell coordinates. Nothing in the repo can tell you which cells are dense;
  `world-inspect` is the tool, and the docs disagree with each other (12,8 versus 18,−5).
- Windows behaviour of the quality/robustness gates: CI is Linux nextest with `--all-features`,
  the campaign runs Windows `cargo test --workspace --all-targets -j1`.

## Next step for the lead

1. `cargo build --workspace --release` (no release binaries exist yet).
2. Run the converter once (exact command in section 4 of the note), then read
   `integration-report.json` and the closure report before spending a campaign.
3. Only after both are green: pick three distinct grid pairs, sign a visual review, and expect to
   need two or three campaigns (`-UpdateBaseline` first — see section 5 of the note).

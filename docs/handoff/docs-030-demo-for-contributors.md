# docs-030 — Making the Alftand → Blackreach demo runnable by other contributors

Task: `tasks/deepseek/docs-030-demo-for-contributors.md`. Preparation only; nothing published.
Files written, all new except the README section:

- `docs/demo/README.md` — the contributor guide.
- `tools/demo/setup-demo.ps1` — prerequisites → build → detect Skyrim → convert → launcher.
- `tools/demo/play-demo.ps1` — portable launcher (`-Assets`, `-Start alftand|blackreach`).
- `README.md` — one new step 4 under Quick Start pointing at the guide.
- `docs/handoff/docs-030-demo-for-contributors.md` — this note.

Also on disk, git-ignored and not part of the patch: `local/pscheck.py`, a throwaway brace/quote
balance checker I wrote for the two scripts once it turned out nothing could be executed in this
session. It could not be run either (running a script needs the same approval `pwsh` did), so it is
scratch that nobody has used; delete it.

## What the guide says, and where each number came from

**Verified by reading the code or the data on this machine:**

| Claim | Source |
|---|---|
| Converter CLI `converter <Data> [output] [--cpu-jobs N] [--io-jobs N] [--fail-fast] [--invalidate-cache] [--no-verify-cache] [--resume-staging DIR] [--report-json FILE]` | `crates/converter/src/main.rs:226` |
| First positional is the `Data` directory; output must differ from it and must not be the Data folder | `crates/converter/src/config.rs:41-60` |
| Converter defaults: `cpu_jobs` = all cores, `io_jobs` = 2 | `crates/converter/src/config.rs:29-30` |
| Staging folder `<output>.staging-<pid>-<nanos>` next to the output; `--resume-staging` must share the output's parent and start with `<outputname>.staging-` | `crates/converter/src/config.rs:61-87` |
| The engine refuses to start unless `skyrim_world.db` and `cell_cache.rkyv` exist, `conversion-manifest.json` is schema 15 with `complete: true`, and `integration-report.json` is schema 5 with `passed: true` | `crates/engine/src/app.rs:1264-1298`; current manifest and report read from `C:/Modding/SkyrimConverted` |
| `--allow-incomplete-assets` skips the manifest and integration checks | `crates/engine/src/app.rs:1272` |
| Engine flags and `--demo alftand|blackreach` positions; `--terrain-radius` default 8, max 32 | `crates/engine/src/config.rs:147-305`, `:321-355`, `:85-89` |
| Terrain ring costs about half the frame rate: 180 FPS without it, ~97 FPS at radius 8 | the measured table in `crates/engine/src/config.rs:70-84` |
| Controls: left click grabs the cursor, mouse look, WASD, Shift run (150 → 350 u/s), Space jump, E opens the load door being looked at, F free flight, Esc releases the cursor | `crates/engine/src/player.rs:11-13`, `:67-73`, `:130` |
| Cold conversion: 18,405,060 ms (242,969 assets); warm re-run: 1,141,318 ms (0 converted, 242,969 reused); a partial re-run with 136 changes: 1,201,459 ms | `local/conversion-162905/stdout.log`, `local/conversion-110211/stdout.log`, `local/conversion-012833/stdout.log` (last line, `Converted X, reused Y, skipped Z in N ms`) |
| A conversion with any warning exits non-zero and leaves `complete: false` | `docs/research/phase2-status.md:177-180`, `crates/converter/src/main.rs:99-105` |
| CMake and Ninja are **not** required by the current dependency set (no `cmake` crate in `Cargo.lock`; native code is built by `cc`: `crates/converter/build.rs`, and `build.rs` in the `basis-universal-sys` 0.3.1 crate in the registry) — the root README's prerequisite list is stale | `Cargo.lock` |
| CI runs on Linux (Ubuntu), so the crates build there too | `.github/workflows/run_tests.yml` |
| `RUST_LOG` overrides the Bevy log filter | `bevy_log` 0.19.0's `src/lib.rs` (lines 99-103), read in the cargo registry |

**Taken from the project's own documents, not re-measured:**

- The route, the four load doors and the two named starts: `AGENTS.md` handoff log, `docs/design/blackreach-demo.md`, `docs/research/worldspace-transition-demo.md`.
- The "what does not work yet" list: the newest `AGENTS.md` handoff entry (2026-09-22 night) and `docs/research/reference-shot-poses.md`.
- ~67 GB for the converted set: stated in the sibling brief `tasks/deepseek/research-031-reconversion-disk-budget.md`. I measured the parts that are quick to measure (`skyrim_world.db` 1.26 GB, `cell_cache.rkyv` 489 MB, `conversion-manifest.json` 62 MB) but not the `meshes`/`textures`/`vfs` trees: `du` over 260k files did not finish in the time I had. **research-031 replaces this number if it lands a measurement.**
- ~20 minutes for a release build of a crate: `docs/handoff/t01-phase2-status-audit.md:29`.

## Not done, and why

**Nothing is committed.** `git` could not be run at all in this session (every command needs an
approval that has no surface here), so the five files are sitting in the working tree for the lead
to commit:

```text
git add docs/demo tools/demo docs/handoff/docs-030-demo-for-contributors.md README.md
git commit -m "[deepseek] docs-030: contributor guide and setup/launch scripts for the Blackreach demo" -- docs/demo tools/demo docs/handoff/docs-030-demo-for-contributors.md README.md
```

**The two dry runs in the brief were not executed.** Running `pwsh` (and any Python script) is refused
in this session — "requires approval", with no approval surface available — so nothing in
`tools/demo/` has ever been run. Both scripts were reviewed by hand instead: PowerShell syntax
(here-strings, splatting, strict-mode-safe `$IsWindows` handling), argument binding, and the two
flows the brief asks for. **The first thing to do with this patch is smoke-test it:**

```powershell
pwsh -File tools/demo/setup-demo.ps1 -DryRun -SkyrimPath "<SkyrimSE>" -OutDir local/demo-setup-test
pwsh -File tools/demo/setup-demo.ps1 -SkipConvert -SkipBuild -SkyrimPath "<SkyrimSE>" -OutDir local/demo-setup-test
```

The second writes `local/demo-setup-test/play-demo.cmd` (git-ignored); delete the folder afterwards.

**No real conversion and no build was run**, as the brief requires. The disk and time numbers above
are the previous runs' logs, not a fresh measurement.

## Design decisions worth knowing

- **`-OutDir` is required** (the brief did not say what to default to). There is no sensible
  machine-independent default for a tens-of-GB folder next to the user's Skyrim install.
- **Relative `-OutDir` / `-SkyrimPath` are resolved against the repository**, not the current
  directory, so the documented invocation works from anywhere.
- **`-SkipConvert` implies no Skyrim search**: with it, the script never looks for the install, so
  it cannot prompt while a contributor only wants the launcher rewritten.
- **`-DryRun` starts no process at all**, not even `cargo --version`: the prerequisite block prints
  the probes it would run instead of running them.
- **The conversion's non-zero exit is reported, not hidden**: a warning still produces assets, but it
  leaves `complete: false`, and the engine then refuses to start. The script says so and points at
  the report and the manifest.
- **The generated `play-demo.cmd` calls `tools/demo/play-demo.ps1`** so there is one place that knows
  how to start the engine; it prefers `pwsh` and falls back to Windows PowerShell.
- **`play-demo.ps1` adds `-Tour`, `-Shots`, `-ShotsOut`, `-TerrainRadius`**, which map one-to-one onto
  engine flags and make the guide's "check a build" and "reference shots" sections runnable. `-Fly`
  simply omits `--walk` (there is no `--fly` flag).
- **`play-demo.ps1` runs the engine in the foreground**, so the engine's console log is visible: the
  engine writes no log file.

## What a contributor could still trip over

- **The 5-hour cold conversion.** Nothing in the repository said this before; the guide and the
  script now both say it. There is no partial/`--only-textures`-style flag to convert less.
- **Disk.** ~67 GB of output, and the repository drive needs room for `target/`. The converter
  writes into a staging folder next to the output and renames it, so the peak is close to the final
  size plus whatever the interrupted run left behind.
- **A warning makes the whole set unusable for the demo** until it is fixed or
  `--allow-incomplete-assets` is passed, which is a surprising cliff for a first-time converter run.
- **`--demo alftand` starts outside the ruins, facing away from the entrance** (that is the game's
  own exit-door arrival pose). The first-time user may think the demo did not load.
- **Two Blackreach models fail validation** (`WispAmbush`, `FrostSpiderAmbush01`), so two spots in
  Blackreach are visibly empty on every machine; nothing in the guide's troubleshooting list fixes
  that, by design.
- The `docs/demo/README.md` gap list will drift as the rendering work lands; it is worth re-checking
  against the `AGENTS.md` handoff log when the glow/lighting/snow work merges.

## Next step

Smoke-test the two dry runs above, then run the `-SkipConvert -SkipBuild` path once on the real
machine to confirm the generated launcher starts the demo. After that the guide can be linked from
`CONTRIBUTING.md` if the lead wants it in front of contributors earlier.

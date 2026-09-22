# The streaming fixture with the terrain ring never exits (research-028)

**Verdict: the fixture never ends by itself. It is not a terrain-ring bug.** The lead's command
named no benchmark window, and the only exit in a fixture run is the benchmark report path, which
is disabled when neither `--benchmark-duration` nor `--benchmark-frames` is given
(`crates/engine/src/metrics.rs:104`). The same fixture with a duration and the ring at radius 8
validated and exited 0. The campaign's streaming scenario (`scripts/phase2-acceptance.ps1:259`)
always passes a duration, and it never turns the ring on at all, so Phase 2 is not exposed to this
hang. The one thing the fixture genuinely cannot do is exercise ring *residency* - section 4.

Two claims in the brief are corrected by the evidence:

- impl-021's fixture run *did* have the ring on. Its `ring_cells_empty = 264` is 17² − 25, the exact
  number of terrain-only cells a radius-8 ring plans around a radius-2 grid
  (`local/impl/impl-021-distant-terrain/local/impl-021/fixture-r8.json:35`).
- `terrain_cells = 0` does not mean the ring was idle. In a fixture run the ring's cells can never
  become resident, because the fixture's cell cache is empty (section 4). `terrain_cells` is
  structurally 0 for every fixture run, whatever the radius.

## 1. What the fixture does, and where it stops

A `--streaming-fixture` run replaces the configured assets with a generated SQLite/cache fixture in
`%TEMP%` (`crates/engine/src/app.rs:50`) and registers three systems
(`crates/engine/src/app.rs:214`):

- `drive_streaming_fixture` (`crates/engine/src/app.rs:379`) moves the camera through six jumps at
  frames 4, 5, 6, 14, 22 and 30, then returns it to the origin on the last one, and stops.
- `validate_streaming_fixture` (`crates/engine/src/app.rs:409`) runs from frame 90 on. It passes when
  the world has settled, the request/response/stale/unload/rebase counters are all non-zero, the
  inner grid is fully resident and nothing is resident outside the widest band the plan keeps:
  `expected_resident` is `(2 × stream_radius + 1)²`, 25 at the default radius 2, and
  `maximum_resident` is `(2 × max(terrain_radius + 1, unload_radius) + 1)²` - 361 with the ring at 8,
  49 with it off (`crates/engine/src/app.rs:418`, `crates/engine/src/app.rs:422`). Resident cells
  above the inner grid are the ones the plan no longer wants but has not unloaded yet, because the
  unload band is wider than the inner grid - which is why the ring-off run reports 32 of them
  (section 3). On success it sets `metrics.streaming_fixture_validated` and
  `state.finished` (`crates/engine/src/app.rs:440`); if it is still failing at frame 300 - about 5 s
  at 60 Hz - it logs `streaming fixture did not settle or violated its lifecycle contract` and sets
  the same flag (`crates/engine/src/app.rs:444`).

Neither branch writes `AppExit`. The only `AppExit` on this path is `collect_and_finish`
(`crates/engine/src/metrics.rs:316`), and that system returns before doing anything when no
benchmark window was asked for (`crates/engine/src/metrics.rs:104`). So:

```
--streaming-fixture                       -> renders for ever
--streaming-fixture --benchmark-duration N -> measures N seconds, writes the report, exits
```

`state.finished` only stops the fixture driving the camera; it is a fixture-state flag, not an exit
(`crates/engine/src/app.rs:384`).

## 2. The lead's run: the fixture had finished, and nothing stopped the app

`local/demo/fixture-ring.err` is the run in question: 15129 lines, of which 15121 are the per-frame
diagnostics. Its 8 other lines are adapter info, two renderer capability lines, the window,
`OpenSkyrim runtime initialized`, and three gamepad lines - **there is no `ERROR` line, and no
`engine` warning at all.** It logs `frame_count` once a second for 378 s (13:28:15.49 to
13:34:34.17), ending at 23712 frames, about 63 FPS.

That is the proof the fixture did not hang in the streaming code. A fixture that cannot settle logs
`streaming fixture did not settle or violated its lifecycle contract` at frame 300
(`crates/engine/src/app.rs:444`), so a 23712-frame run with no error reached `valid` and set
`state.finished` before frame 300 - the ring's 264 terrain-only cells notwithstanding. What it then
did is render the synthetic scene for ever, exactly as the code above says it must.

## 3. Reproduction and timings

Scripts: `tools/research/fixture_ring_probe.py` (the four shapes) and
`tools/research/fixture_no_benchmark_probe.py` (the hang with the ring on and off);
logs and reports in `local/research-028/`.

| shape | command | result |
| --- | --- | --- |
| lead | `--streaming-fixture --terrain-radius 8`, no benchmark window | ran 23712 frames / ~379 s and never exited (the lead's log); in this session still running when a 60 s timeout fired, killed by the probe script |
| same, ring off | `--streaming-fixture --terrain-radius 2`, no benchmark window | ran 14132 frames / 194 s, no `ERROR`, no exit - still running when a 300 s timeout fired, killed by the probe script |
| ring 8 + duration | `--streaming-fixture --terrain-radius 8 --benchmark-duration 20` | exit 0, `streaming_fixture_validated = true`, `passed = true` |
| ring 2 + duration (old engine) | `--streaming-fixture --terrain-radius 2 --benchmark-duration 20` | exit 0, `streaming_fixture_validated = true`, `passed = true` |
| campaign shape | `--streaming-fixture --max-commit-ms 16.67 --benchmark-duration 20` plus the campaign's profiling flags | exit 0 after 145.6 s, scenario `streaming`, 9033 frames, 452 FPS, `streaming_fixture_validated = true`, `passed = true` |

Measured numbers from the reports (`local/research-028/*.json`; this machine had other agentic runs
active throughout, so FPS and wall clock are rough and should not be read as a ring cost):

| metric | ring 8 | ring 2 (ring off) |
| --- | --- | --- |
| frames in the 20 s window | 8700 | 6959 |
| average FPS | 435 | 348 |
| p95 frame | 3.29 ms | 4.81 ms |
| requests / responses / stale | 783 / 783 / 475 | 167 / 167 / 133 |
| resident cells (peak) | 25 (25) | 32 (32) |
| peak concurrent requests | 287 | 32 |
| `terrain_cells` (peak) | 0 (0) | 0 (0) |
| `ring_cells_empty` | **264** | **0** |
| lifecycle / commit-budget failures | 0 / 0 | 0 / 0 |
| `origin_rebases` | 6 | 6 |
| entity count | 506 | 513 |

The ring adds 616 fixture requests (783 against 167) and nothing else that this fixture can see:
`ring_cells_empty` 264 is exactly the ring, and it is 0 with the ring off.

The hang is the same with the ring off: `--streaming-fixture --terrain-radius 2` and no benchmark
window rendered 14132 frames over 194 s in this session, logged no `ERROR` and never exited
(`local/research-028/nobench-r2.err`). Since the failure branch would have fired at frame 300, that
run had also validated. With the ring on and the ring off, the only thing missing is an exit.

The real assets agree. The lead's own ring-on runs with the converted data -
`local/demo/ring.err`, `local/demo/ring2.err`, `local/demo/rotfix.err` - contain no `ERROR`, no
`streaming lifecycle invariant failed` and no panic; the lifecycle validator only logs when it finds
a violation, so the ring is not leaving stray or missing cell roots in an interactive run either.

One deadline in the fixture is measured in frames, not seconds: the failure branch fires at frame
300 (`crates/engine/src/app.rs:444`), which is 5 s in the lead's vsync'd 60 FPS run but under a
second at a benchmark's few hundred FPS. The ring's 264 requests all have to be answered inside that
window, and here they were (the fixture validated, no error), but the headroom shrinks every time
the machine or the GPU gets faster. A wall-clock deadline for "settled" would be the honest bound -
the fixture's purpose is to prove the lifecycle settles, not to prove it settles within 300 frames.

Wall-clock caveat for anyone repeating this. Every phase outside the frame loop is far slower in a
worker session than in the lead's log. ring 8: 80 s between process start and the first log line,
21 s of rendering, then ~30 s after the report before the process ended (writing and removing the
fixture). ring 2, measured while two other engine runs were active on the machine: 198 s before the
first log line. The lead's log shows the same startup at under 0.5 s. That time is spent before the
app's first log, so it is fixture-database creation and process start-up, not streaming; the
acceptance script keeps it out of `%TEMP%` altogether
(`scripts/phase2-acceptance.ps1:34`: `$env:TEMP = target\test-temp`). A fixture run that looks
frozen for a minute or two in a worker session is probably still writing its SQLite fixture, and its
report's `elapsed_seconds` is the only comparable timing.

## 4. What a fixture run can and cannot say about the ring

`StreamingFixtureDirectory::populate` writes an **empty** cell cache - `CellCache { version, cells:
Vec::new() }` (`crates/engine/src/app.rs:327`) - and empty reference and landscape tables
(`crates/engine/src/app.rs:301`). The plan, on the other hand, is real: with `terrain_radius` 8 and
`stream_radius` 2 the wanted set is the 17×17 grid out to `max(terrain_radius, stream_radius)`
(`crates/engine/src/streaming.rs:873`), where everything within 2 cells is `CellDetail::Full` and
everything out to 8 is `CellDetail::Terrain` (`crates/engine/src/streaming.rs:941`) - 25 full cells
and 264 terrain-only ones.

Every one of those 264 requests comes back with no terrain, so each is counted in `ring_cells_empty`
and its cell is marked failed (`crates/engine/src/streaming.rs:712`, `crates/engine/src/streaming.rs:764`),
with `debug!` rather than `error!` because an empty ring cell is normal. A cell that failed at a
tier is not asked for that tier again (`crates/engine/src/streaming.rs:451`), so the ring settles
immediately and the fixture can pass. `terrain_cells` counts *resident* cells whose detail is
`Terrain` (`crates/engine/src/streaming.rs:544`) - none can ever exist in a fixture run.

So a fixture run proves the ring's planning and its empty-cell handling (264 requests, 264 empty
answers, no failures, no invariant breakage). It cannot prove that ring terrain is fetched,
validated, committed, drawn, fogged or unloaded. For that the ring needs the converted assets: an
interactive run, or a fixture whose cache holds terrain out to the ring's radius, which
`populate` does not build today.

Existing coverage of the ring is unit-level: the tier bands
(`crates/engine/src/streaming.rs:941`) are tested (`crates/engine/src/streaming.rs:2958`,
`crates/engine/src/streaming.rs:3006`), as are the far plane, the fog and the ring's effect on the
atmosphere choice (`crates/engine/src/app.rs:1815`, `crates/engine/src/app.rs:1887`).

## 5. The Phase 2 campaign

The streaming scenario is `--streaming-fixture --max-commit-ms 16.67`
(`scripts/phase2-acceptance.ps1:259`) plus, for every scenario, `--benchmark-duration`,
`--benchmark-output`, the profiling flags and the thresholds
(`scripts/phase2-acceptance.ps1:276`). It names no `--terrain-radius`, and any run with a benchmark
window turns the ring off unless the radius is named
(`crates/engine/src/config.rs:299`): `terrain_radius = stream_radius`, so the campaign's fixture
scenario is the engine as it was before the ring, and this hang cannot reach it. The campaign does
require the fixture to validate - the fixture's screenshot is only taken once
`streaming_fixture_validated` is set (`crates/engine/src/app.rs:1752`), and the report's
`no_streaming_failures` gate reads the same flag (`crates/engine/src/metrics.rs:169`) - and that part
passes.

Two things worth fixing while this is fresh:

- `Invoke-RecordedCommand` (`scripts/phase2-acceptance.ps1:103`) runs its command with no timeout,
  so a run that never returns hangs the whole campaign instead of failing it.
- Nothing in the campaign or the fixture contract notices the ring. `--terrain-radius 8` added to
  the streaming scenario would exercise the ring's plan and empty-cell path in CI: my run of that
  exact flag set (`--streaming-fixture --terrain-radius 8 --benchmark-duration 20`, with the
  campaign's `--max-commit-ms 16.67`, which is the 16 670 µs default of
  `crates/engine/src/config.rs:101` set to itself at `crates/engine/src/config.rs:187`) exited 0
  with the fixture validated and no commit-budget violation. Requiring `terrain_cells > 0` there
  would fail on the empty fixture cache, so the honest ring assertion for the fixture today is
  `ring_cells_empty + terrain_cells > 0` when the ring is on.

## 6. Minimal fix

`crates/engine/src/app.rs`, `validate_streaming_fixture` (`crates/engine/src/app.rs:409`): give the
system an `AppExit` writer and let it end the run when no benchmark window will.

```rust
fn validate_streaming_fixture(
    config: Res<EngineConfig>,
    mut state: ResMut<StreamingFixtureState>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
    mut exit: MessageWriter<AppExit>,          // added
) {
```

- after `state.finished = true;` in the valid branch (`crates/engine/src/app.rs:443`):

```rust
if config.benchmark_frames.is_none() && config.benchmark_duration_secs.is_none() {
    exit.write(AppExit::Success);
}
```

- after `state.finished = true;` in the failure branch (`crates/engine/src/app.rs:451`), the same
  guard with `AppExit::error()`.

The predicate is the one `benchmark_active` already uses (`crates/engine/src/app.rs:117`); extracting
it as `fn benchmark_window_requested(config: &EngineConfig) -> bool` keeps the two in step. With a
benchmark window present nothing changes: `collect_and_finish` still owns the exit, and the report
is still written at the end of the window. No new import is needed - `bevy::prelude::*`
(`crates/engine/src/app.rs:24`) already brings in `AppExit` and `MessageWriter`, which is how
`crates/engine/src/demo_tour.rs:124` takes its own exit writer.

One consequence to accept: `--streaming-fixture` alone then ends about two seconds in, as soon as
the fixture validates, instead of rendering until someone closes the window. Watching the ring is an
interactive run (`--terrain-radius 8` without `--streaming-fixture`), which is what the screenshot
work uses anyway.

The same trap exists for the other four fixtures - `--material-fixture`
(`crates/engine/src/app.rs:220`), `--terrain-water-fixture`, `--transform-bounds-fixture` and
`--renderer-fixture` (`crates/engine/src/app.rs:223`) all validate and never exit - so the guard
belongs in their validators too, or in one helper they all call.

Alternative, if the lead would rather the fixture always produce numbers: make the fixture imply a
benchmark window in `EngineConfig::from_args`, next to the ring defaulting
(`crates/engine/src/config.rs:299`) - when a fixture flag is set and neither benchmark flag is given,
set `benchmark_duration_secs = Some(20.0)`. That also makes the run exit and leaves
`benchmark-report.json` behind. It is a one-line change but it silently measures a run the user may
have meant as a look-around, so it wants a warning log if it is the chosen fix.

## 7. Tests that would catch it

1. **Unit test of the exit decision, in the engine's own test module.** Extract the decision so it
   can be tested without a GPU:

   ```rust
   fn fixture_exit(valid: bool, config: &EngineConfig) -> Option<AppExit>
   ```

   Then, in `crates/engine/src/app.rs`'s `tests` module (alongside
   `crates/engine/src/app.rs:1815`):

   - `fixture_exit(true, &EngineConfig { streaming_fixture: true, ..default() })` is
     `Some(AppExit::Success)`;
   - the same with `benchmark_duration_secs: Some(20.0)` is `None` (the benchmark path keeps the
     exit);
   - `fixture_exit(false, ...)` is `Some(AppExit::error())`.

   This is the regression that matters: today the command `--streaming-fixture` alone is a run that
   cannot end.

2. **A campaign robustness gate that runs the fixture with no benchmark window and requires exit 0**,
   next to `runtime-rejects-missing-assets` (`scripts/phase2-acceptance.ps1:245`). It needs a
   timeout first, because `Invoke-RecordedCommand` has none - `Start-Process -PassThru` plus
   `Wait-Process -Timeout` (killing only that process id), or the same pattern the Python probes
   under `tools/research/` use.

3. **A ring assertion in the fixture contract** (optional, cheap): when
   `config.terrain_radius > config.stream_radius`, require
   `metrics.ring_cells_empty + metrics.terrain_cells > 0` and
   `metrics.requests_submitted > expected_resident`, in `validate_streaming_fixture` around
   `crates/engine/src/app.rs:427`. That catches the ring being dropped from the plan - which nothing
   currently notices - without pretending the fixture holds ring terrain.

## 8. Open questions

- Why fixture creation takes ~112 s in a worker session's temp directory and under 0.5 s in the
  lead's (section 3). Not investigated further; the campaign is unaffected because it redirects
  `TEMP` into the repository.
- Whether the fixture should grow terrain so that ring residency, commit budgeting and the ring's
  fog can be gated without proprietary assets. That is a change to
  `StreamingFixtureDirectory::populate` (`crates/engine/src/app.rs:301`) plus `shared`'s cell-cache
  contract, not a fix for this hang.

# research-028 - why the streaming fixture with the terrain ring never exits

**Verdict: the fixture has no exit of its own.** The lead's run
(`target/release/engine.exe --assets C:/Modding/SkyrimConverted --streaming-fixture
--terrain-radius 8`) names no benchmark window, and the only `AppExit` a fixture run can reach is
the benchmark report path, which returns immediately without `--benchmark-duration` or
`--benchmark-frames` (`crates/engine/src/metrics.rs:104`, `crates/engine/src/metrics.rs:316`). The
terrain ring is not involved: the same fixture with a duration and the ring at radius 8 validated
and exited 0, and its own log shows the fixture had already finished before the lead killed it
(23712 frames, no error, and the failure branch logs one at frame 300, `crates/engine/src/app.rs:444`).

## Verified

- Cause and code path, with citations: `docs/research/fixture-ring-timeout.md`.
- Reproduction (`local/research-028/`, `tools/research/fixture_ring_probe.py`):
  - `--streaming-fixture --terrain-radius 8`, no benchmark window: still running when a 60 s
    timeout fired, killed by the probe script (the lead's own log shows the same run reaching
    23712 frames and never exiting).
  - `--streaming-fixture --terrain-radius 8 --benchmark-duration 20`: exit 0 after 132.4 s wall,
    8700 measured frames, 435 average FPS, `streaming_fixture_validated = true`, `passed = true`,
    `ring_cells_empty = 264`, `terrain_cells = 0`, 0 invariant failures, 0 commit-budget violations.
  - `--terrain-radius 2` and the campaign's own flag set: `local/research-028/ring2.json`,
    `local/research-028/campaign.json`.
- impl-021's `fixture-r8.json` **did** have the ring on: `ring_cells_empty = 264` is 17²−25, the
  exact size of a radius-8 ring around a radius-2 grid.
- `terrain_cells = 0` in any fixture run is structural, not a sign the ring was idle: the fixture's
  cell cache is empty (`crates/engine/src/app.rs:327`), so every terrain-only request is answered
  with nothing and counted in `ring_cells_empty` (`crates/engine/src/streaming.rs:712`,
  `crates/engine/src/streaming.rs:764`), and no ring cell can ever be `Resident`.
- The acceptance campaign is not exposed: its streaming scenario names no `--terrain-radius`
  (`scripts/phase2-acceptance.ps1:259`) and any run with a benchmark window turns the ring off
  unless the radius is named (`crates/engine/src/config.rs:299`), so that scenario runs the engine
  as it was before the ring.

## Still unknown

- Why a fixture run in this session writes nothing for ~112 s before its first log line (the same
  step takes under 0.5 s in the lead's log). It is before the app's first log, so fixture creation
  and process start-up, not streaming; the campaign redirects `TEMP` into the repository
  (`scripts/phase2-acceptance.ps1:34`), which is the shape to use. Wall-clock numbers from this
  session's runs are not comparable with the lead's for this reason.
- Whether the fixture should grow terrain so that ring residency, ring commit budgeting and the
  ring's fog can be gated without proprietary assets. Today the fixture can only cover the ring's
  plan and its empty-cell path, never ring geometry on screen.

## Next step

Apply the fix in `docs/research/fixture-ring-timeout.md` section 6 (an `AppExit` writer in
`validate_streaming_fixture`, guarded on no benchmark window; the other four fixture validators have
the same trap) with the unit test from section 7, and consider giving the acceptance runner a
per-run timeout (`scripts/phase2-acceptance.ps1:103` has none, so a run that never returns would
hang a campaign rather than fail it).

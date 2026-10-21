# research-028 - Why does the streaming fixture with the terrain ring never exit?

Read `tasks/deepseek/context.md` first.

## Goal

The lead ran:

```
target/release/engine.exe --assets C:/Modding/Mudcrab/SkyrimConverted --streaming-fixture --terrain-radius 8
```

on the current build, and it did not exit within 400 s. It was killed; its logs are
`local/demo/fixture-ring.err` and `.out`. The worker that built the ring (impl-021) reports a
validated fixture run (`local/impl/impl-021-distant-terrain/local/impl-021/fixture-r8.json`), but
with `terrain_cells 0`, so the ring was not exercised. Find out why the fixture does not finish,
and whether that is a real bug in the terrain ring (impl-021, commit `5211bad`) or in the fixture.
The Phase 2 acceptance campaign (`scripts/phase2-acceptance.ps1`) runs the streaming fixture, so
this matters.

## How

- Read how the fixture decides it is done: `crates/engine/src/app.rs` (`drive_streaming_fixture`,
  `validate_streaming_fixture`, `StreamingFixtureState`) and the streaming code it waits on
  (`crates/engine/src/streaming.rs`).
- Reproduce: run the command above yourself with `--benchmark-output <file>` if the fixture takes
  one (check `config.rs`), or read the logs. Compare with `--terrain-radius 2`, which is the old
  engine. Use `local/research-028/` for output.
  Run the engine from a small Python script under `tools/research/` using
  `subprocess.run(..., timeout=...)`, so a hung run is killed by the script itself (only its own
  child). Never kill processes by name: the user may have the demo open.
- Find the stuck condition: a fixture step waiting for residency that ring cells never reach, the
  held-response queue, a terrain-only cell counted as pending forever, the `is_resident` semantic
  change, or something else. Give `file:line`.

## Deliverable

- `docs/research/fixture-ring-timeout.md`: cause, evidence, the minimal fix (as a patch
  description with `file:line`, not an edit: you may not edit `crates/`), and a test that would
  catch it.
- Commit only that file and a handoff note in `docs/handoff/`, with `[deepseek]` subjects.

## Report

The cause, the evidence, the proposed fix, and the timings of the old and new engine.

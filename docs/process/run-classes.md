# Run classes: timing runs and test runs (the user, 2026-10-14)

Every engine run on this PC is one of two classes. The class decides what may run beside it.

| Class | What | At once | Gate | Window title |
|---|---|---|---|---|
| TIMING | Any run whose numbers you will quote: `--benchmark-*`, `--portal-bench`, `--tour-bench`, perf baselines, fps, frame-time or load-time claims | One on the whole machine; nothing else heavy runs beside it | A timing hold (name starts `timing`, or `--quiet-disk`), with `--drain` | `Mudcrab [TIMING] <kind>: <label> \| <thread>` |
| TEST | Everything else that opens an engine: bug repro, visual check, `--shots`, demo tour, `--route`, `--start-shot`, streaming fixture, BRP session | Up to 3 on the machine | `build_gate.py test`; it refuses while any hold is set | `Mudcrab [TEST] <kind>: <label> \| <thread>` |
| Build | cargo | The existing slots | `build_gate.py run` | n/a |

Interactive play (no automation flag) stays plain `Mudcrab`.

**Which class is mine?** If you will read a number from the run and quote it, it is TIMING. Otherwise it is TEST. A TEST run never reports fps as a result: it shares the machine and may be slow because of it. The engine picks the class from its own flags (`EngineConfig::window_title`, `times_frames`), so a window cannot claim the wrong class.

## Is a timing run worth it? (the user, 2026-10-15)
A timing run holds the whole machine for 20 minutes or more and blocks other threads' builds, so it is never a routine step of a PR. Run one only when you can write, before starting, all three of these in the thread:
1. **Mechanism:** what the change adds, removes or moves on the CPU or GPU path (more draw calls, a new pass, a bigger loop, a lower resolution, a heavier shader, more memory traffic). "It touches render code" is not a mechanism.
2. **Prediction:** which number should move and about how far, and what result would make you change the work.
3. **Scene:** a scene that exercises that mechanism (the camera really sees what the change draws), so a flat result means something.

Skip the run, and write one line saying why, when the change cannot plausibly move cost: a clip plane or culling change that draws no more than before, a colour or constant, UI, docs, tests, converter-only work, a refactor with the same work per frame. A reviewer asking for a number is a reason; a habit of quoting one is not.
Never run one just to fill a "Benchmark Proof" section: the section is dropped when there is nothing to prove (STYLE.md).
If you do run one, a flat result is a result: report it in one line and stop; do not repeat runs to find a difference.

## Profiler captures (2026-10-15)
Tracy, PIX and RenderDoc captures are TEST runs: the profiler slows the engine, so no number from one is quoted as fps or frame time. Start them with `tools/perf/perf_capture.py` (it uses `build_gate.py test`). Which tool answers which question: `docs/process/profiling-tools.md`.

## Starting a TEST run
```
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py test --owner "<thread title>" --label "<what>" -- <engine.exe> <args> --run-label "<what>" --run-owner "<thread title>"
```
- Exit 4 means a hold is set (a timing window or a conversion; the hold's owner and END-by time are printed) or all 3 TEST slots are busy. Do not work around it: start the run after the hold clears. There is no queue.
- A TEST run that needs no gate check because it is a unit test without an engine window is not a TEST run; cargo tests go through `build_gate.py run`.

## Starting a TIMING run
1. `build_gate.py hold set timing-<id> --owner "<thread>" --reason "<what>" --until HH:MM --drain`. The hold is refused while any other hold is set. `--drain` waits for running builds and TEST runs to end, then for quiet disks.
2. Run the benchmark (`tools/bench/portal-bench.ps1`, `scripts/phase2-profile.ps1`). The first run after a build is a discarded warm-up. Set `MUDCRAB_RUN_OWNER` once per thread so the scripts add `--run-owner`.
3. `build_gate.py hold clear timing-<id>` the moment the run ends (realistic END-by time; a stale hold blocks every build and TEST run).
4. Say in the thread when the window starts and ends.

## Window names
The title is `Mudcrab [CLASS] <kind>: <--run-label> | <--run-owner>`. Always pass `--run-label` (what the run is) and `--run-owner` (the thread's title, "Fix: doors") on every automated run, so the taskbar and Alt-Tab say what and whose each engine is. Screenshots for PRs stay cropped to the window; check the title is not in the crop.

Source: `crates/engine/src/config.rs` (`window_title`, `--run-owner`), `tools/build_gate.py` (`test`, test slots in the gate folder).

**Status:** DeepSeek research-157, 2026-09-24, kept verbatim. Numbers come from existing logs (the worker had no shell); nothing here is adopted yet.


**research-157 — where the time goes in the automated checks**

---

## 0. What I could and could not do

**No shell tool was granted in this session** (my tools are read-only file search/read plus messaging). I could not run `cargo` or the engine at all, so **every baseline number below is a measurement already recorded in the repository** — engine logs with per-line wall-clock timestamps, the acceptance campaign's own `quality-gates.json`, and worker harness logs. I name the file for each so you can spot-check. The *proposed* savings are arithmetic over the measured phase model, not measurements; each proposal carries the exact command to confirm it.

Also: the user is on the GPU right now, so the tour timings I quote are upper bounds (I say so again where it matters).

## 1. Measured baseline

| What | Command | Measured | Source |
|---|---|---|---|
| Riverwood walk tour | `target\release\engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour <dir>` | **232.8 s** of tour (first `tour route:` line → `tour PASSED after 8 crossings`); ~236 s per process | `local/demo/base154-riverwood.out` (timestamps 22:13:38.336 → 22:17:31.134) |
| Alftand walk tour | same, `--demo alftand` | **132.5 s** | `local/demo/swing2-alftand.out` (03:14:51.748 → 03:17:04.250) |
| Engine start-up | any tour | **0.88 s** from first log line to `tour route:` | same logs |
| `cargo fmt --all -- --check` | (recorded gate) | **0.48 s** | `local/acceptance-shakedown/20260922-003549-…/quality-gates.json` |
| `cargo test --workspace --all-targets -j1` | (recorded gate) | **91.5 s** wall, of which **90 s is compilation** ("Finished `test` profile … in 1m 30s") | same run, `logs/tests.log` |
| Test *execution* | 15 binaries in that run | **≈0.25 s total**; largest single binary 0.14 s (converter lib, incl. its end-to-end pipeline tests) | `logs/tests.log:143,258,…` |
| First `cargo test -p engine <name> -j1` in that campaign (engine test target cold) | | **1m 33s**; the other 13 single-test commands after it: **0.5-0.6 s** each (warm) | `logs/app-tests-…log`, `logs/world-*`, `logs/streaming-*` |
| `cargo clippy --workspace --all-targets -j1 -- -D warnings` | (recorded gate) | **12.79 s** (warm-ish: only Checking 4 crates) | `quality-gates.json` |
| `cargo clippy -p engine --all-targets -- -D warnings` (warm) | worker harness | **0.81 s** | `local/agents/impl-047-result.txt:14` |
| `cargo build --workspace --release -j1` | (recorded gate) | **913.8 s** — and `release-build.log` shows it compiled **only the engine crate** (deps were warm). `codegen-units = 1` + thin LTO, single-threaded | `quality-gates.json`, `logs/release-build.log:1-2` |
| `cargo build --release -p engine` (warm deps, parallel) | worker harness | **3m 11s / 3m 56s / 4m 13s** | `local/agents/impl-021-resume.out.txt:31`, `local/impl/impl-103-release-build.log:3`, `local/agents/impl-052-resume.out.txt:90` |
| `--shots` settle per pose | `--shots tools/reference/*.json` | **0.05-2.10 s** per shot, 25 shots, none timed out (30 s ceiling) | `local/reference/render1/shots.log:2-50` |
| Screenshots written by one Riverwood walk tour | | **1,264 PNGs** at 1600×900 in `walk-through/`, while the tour's own log reports only **493** captured in kept walks → **~770 files from abandoned attempts**; ~160 are actually judged (20 × 8) | glob of `local/demo/base154-riverwood/walk-through/**/*.png` vs `demo_tour` log lines |
| Frame rate during capture vs not | | **92 FPS while capturing** (41 frames in the 0.448 s walk of stage 1) vs **167 FPS** baseline (stage 0→1, 3,824 frames in 22.85 s) → **≈+5 ms per captured frame** | same log |

## 2. Where the tour time actually goes

The tour's phase model is exact, and the timestamps validate it to a tenth of a second:

```
stage = SETTLE_SECONDS (10.0, demo_tour.rs:125)
      + Survey 4 × 1.5 s = 6.0 (demo_tour.rs:511)
      + FindDoor (instant: the door is in the cell just entered)
      + PRESTREAM_SECONDS 6.0 (demo_tour.rs:127)
      + WalkThrough + WalkThroughAfter (0.45-2.7 s)
```
Measured clean stages: **22.7 s** door-found → door-found (predicted 22.0). Every door whose 60-unit standoff is inside rock adds **2.5 s** (`WALK_STUCK_SECONDS`, demo_tour.rs:155) — 5 such retries in Riverwood, 3 in Alftand. After the last crossing there is one **more 10 s settle** (the route-end `Settle`) plus 4 × 3 s `LookAround` before the walk test: 22.0 s measured to the walk test in both runs.

So the flat waits:

| | Riverwood | Alftand |
|---|---|---|
| Flat sleeps (9 settles + 8 surveys + 8 prestreams / 5+4+4) | **176 s of 233 s (76%)** | **88 s of 133 s (66%)** |
| Standoff retries | 12.5 s | 7.8 s |
| Real work (walking, opening, crossing) | ~12 s | ~6 s |
| Tail survey + walk test + exit | 29 s | 29 s |
| Capture overhead | ~6 s (est. from the 5 ms/frame delta × 1,264 frames) | ~3 s |

**The tours are ~3/4 pure waiting.** The reference-shot run (`shots.rs`) proves the same waits are unnecessary: it settles on *streaming state* — `SettleCounts::is_quiet()` + `SETTLE_QUIET_FRAMES = 10` (`shots.rs:74-78, 379-421`), with a 30 s ceiling — and every one of 25 poses settled in **0.05-2.10 s**, including the first pose of a session at 1.70 s with cold shaders. The demo tour instead sleeps a flat 10 s, blind to whether anything is still loading.

## 3. Proposals, ranked by minutes saved per day against effort

Assumption for the per-day column: **4 acceptance cycles a day** of "release build + both tours" (scale linearly if you run more). Effort: S = one file/small diff, M = a few files.

### P1. Make the tour's settle condition-based (reuse `shots_settled`) — *S*
- **Change:** in `crates/engine/src/demo_tour.rs`, `Phase::Settle` waits until the streaming counts are quiet (10 quiet frames) with `SETTLE_SECONDS` kept as the ceiling, instead of a flat 10 s. Reuse `crates/engine/src/shots.rs:1297` (`settle_counts`) and `:419` (`shots_settled`) by making them `pub(crate)`; `StreamingMetrics` is already a resource (`app.rs:162`).
- **Expected saving:** 14 flat 10 s waits per cycle (9 Riverwood + 5 Alftand) → 1-4 s each by the `shots.log` evidence → **≈85-125 s per cycle (~1.5-2 min), ≈6-8 min/day**.
- **Risk:** low, and it *strengthens* the check: a flat sleep can still photograph an unstreamed cell on a slow run; a quiet-check cannot. Only unknown is whether a post-crossing settle is slower than a shots pose — hence the 1-4 s band.
- **Verify:** both tours still end `tour PASSED after N crossings` + the walk-test line; compare the `NN-arrived.png` shots against the current ones (they must not get darker/emptier); log the actual settle time per stage so the band is measured, not assumed.

### P2. Cut the fixed per-view waits — *S*
- **Change:** `demo_tour.rs:511` survey 1.5 s → 0.4 s; `:816` `LookAround` 3.0 s → 1.0 s; `:881` final `Done` 2.0 s → 1.0 s.
- **Expected saving:** 48 survey views × 1.1 s + 8 lookaround views × 2 s + 2 s = **≈70 s per cycle, ≈4.5 min/day**. Exact arithmetic; no measurement needed.
- **Risk:** low. Each wait only has to outlast one rendered frame between the screenshot request and the camera turn (0.4 s = 67 frames at 167 FPS, 24 at 60 FPS).
- **Verify:** the survey/lookaround PNGs still exist and are the *unturned* views; `walk-through/NN/frames.txt` still reports its 20-frame window.

### P3. A smoke tour for iteration — *S*
- **Change:** `--tour-doors N` (or `--tour-quick`) in `crates/engine/src/config.rs` (`PortalOptions`, next to `--demo-tour` at `config.rs:311`), read by `DemoTour` to walk the first N doors, skip the pre-route survey and the route-end look-around, and print `tour SMOKE` instead of `PASSED`.
- **Expected saving:** an iteration check of **~20-25 s instead of 133-233 s** — the one change that makes "run the tour after every engine edit" affordable. Not a saving on the sign-off run itself.
- **Risk:** someone signs off on a smoke run. Mitigate with the distinct verdict word and by keeping `--demo-tour` (full) in the acceptance command.
- **Verify:** smoke on Riverwood passes 1/1 and the full tour is unchanged afterwards.

### P4. Prune the walk-through captures — *S*
- **Change:** in `capture_walk_frame` (`demo_tour.rs:376`), request a screenshot only while fewer than ~30 frames are held for the current walk, and delete the files that fall outside the kept window when the window is written (`:786-801`); or capture every other frame.
- **Expected saving:** measured 1,264 PNGs → ~160 needed. ~4-6 s per Riverwood tour of main-thread time, **~1.5-2 GB less disk per tour**, and a readable output directory. Low minutes, real quality-of-life.
- **Risk:** losing a frame the window needs. The window is defined by frame *number* around the swap, so a "keep the last N before the swap" ring buffer is the safe form.
- **Verify:** `frames.txt` still says 20 frames in the window for every stage of both tours.

### P5. A fast release profile for tour builds — *S*
- **Change:** add to `Cargo.toml` a `[profile.release-fast]` that inherits `release` with `lto = false` and `codegen-units = 16`; build tours with `cargo build --profile release-fast -p engine`. Leave `[profile.release]` (lto thin, `codegen-units = 1`, lines 54-56) untouched for anything measured or published.
- **Expected saving:** the engine release build is **3m 11s-4m 13s** warm, and the recorded gate shows the engine crate alone taking **15m 13s at `-j1`** with `codegen-units = 1` + LTO. Dropping both is the classic 2-3× — estimate **1.5-2 min saved per build, ≈6-8 min/day**.
- **Risk:** the artifact is not the accepted one. Never benchmark or sign off on it, and note that LTO off changes frame rate, which is the input to the capture density (`CAPTURE_DISTANCE`, `demo_tour.rs:165`).
- **Verify:** build both profiles and run both tours on each; identical verdicts and windows; FPS within ~10%.

### P6. Prestream on a condition, not a flat 6 s — *M*
- **Change:** same quiet-check for `Phase::Prestream` (`demo_tour.rs:127,579`), ceiling 6 s.
- **Expected saving:** 12 × 6 s = 72 s available; the return doors of each house pair lead back to a cell just left, almost certainly still resident, so realistically **30-50 s per cycle**.
- **Risk:** medium — this is the wait that makes the crossing seamless, and the walk-through frames around the swap are the thing the tour exists to judge. A check that returns too early shows up as dark frames right after the swap.
- **Verify:** the swap frames either side of every crossing (the contact sheet), plus the README's "0 dark frames" measurement re-run.

### P7. Stop paying `-j1` in the acceptance campaign — *S*
- **Change:** `scripts/phase2-acceptance.ps1:189,190,194` pass `-j1` to tests, clippy and the release build. Drop it for the quality gates.
- **Expected saving:** on this 8-core/32 GiB machine, the recorded 90 s test build and 913.8 s release build are single-threaded. This is nearly free to change and only costs the campaign, which runs about nightly — so it is a night-time saving, not a per-day one.
- **Risk:** memory. Keep `-j1` whenever a conversion is running (AGENTS.md's commit-limit rule).
- **Verify:** re-run a campaign and compare `quality-gates.json` `elapsed_seconds`.

### P8. Cold-build profile for tests — *S, helps workers more than you*
- **Change:** `Cargo.toml`: add `[profile.test.package."*"] opt-level = 1`. Today `[profile.dev.package."*"] opt-level = 3` is inherited by the test profile (which is why the log says `test profile [optimized]`), so a cold test build compiles all of Bevy with optimizations it never needs.
- **Expected saving:** estimate 30-50% of a **cold** workspace test build (90 s at `-j1`, so ~25-40 s at `-j`). **Warm incremental builds are unaffected** — and warm is the lead's normal case. The beneficiaries are fresh worker worktrees and CI.
- **Risk:** a perf-sensitive test could get slower. Nothing in `cargo test` measures performance (`dummy-content`'s budgets run under `--release`), so risk is low.
- **Verify:** two cold `--target-dir`s, before and after; full suite pass count unchanged and total wall time compared.

### P9. Run both tours in parallel — *S, conditional*
- **Expected saving:** the pair is 366 s serially; overlapped, ~230-280 s → **1.5-2 min per cycle** if neither GPU nor disk saturates.
- **Risk:** the walk-through evidence is frame-number based, not frame-rate based, so contention does not change the verdict — but two engines also write ~3 GB of PNGs, and any *number* from such a run is worthless. Never while the user plays; never when quoting FPS.
- **Verify:** run the pair overlapped, compare both `tour.txt` verdicts and the `frames.txt` windows, and spot-check the swap frames.

## 4. Candidates I checked and am rejecting, with the numbers

- **One engine run for both tours:** start-up is **0.88 s** and the two tours start in different worldspaces; merging saves ~2 s and adds a teleport. Reject.
- **Headless tours:** impossible — screenshots need a window, and `app.rs:98-101` already refuses `--headless` with `--shots`. A smaller window (`app.rs:135`, 1600×900 → 1280×720) would cut readback/encode ~36%, but the waits dominate (**~3-5 s per tour**) and it shrinks the visual evidence. Not worth it without a flag.
- **Cached shaders:** Bevy 0.19 ships only an in-process `PipelineCache` (`bevy_render-0.19.0/src/render_resource/pipeline_cache.rs`); there is no disk-cache feature to enable (no `pipeline_cache` key in bevy_render 0.19's `Cargo.toml`) and the engine sets none. Start-up to the first log line is 0.88 s, so there is nothing to win. Reject.
- **"Which tests dominate `cargo test -p engine --lib`":** none, in time. In the recorded workspace run, 15 binaries executed in ~0.25 s and 90 of the command's 91.5 s was compilation. Test filtering/selection would save nothing; the lever is the build. (Test *count* dominance — `streaming.rs` 42, `door_animation.rs` 39, `player.rs` 32 — is a maintenance fact, not a timing one.)
- **Clippy/test sharing work:** there is nothing to share — clippy is check-level and `cargo test` is codegen-level, in different profile dirs. The 12.79 s workspace clippy vs 0.81 s engine-only clippy is real but small; not a lever.

## 5. Bottom line

Cycle today ≈ **3.5-4.5 min release build + 6.1 min of tours ≈ 10 min**. P1+P2+P4 alone turn the tours into ~2 min (**−4 min**), P5 turns the build into ~2 min (**−2 min**), and P9 overlaps what remains — so **~10 min → ~4 min per cycle, ~24 min/day at 4 cycles**, with the tour still checking everything it checks today.

## 6. Things I am unsure about, and one coordination note

- The 1-4 s post-crossing settle band is inferred from `shots.log`, which measures poses, not post-crossing streaming. P1 should log its actual settle times on the first run and let you tighten the claim.
- The P8 and P5 percentages are typical, not measured here; both have a one-command measurement above. I could not measure anything myself (no shell tool).
- The engine log for the Riverwood run is 14,787 lines because `FrameTimeDiagnosticsPlugin` + `LogDiagnosticsPlugin` + `RenderDiagnosticsPlugin` are added **unconditionally** (`app.rs:174-180`), including GPU timestamp queries. Gating them behind a `--diagnostics` flag is cheap and makes the tour log readable; I did not rank it because the CPU cost looks like seconds, not minutes.
- Sibling `research-158-portal-test-utility` has already reported (its report is at `local/agents/runs/research-158-portal-test-utility.1/report.md`) that the tour asserts nothing about the door being *open* — only crossings and the walk test. That changes the risk calculus for P1/P2/P6: if you adopt that finding and add real assertions, the tour becomes more valuable per second, and these cuts pay off twice. No sibling is live now, so I did not send this.


# Goal
Read the Phase 2 A1 timing window (2026-10-09) and answer three questions with numbers:
1. Does impl-1060's model-load cap (0 / 8 / 32 loads in flight) move the door-crossing stall?
2. Does impl-1059's unload budget (0 vs 3 ms per frame) move the flying run's worst frames?
3. What do impl-1077's spans say owns the crossing stall?
Read only. Track: Phase 2.

# Context
- Evidence: D:/Mudcrab/evidence/p2-window-2026-10-09/. Variants:
  - cap-0, cap-8 and cap-32: portal-door-tour x3, `--max-asset-loads-in-flight` N;
  - unload-0 and unload-3: lod-flythrough x3, `--max-unload-ms-per-frame` N.
  Each has baseline.json and baseline.md (compare sections against cap-0 and unload-0), plus runs/<stamp>/<scenario>/<window>/run-N/ with frame-series.csv, frame-times.csv, render-cycles.csv, cpu-spans.json, streaming.json, summary.md and tour/.
- The plan and what each run should read: C:/Modding/Mudcrab/mudcrab/local/plans/p2-timing-window-2026-10-08.md. Read it first.
- Headline medians are already pulled; do not stop at these:

| variant | fps | p95 | p99 | p99.9 | worst (ms) |
|---|---|---|---|---|---|
| cap-0 | 85.1 | 23.4 | 47.7 | 74.0 | 366 |
| cap-8 | 83.4 | 23.7 | 47.6 | 80.2 | 311 |
| cap-32 | 90.0 | 22.3 | 42.6 | 68.0 | 323 |

  The flying window: unload-0 is 76.6 fps, p95 22.3, worst 76.0; unload-3 is 76.2 fps, p95 21.4, worst 70.1.
- Background:
  - local/research/engine-foundations/analysis-1074-despawn-cost.md (the open split this window should settle);
  - local/agents/runs/review-1080-p2-crossing-spans.1/report.md (how to read the spans: load_finalize vs handle_drop vs the PreUpdate span; render-cycles cycle n joins frame-series row n-1);
  - local/agents/runs/review-1079-p2-load-pacing.1/report.md (the cap paces only per-reference .glb loads).

# Questions
1. **The crossings.** Find each crossing in every portal-door-tour run (6 per run, from tour/ or the streaming markers). For the worst frames around each crossing, give frame ms, main_schedule/PreUpdate, Update and PostUpdate, the render-thread time (render-cycles joined), and the spans:
   - assets/load_finalize;
   - assets/handle_drop;
   - streaming/space_change_unload, _apply and its roots/entities gauges;
   - render/extract_commands_apply.
   Which one owns the stall? Give a per-variant table (median over runs and crossings) of the worst crossing frame and its owner.
2. **The cap.** Does cap 8 or 32 lower the crossing worst or the PreUpdate worst, against cap 0, beyond run-to-run spread (give min-max)? Does it lengthen the prestream wait or the landing fade? Look in tour/ or the streaming.json counters (frames_asset_load_cap_binding, asset_loads_queued). Is the headline fps difference real or noise?
3. **The unload budget.** For the flying runs: Update worst, retire_backlog_drains, retire_backlog_overflows, and the frames where retiring happens. Does 3 ms lower the worst frames compared with 0, beyond spread?
4. **The 1074 split.** Is the crossing cost the despawn (space_change_unload_apply, handle_drop), or load finalize, or the render-side removal? Settle it with numbers.
5. **What it means for 160 fps.** Frames over the 6.25 ms budget are owned by render_thread (2,424 of 2,604 at cap 0). Say in 3 lines how much the crossings matter against that.

# Scope
Read only. Python for parsing is fine (`python -I`); no engine runs and no cargo. Write the note to local/research/engine-foundations/analysis-1103-window-read-2026-10-09.md and add one INDEX row.

# Report
Status / Summary / Left / Next, then the answers in plain numbers, and a recommendation:
- the default for `--max-asset-loads-in-flight`;
- the default for `--max-unload-ms-per-frame`;
- whether 1060 / 1060.1 / 1077 should merge into portal.

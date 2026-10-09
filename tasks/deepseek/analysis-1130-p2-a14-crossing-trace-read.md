# Goal
Name what costs the two crossing frames on the portal door tour, from a Chrome trace: the ~140-190 ms Update frame (cell despawn) and the next frame's ~150-200 ms PreUpdate (handle_drop) beside the render world's removal. The answer feeds A4 (a cheaper despawn). Track: Phase 2. Analysis only; no engine changes.

# Context
- Trace: D:/Mudcrab/evidence/p2-a14-trace-2026-10-09/trace-tour.json (7.3 GB, Chrome trace JSON array, one event per line; "ph":"B"/"E" pairs and "X" events, "tid" per thread, "ts" in microseconds). Bevy 0.19 trace_chrome build of portal 50ca9ee7 (release + bevy/trace_chrome). NEVER load it whole: stream it line by line (e.g. Python reading lines and json.loads on each, skipping the first "[" and last "]").
- The same run's profiling bundle (frame numbers and per-frame real_ms, and the crossing frames): D:/Mudcrab/worktrees/p2-trace/target/profiling/20261009-161615-AMD-Ryzen-7-5800X-8-Core-Processor--NVIDIA-GeForce-RTX-3090-driver-32.0.16.1692/portal-door-tour/run-1 (frame-series.csv, summary.md, streaming.json, cpu-spans.json). Worst frames there: frame 809 at 833 ms (tracing makes everything slower; compare relative shares), and frames near 167-178, 303, 2099-2105 around 95 ms.
- Prior reads: local/research/engine-foundations/analysis-1074-despawn-cost.md (the 434-cell crossing: 257 terrain-only ring cells + 176 LOD chunks + one heavy interior), local/research/engine-foundations/analysis-1103-window-read-2026-10-09.md (crossing = c+1 Update despawn 140-184 ms, c+2 PreUpdate handle_drop 148-204 ms in parallel with render removal 160-220 ms).
- The trace's "name" field holds Bevy's span names (e.g. "system: name=..." , "schedule: name=Update", "sub app: name=RenderApp").

# Steps
1. Find the crossing frames in the trace: the frames whose main-thread "schedule: name=Update" or "PreUpdate" span is the longest (top 6), and match them to the frame-series frames by time order (frame spans: "frame" or the main schedule spans).
2. For each crossing frame: list the top 15 spans by inclusive duration inside Update, PreUpdate and the render sub app on every thread (system names, thread ids), and the self time of the top ones (inclusive minus children).
3. Say which systems own the time: is it Bevy's own despawn (World::despawn / hierarchy, commands apply / ApplyDeferred), asset handle drops (handle_drop / asset events), render-world removal (extract, RenderEntity sync), or our code (crate engine::streaming / portal / lod). Give the share of each in ms.
4. Note anything that runs on one thread while the others idle.
Do not run the engine or cargo. Before reading the 7 GB file, check python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status; if a hold is listed, wait (disk reads disturb timing windows).

# Foundations check
Measurement only. In the report, say which fix layer the owner points to (converter precompute, engine batching, Bevy internals; and if Bevy internals, whether Bevy 0.20 changes it, or "unchecked").

# Done when
The crossing frames' costs are named by system with ms and thread, with the evidence lines.

# Report
Status / Summary / Left / Next, a table per crossing frame (span, thread, inclusive ms, self ms), and the conclusion for A4. Write the note to local/research/engine-foundations/analysis-1130-a14-crossing-trace-2026-10-09.md and add one row to local/research/INDEX.md.

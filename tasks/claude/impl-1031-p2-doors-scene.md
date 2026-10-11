# impl-1031: #184 doors scene (queued; starts after impl-1030)

Branch phase2/perf-doors, stacked on phase2/perf-routes (impl-1030's head) + a merge of PR #128
(386267dc). Portal takes it directly; upstream after #128 merges (Upstream Dev, 2026-10-07).

- Route files gain a door step: "cross door <ref FormId>" using #128's crossing (programmatic
  request through door_crossing's own path, the same one the player uses; no shortcut).
- scripts/routes/doors.json: several crossings both directions, Riverwood (research-055):
  Sven's House 0001CBB0 -> cell 0001CB84, Riverwood Trader 0001341F -> 000133C9,
  Sleeping Giant 00013424 -> 000133C6.
- Measure both directions, and the first seconds after arrival on each side (the 40 ms frame
  sits there), not only the crossing frame: per-crossing windows in the report (crossing frame,
  arrival + N s), with the distribution and owners from impl-1030.
- Held-out check: check.json (or check-doors.json) gains door crossings nobody tunes on,
  including one worldspace crossing (Whiterun gate into the WhiterunWorld city worldspace),
  documented as never tuned. Exercises the world-ready fix (65520c98) path.
- Needs the world-ready fix in the branch too (cherry-pick 65520c98 + cfaf2486), otherwise
  world_ready never latches inside interiors.

# Queue after the measuring-tool jobs (Upstream Dev, 2026-10-07; do not start yet)

Source: local/research/engine-foundations/analysis-882-883-cell-load-cost.md (Phase 1).
Rank by the routes tool's over-budget owners first.
(a) DB worker shared by cells and LOD: 2.4 ms queries but 32/61 ms queue waits; separate queues or a second worker.
(b) Bevy's C zstd decoder (10 vs 25.5 ms per texture): check the Bevy 0.19 feature name and 0.20 changes first; a dependency/feature change needs Upstream Dev's sign-off.
(c) Bounds validation per instance; make it per model.
(d) Collider build on the main thread at first placement: waits for Phase 1's measurement and binary-collision proposal (crates/shared, agreed by message).

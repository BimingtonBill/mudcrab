# impl-1040: routes drive the real player (queued; starts after the doors scene lands)

Approved by Upstream Dev 2026-10-07 as part of the 160 fps measuring tool (#184).
Branch: on top of phase2/perf-doors (the doors scene head after the aim-height fix).

- Route runs (default ON for --route; a flag to turn it off for comparison) run world physics:
  WorldPlayerPlugin's real character controller is fed movement towards each waypoint (the same
  input path as walking), at the route speed, so shape casts, ground snapping, Rapier's step and
  broad phase are measured. The camera follows the player as in normal play.
- No teleport ever. If the controller cannot follow (stuck on clutter, a slope too steep, stairs),
  the route fails loudly with the position and the leg, after a bounded time.
- Door steps: the player activates the door as in play; the view-blocked test now runs.
- The ground-follow from CellCache height fields stays only for the physics-off comparison mode.
- Routes must become walkable: travel.json currently leaves the road over hills (worst slope about
  0.87) and check.json falls about 3,500 units; re-pick waypoints on roads/paths where needed,
  keep them data, and keep check routes held out (re-pick without tuning to results).
- Upstream's acceptance scenes stay unchanged in this job.
- Foundations check in the report; tests: controller fed per leg, stuck detection, door
  activation via the player, physics-off mode equals today's behaviour.

**Update 2026-10-07:** scope changed with the user's yes (via Upstream Dev): implement the scenario format v1 in docs/specs/engine/scenarios.md (branch phase2/scenarios, head 5ac9f594) instead of a route-only format, minus the recorder (Chef's side by default; refuse record at load). Port travel/check/doors to scripts/scenarios/. Measured runs at native resolution.

**Update 2026-10-07:** all heavy cargo runs through the machine build gate: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo ...`. Wait for it in one background command, not a foreground shell.

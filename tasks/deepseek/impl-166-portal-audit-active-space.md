# impl-166 - Only the active space collides and has usable doors; crossings read this frame (audit 2, 3, 4)

Owned files: crates/engine/src/player.rs,crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Three findings in the player controller; prove each separately.

## Finding 2: destination geometry can collide with the player

The movement ray cast in `player.rs` (`player_walk`, its `MeshProbe` skip closure) excludes water,
the portal quad and opened door leaves, but does not restrict hits to the **active space**.
`RayCastVisibility::Visible` tests inherited visibility, not render layers, and a portal
destination's meshes stay visible (on render layer 2, for the portal camera; see `isolate_cells` and
`CellRole` in `portal.rs`). So an interior or worldspace whose raw coordinates overlap the player's
could add invisible walls or floors while its portal is open. Suggested fix: skip hits whose owning
cell is not in the active space (the cell role the portal already tracks), keeping today's
exclusions. Test through the real mesh ray-cast path with two overlapping spaces.

## Finding 3: doors of a foreign space stay usable

`E` targeting, the door trigger / auto-load trigger queries and the walk-through iteration in
`player.rs` do not check that the door's cell is in the active space, and prestreamed hidden cells
keep their door components. An invisible door in another interior can win the nearest-door test,
and a foreign auto-load marker can trigger a crossing. Suggested fix: check the owning cell against
the active space in all three paths - not visibility, because legitimate auto-load markers can be
hidden. Test overlapping doors in two interiors, including a hidden auto-load marker in the active
one (which must still work).

## Finding 4: crossing detection reads last frame's camera

`player_walk` moves the camera's `Transform` earlier in the same chained `Update`, but the crossing
detection (`player_walks_through_doors`, and the auto-door camera query) reads `GlobalTransform`,
which only propagates in `PostUpdate`, while the portal now reads the current `Transform` (`63af9d3`
fixed the same bug for the doorway image). Rendering and crossing can disagree by one frame. The
existing tests hide it by writing both transforms by hand. Suggested fix: read the root camera's
current `Transform` consistently (the camera is a root entity), and test in production order
without hand-syncing `GlobalTransform`.

If you need a small accessor in `portal.rs` to ask a cell's role, add it; nothing else there.

## How to work: prove it first

These findings come from an **AI-written outside audit** (Codex, 2026-09-24); line numbers are from
around `d6e883095ef5` and may have moved - find things by name. Only finding 1 has been checked by a
second reader, and nothing has been reproduced in a running engine. So for **each** finding:

1. Write a unit test that exercises the claimed failure through the real code path (production
   system order, no hand-synced state the real game would not have) and **run it on the unfixed
   code**. It must fail for the reason the finding gives.
2. If it does not fail, **do not change production code for that finding**: stop, and report
   plainly that the claim did not reproduce, with the test and what it showed.
3. If it fails, fix it with the smallest change, keep the test, and make it pass.

Every command on an entity that may have been despawned uses the `try_` forms.

## Checks

`cargo test -p engine --lib`, clippy, fmt. Then build with the fast, non-measured profile
`cargo build --profile quick -p engine --bin engine` (the binary lands in `target/quick/engine.exe`)
and run both tours from it: `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour local/impl-166-portal-audit-active-space-riverwood`
and the same with `--demo alftand`. Both must end `tour PASSED` (8 and 4 crossings), with every
`swing ok` / `far door ok` line and no `FAIL`. Report, per finding: reproduced or not (with the
failing test's message), what you changed, and the tour lines.

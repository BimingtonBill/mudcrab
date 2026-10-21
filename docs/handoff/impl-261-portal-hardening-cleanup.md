# impl-261 portal hardening and cleanup

**Commit:** `[deepseek] impl-261: door-layer one-owner guard, failed-interior retry, stale comments`
(worktree `local/impl/impl-261-portal-hardening-cleanup`, base `1347f69`).

## What changed

- **`door_layer.rs` - the one-owner-per-doorway guard.** `manage_door_layer` decided a pair's
  follower against `drivers`, a snapshot of the *previous* frame's drivers taken before the loop, so
  the follower of a pair kept in this frame was still free to be adopted by the next pair: one
  doorway could be owned twice. The keep and spawn paths now share one set, `owned`, seeded with
  every door of the pairs standing before the loop (driver and follower), added to as a pair is kept
  or spawned, and released by `drop_pair` as a pair goes - so a pair handed back can be re-spawned in
  the same frame (the crossing's own case). Two tests: `two_linked_doors_of_the_players_space_own_
  one_doorway` (Whiterun's double gate is one doorway, and the ownership does not move between
  frames) and `a_door_another_pair_already_follows_is_not_claimed_again` (the case the old guard
  missed; fails on the old code).
- **`streaming.rs` - a failed interior is retried.** An exterior that fails in full falls back to
  terrain-only (`full_failed`); an interior has no other tier, and a failed destination counts as
  settled (`transition::destination_is_loaded`), so the camera could walk into an empty space and
  stay. The interior the camera *stands in* is now asked for again up to `INTERIOR_FAILED_RETRIES`
  (3) times; the budget is dropped with the cell, so a cell left and re-entered starts whole. Test:
  `a_failed_interior_is_asked_for_again_while_the_camera_stands_in_it`.
- **Cleanup, comments only.** `app.rs` no longer says nothing reads the portal graph
  (`transition::predicted_doors` does), and `snow.wgsl`'s vertex-alpha note now describes the
  converter's own rule (`apply_vertex_colour_use`) instead of `force_opaque_vertex_colours`.

## Evidence

- `cargo test -p engine --lib`: 674 passed, 0 failed, 3 ignored (both new tests listed and passing;
  the door-layer one fails against the old guard).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.
- Short walked tour, `--demo riverwood --demo-tour <dir> --tour-doors 2`: **PASSED after 2
  crossings**, twice on the final build (`local/tour-261/`, `local/tour-261b/`, `local/tour-261-final/`).

## Found on the way (pre-existing, not this change)

`stage 1: the door did not open within 10 s` on the short Riverwood tour: the door back out to
Tamriel is *already standing open* when the walk reaches it, so the tour's `E` closes it and the
portal view is not photographed. Present at base `1347f69` too: a probe build with the old guard
restored behaves the same (`local/tour-261-base/`). The pre-door-layer runs (`local/tour-241/`,
`local/tour-full-c5cd/`) show the door closed at that point, so the door layer (merged just before
`1347f69`, `edacdfc`) is the likely origin - the far-door drive or the door-layer follower keeps the
door open where auto-close used to shut it. The crossing still happens and the tour passes.

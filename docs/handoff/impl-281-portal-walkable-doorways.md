# impl-281 portal: walkable doorways

Track: portal. `crates/engine/src/player.rs` only.

The composite walk probe (`WorldProbe`) now ignores a non-downward rapier hit whose point lies in the
doorway volume of an open load door of the player's space. The volume (`doorway_volume`) is the
door's model bounds from `measured_portal_extents` (`ExpectedModelBounds`, else `InstanceBounds`) in
the door's own frame: width +32 each side, height +32 above (none below), depth 64 each side of the
model's middle. `open_doorways` builds the list once per frame in `player_walk`. Downward rays
(normalized y < -0.5) are untouched, so a doorway's floor still holds the player. Closed doors, doors
of other spaces and runs without world colliders behave as before. Doors with no usable bounds
(auto-load markers) have no volume.

Tests: `world_probe_tests` (five new). Acceptance: fmt, clippy -p engine, nextest -p engine 848 passed.
Not run: the Alftand tour (no engine runs allowed here).

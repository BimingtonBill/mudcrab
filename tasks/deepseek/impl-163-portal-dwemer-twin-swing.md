# impl-163 - Dwemer load doors borrow their twin's swing instead of scaling their own

Owned files: crates/engine/src/door_animation.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. **The user's decision (2026-09-24):** a load door whose own `Open` clip barely moves
(the Alftand route's Dwemer doors: `DwemerLargeDoorLoad01` 8.7 degrees, `DwemerSmallDoorLoad01` 9.4)
must **borrow the swing of its non-load twin model** (`DwemerLargeDoor01` etc., which swing 86-88
degrees), not scale its own clip up to 90 degrees. Today's log shows every such door taking the
scaling path ("load door's own swing scaled up to open the doorway ... factor=10.3").

`door_animation.rs` already has a twin path (`twin_swing`, `SwingSource::Twin`, and the test
`a_load_door_whose_own_clip_is_narrow_borrows_its_twins_swing`). Find why the real Dwemer doors do
not take it (the twin's model path, how it is derived from the load door's, whether the twin is
found in the converted assets under `C:\Modding\Mudcrab\SkyrimConverted`, the order the two paths are
tried), and make the twin the first choice, keeping scaling as the fallback for a door with no
usable twin. Log which source each model got.

Checks: unit tests for the choice; then `cargo build --release -p engine` and the Alftand tour
(`target/release/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo alftand --walk --demo-tour local/t163`)
must pass, with the log showing the Dwemer doors borrowing their twin. Look at the walk-through
frames of one Dwemer door and say whether it swings open visibly. Riverwood's farmhouse doors are
not affected (their own swing is scaled today; say what they get after your change - a farmhouse
door with no load-free twin must keep the scaled swing).

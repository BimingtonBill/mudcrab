# impl-022 - Keep load doors closed unless their portal is showing

Owned files: crates/engine/src/portal.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md` and
`portal.rs` first.

## Goal

`portal.rs` hides the meshes of **every** load door (`hide_load_door_meshes`), so a doorway is an
opening. Only the one door the portal is currently rendering shows the destination; every other
doorway is a hole you can see straight through. Example: the Alftand Ruined Tower door `0005BDF1`,
seen from the east at 1200 to 1700 units, shows snow through the empty door frame, where the game
shows a closed Dwemer door (see `local/reference/tower_east/*.png` in the lead's checkout, which
you can read).

Show a load door's own mesh (closed) unless it is the door the portal is showing right now. When
the portal picks a door, that door opens (its meshes hide, and the portal quad shows the
destination). When the portal moves to another door or turns off, the door closes again.

## Details

- Auto-load doors (`LoadDoor::auto_load`) are invisible markers with no real door mesh. Keep them
  exactly as today.
- Avoid a visible pop when the portal switches doors: switch the visibility in the same frame as
  the portal target changes.
- The portal quad must not draw over a closed door.

## Tests (write them to fail before your change)

- Given doors A and B with the portal targeting A: A's meshes are hidden and B's are visible.
  Retarget to B, then turn the portal off, and check the visibility each time.
- An auto-load door is never shown.

## Check it (you can build and run the engine)

Run `cargo build --release -p engine`. Then render the tower door with the shots mode
(`docs/design/reference-shots.md`): Tamriel (74750, 78431, -5100), yaw 270, pitch 2, hfov 75. Also
render one pose close enough (about 300 units) for the portal to pick the door. Run the demo tour:

```
target/release/engine.exe --assets C:/Modding/SkyrimConverted --demo alftand --demo-tour local/tour
```

It must still pass 4/4, and its `NN-door.png` must still show the destinations. You can see
images: look at them.

## Scope

Only `portal.rs`. No crosstalk partner. Report per the implementation template.

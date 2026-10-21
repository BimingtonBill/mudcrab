# impl-203 - No flicker on the frame the player crosses a load doorway

Owned files: crates/engine/src/atmosphere.rs,crates/engine/src/app.rs (system registration and ordering only),crates/engine/src/transition.rs (system sets and ordering only),crates/engine/src/lights.rs (ordering only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user (2026-09-25): "there's a frame or two where the world flickers when you transition over a load seam."

## Known

`docs/research/portal-frames.md` section 3 (read it) measured this: the frame a crossing lands in is drawn with the **source** space's atmosphere, and the next frame is not. Stage 07 (inn to street) went from mean 0.20 on the swap frame to 0.48 on the next. The cause: `update_atmosphere` (atmosphere.rs; registered in app.rs) applies when `ActiveCell` changes, but it is ordered nowhere relative to the door crossing (transition.rs `apply_door_crossings` / the crossing systems), so it runs a frame late. The same one-frame lag may exist in other systems that react to `ActiveCell` or `DoorCrossed`:
- `lights.rs` `budget_lights`;
- portal.rs `update_portal` / `isolate_cells` (portal.rs is not yours: report, don't edit);
- `update_destination_atmosphere`.

## What to build

1. Put the crossing and everything that must follow it in the same frame into an explicit order: the crossing, then `ActiveCell`, then atmosphere, light budget and portal re-layering, all before `PostUpdate` transform propagation and extraction. Use system sets in transition.rs that the others order `.after(...)`.
2. Check every `ActiveCell`/`DoorCrossed` reader (grep them) and list which one ran a frame late. Fix those in your files. For portal.rs, give the exact `.after(...)` line to add in your report.

## Checks

- A unit test of the ordering: the crossing frame's `update_atmosphere` output already matches the destination.
- Measure with the tour's walk-through capture. `local/demo/flicker/` is a fresh run on the current build, from the lead: the per-frame PNGs are in `walk-through/00`, `walk-through/01`, and `tour.txt` gives each crossing frame. The capture is known to be off by one around the swap frame (see the research note).
  - Before and after the fix: the mean brightness of the swap frame, and of the frames either side of it, for both crossings.
  - Build with `cargo build --profile quick --bin engine`, then run `--demo riverwood --walk --demo-tour local/t203 --tour-doors 4` (4 crossings).
- The short tour passes.
- Under `local/`, read only `local/t203*`, `local/demo/flicker/` and the research note.

## Report

Every late reader found and what you did about it; the portal.rs line for the lead; before/after brightness per crossing; test names; the tour's last line.

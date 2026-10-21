# impl-210 - Light spills through an open doorway both ways, so the frame does not change lighting at a line

Owned files: crates/engine/src/portal_spill.rs (new),crates/engine/src/portal.rs (only to expose what the spill needs: the open door, the doorway pose, the destination's atmosphere, as `pub(crate)` getters or a small resource; no behaviour change),crates/engine/src/lib.rs,crates/engine/src/app.rs (plugin registration only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's captures `local/captures/2026-09-25_10-27-59/01.png` and `03.png` (notes: "seam, not smooth lighting - looks obviously fake") show the doorway's frame and threshold changing lighting at a hard line.
- The main view (the exterior, lit by the sun and the sky ambient) meets the portal image (the interior, lit by its own ambient, fog and point lights).
- research-205 (`scratchpad` summary in the team log, 2026-09-25) found the cause: each side's lighting is set separately, with no blend at the doorway plane.
- The combined brainstorm (`local/research/brainstorm-portal-summary-2026-09-25.md`, item 1; `brainstorm-207-seamless-visuals.md`, idea 2) ranks this first.

## The design (decided; build this)

Real light crosses an open doorway. Model it with two "spill" lights per open doorway. Both exist only while the portal draws through that door (use `portal_shows_through`), and both fade in and out with the door's swing (the open fraction).

1. **Into the destination.** A `SpotLight` on the destination's render layer (layer 2, the portal image), placed at the destination doorway, a little outside the opening, aimed inward.
   - Colour: the source side's light (for an exterior source, the sun colour times its visible intensity plus the sky ambient). This is daylight falling across the interior floor.
   - The cone is sized to the doorway. The range is about 400-600 units.
   - No shadows at first. If cheap, the spill may cast shadows; measure it.
2. **Into the source.** A `SpotLight` on the source's layers (0/1), at the source doorway, aimed outward.
   - Colour: the destination's ambient and its brightest nearby light (firelight spilling onto the porch).
   - Much weaker outdoors in daylight: scale by the ratio of the two sides' ambient, so it only shows when the outside is darker than the inside, as at dusk and night.
3. **When the player crosses,** the two roles swap with the worlds (the source becomes the destination).
4. **Intensities** use the project's calibrated units (`lights.rs` `EXPOSURE_CALIBRATION`, `atmosphere.rs`). Put every tuning number in a named constant with a comment saying what it was fitted on.

Fit the numbers on Sven's House (captures 01, 03). Then **check them on doorways you did not fit on**: Gerdur's House (capture 02), the Riverwood Trader, Honningbrew Meadery (capture 04, WhiterunWorld), and one dungeon door. Tuning must hold on examples not used to fit it (AGENTS.md).

## Checks

- Unit tests: the spill appears and disappears with the door, fades with the swing, swaps at a crossing, and its colour comes from the correct side.
- `cargo build --profile quick --bin engine`. Before/after renders of the user's poses: `--shots local/captures/2026-09-25_10-27-59/shots.json` with the door open (see shots.rs for how a shot opens a door), plus the held-out doorways. Show crops of the threshold and frame.
- The short tour passes; `--tour-bench` numbers before and after (the spill costs two lights).
- Under `local/`, read only `local/t210*`, the capture folder and the research files named above.

## Report

What changed; the constants and what they were fitted on; the held-out results; test names; before/after crops; the tour's last line; the bench numbers.

# impl-073 - the sky gets a share of the day, and a light stops flooding the room

## What the user sees

In Riverwood at midday the shadowed half of every frame is nearly black
(`local/reference/_shadows-before-after.png`, right half): the sun carries the whole day and the
ambient carries almost nothing, so a surface in shadow has nothing left to light it. Indoors it is
the other way round - `local/reference/rw6/RW-10-trader-shop-floor.png` and
`RW-12-faendals-house.png` are a flat cream wash with no dark anywhere, because every surface in
those 900-unit rooms sits inside a radius-512 point light's near field.

Both defects are the same one: **the direct light is far too strong relative to the ambient fill.**
Outside it is 92:1 where the reference screenshots show about 3:1; inside the converted lights are 7
to 15 times the ambient on surfaces that should be ambient-lit, where the references show 3 to 5.
The full measurement record is `docs/research/light-and-exposure-audit.md` - read it first.

Owned files: crates/engine/src/lights.rs, crates/engine/src/app.rs, docs/handoff/impl-073-light-scale-and-sky-fill.md

Acceptance: cargo test -p engine

## Change 1 - the daylit ambient is derived from the space's own sun

Facts (all in `crates/engine/src/app.rs`, `crates/engine/src/world/lighting.rs`, and Bevy 0.19):

- Bevy's ambient contributes `albedo x AMBIENT_DIFFUSE_WEIGHT x ambient_colour x brightness`, with
  **no `NdotL`** (`bevy_pbr-0.19.0/src/render/pbr_ambient.wgsl`: `EnvBRDFApprox(diffuse_color,
  F_AB(1.0, NdotV))`). At `perceptual_roughness = 1.0` that reduces to a constant:
  `F_AB` gives `r = (0.0, 0.015, 0.468, -0.018)`, so `AB = (-1.04, 1.04) x 0.015 + (0.468, -0.018)`
  = `(0.4524, -0.0024)`, and `EnvBRDFApprox(c, AB) = c x 0.4524 - 0.0024`. Use
  **`AMBIENT_DIFFUSE_WEIGHT: f32 = 0.4524`** (the offset term is negligible and is dropped).
- The sun contributes `albedo x NdotL x illuminance / PI`.
- Today: sun `12,000 x 1.144 x 1.6 / PI` = 6,990; daylit ambient `0.4524 x 0.2623 x 640` = 75.8.
  Ratio **92:1**. In `SR-place-Riverwood_02.jpg` a shadow cast across the path measures 0.269
  against 0.344 for the sunlit ground beside it - about a third.

Build:

- Add `SKY_FILL: f32 = 0.5` - *the light the sky puts on a surface, as a fraction of the light the
  sun puts on a surface facing it* - beside the other sky constants.
- In `space_atmosphere`, for a space the table says is **daylit**, the ambient `brightness` comes
  from that space's own sun instead of the sky base level:

  ```
  brightness = SKY_FILL * sun.illuminance / (PI * AMBIENT_DIFFUSE_WEIGHT * luma(base_colour))
  ```

  Factor that into a small pure function so it can be tested. The sun of a daylit space always
  exists (`sky_sun`). **A daylit space whose sun is off keeps `base.1`** - the existing "a missing
  column is not a black sun" rule; a half-filled row must not become a black room.
- The sunlit contribution becomes `sun x (1 + SKY_FILL)`, so `DAY_SUN_LEVEL` goes from `1.6` to
  `1.6 / (1.0 + SKY_FILL)` = **1.0667**: a surface facing the sun keeps exactly the brightness it
  has today, and a shadowed one becomes `SKY_FILL / (1 + SKY_FILL)` = **1/3** of it.
- `SKY_AMBIENT_LEVEL` (4.0) stays, reworded as the level a daylit space with no sun keeps.
- Do **not** touch `AMBIENT_BASES` (the fallback for a database with no `space_lighting` table),
  `INTERIOR_AMBIENT_*`, `CAVERN_AMBIENT_*`, `SKY_AMBIENT_COLOR`, `SKY_AMBIENT_BRIGHTNESS`, the fog,
  or `sky_sun`.

Consequence to state in the doc comment: Bevy's ambient is unoccluded and normal-independent, so
the fill lands on a surface facing away from the sun as much as on one facing it. That is the flat,
hazy look the reference day has; an occlusion or hemisphere term is a different change.

## Change 2 - a converted light is scaled against the ambient the room actually has

Facts:

- `lights.rs`'s `intensity_for_radius` gives a light the intensity that puts `LIGHT_EXPOSURE` times
  the interior ambient on a surface at **half the light's own radius**, with the intensity scaling
  as the square of the radius. The delivered ratio to the ambient is therefore about **137x at 150
  units, 48x at 256, 20x at 328, 7.7x at 400** for a 512-unit light.
- `HALF_RADIUS_ILLUMINANCE` is built from `INTERIOR_AMBIENT_BRIGHTNESS` = **800**, but a table-lit
  interior's ambient is `INTERIOR_AMBIENT_BRIGHTNESS x INTERIOR_AMBIENT_LEVEL` = 800 x 0.6 = **480**.
  A converted light therefore delivers **83x** the ambient the room actually has, not the 50x the
  constant and its test assert - a 1.67x error by construction.
- The four Riverwood houses (`lead071_db.py lights 0x133c6 0x133c8 0x133c9 0x133ca`) carry five to
  eight lights each, all radius-512 records (`FarmInteriorLight01`, `FarmInteriorFireLight01`,
  `FarmInteriorShadowLight01`, colours `(198,162,100)` and `(173,103,52)`), in rooms about 900 units
  across - so every surface in them is at 0.2-0.6 of a light's radius. Alftand's caves are lit by
  radius-256 records in cells tens of thousands of units across, which is why the same rule never
  showed up as wrong there.
- The evidence for the size of the cut, from `research-074-interior-light-audit.1` (independent,
  read-only) and `docs/research/light-and-exposure-audit.md`:
  - `rw6/RW-11-alvors-house`'s dim band is R/G **1.505**, B/G **0.432**: the house lamp's
    fingerprint, not the ambient's (the four houses' `space_lighting` ambient is `(45,48,48)`,
    R/G 0.89). Solving the band as a two-illuminant mix puts the lights at **7-15x the ambient**.
  - the same frame's dim band is **11.3x** its UESP reference's;
  - in `render-cal/SR-interior-Alftand_12.png` the floor is 15-25x that frame's own dim band where
    the reference's is 2-5x - the Alftand fits passed because they were fitted on pooled *medians*,
    which an evenly lit floor does not move.
  - bound: **4 to 20x, best estimate 10x**.

Build:

- Build `HALF_RADIUS_ILLUMINANCE` from **`INTERIOR_AMBIENT_BRIGHTNESS * INTERIOR_AMBIENT_LEVEL`**
  (make `INTERIOR_AMBIENT_LEVEL` visible to `lights.rs`), so the constant is stated against the
  ambient an interior actually has.
- `LIGHT_EXPOSURE: f32 = 10.0` (from 50.0). With the line above that is an **8.3x** cut in the
  delivered ratio - inside the measured band, at the conservative end of the 10-11x the house frames
  ask for, and chosen so that the Alftand poses the change was *not* fitted on move as little as
  possible.
- Rewrite `LIGHT_EXPOSURE`'s doc comment: it currently justifies 50 with "the value sets how bright
  a torch pool is against the dark room around it" and refers to the pre-`4*PI` `EXPOSURE_CALIBRATION`.
  It must now carry the ratio evidence above and say plainly that 50 was fitted on spaces whose
  surfaces sit *outside* a light's near field, and was never validated in one that does not.
- Do **not** change `EMISSIVE_EXPOSURE`, `ENABLED_LIGHT_BUDGET`, `BUDGET_RECHOOSE_DISTANCE`,
  `INTENSITY_REFERENCE_RADIUS`, `point_light`, `radius_of` or the budget system.

## Tests

CI has no GPU. Test the arithmetic as pure functions.

1. Bevy's own environment written out from the engine it runs in, as `lights.rs` already does for
   its illuminance helper: ambient of brightness `B` and colour luma `L` puts
   `AMBIENT_DIFFUSE_WEIGHT * B * L` on a surface; a directional light of illuminance `I` puts
   `NdotL * I / PI` on one. Assert:
   - a shadowed surface gets `SKY_FILL / (1 + SKY_FILL)` of a sunlit one - the ratio, not just a value;
   - a sunlit surface gets the same total as before this change: compute the old pair
     (`DAY_SUN_LEVEL = 1.6`, ambient 640) and the new pair, agree to 1 %.
2. `space_atmosphere` on the Tamriel fixture row (`crate::world::lighting::fixtures`): the daylit
   ambient brightness is the fill for that row's sun, and its colour is still the record's hue at
   `luma(SKY_AMBIENT_COLOR)`.
3. A daylit row whose sun is zero or absent keeps `SPACE_AMBIENT_BASES.sky`'s brightness.
4. An interior and a cavern are unchanged: `INTERIOR_AMBIENT_BRIGHTNESS * INTERIOR_AMBIENT_LEVEL`
   and `CAVERN_AMBIENT_BRIGHTNESS * CAVERN_AMBIENT_LEVEL`, to the bit. This is the holdout spaces'
   ambient and it must not move.
5. `lights.rs`: a light still delivers `LIGHT_EXPOSURE` times the interior ambient the engine
   applies at half its radius (the existing test, updated to the level-corrected target);
   `INTENSITY_REFERENCE_RADIUS` behaviour and the `the_reference_cap_leaves_every_radius_up_to_512_exactly_as_it_was`
   test are unchanged; the emissive test is unchanged.

Update every existing test the change breaks and no others. Read the module docs you are editing and
`docs/research/light-and-exposure-audit.md` before starting: in this codebase the comments carry the
calibration history and are as much a deliverable as the code.

## Out of scope

- **Nobody runs the engine.** The lead has the GPU and a user is on it. Do not render, do not claim
  a render result, do not touch `local/`.
- **Do not touch:** `crates/converter/**`; `portal.rs`, `transition.rs`, `door_animation.rs`,
  `doors.rs`, `demo_tour.rs`, `portal.wgsl`; `render.rs`; `streaming.rs`; `snow.rs`; `tools/**`.
  `render.rs`'s `exposed_emissive` doc quotes "50 times"; leave it and say so in your report, the
  reviewer owns that sentence.
- **`app.rs` hunks must stay tight.** Another lead is editing other parts of that 3,000-line file:
  change only the lighting constants and `space_atmosphere`, and do not reformat around them.
- Do not re-fit anything else, and do not add a command-line switch or an environment variable.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

## Report

What you changed with file:line, the exact new constant values and the derivation of each, the new
test names and what each pins, every existing test you had to update and why, and anything you
believe is wrong but were not allowed to touch. A real objection with evidence is more useful than
compliance.

# impl-100 - the sky gets a share of the day, and a light stops flooding the room

**Task:** the lead's brief `impl-100-portal-light-scale-sky-fill` (portal track). Base commit
`a2f3691`.

**Acceptance run here** (final state of the files below): `cargo fmt --all -- --check` (no output),
`cargo clippy --workspace --all-targets -- -D warnings` (clean; only the pre-existing
`proc-macro-error2` future-incompat note), `cargo test -p engine` (310 tests: **307 passed, 0
failed**, 3 ignored, plus 3 in the integration group and 4 ignored doctests). Four new tests in
`app::tests`; one existing test updated (`lights::tests`), no other existing test needed changing.

**No engine was run** (the brief's rule: the lead has the GPU). Every number below is arithmetic
from the constants, checked by the tests.

## 1. Change 1 - the daylit ambient comes from the space's own sun

`crates/engine/src/app.rs`:

- **`AMBIENT_DIFFUSE_WEIGHT: f32 = 0.4524`** (`app.rs:1463`). Written out from the shader the engine
  runs: `pbr_ambient.wgsl:20` calls `EnvBRDFApprox(diffuse_color, F_AB(1.0, NdotV))` - the `1.0` is
  the roughness the *call site* passes, not the material's - and `EnvBRDFApprox(F0, ab) = F0*ab.x +
  ab.y` (`pbr_lighting.wgsl:544`). `F_AB(1.0, NdotV)` is `r = (0.0, 0.015, 0.468, -0.018)` for every
  `NdotV`, so `a004 = 0.015` and `ab = (-1.04, 1.04)*0.015 + (0.468, -0.018) = (0.4524, -0.0024)`.
  A surface gets `albedo * (0.4524*ambient - 0.0024)`; the offset is dropped (under 2 % of the term
  at the dimmest ambient this engine lights a space with). The specular half of the same expression
  is not albedo-proportional and is not modelled.
- **`SKY_FILL: f32 = 0.5`** (`app.rs:1486`) - the light the sky puts on a surface as a fraction of
  the light the sun puts on one facing it.
- **`sky_fill_brightness(sun_illuminance, base) -> f32`** (`app.rs:1705`), a pure function:
  `SKY_FILL * illuminance / (PI * AMBIENT_DIFFUSE_WEIGHT * luma(base))`. It is *stated against the
  colour's luminance* because the record's ambient colour is already scaled to `base`'s luminance
  (`scale_to_luma`), so the fill is one number whatever hue the record has.
- **`space_atmosphere`** (`app.rs:1648`): the ambient brightness is the fill for a daylit space with
  a sun, and `base.1` otherwise. The condition is `daylit && sun.illuminance > 0.0`, which is the
  brief's "a daylit space whose sun is off keeps `base.1`": the only reachable off-sun case is a row
  that publishes no daylight column at all (`is_daylit` is true for a *missing* column,
  `sky_sun` returns `SunLight::OFF`), and that row keeps 640. Colour handling is untouched.
- **`DAY_SUN_LEVEL`** (`app.rs:1590`) is now `1.6 / (1.0 + SKY_FILL)` = 1.0666667, written as the
  division so it cannot drift from `SKY_FILL`. On the Tamriel fixture this gives a sun of
  **12,800.002** where it used to be 19,200.
- **`SKY_AMBIENT_LEVEL`'s 4.0 is reworded** as the level a daylit space with no sun keeps (and what
  the no-table fallback still draws); the value is untouched, as are `AMBIENT_BASES`,
  `INTERIOR_AMBIENT_*`'s values, `CAVERN_AMBIENT_*`, `SKY_AMBIENT_COLOR`, `SKY_AMBIENT_BRIGHTNESS`,
  the fog and `sky_sun`.
- The stale `Hdr`/`Bloom` sentence at `app.rs:2190` now reads "`LIGHT_EXPOSURE` = 10 times the
  ambient an interior applies - this file's `INTERIOR_AMBIENT_BRIGHTNESS` at
  `INTERIOR_AMBIENT_LEVEL` - at half its radius".

Measured on the Tamriel fixture row (`space_lighting` of `SkyrimCloudy`), by the tests:

| | before | after |
|---|---|---|
| sun illuminance | 19,200 | 12,800 |
| ambient brightness | 640 (fixed) | **17,167** (the fill) |
| on a sunlit surface (unit albedo, `NdotL` 1) | 6,111.6 sun + 76.0 ambient | 4,074.4 sun + 2,037.2 fill = **6,111.6** |
| on a shadowed surface | 76.0 | **2,037.2** |
| shadow / sunlit | 1.2 % | **33.33 %** |

## 2. Change 2 - a converted light is scaled against the ambient the room has

`crates/engine/src/lights.rs`:

- **`LIGHT_EXPOSURE: f32 = 10.0`** (was 50.0, `lights.rs:142`), with the doc rewritten to carry the
  measured band (4 to 20x, best estimate 10x; the `RW-11-alvors-house` two-illuminant solve at 7-15x
  and its dim band at 11.3x its reference; the Alftand floor at 15-25x its own dim band against the
  reference's 2-5x) and to say plainly that 50 was fitted on spaces whose surfaces sit outside a
  light's near field and was never validated in one that does not.
- **`HALF_RADIUS_ILLUMINANCE`** (`lights.rs:187`) is now built from
  `INTERIOR_AMBIENT_BRIGHTNESS * INTERIOR_AMBIENT_LEVEL` - the ambient the room actually has (480,
  not 800). The old constant read as "50 times the ambient" while delivering 83x it; the two changes
  together are a **8.33x** cut in what a light puts on a surface (50*800 -> 10*480).
- **`INTERIOR_AMBIENT_LEVEL`** (`app.rs:1543`) is now `pub`, as
  `docs/design/portal-plugin.md:272` planned, so `lights.rs` states its scale against the same
  number the room is lit with instead of a copy. Value unchanged.
- Untouched: `EMISSIVE_EXPOSURE`, `ENABLED_LIGHT_BUDGET`, `BUDGET_RECHOOSE_DISTANCE`,
  `INTENSITY_REFERENCE_RADIUS`, `point_light`, `radius_of`, the budget system, and the two other
  levels. The `EMISSIVE_EXPOSURE` doc's "the ambient of `app.rs` is 800 and a converted light is
  `LIGHT_EXPOSURE` times that" now reads 480 and 4,800 - the magnitude the glow has to be seen
  against, not the constant.

A 512-unit torch's intensity goes from 1.18e11 to **1.41e10**. (The module doc's "about 2.1e10" for
the *old* value was wrong by 5.6x - the arithmetic from the code gives 1.18e11 - so the sentence now
carries the new, derived 1.4e10.)

## 3. Tests

New, in `app::tests` (helpers `ambient_on_surface`, `sunlit_surface` write Bevy's own arithmetic out
from the shaders rather than from the engine's same assumption, as `lights.rs`'s `illuminance` does):

- `the_sky_fills_a_shadowed_surface_to_a_third_of_a_sunlit_one` (`app.rs:3009`) - the ratio
  `SKY_FILL/(1+SKY_FILL)` *and* 1/3; the daylit brightness is `sky_fill_brightness` for that row's
  sun, **17,167** at the reference day (pins all three constants jointly); the record's hue survives
  at `luma(SKY_AMBIENT_COLOR)`; the fill is 27x the fixed level it replaced.
- `a_sunlit_surface_keeps_the_day_the_old_pair_gave_it` (`app.rs:3061`) - the old pair (1.6, 640)
  against the new, three ways: the new total equals the **old sun's own contribution** to 1e-3, the
  difference against the old *total* is exactly the old fill, and that difference is under 2 %.
- `a_daylit_space_whose_sun_is_missing_keeps_the_sky_base_level` (`app.rs:3103`) - a row with no
  daylight columns keeps `SPACE_AMBIENT_BASES.sky.1` and no sun; the same row with a day takes the
  fill.
- `an_interior_and_a_cavern_keep_the_brightness_they_had` (`app.rs:3153`) - the three Alftand cells
  and the two cavern worldspaces at `INTERIOR_AMBIENT_BRIGHTNESS * INTERIOR_AMBIENT_LEVEL` and
  `CAVERN_AMBIENT_BRIGHTNESS * CAVERN_AMBIENT_LEVEL` to the bit, plus a check that the test is not
  vacuous (the fill would be 13.9x the interior level if it ever reached one).

Updated, and *only* this one: `lights::tests::a_light_lights_a_surface_at_half_its_radius_like_the_
interior_ambient_does` (`lights.rs:437`) - its target is now `LIGHT_EXPOSURE *
INTERIOR_AMBIENT_BRIGHTNESS * INTERIOR_AMBIENT_LEVEL`, which is what makes it catch the 1.67x error
the old target agreed with. `the_reference_cap_leaves_every_radius_up_to_512_exactly_as_it_was`,
`intensity_follows_the_square_of_the_radius`, the emissive test and the whole budget group pass
unchanged (`intensity_for_radius(512)` is still > 1e7). On the `app.rs` side no existing test broke:
the fallback, interior, cavern, Blackreach, fog and daylight-scaling tests all reference the
constants symbolically.

## 4. What I could not do, and where the numbers came from

- **`docs/research/light-and-exposure-audit.md` does not exist in this worktree** (nor in this
  worktree's git history, `git log --all`). It is not in `docs/research/` here, and reads outside the
  worktree are blocked by this session's permissions, so the "read it first" instruction could not be
  followed. Everything in Change 1 and Change 2 came from the brief instead, which carries the same
  facts; the figures attributed to the audit in the doc comments (92:1, the `RW-11` fingerprints,
  11.3x, 15-25x against 2-5x) are quoted from the brief and were **not** checked against the audit
  itself.
- **The brief's "the two days agree to 1 %" is 1.23 %.** The new pair's total on a sunlit surface is
  the old *sun* contribution exactly (that is what `1.6/(1+SKY_FILL)` buys), so the gap against the
  old *total* is the old 640-level fill: 76.0 of 6,187.5. The test asserts the exact structure (the
  residual is the old fill) plus a 2 % bound rather than a 1 % one; nothing was fitted to make 1 %
  true, since the brief pins the constant (1.0667) three times and only the tolerance once.
- **`crates/engine/src/render.rs:754` still says a light pool is "50 times" the ambient.** It is
  outside my owned files, as the brief says; the sentence is the reviewer's to change.
- **`docs/pr/upstream-pr-plan.md:693`** lists "`DAY_SUN_LEVEL = 1.6`, interior ambient 800 → 480,
  outdoor fill 160 → 640" as the schema-17 calibration. That is a record of what those commits did,
  but the tree no longer has that shape; if the plan is meant to describe the current branch it needs
  a line. Not my file.
- **Pre-existing, left alone**: `DAY_SUN_LEVEL`'s doc says "The sun at Tamriel is then worth 1.6 x
  12,000 x 1.144". The engine's own arithmetic for the Tamriel fixture row is `12000 * (0.6247694 /
  0.6247694) * 1.6` - a factor of 1.0, not 1.144 - so either the sentence refers to the weather of
  the audit's Riverwood frame (a brighter day) or it is stale. I could not tell which without the
  audit, so I left the sentence and did not build my new paragraph's numbers on it.
- **No render, no engine run, no `local/` writes**, per the brief. The look of the day and of the
  four houses is the lead's to check; the two numbers to look at are the shadow/sun ratio (1/3) and
  the fill (17,167 at the reference day, 27x the level it replaced).
- **A row with a black `ambient` record colour** keeps the behaviour it had: the fill is a
  brightness, and a black colour multiplies it to nothing. No regression, but a daylit space whose
  record ambient is `(0,0,0)` is still unlit - if that turns up in the data, the fix is on the
  colour side and is not this change.

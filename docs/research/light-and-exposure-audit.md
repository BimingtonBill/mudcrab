# The light and the exposure: what is wrong, what was measured, and what to fit

**Lead-071, 2026-09-23.** An audit of the three problems the brief names - shadows that render
black, interiors that render blown white, and whether the per-space lighting is worth re-fitting
before the next reconversion. Everything here is measured from frames that already exist on disk
and from the converted database; **no render was made** (the lead has the GPU and a user is on it),
and no engine was run. Measurements are linear-light Rec.709 luminance over sRGB-decoded 8-bit
pixels, the same currency `tools/research/exposure_stats.py` uses, and were taken with
`tools/research/lead071_measure.py` (written for this audit; read-only) and
`tools/research/lead071_db.py`.

An independent audit of the interior conclusion was run as `research-074-interior-light-audit.1`
(DeepSeek, read-only); its findings are folded in below and attributed.

## 0. The one-line answer

**In this engine the direct light - the sun outside, the converted `LIGH` point lights inside -
carries far too much of the frame relative to the ambient fill.** Outside it is about 90:1 where the
reference screenshots show roughly 3:1, which is why shadows have no sky in them; inside it is 7 to
15 times the ambient on surfaces that should be ambient-lit, where the references show 3 to 5, which
is why rooms wash out evenly. Both are the *ratio*, not the level: the ambient levels themselves are
the ones the Alftand reference poses were signed off at, and they should not move.

## 1. Method and what bounds the evidence

- **Two frames of a place are not the same picture.** The Riverwood shots file was re-fitted
  (`research-069`, commit `12dbfa2`, 13:40) **one minute after** `local/reference/rw6/` was rendered
  (13:39), so `rw6` holds the *old* poses: several frames (RW-04, RW-10, RW-12) are a different
  composition from the reference they are paired with. Nothing here rests on a pose match; the two
  measurements that carry the argument are a *within-frame ratio* and a *colour mix*, both of which
  are pose-independent.
- **The reference set is itself noisy.** The four Riverwood interior references have p50 luminances
  of 0.013, 0.022, 0.078 and 0.034 - a 6x spread for four rooms of the same village shot in the same
  game. So "matches the reference" is only meaningful to within a factor of two or three, and a fit
  that chases one frame's p50 chases noise.
- **A black void is not an unlit surface.** `docs/research/visual-gaps-spec.md` records that a third
  of `SR-interior-Alftand_02`'s render is un-streamed cell showing the clear colour; frames with
  voids have depressed medians that say nothing about lighting. This is why the ratio below is taken
  between two regions of the *same* frame.

## 2. Problem 1 - the sun carries the day alone

The arithmetic of the current day for Tamriel, from `crates/engine/src/app.rs` and Bevy 0.19:

| term | formula | today |
|---|---|---|
| sun on a surface facing it | `NdotL x illuminance / PI` | `12,000 x 1.144 x DAY_SUN_LEVEL(1.6) / PI` = **6,990** |
| ambient on any surface | `0.4524 x colour_luma x brightness` | `0.4524 x 0.2623 x 640` = **75.8** |
| ratio | | **92 : 1** |

`DAY_SUN_LEVEL` was fitted on the **pooled median** of the nine Tamriel poses, and the ambient level
`SKY_AMBIENT_LEVEL = 4.0` was fitted on their **5th percentile** (`app.rs`, the comment on
`SKY_AMBIENT_LEVEL`) - both measured on renders made *before* the sun cast a shadow (`8bd7a01`,
13:35). At that time no surface was ever in shadow, so the 5th percentile was dark *albedo*, not a
shadow, and the fill it justified (640) is spent on a job it was never fitted for.

**What the reference asks for.** In `SR-place-Riverwood_02.jpg` a shadow cast by the walkway across
the path measures 0.269 mean against 0.344 for the sunlit cobbles beside it - a shadow at about
three quarters of the lit surface - and in the other references the shadowed ground sits at a third
to a half. The engine's is 1/92. `local/reference/_shadows-before-after.png` is the same defect at a
glance: the right half's decking and posts go nearly black, and the "before" half is the flat,
evenly lit look the reference actually has.

**What to change.** Give the sky a share of the day and take it out of the sun, so that a surface in
shadow is a *third* of one facing the sun and a surface facing the sun keeps the brightness it has
today: `SKY_FILL = 0.5`, the daylit ambient's brightness derived from the space's own sun, and
`DAY_SUN_LEVEL` 1.6 -> `1.6 / (1 + SKY_FILL)` = 1.0667. The derivation, and why the fill must be
computed per space rather than left as a constant, is in
`tasks/deepseek/impl-073-light-scale-and-sky-fill.md`.

Note what this does *not* fix: Bevy's ambient is unoccluded and normal-independent, so the fill
lands equally on a surface facing the sky and one facing away from it. The reference look - a soft,
hazy day - is what that gives, and it is what the engine's flat ambient has always given. A
hemisphere or an occlusion term is a different change.

## 3. Problem 2 - the lights carry the room alone

### 3.1 The measurement that settles it

`local/reference/render-cal/SR-interior-Alftand_12.png` is a Blackreach frame from the run the
interior ambient was calibrated on. Its own 15th-45th percentile band - the statistic the ambient
level was fitted on - reads `(0.017, 0.024, 0.023)`, and the big pale floor slab in the same frame
reads several times that. In the frame's UESP reference the same relationship is 2 to 5x.

Better still, **the Riverwood houses** (`local/reference/rw6/RW-09..RW-12`, measured against
`local/reference/riverwood/`):

| frame | render p50 | reference p50 | render dim band (linear RGB) | reference dim band | light's share of the dim band |
|---|---|---|---|---|---|
| `RW-11-alvors-house` | 0.568 | 0.078 | `(0.644 0.428 0.185)` | `(0.055 0.038 0.018)` | 87-94 % |
| `RW-10-trader-shop-floor` | 0.615 | 0.022 | `(0.610 0.437 0.198)` | `(0.013 0.011 0.007)` | - |
| `RW-12-faendals-house` | 0.699 | 0.034 | `(0.782 0.557 0.276)` | `(0.021 0.020 0.012)` | - |
| `RW-09-inn-common-room` | 0.058 | 0.013 | `(0.040 0.043 0.037)` | `(0.006 0.006 0.005)` | ~10 % |

The "light's share" column is `research-074`'s colour-mix solution: the frame's dim band is solved
as a two-illuminant mix of the house lamp `FarmInteriorLight01` `(198,162,100)` (linear R/G 1.56,
B/G 0.35) and the cell's own ambient, which is

```
cell 0x133C6/0x133C8/0x133C9/0x133CA  ->  space_lighting ambient (45,48,48), linear R/G 0.89, B/G 1.00
```

(the four Riverwood houses share lighting template 659862). The measured dim band of `RW-11` is R/G
**1.505**, B/G **0.432** - the lamp's, not the ambient's. Solving the mix gives the ambient 6 % of
the band on R/G and 13 % on B/G, i.e. **the lights are 7 to 15 times the ambient on exactly the
surfaces that should be ambient-lit**, and that estimate is a *lower* bound because tonemapping
desaturates both terms.

Two things make this a strong result rather than a plausible one:

- **It is pose-independent and exposure-independent.** It is a ratio between two terms inside one
  frame, so a mis-fitted pose and the engine's tone curve both cancel.
- **The control works.** `research-074` ran the same fingerprint on
  `render-cal/SR-interior-Alftand_12.png`, a frame that *is* ambient-lit, and got the ambient
  base's hue to within 5 %. Where the ambient is the illuminant the method returns the ambient;
  where the lights are, it returns the lamp.

`research-074` also bounded the factor three independent ways: the code's own accounting (**1.67x**,
see 3.3), the Alftand floor-vs-dim-band contrast (**4x** the reference's), and the house dim bands
against their references (**11.3x** for RW-11). Its verdict: best estimate **10x, defensible range
4-20x**.

### 3.2 Why it hits Riverwood and not Alftand

`crates/engine/src/lights.rs` gives every converted light the intensity that puts `LIGHT_EXPOSURE`
times the interior ambient on a surface at **half the light's own radius**, and the intensity
scales with the square of the radius. The delivered ratio to the ambient is therefore

```
light/ambient  =  50 x (radius/2 / d)^2 x window(d)   [in units of the ambient]
```

which at radius 512 is **137x** at 150 units, **48x** at 256, **20x** at 328, **7.7x** at 400 and
**2.6x** at 449.

The Riverwood houses are about 900 units across and carry five to eight **radius-512** lights
(`FarmInteriorLight01`, `FarmInteriorFireLight01`, `FarmInteriorShadowLight01`; the light references
in those four cells are in `lead071_db.py lights 0x133c6 0x133c8 0x133c9 0x133ca`). *Every* surface
in such a room sits at 0.2-0.6 of a light's radius, so the whole room is in the 20-137x regime and
reads flat and white - which is exactly what `rw6/RW-10` and `rw6/RW-12` look like. Alftand's ice
caves are lit by radius-256 records (`IceCave_Blue_Omni`, `DwarvenOmniLight`, `IceCandleLight01NS`)
in cells tens of thousands of units across, so most of its surfaces are outside the near field and
the same rule looks right there. **The rule was never wrong in Alftand; Alftand simply never enters
the regime the houses live in.**

`RW-09-inn-common-room` is the control inside Riverwood: the inn is the biggest room in the village,
its lights are 417-665 units from the eye, and the frame is *not* washed. Its dim band is
ambient-like (R/G 0.93) at only ~10 % light, which is what "far from the lights" looks like.

### 3.3 A second, smaller error in the same formula

`HALF_RADIUS_ILLUMINANCE` is built from `INTERIOR_AMBIENT_BRIGHTNESS` = **800**, but a table-lit
interior's ambient is `INTERIOR_AMBIENT_BRIGHTNESS x INTERIOR_AMBIENT_LEVEL` = 800 x 0.6 = **480**.
A converted light therefore delivers **83x** the ambient the room actually has at half its radius,
not the 50x `LIGHT_EXPOSURE` and its test assert. That is a 1.67x error by construction, and it is
worth fixing at the same time: the constant should be built from the ambient the interior level
actually applies.

### 3.4 The geometry cross-check

The measured 7-15x on the *dim* third of `RW-11` is the sum of several lights at once, and the table
below is what the engine's own formula predicts for one radius-512 light at a given distance
(`light/ambient = 50 x (256/d)^2 x (1 - (d/512)^4)^2`). The pose's eye (`-513,-206,360`) has five
lights inside 530 units - at 295, 308, 318, 339 and 389 (`lead071_db.py lights 0x133c8`) - and the
surfaces the frame shows sit 300-900 units from them:

| distance to the light | one light, as a multiple of the ambient |
|---|---|
| 300 | 28x |
| 350 | 16x |
| 400 | 8.1x |
| 450 | 2.6x |
| 500 | 0.11x |

Two readings fall out of it, and both match the frames. The **dim third** of the room - the surfaces
400-500 units out, where one light gives 2.6-8x - measures 7-15x, so two or three of the six are
reaching it. The **near surfaces** (the counter, the wall by the hearth, 300 units out) get 28x from
one light and more from the others, which saturates them - and a frame whose every surface is
between 8x and 100x of its ambient is exactly the flat, contrast-free cream `RW-10` and `RW-12` are.
The window closes at the radius, which is why nothing in the frame is *dark*: a surface 500+ units
from every light is below 1x, and there is almost none.

### 3.5 What is *not* the cause

- **`Bloom`** cannot do this. `Bloom::NATURAL` is `intensity 0.15`, energy-conserving, and the
  composite is `0.85 x original + 0.15 x pyramid`, with the pyramid's weights decaying by a factor
  of 0.63 per mip (`bevy_post_process-0.19.0/src/bloom/`). The frame-wide term is under 0.2 % of the
  frame's local average, and energy-conserving mode means a uniformly lit frame comes back
  unchanged. `research-074` verified the shader arithmetic and the running pass.
- **Emissive cards plus bloom**, for the same reason, and because a dark object beside a blown one
  survives in these frames: in `rw6/RW-11` the iron-banded chest stays dark with a near-white floor
  beside it, and in `rw6/RW-12` the hanging meat does. An additive haze adds to both; a light scales
  with albedo.
- **The specular work** (`59f1f20`). `rw1` (11:36, before the glossiness change), `rw4` (12:20) and
  `rw6` (13:39, after) are visually indistinguishable for `RW-11`. The fix *did* land - `RW-09`'s
  frame mean fell 2.5x, which is the pillar highlights the specular work removed - but it did not
  touch the wash, and `RW-10` moved by 4 %.
- **The per-space ambient level.** See section 4.

## 4. Problem 3 - is the per-space lighting worth re-fitting now?

**No. Wait for the specular-mask reconversion.** Three reasons, in order of weight:

1. **The last two material changes already invalidated the fit, and a third is coming.** The
   constants in `app.rs` were fitted on renders made at 10:26 on 2026-09-23. Since then the
   vertex-alpha fix (`abaa14f`, 12:20) changed 1,337 meshes, the glossiness-to-roughness fix
   (`59f1f20`, 12:23) changed the specular of all 59,268 materials, and the specular mask will dim
   that response by up to ten times on some surfaces. Re-fitting now repeats the mistake that
   produced this task: a fit against data that is about to change again.
2. **The ambient and the lights cannot be separated in the frames we have.** `research-074`'s
   warning is the important one: the house dim bands are consistent with the *ambient* being 2-3x
   too bright as well as with the lights being 10x too strong, because both terms are multiplied by
   the same unknown albedo. Cutting the lights first, then rendering, is what makes the ambient
   measurable on its own - the dim third of a room with no light in it *is* the ambient.
3. **The exterior fit is the one Claude has actually signed off**, and it is a median fit on nine
   poses; moving it before the material change lands means signing it off twice.

What *should* move now is the two structural errors in section 2 and section 3, because neither is a
fit to a frame's statistics: the sky fill is derived per space from that space's own weather, and
the light scale is a constant that was never validated in the regime the houses live in. Both must
be checked on the Alftand and Blackreach poses they were not fitted on.

## 5. The change recommended, and its holdouts

| # | file | change | why |
|---|---|---|---|
| 1 | `app.rs` | `SKY_FILL = 0.5`; the daylit ambient brightness derived from the space's own sun; `DAY_SUN_LEVEL` 1.6 -> 1.0667 | the sun 92:1 over the fill is where the black shadows come from; a lit surface keeps its brightness, a shadow becomes a third of it |
| 2 | `lights.rs` | build `HALF_RADIUS_ILLUMINANCE` from the ambient the interior level actually applies (800 x 0.6) | the light was scaled against an ambient no interior has: 1.67x by construction |
| 3 | `lights.rs` | `LIGHT_EXPOSURE` 50 -> 10 | with #2 that is an **8.3x** cut, inside the measured 4-20x band and at the conservative end of the 10-11x the house frames ask for |

**Holdouts** - the poses the change was *not* looked at against, which the render must check:

- **`local/reference/rw6` `RW-01`, `RW-02`, `RW-04`, `RW-07`, `RW-08`** (exteriors): the sky fill
  must lift the dark end without lifting the pooled median past the references'. Expected signature:
  `dark` (fraction below 0.05) falls from 17.7-54 % towards the reference's 6.7-41 %, `p05` rises
  1.5-2.5x, `p50` moves less than 20 %.
- **The 14 `render-cal/SR-interior-Alftand_*` poses** (the interiors the ambient was fitted on): the
  dim bands must not move - they are ambient-lit by construction - while the floor and pool
  structure gets its contrast back. This is the change's main risk: `research-074` found no
  Alftand/Blackreach frame whose blown pixels are demonstrably a *point light's*, and its
  counter-example is the emissive mushroom cards, so if the Alftand dim bands fall, the cut is too
  deep.
- **`render-cal/SR-place-Blackreach*.png`** and the nine `SR-place-*` Tamriel poses.

**Measure with:**

```
python tools/research/lead071_measure.py same local/reference/rw6 local/reference/riverwood
python tools/research/lead071_measure.py same local/reference/render-cal local/reference/uesp
python tools/research/exposure_stats.py --shots tools/reference/uesp_shots_4x3.json --render <dir>
```

## 6. What is left open

1. **Whether the interior ambient is also 2-3x too bright.** Not decidable from these frames
   (`research-074`, item 4). The single render that decides it: the four house poses with the lights
   off, or failing that the poses after change #3 above lands - the dim third of a lit room with no
   light in it is the ambient, and nothing else is.
2. **`EMISSIVE_EXPOSURE` = 100 looks high on its own evidence.** In
   `render-cal/SR-interior-Alftand_12.png` 8.3 % of the pixels are above 0.9, the glaring ones the
   glowing mushroom cards, which are white where the reference's are dark blue with a cyan rim. The
   constant was fitted on Blackreach against a light scale (50) that is itself suspect, and if the
   lights come down 8.3x the glows become 8.3x relatively brighter. Not changed here: it is not one
   of the three problems, and it wants its own measurement.
3. **The `render.rs` doc comments** that quote "50 times the interior ambient" (`exposed_emissive`'s
   doc, and `lights.rs`'s reference to it) go stale with change #3. `render.rs` is not in this
   task's territory; whoever lands the change owns that sentence.
4. **`XRDS` as a light radius is unreliable data.** 13,677 light references carry an override, 3,320
   of them negative, and the positives range from 75 to **3,736,737** Creation units. The four
   Riverwood houses are almost entirely negative, so every one of their lights falls back to the
   record's 512; Alftand's are almost entirely positive, so its lights are lit at radii the records
   never name (850.8 for `IceCandleLight01NS`, whose record radius is 256). Both demos were
   calibrated with this rule, so it is not this task's to change - but the fit that comes after the
   reconversion should decide whether `lights.rs` keeps using it, and that decision belongs to
   whoever owns `crates/converter/**` and this file together.

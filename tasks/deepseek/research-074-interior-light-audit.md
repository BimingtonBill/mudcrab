# Goal

Independently audit one claim that a calibration change is about to be built on, and try to
**falsify** it. The claim:

> In the Riverwood house interiors, and in Blackreach, the surfaces that render too bright are lit
> **by the converted `LIGH` point lights**, not by the per-space ambient and not by emissive plus
> bloom; and they are too bright by a factor of roughly 4 to 10 relative to the ambient.

If the claim is wrong, say so with the numbers that show it. If it is right, strengthen or bound it:
say what the best estimate of the factor is and which frame gives it. A well-argued partial answer
is worth more than a padded one. **You cannot run the engine** - the lead has the GPU and a user is
on it - so everything must come from frames that already exist, the converted database, and the
code.

# Context

The project is Mudcrab (`C:\Modding\MudcrabProject`), a Skyrim-to-Bevy engine. 1 Creation unit
= 1 Bevy world unit; a cell is 4096 units; 1 unit is about 1.43 cm. The engine renders one fixed
day: a `DirectionalLight` sun, a `GlobalAmbientLight`/per-camera `AmbientLight` for the space, and
converted `LIGH` records as `PointLight`s.

What the engine does today, all in `crates/engine/src/`:

- `lights.rs`: `intensity_for_radius` gives a converted light an intensity such that it puts
  `LIGHT_EXPOSURE` (50) times `INTERIOR_AMBIENT_BRIGHTNESS` (800) on a surface at **half the light's
  own radius**, with Bevy's inverse-square falloff and `range` = the record's radius (or the
  reference's `XRDS` override when that is positive).
- `app.rs`: `space_atmosphere` resolves each space from a `space_lighting` row. A table-lit interior
  gets `INTERIOR_AMBIENT_COLOR` (luma 0.678) scaled to the record's hue at
  `INTERIOR_AMBIENT_BRIGHTNESS x INTERIOR_AMBIENT_LEVEL` = 800 x 0.6 = **480**. A daylit space gets
  a sky fill of 640 at the moment. Bevy's ambient contributes `albedo x 0.4519 x colour x
  brightness` with no `NdotL`; the sun contributes `albedo x NdotL x illuminance / PI`.
- `render.rs`: a material whose glTF `KHR_materials_emissive_strength` is above 1 has its emissive
  multiplied by `EMISSIVE_EXPOSURE` = 100. A fire card publishes emissive about 1.4 and a
  Blackreach mushroom cap 2.0-3.6.

The frames to use (all PNG/JPG that already exist):

| what | where |
|---|---|
| the four Riverwood house interiors, current build | `local/reference/rw6/RW-09..RW-12-*.png` |
| the same poses on an older build (before the specular fix) | `local/reference/rw1/`, `local/reference/rw4/` |
| their UESP references | `local/reference/riverwood/SR-interior-*.jpg` |
| the Alftand/Blackreach calibration renders | `local/reference/render-cal/SR-interior-Alftand_*.png`, `local/reference/render-cal/SR-place-Blackreach*.png` |
| their UESP references | `local/reference/uesp/` |

Measurements already taken (linear-light Rec.709 luminance, sRGB-decoded, from 8-bit pixels):

- `rw6/RW-11-alvors-house` mean 0.559, p50 0.568, 0 % of pixels below 0.05, and its 15th-45th
  percentile "dim band" means `(0.644 0.428 0.185)` linear in RGB - i.e. even the dimmest third of
  the frame is near white. Its reference is mean 0.149, p50 0.078, 36.4 % below 0.05, dim band
  `(0.055 0.038 0.018)`.
- `rw6/RW-10-trader-shop-floor`: render mean 0.578 / p50 0.615 / dim `(0.610 0.437 0.198)`; its
  reference mean 0.046 / p50 0.022 / dim `(0.013 0.011 0.007)`.
- `rw6/RW-09-inn-common-room` (a much bigger room, the same kinds of light): render mean 0.105 and
  it is *not* washed out; its reference mean 0.031.
- `render-cal/SR-interior-Alftand_12.png`: the big floor slab reads roughly 0.3-0.5 linear while
  that frame's own dim band is `(0.017 0.024 0.023)` - a ratio of about 15-25x. Its UESP reference's
  floor is only about 2-5x its own dark areas.
- `render-cal`'s Alftand interiors match their references' dim bands (that is what the interior
  ambient level was fitted on) but several of their bright areas are washed.

The four Riverwood house cells (0x133C6 inn, 0x133C8 Alvor, 0x133C9 Trader, 0x133CA Faendal) are
about 900 units across and carry five to eight light references each, all of them radius 512
records: `FarmInteriorLight01`, `FarmInteriorFireLight01`, `FarmInteriorShadowLight01` at colours
`(198,162,100)` and `(173,103,52)`. Their camera poses are 100-500 units from those lights, and the
reference frames' lights are at 279-470 units from the eyes. Alftand's ice-cave lights are radius
256 records (`IceCave_Blue_Omni`, `DwarvenOmniLight`, `IceCandleLight01NS`) in cells tens of
thousands of units across.

# Scope

**In scope** - answer these, with numbers:

1. Which term lights the *dim* third of an interior frame - the ambient, the point lights, or the
   emissive/bloom? A clean way to separate them: the ambient is a near-neutral colour
   (`(45,48,48)` scaled, and `INTERIOR_AMBIENT_COLOR` is a pale green-teal), the house lights are
   warm orange `(198,162,100)` / `(173,103,52)`, and a fire's emissive is warm too. The measured
   dim band of `rw6/RW-11` is strongly red-biased (linear R/G 1.5) while the ambient is not - follow
   that thread through the other frames before you accept it, and look at the images themselves
   (you can read PNG/JPG files) rather than only the statistics.
2. Whether the blowout is the point lights at all, or a fire's emissive card plus `Bloom` washing
   the frame. The frame `rw6/RW-09-inn-common-room` is not washed out and its fire pit appears
   unlit; `RW-10`, `RW-11`, `RW-12` are washed and all three have a burning hearth. That is a
   confound - say which of the two explanations the evidence actually supports, and what would
   separate them.
3. What factor the light contribution is over what the references show, with the frame and the
   statistic each number comes from. If you can bound it (e.g. "at least 4x, at most 12x") do that
   rather than giving one number.
4. Whether the interior **ambient** (480) is also wrong. The lead believes it is not: the 13
   Alftand interior reference poses' dim bands match the render's within about 30 %, and those
   surfaces are ambient-lit. Check that against the reference images, and say if you disagree.

**Out of scope:** proposing code changes, editing anything, and anything about the sun, shadows,
weather, the terrain ring, doors, or the converter.

Read-only. Do not modify any file. Do not run the engine.

## Tools you may have

You may have Read/Grep/Glob. If your harness also allows `Bash(python tools/*)`, two scripts are
there for this audit and are read-only:

```
python tools/research/lead071_measure.py one <image> [<image> ...]
python tools/research/lead071_measure.py crop <image> <x0>,<y0>,<x1>,<y1> [...]   # fractions of the frame
python tools/research/lead071_measure.py same <render-dir> <reference-dir>
python tools/research/lead071_db.py lights <hex-cell-id> [...]      # a cell's light references
python tools/research/lead071_db.py base <hex-light-record-id> [...] # one LIGH record
python tools/research/lead071_db.py spaces <hex-space-id> [...]
```

The database is `C:\Modding\Mudcrab\SkyrimConverted\skyrim_world.db` (read-only). If python is denied, say
so in your report and do the audit from the images, the code and the numbers above.

# Done when

- Each of the four questions has an answer, or an explicit "I could not establish this, and here is
  what blocked it".
- Every number in the report names the file it was measured from (and the crop, if it was a crop).
- You have said which of your own conclusions you consider weak.

# Report

Report back in these sections, briefly and factually:

1. **Verdict on the claim** - confirmed / falsified / partly, in two sentences.
2. **The evidence**, one numbered item per measurement: the file, what was measured, the number.
3. **What would separate the two explanations** in question 2, if you could not.
4. **The factor** (question 3), as a range with the frame that bounds each end.
5. **Disagreements** with anything in this brief, and how confident you are.

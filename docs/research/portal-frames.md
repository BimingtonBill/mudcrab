# What the Riverwood walk-through frames say about the doorway image

**Date:** 2026-09-23. **Scope:** the four questions lead-072 was asked about the doorway the portal
draws through an open load door - does the doorway image line up with the room it becomes, is it lit
like that room, does the frame after the crossing wear the old side's atmosphere, and is the door
drawn over it. Measured on frames already on disk, with a new instrument,
`tools/research/portal_frame_alignment.py`, and by looking at the frames themselves.

**The captures used here all predate impl-066's float portal target.** The doorway image in them is
the clipped 8-bit one (the washed-out page `docs/handoff/impl-066-portal-matches-the-world.md`
describes), so the *lighting* half of question 2 cannot be settled from them; a run on the current
build can. Everything else below is independent of that.

## The instrument

```
python tools/research/portal_frame_alignment.py <walk-through stage dir> [more dirs] \
    [--window 12] [--search 24] [--size 480]
```

Per stage it prints the frame-to-frame change over the whole capture, the same within the window
around the frame `frames.txt` records as the swap, the per-frame luminance around the swap, the
best-fit 2D offset of the last pre-swap frame against a steady frame after it (with the correlation
there, the correlation at zero offset, and a gradient-ratio scale proxy), and the brightness of both.

## 1. The doorway image lines up with the room it becomes - no defect found

Stage 00 (`local/demo/riverwood2/walk-through/00`, the street into Sven's house), where the last
pre-swap frame is the doorway image filling the view and the steady frame after it is the same room
drawn by the main camera:

| pair | best-fit offset | correlation at that offset | correlation at (0,0) |
|---|---|---:|---:|
| f01720 (swap) vs f01729 (steady) | **(0, 0) px** | 0.966 | 0.966 |
| f01721 vs f01729 | - | - | **1.0000** (identical images, equal means) |

Against a ±24 px search on a 480 px-wide image (±128 px at capture resolution), the best offset is
zero, and once the frame after the swap is reached the frames are *pixel-identical*. There is no
shift and no scale to find.

The other stages cannot answer this question from their frames: their last pre-swap frame is a dark
room with a bright doorway aperture in it, and the frame after the swap is the street - two different
compositions, not two renderings of one view. Correlation there is 0.14-0.57 and means nothing.

The code audit agrees (a separate worker's report, `research-074-portal-alignment-audit.1`): the quad
samples the portal target at the fragment's own screen position taken from the **drawing** camera's
view uniform - the same formula as Bevy's `frag_coord_to_uv` - so where the quad stands, which way it
faces and how big it is cannot move the image; and `portal_projection` writes only the projection's
third row (`clip.z`), leaving projected x/y untouched. The audit's one *unverified* candidate is not a
registration error: the quad stands in the plane through the door model's box centre
(`doorway_in_frame`) while the portal's oblique clip plane passes through the door reference's origin
(`doorway_clip_plane`), 12-13.5 units apart for `FarmhouseLDoor01`. If that shows at all it is a band
at the opening's edge, not a displaced interior. It needs one render to settle.

## 2. Brightness of the doorway image against the room it becomes

Stage 00 is the one stage where both frames are the same view, so it is the only clean comparison in
these captures:

| frame | what it is | mean | p50 | p95 |
|---|---|---:|---:|---:|
| f01719 | the doorway image (Sven's house through the opening) | 0.7143 | 0.7451 | 0.7686 |
| f01720 | the same room, first frame after the swap | 0.8268 | 0.8471 | 0.9608 |
| f01721+ | the same room, steady | 0.7931 | 0.8118 | 0.9529 |

The doorway image is **dimmer in the mean and far flatter**: its 5th to 95th percentile span is a
third of the steady room's. That flatness is the 8-bit clip impl-066 removed, so it is not evidence
about the sun either way. What the frames *do* show is that the doorway image is not the same picture
as the room it becomes, which is the thing the new float target and the destination sun are for.

The code side is unambiguous and does not depend on these frames: Bevy filters directional lights per
**view** by render layers (`bevy_pbr-0.19.0/src/render/light.rs:1642-1654`, `view_layers` at `:1613`),
the engine's sun carries no `RenderLayers` (`app.rs:2117`, so layer 0) and the portal camera renders
layer 2 (`portal.rs:137`). **The doorway image is drawn with no sun at all.** The fix and its
reasoning are in `docs/design/door-over-the-portal.md` section 3.

## 3. The first frame after a crossing wears the old side's atmosphere - confirmed, and it is large

The frame the crossing lands in is drawn with the **source** space's atmosphere; the next frame is
not. Measured in two stages, and visible in the images:

| stage | the swap frame | the steady frame after it |
|---|---|---|
| 00, street into Sven's house | f01720: mean 0.8268, p95 0.9608 - the interior lit by the *street's* daylight | f01721+: 0.7931, 0.9529 (**+4.2%**) |
| 07, Sleeping Giant Inn into the street | f12583: mean **0.2028**, p95 0.31 - the street drawn with the *inn's* atmosphere: near-black backdrop where the sky is, no sun, dim teal ambient | f12584: 0.4812, 0.6431 (**2.4x**) |

The stage-07 pair is the clearest thing in this document: the same view, one frame apart, the first
in the inn's atmosphere and the second in the daylight. The on-screen objective text lags with it
("Now in RiverwoodSleepingGiantInn" on the pop frame, "All four houses, and no loading screen between
them" on the next), which is the same one-frame lag in a second system.

Stages 01, 03 and 05 show no pop in their captures - because the capture did not photograph the
frame: `frames.txt`'s swap frame has no `f(swap-1).png` beside it, and the frames around it come in
identical pairs, so the frame the pop is in is missing. That is also why `frames.txt`'s swap frame is
consistently **one later** than the frame the swap is actually drawn in (stage 00: drawn at f01720,
recorded 1721; stage 07: drawn at f12583, recorded 12584). impl-066 noticed this off-by-one in one
run; it is a property of the capture, not of the crossing.

Cause and fix (not this note's to land): `update_atmosphere` (`app.rs:1789`, registered at
`app.rs:211`) applies when `ActiveCell` changes and is ordered nowhere relative to the door
transition, which writes `ActiveCell` in `apply_door_crossings` (`transition.rs:601`). Ordering it
after `transition::DoorTransition` makes the swap frame already the destination's.

## 4. The door over the doorway: the frames cannot show it

No frame in either run shows a door leaf drawn over the opening, and none shows one missing from it
either: in the approach frames the doorway image fills the view, and a leaf at 90 degrees is edge-on,
so its projection is a thin vertical strip that merges with the frame's own edge. The dark strip at
the right edge of `f01700` is source-space frame geometry, not the leaf. The geometry argument for
what is happening is impl-066's (the quad stands 13.5 units towards the `DoorBlack` side of the
leaf's plane, so a leaf swung past that plane is behind it), and the fix - drawing the door a second
time in the destination image, at its mapped pose - is
`docs/design/door-over-the-portal.md` section 2. Units and a run are what settle it; a frame cannot.

## 5. What a run on the current build should capture

1. A `--demo-tour --walk` Riverwood run, whose `walk-through/NN/` frames are the instrument for
   everything above; the alignment offset must stay (0, 0) and the pop at the swap must be gone.
2. A doorway seen from *inside* a house with the door open, looking at the daylit street - the frames
   that show the doorway image against the street walked into, which is the sun's measurement.
3. Two frames of the same doorway a fraction of a second apart while the leaf swings, which is the
   mirror's measurement.

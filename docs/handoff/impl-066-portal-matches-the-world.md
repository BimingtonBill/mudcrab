# impl-066 — the doorway matches the world around it

**Task:** the lead's brief `impl-066-portal-matches-the-world` (no copy under `tasks/deepseek/` in
this worktree). Three user notes on the portal: resolution, lighting, and the door drawing over it.

**Files changed:** `crates/engine/src/portal.rs`, this note. Nothing else — `crates/engine/src/
app.rs` and `crates/engine/src/demo_tour.rs` are the lead's, and two follow-ups for them are in
"Left for the lead" below.

**Acceptance run here:** `cargo fmt --all -- --check` (clean), `cargo clippy --workspace
--all-targets -- -D warnings` (no lint; only the pre-existing `proc-macro-error2` future-incompat
note), `cargo test -p engine` (300 passed, 0 failed, 3 ignored — the three are the opt-in ones that
read the converted assets; doc-tests unchanged). Five tests are new, all in `portal.rs`.

The engine was **not** run against the converted assets (the lead has the GPU and is
mid-reconversion), so the note-2 measurement below is the *before* half, taken from the lead's own
`local/demo/rwcrash` walk-through. "Left for the lead" says exactly what to run for the after half.

## Note 1 — the doorway is drawn at the window's resolution

`PORTAL_TEXTURE_WIDTH/HEIGHT` (a fixed 1024x576) are gone; the target now follows the main camera's
own render target.

* **Where the size comes from.** `Camera::computed.target_info.physical_size` on the main camera
  (`bevy_render-0.19.0/src/camera.rs` `camera_system`), read by `resize_portal_target`. That is the
  physical size of whatever the main camera draws into — the window with its scale factor, with the
  camera's viewport applied — and it is recomputed on `WindowResized`. Reading it rather than the
  `Window` component follows the camera that is actually drawn; the cost is at most one frame of
  lag, because `camera_system` runs in `PostUpdate` and this system runs in `Update`.
* **The first frame.** The main camera is spawned in `Startup` and its target info exists from
  `PostStartup` on, so the first `Update` of a run already has it. Until then — and in any run with
  no window to measure — the target stays at `PORTAL_TEXTURE_FALLBACK_SIZE` (1024x576, the old
  size). The portal camera draws nothing before a door is open, so no frame of a run can show the
  fallback.
* **Resizing is a new image, not a resize in place.** `Image` has no resize, so a changed size
  allocates a new image through `portal_target_image` and repoints both ends of it in the same
  frame: the portal camera's `RenderTarget`, and the quad's material (`PortalMaterial`'s
  `portal_texture`, whose bind group is rebuilt from the changed asset). The shader samples by
  screen-space UV, so nothing else about the quad depends on the size.
* **What the "did it change?" test is keyed on.** The size of the image the `PortalTexture`
  resource already points at (`Image::size`), not a copy of the last request: one source of truth,
  and no allocation on a frame whose size is unchanged.
  `the_portal_target_follows_the_main_camera_and_is_only_rebuilt_when_it_changes` drives the real
  system through four frames of an unchanged size and asserts both that the handle is the same and
  that `Assets<Image>::len()` did not grow, then resizes the main camera and asserts the new size,
  the camera's target and the material all move together. `a_minimized_window_leaves_the_portal_target_where_it_was`
  covers a 0x0 and a half-empty target (a minimized window) keeping the image it has.
* **The ceiling: `PORTAL_TEXTURE_MAX_SIZE` = 2560x1440**, with one uniform factor on both axes.
  A doorway is a fraction of the frame, and the room around it is drawn at the main camera's own
  target size, so a target bigger than the view cannot show more than a pixel per pixel of the view
  — while a maximised 8K display (7680x4320) would be 265 MB at `Rgba16Float` plus the same again
  for the texture the camera renders into. At the ceiling that is 29.5 MB. One factor for both axes
  keeps the doorway's pixel aspect ratio equal to the view's: capping each axis on its own would
  stretch the doorway on an ultrawide or portrait window.
  `the_target_size_follows_the_window_and_clamps_a_large_one` covers 1600x900, smaller-than-cap
  (not scaled up), exactly the cap, 4K, 8K, 0x0, a half-empty target, and the two stretch cases.

## Note 2 — the range the doorway image survives in

The target format was `Rgba8Unorm` viewed as `Rgba8UnormSrgb`. It is now
`PORTAL_TEXTURE_FORMAT = TextureFormat::Rgba16Float`, with no second view format (an sRGB view
would encode on write and decode on sample — a round trip through 8-bit precision, which is the
thing being fixed). What the portal camera writes is then the destination's own scene-referred
light, above white included, and the quad hands it to the main camera's tonemapper and bloom
unchanged. The portal camera still has `Tonemapping::None` and no bloom of its own, for the reason
the existing comment gives: the frame is tonemapped once, by the main camera.

**What that fixes, measured on the lead's own run.** `local/demo/rwcrash/walk-through/00/`, Sven's
House entered from the Tamriel side; the doorway fills the frame for the last stretch of the walk,
so the whole image is the doorway:

| frame | what it is | mean | p50 | p95 | mean of the top 10% |
|---|---|---:|---:|---:|---:|
| f01405–f01416 | the doorway image (portal) | 0.479 | 0.528 | 0.552 | 0.558 |
| f01418+ | the same room after the crossing | 0.616 | 0.634 | 0.915 | 0.921 |

The doorway's median and 95th percentile are the same number to two decimals: more than half of the
frame sits on the tonemapper's ceiling (the 8-bit target clamped everything above white to 1.0, and
the tonemapper mapped all of those to one value), while the same room walked into has a real range
from 0.63 to 0.92. That is the washed-out page the user saw, and it is the clip the float target
removes. Reproduce both rows with the committed tool — the figures above are its own output, and
`f01416`/`f01418` are the doorway's last pre-swap frame and the first steady post-swap frame:

```
python tools/research/exposure_stats.py \
  local/demo/rwcrash/walk-through/00/f01416.png local/demo/rwcrash/walk-through/00/f01418.png
```

**What is *not* the format.** The portal camera renders the destination with the *engine's* global
`DirectionalLight`, which cannot be per camera (`update_destination_atmosphere`'s doc comment has
this already): a doorway from a sunlit exterior into an interior is drawn with the exterior's sun
on it, where the same interior entered has the interior's own sun (off). The run above shows that
too, in the frame the crossing lands in: `f01417` is the first post-swap frame, and it is 8%
brighter than `f01418`+ (`mean` 0.668 vs 0.616, correlation with `f01418` 0.973 against 0.680 with
`f01416`), i.e. the interior lit by the exterior's atmosphere for one frame. Two consequences for
the lead, both outside this task's scope (`app.rs` is not mine):

1. The doorway of an interior seen from daylight is still lit too brightly, whatever the target
   format — that is the sun leak, not the range. It is a much smaller effect than the clip was, and
   how much smaller the after-measurement below will show.
2. There is a **one-frame atmosphere pop at the crossing**: `update_atmosphere` applies on
   `ActiveCell` changing, and that change happens in the transition set, so the first frame on the
   far side is drawn with the near side's atmosphere. If the lead wants the swap to be exactly
   seamless, that is a system-order change in `app.rs`, not a portal change.

## Note 3 — the door, the window, and which is in front

**What actually happens for Riverwood's doors: they animate, and the door is drawn.** Evidence, all
of it readable in the checkout or in the lead's own run logs — the engine was not run here:

* The model carries the clips: `tools/research/door_animation_nif.py glb
  C:/Modding/SkyrimConverted/meshes/architecture/farmhouse/farmhouseldoor01.glb` shows two
  animations, `Open` and `Close`, each one rotation channel on node `Door` (the leaf's pivot at
  `[48, 4, 88]`, leaf 96x176 units, hinge at its `+x` edge), and the node list also carries
  `DoorBlack`, the game's black filler in the wall's opening.
* The engine resolves them at runtime: `local/demo/rwcrash.out` has exactly one
  `engine::door_animation` line — `door=0001CBB0 model=meshes/Architecture/Farmhouse/FarmhouseLDoor01.glb
  from=17.999982 to=90.000045 factor=5.0`, "load door's own swing scaled up to open the doorway" —
  and no `no animation player ... treating it as a static door` warning anywhere in the run. The
  log is once per *model*, so one line for `FarmhouseLDoor01` covers all seven house doors that
  share it. Nothing on that route fell back to the static path.
* So `show_load_door_leaves` never hides them: its only whole-reference hide is
  `DoorState::hides_whole_reference()`, which is `Open { animated: false }` — a door whose model is
  its leaf because the engine cannot animate it. That case is kept (the brief allows it) and the
  reason is now written next to the rule in `drawn_door_visibility`.
* Where the leaf and the window overlap, which is drawn is the **depth buffer's** answer, and there
  is nothing layer-shaped in between: the quad is an opaque mesh of the main view and so is the
  door, and `RenderLayers` selects which camera sees an entity, not which pass it is in. Both stand
  in one view with one depth buffer, so the nearer of the two wins per pixel, exactly as for two
  pieces of wall.
* The numbers put the quad on the far side of the leaf for this model: the reference's origin is the
  model's origin, `doorway_in_frame` lays the quad out at the *centre* of the model's bounding box,
  and that box runs `y -9..36` (leaf `-9..4`, `DoorBlack` `0..36`), so the quad stands 13.5 units
  towards the `DoorBlack` side of the leaf's own plane. Whether that side is the player's is the
  door's `outward` (link data, not model axes) — and the frames say it is: in
  `walk-through/00/f01250.png` and `f01330.png` the swinging leaf is drawn large and close, filling
  one side of the view as the camera walks past the doorway, with the doorway itself behind it.
* **The one case that is still a hole in the picture, and it is geometry, not the rule.** A leaf
  that swings fully *past* the doorway plane, away from the player, is behind the window and is
  covered by it wherever it would show through the opening. The engine cannot do better without
  drawing source-space geometry into the destination's image (the leaf is in the source cell, and
  the portal camera renders the destination's layer only): the fix would be a second pass or a
  depth-independent overlay for the door's own model, which is a larger change than this brief. It
  shows as a thin foreshortened wedge at the opening's edge, and only from the side the door opens
  away from.

**What changed in the code.** The rule itself is behaviour-preserving; it is now the pure function
`drawn_door_visibility(auto_load, state, portal_shows_this_door, waiting)` with the reasoning in its
doc comment, and the brief's table is `a_doors_model_is_drawn_unless_it_is_the_hole_the_doorway_needs`:
14 rows run through the real `show_load_door_leaves`, each asserting both the root's `Visibility`
and the mesh's inherited visibility, in two passes (no portal up; then the portal showing each of
its own rows in turn). The regressed row — `Opening`/`Open { animated: true }` with the portal
rendering through the door ⇒ **drawn** — is in it, as are the static-door hole, the auto-load
marker, and the held crossing.

## Left for the lead (files I do not own)

1. **`crates/engine/src/shaders/portal.wgsl:14-16`** now describes the old behaviour. The sampled
   colour is scene-referred, not display-referred:

   ```
   // The sampled color is the destination's scene-referred light, untonemapped: the portal camera
   // does not tonemap its target (`Tonemapping::None`), and the main camera's tonemapper finishes
   // the doorway exactly as it finishes the room around it. The values above white that reached the
   // target are still above white here.
   ```

2. **`crates/engine/src/demo_tour.rs:506-513` will now log an error per stage.** The tour captures
   `Screenshot::image(portal.0)` at the *Prestream* phase, before the door is open, and the capture
   is converted to a PNG through `Image::try_into_dynamic`, which supports 8-bit formats only
   (`bevy_image-0.19.0/src/image_texture_conversion.rs:174-197`) — so `save_to_disk` reports
   `UnsupportedFormat(Rgba16Float)`. Those PNGs were already blank in every earlier run for a
   related reason: the portal camera has rendered nothing into the target yet when the shot is
   taken. Either capture the target after the crossing (the useful frame — it holds the last
   destination view) or drop the readback.
3. Two runtime checks worth the GPU time, both from the notes above: the after-measurement for
   note 2 (a `--shots` pose facing a doorway with the door open, plus the same pose after
   crossing — or simply the same `walk-through` frames of a `--demo-tour` run, comparing the last
   pre-swap frame with `f01418`+), and a look at the window resize during a run (the doorway should
   change resolution with the window and never show a stale frame).

## Things I could not settle without the engine

* The frames of the Riverwood walk-through show the doorway filling the view from the *first*
  captured frame on (`in_front <= 200` units), and the tour's own `door.png`, taken at a
  320-unit standoff, shows only a piece of the door. That is not what a 96x176 model at 320 units
  subtends, and it is not this task's business to explain (the camera placement, the door's
  `outward` and the measured doorway are the crossing's and the tour's), but if the standoff is not
  where the tour thinks it is, the walk-through's "the doorway fills the view" reasoning rests on
  something else. Worth a look while the lead is in a run.
* The walk-through's `frames.txt` and the PNG names disagree by one frame in that run: the image
  named `f01417.png` is already the far side (correlation 0.973 with `f01418`, against 0.680 with
  `f01416`), while `frames.txt` records the swap on frame 1418.

## Not done, deliberately

`PORTAL_QUAD_OFFSET` stays 0 and the quad stays where the crossing fires (there is a test pinning
that); no depth bias, no recession into the wall — both were considered as a way to make "the door
over the window" unconditional and both change more than they fix (a bias is in depth-texture units
and cannot express the 96 units a swung leaf can sit behind the plane; a recessed quad stops
covering the opening at close range, which is the blank-doorway bug `PORTAL_QUAD_OFFSET`'s comment
records). The visibility rule, the door animation, the crossing, the streaming and the per-space
lighting values are all untouched.

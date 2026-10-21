# impl-066 - The doorway should be indistinguishable from the room around it

## Why

The user played the Riverwood demo and gave three notes on the portal - the view *through* an open
load door into the next space. All three are real and diagnosed below. The goal they describe is
one goal: **a player should not be able to tell the doorway image apart from the world around it.**

Read `docs/design/animated-doors-and-seamless-crossing.md` and the module docs at the top of
`crates/engine/src/portal.rs` before starting.

### Note 1 - "the portal should be the same resolution as the world around it"

`crates/engine/src/portal.rs:142-143`:

```rust
const PORTAL_TEXTURE_WIDTH: u32 = 1024;
const PORTAL_TEXTURE_HEIGHT: u32 = 576;
```

A fixed 1024x576 render target, composited into a window that is usually far larger. The doorway
is visibly softer than its surroundings.

Make the portal target track the **main camera's target resolution**, and follow it when the
window is resized. Points to settle, and to say how you settled them:

- Where the window's current size can be read at the time the target is created, and what to do on
  the first frame if it is not known yet.
- Resizing a `bevy::render::render_resource::Image` means creating a new one and repointing the
  `RenderTarget` and the quad's material; do not resize in place.
- **Do not reallocate every frame.** Only when the size actually changes. Say what you keyed that
  on and prove it in a test.
- Keep a sane ceiling so a very large window cannot allocate an absurd target, and say what you
  chose and why.

### Note 2 - "the portal lighting should match"

This is an exposure and tonemapping mismatch, not a lighting-data bug. The main camera
(`crates/engine/src/app.rs:2004-2005`) carries `Hdr` and `Bloom::NATURAL` and is tonemapped. The
portal camera (`portal.rs:689-713`) carries `Tonemapping::None`, no `Hdr` and no bloom, with a
comment at `:697` reasoning that tonemapping it twice would darken the doorway and that a bloom
pass of its own would be composited and then bloomed again. That reasoning is right about the
double application and wrong about the consequence: the portal renders to a **low-dynamic-range**
target, so everything above white in the destination **clips in the portal texture** before the
main camera's tonemapper ever sees it. A bright destination flattens and a dim one lands on a
different part of the curve than the same room would if you walked into it.

Establish the actual format of the portal target today, then make the portal image reach the main
tonemapper with its range intact - the obvious candidate is an HDR-capable target format so that
the destination's above-white values survive into the composite, with the quad's material not
re-tonemapping them. **Do not simply add `Tonemapping` and `Bloom` to the portal camera**: the
existing comment explains why that is wrong, and it is still right.

The check that matters is a measurement, not an opinion: pick a spot where the player can stand
and see a room through a doorway, capture the doorway image and the same room after crossing, and
compare the pixel statistics of the same surface in both. State the before and after numbers.
`--shots` (`docs/design/reference-shots.md`) renders exact poses, and
`tools/research/exposure_stats.py` measures them.

### Note 3 - "the door should animate and render over/on top of the portal"

`show_load_door_leaves` (`portal.rs:847`) hides the **whole door reference** when the portal is
open and the door is not animating (`portal.rs:861-865`, `hides_whole_reference`). So for a door
the engine cannot animate, the door vanishes and the player looks through an empty hole.

Riverwood's seven house doors all share `FarmhouseLDoor01`, whose `Open`/`Close` clips are in the
converted glb, so they should be on the animated path. Find out what actually happens for them at
runtime, and in particular whether the swung leaf is **drawn in front of the portal quad**: the
quad sits in the doorway plane (`PORTAL_QUAD_OFFSET = 0`) on `PORTAL_QUAD_LAYER = 1`, the main
camera draws layers `[0, 1]` (`portal.rs:128, 137`), and the door's own model is on the default
layer. Determine whether the leaf is depth-sorted against the quad correctly, z-fights with it, or
is hidden by the visibility rule above.

The behaviour to reach: **the door swings open on its own clip and is drawn solid, in front of the
portal image, with the destination visible through the opening it leaves.** A door the engine
genuinely cannot animate is the only case that may still hide, and if you keep that case, say so
and explain why in the code.

## Owned files

- `crates/engine/src/portal.rs`
- `crates/engine/src/door_animation.rs` - only if note 3 requires it; say why if you touch it
- `docs/handoff/impl-066-portal-matches-the-world.md` - your notes

`crates/engine/src/app.rs` is **not** yours: if the main camera has to change, state the exact
lines in your handoff note and leave them for the lead.

## Tests to write

CI has no GPU and no game data, so test the logic:

- The target size follows a given main-target size, is clamped at your ceiling, and is stable -
  a repeated call with an unchanged size must not ask for a new image. Include a 0-sized and a
  huge window.
- Whatever decides note 3's visibility: a table of (door state, portal open, animated, auto-load)
  to the wanted visibility, including the case that regressed - an animated door with the portal
  open must be **visible**.
- Anything you factor out for the format choice in note 2.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

## Out of scope

Do not change the crossing itself, the arrival camera, the door clip scaling or twin-borrowing,
the streaming, or the per-space lighting **values** - note 2 is about the range the portal image
survives in, not about what the destination's lighting is. Do not run the engine against the
converted assets: the lead has the GPU and is mid-reconversion.

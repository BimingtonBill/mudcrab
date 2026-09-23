# impl-068 - Make shadows work

## What the user sees

Nothing casts a shadow. In Riverwood at midday the houses, the trees and the mill sit on flat,
evenly lit ground; the reference screenshots in `local/reference/riverwood/` have hard sun shadows
everywhere.

## What is already there, and the leading hypothesis

The sun is a `DirectionalLight` with **`shadow_maps_enabled: true` already set** - twice, once per
camera setup (`crates/engine/src/app.rs:606-610` and `:856-861`). So this is not a matter of
turning shadows on.

The strong suspect is **world scale**. There is no `CascadeShadowConfig` anywhere in the crate
(`grep` finds none), so the sun uses Bevy's default cascade configuration, which is built for a
metre-scale world. Skyrim's units are not metres: an exterior cell is **4096 units** across and
1 unit is about **1.43 cm** (`docs/design/riverwood-demo.md`, section 0), so a house is hundreds of
units and the streaming radius is tens of thousands. If the default cascades cover a few tens of
units around the camera, the shadow map is spent on a patch smaller than a doorstep and nothing
visible is ever in it.

**Establish this before fixing it.** Read Bevy 0.19's `CascadeShadowConfig` default in
`C:\Users\Brannock\.cargo\registry\src\index.crates.io-*\bevy_light-0.19.0\` (or wherever the
`CascadeShadowConfig` type lives in this version) and report the actual default
`maximum_distance`, `first_cascade_far_bound` and cascade count. Then say what they mean in
Skyrim units. If the numbers do not support the hypothesis, say so and find the real cause -
`docs/research/visual-gaps-spec.md` and the handoff log in `AGENTS.md` are the record of what has
already been ruled out in this area.

## What to build

1. **A cascade configuration fitted to this world's scale**, on both directional lights. Derive
   the numbers rather than guessing: the full-detail streaming radius is `--stream-radius`
   (default 2 cells) and the terrain ring is `--terrain-radius` (default 8), and
   `crates/engine/src/config.rs` has both. Say in a comment what distance you covered and why, and
   what it costs: cascade count and shadow map size are the two knobs, and both cost frame rate.
2. **`DirectionalLightShadowMap`** - the resource that sets the shadow map resolution. State its
   default, choose a value, and justify it.
3. **Depth bias.** At this world scale the default bias is likely to produce acne or peter-panning.
   If you change `shadow_depth_bias` / `shadow_normal_bias`, say what you saw in the numbers that
   made you, and do not tune blind - if you cannot justify a change, leave the defaults and say so.
4. **Leave point-light shadows off.** `crates/engine/src/lights.rs:185` sets
   `shadow_maps_enabled: false` deliberately and the engine budgets 64 nearest lights; turning
   shadow maps on for those is a separate, much more expensive change. Do not touch it.
5. **Do not break the terrain ring.** `crates/engine/src/streaming.rs:1130` puts `NotShadowCaster`
   on the ring; that is deliberate (the ring is distant low-detail terrain) and stays.

**Frame rate is part of the deliverable, not an afterthought.** Shadows at this scale can be
expensive. Say what you expect the cost to be and which knob the lead should turn if it is too
much.

## Owned files

- `crates/engine/src/app.rs` - **only** the two `DirectionalLight` spawns and any resource insert
  they need. Another task may be editing other parts of this file; keep your hunks tight.
- `docs/handoff/impl-068-shadows.md` - your notes, including the measured defaults from item 0

## Tests to write

CI has no GPU, so test what is testable: whatever function or constant produces the cascade
configuration - that it covers the streaming radius at the default settings, that it still does at
a larger `--terrain-radius`, and that the cascade bounds increase monotonically. If you write the
config inline with no function to test, factor out the smallest thing that can be.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

## Out of scope

Do not run the engine - the lead has the GPU and a user is playing the demo on it. Do not touch
the per-space lighting values, the ambient calibration, the emissive/bloom path, point lights, or
the portal camera (another task owns `portal.rs`). Do not add a command-line switch for shadows.

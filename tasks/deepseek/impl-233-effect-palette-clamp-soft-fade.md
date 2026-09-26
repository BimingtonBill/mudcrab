# impl-233 - Hearth flames: clamp the palette lookup; soft fade near geometry

Owned files: crates/engine/src/shaders/effect_palette.wgsl,crates/engine/src/effect_palette.rs,crates/engine/src/render.rs (only where the effect material's uniforms are filled)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user affirmed this queue item on 2026-09-26. The diagnosis is research-230 at `local/research/research-230-hearth-flames.md`; read it first. Images are in `local/t230/`.

## 1. Clamp the palette lookups (the main fault: the hearth's translucent sheet and the blue tip band)
The palette texture (`gradfireexplosion`) is sampled with glTF's default Repeat wrap, because the GLB names no sampler.
- Where the flame is transparent, the lookup reads column 0, which is blended with column 255.
- At the tips it reads row 0, which is blended with row 63, the blue row.

In `effect_palette.wgsl`, clamp both palette lookups (colour and alpha, around lines 57 and 67) to the texture's texel centres, and use `textureSampleLevel(..., 0.0)`. Get the size with `textureDimensions`. This works whatever sampler the texture carries.
- Confirm with research-230's CPU re-creation (`local/t230/sim-palette-repeat-vs-clamp.png`): the mean alpha over the clear area should be about 0.
- Render the Sleeping Giant hearth (`local/gallery/glow-check.json`) before and after. The sheet and the blue band should be gone, with the flame cores unchanged.

## 2. Soft particles (softFalloffDepth)
The data carries `softFalloffDepth` (16, 32 or 100 on the hearth cards) in `OPEN_SKYRIM_material`. Every 3D camera, the portal's included, already has `DepthPrepass` and MSAA off (research-230), so the prepass depth texture can be bound.
- Pass `softFalloffDepth` to the effect material.
- In the shader, inside `#ifdef DEPTH_PREPASS`, fade alpha by `saturate((scene_depth - fragment_depth) / softFalloffDepth)` in view-space units. Bevy 0.19 has `prepass_utils::prepass_depth` and a view-space conversion.
- A card without the value draws as before.
- Render again. The hard cut lines where the cards meet the logs and the pit should soften.

## 3. Optional, if small
The flame atlas is loaded as sRGB, so its green channel, the palette index, is darkened before the lookup. Load that texture linear only if the change is local to the effect material path; otherwise note it for later.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` with `CARGO_TARGET_DIR=D:/Modding-overflow/portal-quick-target` and `CARGO_BUILD_JOBS=6`.
  - Other portal coders share that target dir, so cargo's lock runs one build at a time; the machine's memory commit only fits one.
  - Straight after each build, copy `engine.exe` to `local/t233/engine.exe` and run the copy, because the next coder's build overwrites the shared one.
- Tests and clippy use `--target-dir D:/Modding-overflow/shared-check-target`.
- Assets: `C:\Modding\SkyrimConverted`.
- No timing or benchmark runs.
- Under `local/`, read only `local/t230/`, `local/t233/`, `local/gallery/` and the research file.

## Commits
Subjects start `[claude] impl-233:`; trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No amend, rebase or force-push.

## Report
- Before/after crops for 1 and 2;
- the clear-area alpha before and after;
- test names;
- whether 3 was done;
- the commit hashes.

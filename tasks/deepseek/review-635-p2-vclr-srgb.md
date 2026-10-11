# Goal
Review branch phase2/terrain-vclr-srgb-v2 (2 commits on origin/main a896b90, worktree D:/Mudcrab/worktrees/p2-vclr): is decoding Skyrim terrain vertex colours (LAND VCLR) from sRGB before Bevy's linear multiply correct, and is the change safe to open as a draft PR?

# Context
- The change: crates/engine/src/streaming.rs `build_terrain_quadrant_mesh`, VCLR bytes now go through `Color::srgb_u8(..).to_linear()` instead of `byte / 255.0`. The commit message gives the reasoning (VCLR is a gamma-space tint multiplied into the texture in the game; Bevy multiplies vertex colour in linear space). A new test checks 128 decodes to ~0.216. See `git -C D:/Mudcrab/worktrees/p2-vclr log -2 -p`.
- Terrain shader: crates/engine/src/shaders/terrain.wgsl, which starts from `pbr_input_from_standard_material` (Bevy multiplies vertex colour into base colour there under VERTEX_COLORS) and then multiplies in the layer textures.
- Measured on a real converted install (before/after `--shots`, same poses): 1-4 % of pixels change, max per-pixel sum difference 69-120 of 765, concentrated where VCLR is dark; most terrain VCLR is near white.

# Web questions
- How does Skyrim (Creation Engine, SSE) apply LAND VCLR vertex colours to terrain: are they multiplied with the diffuse in gamma space (no sRGB decode), and is there any documented or modder-tested evidence (xEdit, CK wiki, UESP mod file format, NifSkope/terrain shader discussions)?
- In Bevy 0.19, is `ATTRIBUTE_COLOR` treated as linear by the PBR shader (no sRGB decode of vertex colours)?

# Scope
- Read only. Run only `git log`, `git diff`, `git show` and reading/searching tools. Do not build or edit.

# Done when
- The sRGB claim is supported or refuted with sources (cite URLs and quote short lines), or marked "not settled".
- The verdict is one of accept, fix first, reject.

# Report
Findings as `path:line - problem - why it matters`, then the web evidence with URLs, then the verdict. Under 400 words.

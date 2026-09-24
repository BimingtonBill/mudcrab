# websearch-156 - Immersive Portals (Minecraft) and other seamless-portal projects

## Goal

Find how comparable projects implement seamless portals, with primary sources:

1. **Immersive Portals mod** for Minecraft (author qouteall; Fabric/Forge; its wiki, GitHub source
   and the author's write-ups): rendering method, how it loads and renders chunks visible through
   portals in another dimension, recursion, performance options, how it keeps the portal view in sync
   with camera movement, crossing and collision, lighting/fog across dimensions, known limitations.
2. **Other projects**: e.g. Prey (2006, id Tech 4 portals), Antichamber, Manifold Garden, Superliminal,
   Viewfinder, Sebastian Lague's "Coding Adventure: Portals" (Unity, open source), Valve's
   Half-Life: Alyx / Source 2 if relevant, Godot or Unity/Unreal portal plugins with good write-ups,
   and anything built on **Bevy** (examples, crates such as bevy_portals, Discord/GitHub discussions)
   - Bevy is this project's engine, so Bevy-specific findings on render-to-texture cameras, camera
   ordering and render layers are especially valuable.

Prefer a few well-sourced projects over many shallow ones.

## Context (all you need; you cannot see any files)

A hobby project re-implements Skyrim in Rust on the **Bevy 0.19** engine. Its signature feature is a
**seamless load door**: when a door opens, a second camera renders the room behind it (another
"space" - an interior cell, or another worldspace) into a full-window HDR texture, and a quad in the
doorway samples that texture, so the player sees the next room through the door and walks in with no
loading screen. The destination is streamed in ahead of time (within 800 units of the door). The map
from the source doorway to the destination doorway is a rigid yaw-only transform; the second camera
uses a clip plane at the destination doorway. Crossing swaps the active space in one frame. Spaces are
kept apart with Bevy render layers.

Open problems the project wants prior art for (say which findings bear on which):
A. The doorway image **lags the camera slightly** when the player moves or turns.
B. Performance: the second camera draws a whole frame; ideas are rendering at the doorway's on-screen
   size, skipping it when not visible, stencil/scissor to the doorway shape, oblique near plane,
   cheaper shadows, recursion limits.
C. Seamless crossing: no pop, no one-frame wrong atmosphere/lighting, camera never clipping through
   the door frame, player physics/collision at the threshold.
D. Lighting across the portal: the destination's lights leaking into the source space (GPU light
   clustering ignores render layers); light/fog/exposure continuity at the threshold.
E. Streaming/lifecycle: preparing the destination before the door opens, closing/unloading doors the
   player walked away from, loading what is visible through a portal.
F. Visual polish: eye adaptation, vignettes, fades that hide pop-in.

## Output

A report of findings, each with: the claim, the source URL, its date, and which of A-F it bears on.
Primary sources first (developer talks, official docs, source code, the authors' own write-ups);
check every link you report. Say plainly when something is folklore or you could not confirm it.
Keep it to what a developer could act on. End with the 5 most useful ideas for this project, ranked.

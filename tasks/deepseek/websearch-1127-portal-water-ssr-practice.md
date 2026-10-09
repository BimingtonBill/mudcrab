# websearch-1127-portal-water-ssr-practice (track: Portal / Mudcrab Plus "Water Plus"; queue B13)

## Question
We render a river (Skyrim's Riverwood, in our own engine on Bevy 0.19, forward renderer) with a planar reflection
camera. We want to replace it with screen-space reflections on the water. How do others hide SSR's weaknesses on
water: rays that leave the screen, objects behind others, the edges of the screen, the camera looking down?

## Find, with sources (URLs, dates)
1. Skyrim's own renderer: what vanilla Skyrim SE does for water reflections (planar reflection pass, cubemaps,
   LOD in the reflection). ENB and Community Shaders: their water reflection features (SSR on water, "dynamic
   cubemaps", "water reflections"), how they fill misses and what they cost (frame-time numbers if published).
2. Unreal Engine: SSR plus planar reflections plus reflection captures on water; the usual hybrid (SSR, then probe /
   sky fallback); "screen space reflection edge fade", "hierarchical-Z ray march". Unity HDRP water: SSR, ray-traced,
   planar.
3. Published techniques: stochastic SSR (Frostbite, Stachowiak 2015), hi-Z tracing (McGuire & Mara 2014), SSR on
   water in games (e.g. GTA V, Far Cry, Witcher 3, Red Dead), screen-edge and back-facing fades, temporal
   reprojection of misses, mixing with a low-res cubemap.
4. Bevy specifically: the state of `ScreenSpaceReflections` in Bevy 0.19/0.20 (deferred-only? forward support?),
   any examples or crates doing SSR or planar reflections on water in Bevy (bevy_water, bevy_ocean, etc.).
5. Typical GPU and CPU cost of SSR at ~1080p on a high-end GPU (RTX 3090 class) vs a second planar view.

## Report
A short ranked summary of the approaches that fit a forward renderer with a custom water shader, what each hides
and how, cost figures with sources, and links. Mark anything unsourced. No code edits.

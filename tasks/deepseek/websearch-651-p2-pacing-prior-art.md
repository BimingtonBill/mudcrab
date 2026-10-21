# Goal
Collect prior art on how game engines spread world-streaming work across frames so play never stutters, to inform a rework of a Bevy 0.19 open-world engine's streaming pacing.

# Context
The engine streams a cell grid (Skyrim-style exterior cells, 4096 units square) around the camera. Today each kind of work has its own fixed per-frame count or byte limit: cell commits (a millisecond budget), cell unloads (2 per frame), model scene spawns (4 per frame), GPU uploads (16 MiB per frame). Problems: a few frames still take 18-25 ms at cell crossings; at high speed thousands of models wait seconds; the limits don't adapt to the actual frame time; queues are oldest-first.

# Web questions
1. Frame-time-budgeted streaming: how do Unreal (level streaming, `s.LevelStreamingComponentsRegistrationGranularity`, async loading time limits, "time slicing"), Unity (addressables, `Application.backgroundLoadingPriority`, async upload buffer and time slice), id Tech, Frostbite or Decima talks describe spreading streaming work by milliseconds rather than counts? Exact setting names and defaults where public.
2. Priority: how engines order streaming requests (distance, screen size, velocity look-ahead, "needed for collision first"). Any public papers or GDC talks with numbers.
3. Bevy specifically (0.15-0.19): `RenderAssetBytesPerFrame` / render asset upload limits, async scene spawning, how to measure render-world prepare cost, known stutter sources when spawning many glTF scenes, and any community crates or discussions about frame-budgeted spawning or streaming (bevy GitHub issues, discussions, Bevy blog posts).
4. Adaptive budgets: controllers that raise or lower the per-frame work from measured frame time (e.g. PID or AIMD style), and pitfalls (oscillation, vsync).
5. Skyrim/Creation Engine and OpenMW: how they load cells (uGridsToLoad, background loading thread, "BackgroundLoadLoadedObjects"), and what OpenMW does to avoid hitches on cell changes.

# Report
For each question: findings with source links (prefer primary sources: official docs, talks, engine source, maintainers), quote the key sentence in under 15 words, and say how confident you are. End with five concrete ideas for the engine described above, each tied to its source. Under 1,500 words.

# Goal
Find mods, SKSE plugins, tools and methods that capture non-image information from a running The Elder Scrolls V: Skyrim Special Edition / Anniversary Edition game, useful for developing a compatible engine: world and cell state, loaded objects, scene graph, lighting and weather values, physics, performance.

# Context
- Hobby engine project with a legitimate Skyrim SE install. Want ground truth to compare with our own engine's behaviour.
- Interested in: SKSE plugins or Papyrus tools that dump references in a cell (form IDs, base objects, positions, rotations, scale, enable state), loaded NIF/mesh and texture lists, scene graph (NiNode tree) dumps, current weather/time/lighting/fog/imagespace values, cell lighting and light lists, havok/collision info, navmesh, actor AI state, loaded cell grid and LOD state; Dear ImGui-based in-game debug overlays and menu frameworks (SKSE Menu Framework, "Dev tools", NetScriptFramework tools, CommonLibSSE-NG based dumpers); CLib-NG developer references; SKSE logging; Papyrus Extender / PO3 tweaks queries; Creation Kit / xEdit (SSEEdit) exports for static data; Spriggit; Nifskope; any "runtime dump" or "debug overlay" mod.
- Performance capture: PresentMon, CapFrameX, NVIDIA FrameView, frame-time logging, GPU/CPU profilers usable with Skyrim SE (Tracy, Superluminal, VTune), and benchmarking methods modders use (fixed route, `coc` test cells like the Whiterun/Riverwood scenes, "benchmark" mods).
- Also: how to record video/inputs for replay, and anything about reading the game's memory legitimately via SKSE (Address Library, CommonLibSSE-NG) to expose data.

# Done when
- For each candidate: what it captures, output format, Skyrim version support, source/licence (open source or not), URL, maturity. Two independent sources per claim where possible.

# Report
- Ranked shortlist grouped by data kind (world state, scene graph, lighting/weather, physics, performance), then details with URLs, source count, confidence. Say plainly what you could not find. Note where sources disagree.

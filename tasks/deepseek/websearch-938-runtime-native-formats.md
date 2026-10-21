# Goal
Check the facts behind a design discussion: should an open-source Skyrim engine in Rust/Bevy convert Skyrim's files ahead of time, convert them on demand, or load Skyrim's native formats (NIF meshes, DDS textures, ESM plugins, BSA archives) directly at runtime?

# Context
- The engine uses Bevy 0.19 (Rust). Today it converts everything ahead of time into glTF 2.0 meshes, KTX2 textures (BCn blocks kept as-is, others Basis UASTC), and a SQLite world database; a full Skyrim SE conversion takes ~45 min and ~11 GB.
- A comparable project, ByroRedux (github.com/matiaszanolli/ByroRedux, MIT, Rust + Vulkan), loads Skyrim's native files at runtime.
- Long-term targets include Linux/Steam Deck and (far future) Android.

# Questions
1. Bevy 0.19 and DDS: can Bevy load DDS textures natively (which formats: BC1-BC7, uncompressed, mipmaps, cubemaps), behind which cargo feature, and with what limits? Same for KTX2.
2. Mobile GPUs and BC: which Android/mobile GPUs support BCn texture compression (Vulkan `textureCompressionBC`), versus ASTC/ETC2? Rough share; Adreno/Mali/Xclipse; and does Steam Deck support BC? Cite device databases (e.g. gpuinfo.org) or vendor docs.
3. ByroRedux: what does its README/docs say about load times, memory use, NIF coverage, and caching? Does it cache parsed/converted assets between runs?
4. Precedent: OpenMW loads Morrowind's native NIF/DDS/ESM at runtime. What do its docs say about load times and memory, and does it pre-process anything? Any other engine reimplementations that chose ahead-of-time conversion instead (and why)?
5. Runtime NIF loading cost in general: any published numbers for parsing NIF files at runtime (per file or per cell).

# Done when
- Each question answered with sources, or marked not found.

# Report
- Answers first, each with URLs, number of sources and confidence. Note where sources disagree. Under 900 words.

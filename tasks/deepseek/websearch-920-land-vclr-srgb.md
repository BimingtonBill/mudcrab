# Goal
Find out whether the per-vertex colours in Skyrim's LAND records (the `VCLR` subrecord, 33x33 RGB bytes per cell) are meant to be read as sRGB (gamma-encoded) or as linear values, and how Skyrim's own terrain shader applies them. The answer decides whether an engine that renders in linear space must convert VCLR from sRGB to linear before multiplying it into the terrain colour.

# Context
- An open-source Rust/Bevy engine renders converted Skyrim SE data in linear colour space. It currently multiplies VCLR bytes / 255 directly into the terrain base colour. A proposed change converts them with the standard sRGB transfer function first (128 -> 0.216).
- Scanning 1,812 dry cells, VCLR is mostly near white (darkest cell mean 192/255).

# Web questions
1. What do UESP / en.uesp.net Skyrim Mod:Mod File Format LAND pages, xEdit/TES5Edit definitions, or the Creation Kit wiki say about VCLR (vertex colours / "vertex shading")?
2. Does Skyrim's (or Oblivion's / Fallout 4's) terrain or lighting shader treat vertex colours as sRGB or linear? Any shader decompilations, Community Shaders (Skyrim SE shader rewrite) sources, ENB notes, or OpenMW notes on land vertex colours and gamma?
3. Did Bethesda's engines of that era (Gamebryo/Creation, DX9/DX11) render in gamma space (no sRGB conversion, all maths on gamma values)? If the whole pipeline is gamma-space, what is the correct way to reproduce a gamma-space multiply in a linear renderer?
4. How do other reimplementations handle it: OpenMW (Morrowind VCLR), any Skyrim-to-Unity/Unreal/Godot terrain importers, NifSkope?

# Scope
Web only; cite every claim with its URL; say clearly what you only inferred. No private data.

# Done when
Each question answered or marked "not found", with sources.

# Report
A short verdict first (sRGB / linear / gamma-space pipeline, and the recommended conversion), then per-question findings with URLs, then confidence.

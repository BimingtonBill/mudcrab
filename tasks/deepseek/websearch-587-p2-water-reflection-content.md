# Goal
Find out exactly what Skyrim Special Edition draws into its water reflections, and which game
data and INI settings decide it, so a reimplementation can reflect the same things (and nothing
more).

# Context
We are writing an open-source engine that renders Skyrim SE's converted data. Today our water
reflection is a second camera that redraws the whole scene (every object, shadows included) at
1024x576 every frame, and it costs about as much CPU as the main view. We suspect vanilla Skyrim
reflects much less. Answer from sources, not from memory, and cite each claim with its URL.

# Web questions
1. Which things does vanilla Skyrim SE's water reflection render: terrain (full detail and/or LOD),
   distant object LOD, trees, sky/clouds/sun/moons, actors, the player, ordinary placed objects?
2. The per-reference flags and records involved: the REFR record flag "Reflected By Auto Water"
   (which bit?), the XPWR subrecord ("water reflection/refraction" list: what it holds and how it
   is used), any "Is Marker"/"Visible when distant" interplay, and the WATR record's reflection
   fields (reflection amount/colour, the "Reflections" flag bits). Give record/field layouts from
   UESP or xEdit definitions where possible.
3. The INI settings that control it and their SE defaults: bUseWaterReflections,
   bReflectLODObjects, bReflectLODLand, bReflectLODTrees, bReflectSky, bAutoWaterSilhouetteReflections,
   bForceHighDetailReflections, iWaterReflectWidth / iWaterReflectHeight, fBlockLoadDistance-type
   limits, and what the in-game "Water reflections" options (e.g. "Reflect land/objects/trees/sky")
   switch.
4. How the reflection is rendered: one planar reflection per frame for the nearest water plane, a
   cubemap, a lower update rate? Resolution? Are shadows or lights drawn in it?
5. Known performance notes from modders (e.g. why "high detail reflections" costs FPS).

# Scope
Read-only research. Write nothing in the project.

# Done when
Each of the 5 questions has an answer with at least one cited URL, or is marked "not found" with
what you searched.

# Report
A short section per question: the answer, the exact field/flag/INI names and values, citations
(URL per claim). Then a 5-line summary: "a faithful reflection draws X, Y, Z; it skips A, B".

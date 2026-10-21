# Goal

Find how Skyrim (Special Edition) animates the texture of an effect-shader mesh such as a hearth fire, so an engine can reproduce the flickering flames instead of drawing one still frame.

# Context

Skyrim's fire models (for example `meshes/clutter/woodfires/fireplacewood01burning.nif`) draw flames as flat cards with a `BSEffectShaderProperty` whose source texture is a flame atlas (`textures/effects/fxfireatlas04.dds`, several flame frames in one image) coloured through a greyscale palette. In the game the flames move; an engine drawing the card with a fixed UV shows one pale still frame.

Questions:
1. Which NIF controllers animate an effect shader's texture: `BSEffectShaderPropertyFloatController` (which controlled variables: U offset, V offset, U scale, V scale, alpha, emissive multiple...), `NiTextureTransformController`, `NiFlipController`, `BSEffectShaderPropertyColorController`? Which of them do Skyrim's fire and torch meshes typically use?
2. How is a flame atlas stepped: continuous UV scrolling, or discrete frames (a flipbook: N columns x M rows, one frame per interval)? Where do the frame count and speed live (the controller's `NiFloatInterpolator` keys, the `NiTimeController` frequency and phase, texture clamp mode)?
3. How does `BSEffectShaderProperty`'s own `UV Offset` / `UV Scale` fields combine with a controller's animated values?
4. The byte layout of `BSEffectShaderPropertyFloatController` and its "controlled variable" enum values in nif.xml (SSE), and how `NiFloatInterpolator` / `NiFloatData` keys are stored (key type: linear, quadratic, TBC).
5. Any reimplementation that plays these (NifSkope's renderer, OpenMW for Morrowind's NiFlipController, other open-source Skyrim engines) - what they do.

# Report

A short answer per question: the rule, source URLs, how many independent sources agree, and your confidence. Say clearly where sources disagree or where something is unknown.

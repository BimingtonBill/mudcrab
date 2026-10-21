# Goal
Find the best mods, tools, console commands, ini settings and methods to take clean, repeatable, high-quality screenshots and reference videos in The Elder Scrolls V: Skyrim Special Edition / Anniversary Edition, for use as ground-truth reference images when developing a Skyrim-compatible engine.

# Context
- Hobby engine project; we need controlled, repeatable reference shots to compare against our own renders from the same camera pose.
- Interested in: exact camera placement and pose readout (position, rotation, FOV) and setting (console `tfc`, `tcl`, `setfov`, `player.setpos`, `sgtm`, `fw`/`sw`/`set gamehour to`, `tm` UI hiding, `tai`/`tcai`, `tdetect`), freeze weather/time, hide HUD, disable post-processing/effects for neutral shots, loading a given worldspace/cell via `coc`, taking screenshots (built-in `PrintScreen`/`screenshot`, ini keys like bAllowScreenshot / sScreenShotBaseName / iScreenShotIndex), super-resolution/tiled high-res capture, ReShade screenshot features (depth buffer, before/after, per-pass output), photo-mode and free-camera SKSE mods (e.g. Photo Mode, Improved Camera, Free camera), screenshot-hotkey mods, lossless capture (PNG/EXR vs JPEG), HDR capture, OBS/ShadowPlay/NVIDIA App, scripted/automated tours (SKSE Papyrus or AutoHotkey) that move the camera along a path and shoot at each point, dumping the camera pose alongside each shot (metadata).
- Also note anything that makes shots non-deterministic (random weather, NPC movement, dynamic shadows/TAA jitter) and how modders neutralise it.

# Done when
- Concrete working recipe(s) with the exact commands/ini lines/mod names, download URLs and licences, which Skyrim version each supports, and two independent sources per claim where possible.

# Report
- Ranked shortlist of methods, then a "deterministic reference shot recipe" (step list), then per-tool details with URLs, source count, confidence. Say what you could not verify.

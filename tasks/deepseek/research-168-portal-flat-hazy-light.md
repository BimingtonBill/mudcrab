# Goal

Find which lighting settings make the portal engine's pictures flat and hazy compared with Skyrim, and list every setting that controls it, with its current value and where it lives. This is research only: change no file.

# Context

Track: portal. Project: C:\Modding\MudcrabProject (Rust, Bevy 0.19). Our engine renders the same camera poses as 61 Skyrim reference screenshots. Measured over those 61 (median, Skyrim against ours):
- near-black pixels (relative luminance < 0.05): 36.9 % in Skyrim, 4.7 % in ours. Outdoors: 23 % against 0.2 %.
- the brightest 5 % starts at 0.54 in Skyrim and 0.39 in ours.
- saturation is 0.43 in Skyrim and 0.35 in ours.
- mean brightness is about equal (ours 1.04x).

So shadows are lifted and highlights are compressed. Reviewers also saw a grey or blue fog filling some interiors that Skyrim keeps clear (Candlehearth Hall, Honeyside, the Temple of Kynareth, Alftand), and a haze over outdoor views.

The upstream engine (the project we forked) renders the same outdoor views with deep, dark cast shadows. Its lighting is all in one file, a copy of which is at:
C:\Users\Brannock\AppData\Local\Temp\claude\C--Modding-MudcrabProject\0c2b98e0-89a5-4fa1-b24d-ea1f183613af\scratchpad\upstream-main-engine\app.rs

Our lighting code (read these):
- crates/engine/src/atmosphere.rs: per-space ambient, sun, fog and backdrop (`space_atmosphere`, `ambient_light`, `atmosphere_fog`, `fallback_atmosphere`, `sky_fill_brightness`)
- crates/engine/src/world/lighting.rs: the `space_lighting` rows and `AmbientBases` (the calibrated ambient levels)
- crates/engine/src/lights.rs: point lights, `EXPOSURE_CALIBRATION`
- crates/engine/src/app.rs: camera setup (tonemapping, exposure, bloom, HDR), sun shadows
- crates/engine/src/render.rs, if it sets materials' lighting

The pictures (optional; you can view images): side-by-side pairs, Skyrim on the left and ours on the right, in C:\Modding\MudcrabProject\local\reference\look-gaps\pairs-world_shots_4x3\ (for example SR-place-Morthal.jpg, SR-interior-Candlehearth_Hall.jpg) and pairs-riverwood_shots\ (RW-03-trader-front.jpg).

Do not read anything else under local/ (worker worktrees and logs there are not the real tree).

# Scope

Read only. Change no file.

# Done when

You have covered, for exteriors and for interiors separately:
1. the ambient light: every constant or formula that sets its colour and brightness, with values;
2. the sun: illuminance, colour and shadow settings (cascades, distance, which lights cast shadows);
3. fog: where its colour, start, end and falloff come from, and when an interior gets fog at all;
4. the camera: tonemapper, exposure, bloom, HDR, any colour grading;
5. the same four for the upstream app.rs, side by side;
6. your ranking of which differences most likely cause the lifted shadows and the interior haze, with the reasoning, and what change you would try first for each (a value or a rule, not code).

# Report

Markdown, under these headings: Exterior, Interior, Camera, Upstream compared, Most likely causes (ranked), First things to try. Cite every value as path:line.

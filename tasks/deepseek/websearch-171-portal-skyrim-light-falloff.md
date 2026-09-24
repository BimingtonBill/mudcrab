# Goal

Find how Skyrim (Special Edition) attenuates its point lights (`LIGH` records) and how interior ambient lighting from the cell's lighting template combines with them, so an engine can match the look of Skyrim's interiors.

# Context

An engine using physically based inverse-square point lights (Bevy) finds Skyrim interiors far too dark between lights, and too bright right next to a light. We need Skyrim's actual rule.

Questions:
1. What is Skyrim's point-light attenuation formula as a function of distance and the light's radius (the `LIGH` record's radius field)? For example `1 - (d/r)^2`, a smoothstep, or linear. Is there a "fade" value in the record that multiplies intensity, and what range does it take?
2. How does the cell's `XCLL` / lighting template ambient contribute: a flat ambient term, "directional ambient" (the six-direction `DALC` cube), or both? Is there a "light fade begin / end" distance for the whole cell?
3. How many point lights can light a surface at once in Skyrim (the per-object light limit), and does Skyrim skip shadows for most lights?
4. Any reimplementations with the formula in code: Community Shaders (`doodlum/skyrim-community-shaders`, `Lighting.hlsl`), ENB documentation, NifSkope, Creation Kit wiki. Say what each shows.

# Report

A short answer per question: the rule, source URLs, how many independent sources agree, and your confidence. Say clearly where sources disagree or where something is unknown.

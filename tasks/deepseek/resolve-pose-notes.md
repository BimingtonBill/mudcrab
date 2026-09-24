# Goal
For each reference shot below, read the user's note and the Skyrim screenshot, and name the
place in the game data the screenshot actually shows. The automatic ball-park will look there
instead of where it looked before. Be quick: one decision per shot.

# Context
- The input file is named in the "This run" section. It is a JSON list with, per shot:
  - `shot`: the shot name;
  - `reference`: the screenshot's path under the project;
  - `note`: what the user typed while placing it (e.g. "wrong cell, this is the basement",
    "view is from the courtyard", "can't find it, maybe Solstheim");
  - `tried`: places already tried that were wrong;
  - `candidates`: real editor ids from the world database. Interior cells and worldspaces whose
    names share a word with the shot's place name or the note.
- Open the screenshot and look at it before choosing.

# Rules
- Choose only from `candidates`, or answer null. Never invent an editor id, and never pick one
  in `tried`.
- If the note names a place and a candidate clearly matches it, choose that candidate.
- If the note only says what is wrong ("not this room"), choose the candidate that best fits the
  screenshot, for example a hall with long tables, a basement, or a courtyard.
- If nothing fits, answer null. The shot then stays with the user.

# Scope
Read-only. Change nothing, render nothing.

# Report
Only a JSON object mapping each shot to an editor id or null, e.g.
`{"SR-interior-Blue_Palace": "SolitudeBluePalace", "SR-place-Raven_Rock": null}`.

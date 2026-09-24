# Goal
Grade each numbered pair on the grading pages quickly: does the engine's render (right) show
the same view as the Skyrim screenshot (left)? Judge composition only, never lighting or colour.

# Context
- The pages are `local/reference/grading/<batch>-1.jpg`, `-2.jpg`, and so on. The run's
  "This run" section names the batch. Each row is `#n  <shot name>`, Skyrim on the left and the
  engine on the right. `<batch>.jpg.json` maps each row number to its shot name, or to
  `{"shot", "user_note"}` when the user typed a note while placing that shot. Read the notes:
  they say what the user was aiming for (e.g. "the reference is from the far bank") or why the
  view can't match. Grade the picture, but let a note explain a difference you would otherwise
  count against it.
- The engine has no sky, weather or proper lighting yet. A dark or grey render of the right
  place is fine. Judge where the camera is and what is in frame.

# Grades (use exactly these words)
- `matched`: the same view; the main subject and its surroundings sit where the screenshot
  has them.
- `close`: the same subject from the same side, but the framing, height or distance is visibly
  off.
- `area only`: the right place, but not the same view (wrong side, wrong height, subject not
  framed).
- `wrong`: a different place, an empty or black frame, or the camera inside a wall or rock.

Be strict: `close` means a reviewer would accept it as the same shot, nudged.

# Scope
Read the pages and the index file only. Change nothing. No renders, no database. This should
take a few minutes: one look per row.

# Report
Only a JSON object mapping shot name to grade, e.g.
`{"SR-place-Whiterun": "area only", "SR-interior-Honeyside": "close"}`, covering every row.

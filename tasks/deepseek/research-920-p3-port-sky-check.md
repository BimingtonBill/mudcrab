# Goal
Independently decide how to port upstream main's sky onto this branch (the portal branch after its main merge),
so the result can be compared with another worker's recommendation. Track: Portal (port prep, read only).

# Context
- The branch under study is commit `3570acb8` (the portal branch after its main merge). Your working folder is a
  different, older commit: do NOT read the working files. Read every file of the branch with
  `git show 3570acb8:<path>` and find things with `git grep <pattern> 3570acb8 -- <path>`. Upstream main is
  `origin/main` (a896b90): `git show origin/main:<path>`. Below, "HEAD" means 3570acb8.
- Main has crates/engine/src/sky.rs (a `SkyPlugin`: sky dome mesh + material + shaders/sky.wgsl, plus clear colour and
  fog it applies to cameras). The portal branch has crates/engine/src/atmosphere.rs (per-space clear colour, fog,
  ambient, sun) and crates/engine/src/portal.rs (a second camera that renders the space behind an open door with
  THAT space's atmosphere, every frame the door is open).
- The other worker concluded: do not add `SkyPlugin`; port only the dome, driven per camera from the portal's
  per-space atmosphere. Check that independently; do not assume it.
- Questions, with file:line evidence on both sides:
  1. What exactly does `SkyPlugin` add (resources, systems, components, what it writes each frame)? Is it added
     to the app on HEAD (check app.rs)?
  2. Which of its writes would conflict with atmosphere.rs / portal.rs on HEAD (clear colour, fog, ambient, sun,
     any global resource that assumes one space)? Name the systems on both sides.
  3. Can the dome be used without the rest? What would it need from the portal side (which camera, which render
     layer, what colour/brightness input)? Where is the portal camera's destination atmosphere applied?
  4. Tests on main for the sky, and HEAD tests for atmosphere/portal atmosphere that a port must keep passing.
  5. Your recommendation: wire SkyPlugin, port the dome only, or something else, with reasons.

# Scope
- Read only. You may run: `git show origin/main:<path>`, `git diff origin/main 3570acb8 -- <path>`, `git log`, `git grep`.
- Do not edit or build anything.

# Done when
- Every claim has file:line (HEAD) and/or origin/main path:line. Uncertain points marked "unsure".
- Keep it short: one list per question.

# Report
- Answers 1-5 as lists, then "Risks" (max 5 bullets).

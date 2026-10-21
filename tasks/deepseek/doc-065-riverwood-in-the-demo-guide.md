# doc-065 - The demo guide covers two demos now, not one

## Why

`docs/demo/README.md` is the contribution guide for running the project's demo. It is written
throughout as though there is one: the title is "Running the Alftand → Blackreach demo", section 1
describes that route, and the known-gaps list, the command table and the troubleshooting are all
about it.

There is a second demo now. `--demo riverwood` starts on the Helgen road south-west of Riverwood
and tours the village's four houses. It is not a variant of the first: it is a daylight exterior
village with interiors, where the first is an underground descent.

## What to do

Rework `docs/demo/README.md` so a contributor arriving cold can run **either**. Keep it one
document - do not split it - and keep its voice, its structure and its level of detail. This is an
edit, not a rewrite: most of the file is still correct and should not be touched.

Read these first, and take every fact from them rather than inventing one:

- `docs/design/riverwood-demo.md` - **especially section 8**, which is the only part written after
  the demo was actually built and run. Sections 1-7 are the research that preceded it, and section
  3's route is the walking route.
- `docs/research/foliage-alpha-test.md` and `docs/research/specular-gloss-mapping.md` - the two
  rendering bugs Riverwood exposed. The first is fixed; the second is fixed in the converter but
  **needs a reconversion**, so say plainly which is which.
- `git log --oneline -15` and the handoff log at the top of `AGENTS.md`.

The changes the file needs, at minimum:

1. **The title and the opening.** Name both demos, say in one sentence each what they show and why
   a newcomer might pick one. Riverwood's selling point is the one the Alftand route cannot make:
   the walk through a doorway into an interior and back out, in daylight, with no loading screen -
   measured at **8 crossings of 8 and 0 dark frames in 2,056 frames**
   (`local/demo/riverwood2/tour.txt`).
2. **Section 1's table.** It describes one start and one route. Make it cover both.
3. **The command table** (around line 234). `--demo` takes `alftand`, `blackreach` and now
   `riverwood`; `--demo-tour` is no longer Alftand-only - it walks the route of whichever demo the
   run started in. `play-riverwood-demo.cmd` sits beside the Blackreach launchers in the repository
   root. `tools/demo/play-demo.ps1 -Start <name>` should say what names it takes - **check the
   script rather than assuming it already knows about riverwood**, and if it does not, say so in
   your result; do not edit it, it is not yours.
4. **The known-gaps list** (around line 48). Some of its entries are stale - "no glow or emissive"
   was fixed by `23adbb4`. Check each against the current code and the handoff log, mark what is
   fixed, and add what Riverwood shows that Blackreach did not: no NPCs, no grass, the river drawn
   as a flat cell-aligned plane with no flow, no LOD beyond the terrain ring, the FX cards
   (chimney smoke, street mist, the mill's spray) converting to empty scenes. Say which of these
   are known and specified in `docs/research/visual-gaps-spec.md` and which are not.
5. **Troubleshooting.** Add the one a contributor will actually hit: the engine refuses assets
   whose converter schema does not match its own and says so, which means a converted folder from
   before the schema-18 change has to be reconverted. Give the exact command from the file's own
   conversion section.

## Owned files

- `docs/demo/README.md`
- `docs/handoff/doc-065-riverwood-in-the-demo-guide.md` - your notes, including anything you found
  stale but did not feel entitled to change

## Rules

- **Every claim must come from the repository**, not from what you know about Skyrim or expect an
  engine to do. If you cannot confirm something, leave the existing text alone and list it in your
  notes.
- Do not run the engine, the converter or any script; do not touch `crates/`, `tools/`, the
  launchers, or any file but the two above.
- Do not add screenshots or link to anything under `local/` as though a contributor has it -
  `local/` is git-ignored and exists only on the lead's machine. Referring to it as the lead's own
  evidence is fine where the file already does that.

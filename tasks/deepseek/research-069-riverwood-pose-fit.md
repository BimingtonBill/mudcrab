# research-069 - Fit the Riverwood reference poses against the UESP screenshots

## Why

`tools/reference/riverwood_shots.json` holds twelve camera poses for the Riverwood demo, ten of
them paired with a UESP screenshot in `local/reference/riverwood/`. They were computed from the
game's data by research-055 and have now been rendered three times. Four of them cannot match
their reference at any yaw, for a reason worth stating plainly:

**research-055 put every exterior camera at ground + 120, the player's eye height. UESP's "place"
screenshots are free-camera shots from well above the rooftops.** `SR-place-Riverwood.jpg` is an
aerial looking down on the whole village; `SR-place-Sleeping_Giant_Inn.jpg` looks down on the inn
at roughly 30 degrees from outside its front corner. An eye-height camera cannot frame either.

This task refines the poses by **comparing renders with references**, the same job research-020
did for Alftand (`docs/research/reference-shot-poses.md` is its output and the model to follow).

## What you have

- **The current poses:** `tools/reference/riverwood_shots.json` (the contract is
  `docs/design/reference-shots.md`).
- **The references:** `local/reference/riverwood/`, with `SOURCES.txt` naming each one's page.
- **Three render sets, all of the same twelve poses:**
  - `local/reference/rw1/` - the first renders.
  - `local/reference/rw4/` - the same poses **after** the foliage fix, so the trees are there.
    **Use this set**, not rw1, for anything about composition.
  - `local/reference/rw_sweep/` and `local/reference/rw_sweep4/` - fifteen candidate poses the
    lead swept for the village aerial, the inn, the mill and the street, before and after the same
    fix, with contact sheets `_sheet-*.png`. The mill candidate `C-mill-21000--43800-100` was
    picked from these and is already in the shots file as `RW-05`.
  - `local/reference/rw_sweep2.json` - eight further candidates (a wider inn, a higher village
    aerial) that are **written but never rendered**.
- **The measurements:** `docs/design/riverwood-demo.md` sections 1 and 2 give every landmark's
  position, every building's model and size, and the interiors' bounding boxes.

## What to produce

`docs/research/riverwood-reference-poses.md`, and **an updated
`tools/reference/riverwood_shots.json`**, with:

1. **A judgement per pose**, in a table: `matched` / `close` / `area only`, in research-020's own
   vocabulary, with one sentence on what is right and what is not. Be hard on this - "close"
   should mean a reviewer would accept it as the same view.
2. **A refined pose wherever you can justify one**, with the arithmetic. Say which landmark you
   aimed at, from where, and why that height and pitch. For the aerial and the inn the poses must
   come off the ground: derive the camera from the subject's size and the framing the reference
   shows, and say so.
3. **`RW-06-river-to-the-bridge` and `RW-08-guardian-stones` have no UESP reference** (RW-08 has
   `SR-place-Guardian_Stones.jpg`, RW-06 has none). Say what RW-06 is for and whether it earns its
   place in the set.
4. **The notes in the shots file are part of the deliverable.** Each shot's `note` should say what
   it is meant to show and how confident you are, in the style the Alftand set uses - read
   `tools/reference/uesp_shots_4x3.json` for the register. Keep every field of the contract valid.

## How to work

You can look at the images. Compare the render and its reference side by side and reason about
what is in frame, in what order, at what scale - the order of buildings across the frame, which
landmark sits at which fraction of the width, how much sky, what is in the foreground.

**You cannot render.** The lead has the GPU and a user is playing the demo on it. So your refined
poses are proposals with stated reasoning, exactly as research-055's were, and the lead will
render them and iterate. Say for each one what you expect to see if it is right - that is what
makes the next render a test rather than a look.

Do not invent positions. Every coordinate must come from `docs/design/riverwood-demo.md`, from the
existing shots file, or from arithmetic on those that you show.

## Owned files

- `docs/research/riverwood-reference-poses.md`
- `tools/reference/riverwood_shots.json`
- `docs/handoff/research-069-riverwood-pose-fit.md`

## Out of scope

No code. Do not touch `crates/`, and do not edit any file under `local/` - those are the lead's
render outputs and the references themselves.

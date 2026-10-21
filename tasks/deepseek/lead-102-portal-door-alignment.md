# lead-102-portal-door-alignment - Make every doorway's interior line up, across the world

You are a **lead**: you plan, split the work, spawn workers, review what they return, and report.

## Read this constraint first - it cost a whole round last time

**The harness will refuse you any coder that touches `crates/**`.** `.deepseek-agents.json`
lists `crates/**` in `denyEdit`, and a lead "may not give a coder a protected file"; the route that
lifts it (`tools/ds_impl.ps1`) needs an approval your session cannot grant. Three leads hit this on
2026-09-23 and landed no code. **Do not try.** Your deliverable is the research, the design, and
ready-to-run `impl-` briefs; **Claude launches the coders.** Research workers in read mode are
fine, and you may write under `tools/research/` and `docs/`. Websearch workers are refused to an
edit-mode lead, so answer web questions from local sources.

## The problem

### What the user found

The user stood outside Sven's House in Riverwood, opened the door, and looked through it. **The
whole interior seen through the doorway is at the wrong angle** - off even when the camera is level
and square to the door - so its floor, walls and beams do not line up with the doorframe around
them. Screenshot: `local/demo/user-portal-angle.png`.

The user's diagnosis, which is the working hypothesis to test: **Skyrim never needed a door's two
sides to line up.** A load door is a teleport. The game puts the player at the `XTEL` arrival point
with the `XTEL` heading, and the room behind them may stand at any angle to the doorframe they walked
through. Bethesda's data only has to be consistent enough to land the player sensibly; it never had
to make the exterior door and the interior door agree as geometry. A portal is the first thing that
asks them to.

The user also said, and it is the point of this task: **this probably affects other transitions
too, so a good fix checks places across the world**, not only Sven's House.

### Why the existing measurement missed it

`docs/research/portal-frames.md` section 1 reports "no defect found": the last frame before a
crossing and a steady frame after it align at a (0, 0) offset. That is true, and it is the one test
that cannot see this bug. At the moment of crossing the camera stands in the doorway plane, where any
map between the two sides is closest to the identity - and every captured frame was level and
square to the door. Do not re-derive that result; it holds, and it is not evidence either way.

### Read first

- `docs/design/animated-doors-and-seamless-crossing.md` - how the crossing and its rigid map `M` work.
- `crates/engine/src/transition.rs` (`crossing_pose`, `arrival_camera_rotation`, the shared
  door-to-arrival map) and `crates/engine/src/portal.rs` (`portal_pose`, `door_frame`,
  `LoadDoor::outward`). **Work out exactly how the engine builds the transform that takes the
  viewer's pose on the source side to the portal camera's pose on the destination side**, and write
  it down as a formula with each input named. Cite `path:line` for every input.
- `crates/shared/src/coordinates.rs` for the rotation contract. Creation angles turn **clockwise**:
  an object is rotated by `Rx(-x) Ry(-y) Rz(-z)`, and an `XTEL` heading `z` faces `(sin z, cos z)`.
- The history: `tasks/deepseek/impl-023-door-front-from-xtel.md` (queued in September, never run)
  records an earlier finding that the portal took a door's front from the model axis, which is wrong
  for many door models. Find out whether that was ever fixed, and whether it is part of this.

### What to measure

The data: `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` (converter schema 19). `door_links` has
`ref_id, destination_ref_id, pos_x/y/z, rot_x/y/z` (the `XTEL` arrival), `destination_cell_id,
destination_worldspace_id`; `references` has each door's own `pos_x/y/z` and `rot_x/y/z`. A pair is
reciprocal when door A's `destination_ref_id` is door B and B's is A.

For **every reciprocal pair** (there are about 2,204 links), compute:

1. **`M_engine`** - the transform the engine actually uses today, from the formula you established
   above, applied to the pair's real data.
2. **`M_geom`** - the transform that makes the two doorways coincide as geometry: the source door's
   own frame mapped onto the destination door's own frame, turned half a revolution about the
   vertical so that walking *in* one door means walking *out* of the other. State which axis of a
   door's frame you take as its outward normal and why - this is where impl-023's model-axis problem
   lives, so check it against the data rather than assuming it.
3. **The difference between them**, as yaw / pitch / roll in degrees and translation in units, and
   separately **the angle between each `XTEL` arrival heading and its door's own facing**.

Then report:

- **The distribution** over all pairs: how many are within 1 degree, 5, 15, 45, and how many worse.
  A histogram as a table is enough.
- **Sven's House specifically:** outward door `0x0001CBB0` (in Tamriel), return door `0x0001CBAF`
  (interior `0x0001CB84`). The user can see its error; your number for it must be large enough to
  explain the screenshot, or your model is wrong - say which.
- **The worst twenty pairs**, with their cells' names, so there is somewhere to look.
- **A spread of places to check across the world**, as the user asked. Pick about eight door pairs
  covering: a Riverwood house (at least one besides Sven's), a Whiterun building, a Solitude
  building, a cave or dungeon entrance, a Dwemer ruin (the Alftand route's four doors are known:
  `0x00015D48`, `0x00092809`, `0x0009256A`, `0x0006998D`), and whatever else the distribution says
  is typical and atypical. For each give the door ref, both cells, the measured error, and **a
  standing position and heading a camera would use to look through that doorway** - Claude will walk
  or render these to check a fix by eye.

### The design question

Given the numbers, recommend how the portal should map the two sides, and **how the crossing must
change with it**. The two are coupled: today the seamless crossing carries the player's pose through
the same map, so a portal drawn with `M_geom` but a crossing that lands at the `XTEL` point would pop
at the moment of crossing. Consider at least:

- **Draw and cross with `M_geom`.** The player arrives wherever the geometry puts them, not at the
  `XTEL` point. Is that always on the floor inside the room? What does the data say about how far
  the two land apart?
- **Draw with `M_geom`, cross with `XTEL`, and hide the difference** - where, and how?
- **Keep `M_engine` and correct its inputs** - if the error is in *how* the engine reads the data
  (the model axis, a sign, an angle order) rather than in the data itself, the fix is small. Check
  this first; it is the cheapest outcome and a real possibility.

Say which you recommend, what each costs, and which door pairs would still look wrong under it.

## How to split it

A starting point, not a rule:

1. **Establish the engine's formula once, first.** Every measurement depends on it, so one careful
   worker (or you) writes down exactly how the engine builds `M_engine`, with `path:line` for every
   input, before anything is measured. Check impl-023's model-axis question here too.
2. **Review the existing script.** `tools/research/portal_door_alignment.py` (711 lines) was
   written by research-101, which was stopped before it reported. Read it critically - it encodes a
   formula for `M_engine` that nobody has checked. Fix it or replace it; do not trust it.
3. **Measure across the world in parallel.** Split the ~2,204 reciprocal pairs or the regions
   across read-mode workers, each running the reviewed script on its share and reporting the
   distribution and outliers.
4. **Synthesise** `docs/research/portal-door-alignment.md`: the distribution, Sven's House's
   number (which must be large enough to explain `local/demo/user-portal-angle.png` - if it is not,
   the model is wrong), the worst twenty pairs, and the ~eight check sites across the world with a
   standing position and heading for each.
5. **Recommend the design.** The choice between the options above is a design decision Claude
   makes, so give a recommendation with the costs and the doors each option still leaves wrong.
   **If the cause is the engine misreading the data** - an axis, a sign, an angle order - say so
   plainly: that is a bug fix, not a redesign, and it is the outcome to hope for.
6. **Write the coder briefs** Claude will run, as `tasks/deepseek/impl-1NN-portal-<slug>.md`,
   numbered 103 upward (this track's block is 100-199; 101 and 102 are taken). Each needs owned
   files, tests that would catch it being wrong, and acceptance commands. The tests matter: this bug
   passed every existing test and the scripted tour, because both only ever looked through a
   doorway level and from the plane of the door.

## Constraints

- **No changes to `crates/`**, by you or any worker - see the top of this brief.
- **Nobody runs the engine.** Claude renders and walks the check sites.
- Every number must come from the database or from arithmetic you show.
- Commit nothing. Leave your files in the working tree; Claude reviews and commits.

## Owned files

- `docs/research/portal-door-alignment.md`
- `tools/research/portal_door_alignment.py`
- `tasks/deepseek/impl-1NN-portal-*.md` (103 upward) and any `research-` briefs you spawn

## Report

What the cause is, with the evidence; the distribution; the design you recommend and why; the check
sites; the briefs you wrote; and what you are unsure of.

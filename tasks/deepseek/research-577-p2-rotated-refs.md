# Goal
Track: Phase 2. Find out why two placed objects render rotated wrongly, and name the fix. Change no
code.

# Context
- The user reported, while playing (captures with a screenshot, camera pose and state per shot in
  `local/captures/2026-09-25_03-57-58/`; `notes.md` has their words):
  - `02.png` / `02.json`: interior cell 0x133C9 (RiverwoodRiverwoodTrader), "this shop bench is
    rotated wrong" - it is the L-shaped shop counter in view.
  - `05.png` / `05.json`: worldspace 0x1A26F (WhiterunWorld), "dragonsreach bridge is rotated the
    wrong way".
  Look at both images.
- Converted data: `C:\Modding\Mudcrab\SkyrimConverted\skyrim_world.db` (SQLite; tables `references`
  with `pos_x/y/z`, `rot_x/y/z` in radians and `scale`, `cells`, `statics` with `model_path`), GLBs under
  `C:\Modding\Mudcrab\SkyrimConverted\meshes\`, source NIFs under `C:\Modding\Mudcrab\SkyrimConverted\vfs\meshes\`.
  Read the database with a Python script you write (the `sqlite3` module); open it read-only
  (`file:...?mode=ro` URI).
- How the engine turns a reference into a transform: Creation REFR angles turn clockwise; objects are
  rotated by `Rx(-x) Ry(-y) Rz(-z)` (see `crates/shared/src/coordinates.rs` on this checkout, and the
  AGENTS.md handoff of 2026-09-22 "rotation fix"). A measurement over 82,731 tilted references
  (research-023) agreed with that order to a median 8.2 degrees, but only for terrain-aligned tilts.
- Candidate causes to test: (a) these references have large X/Y rotations where the order or sign of
  the X/Y part is wrong; (b) the model itself has a root-node rotation the converter drops or applies
  twice; (c) a parent/child or `XSCL`/scale sign issue; (d) something else you find.

# Scope
Read-only for the project. You may write and run Python scripts in your own scratch folder only.
Do not modify the database or any file under C:\Modding.

# Done when
For each of the two objects: the reference's form ID, base object and model path; its stored
rotation; what the engine computes; what Skyrim must be doing instead (with evidence: e.g. the
geometry of neighbouring pieces that must line up, the NIF's root transform, or UESP/xEdit
documentation of the record); and the smallest code change that would fix it, with the files.

# Report
The two findings as above, whether they share one cause, how many other references in the
database the same cause would affect (a count from a query), and your confidence in each claim.

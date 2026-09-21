# t02 - Alftand -> Blackreach: what a seamless worldspace-transition demo needs

Read `tasks/deepseek/context.md` first. This is a **research** task: documentation only, no code.

## Goal

The user wants a showcase demo: start at the Alftand ruin entrance in the Tamriel exterior (the Pale),
go inside, and arrive in Blackreach - the vanilla underground worldspace in `Skyrim.esm` - with no
loading screen. In the game this path crosses load doors: Tamriel exterior -> Alftand interior cells
-> Blackreach worldspace. Find out exactly what OpenSkyrim has and lacks for this, and write the
recipe the lead will implement.

The lead's first reading (verify it, do not trust it): the engine takes one `worldspace_id` at start
(`crates/engine/src/config.rs`, `crates/engine/src/app.rs`), and `fly_camera` only moves the camera
- there is no door, teleport or worldspace-switch handling. The Phase 2 audit
(`docs/research/phase2-status.md`, row M5.1) says the streaming fixture never crosses interior <->
exterior; only a unit test covers that boundary.

## Questions, in order

1. **The route in the data.** From `Skyrim.esm` (read-only at
   `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/`, or better, from the converter's
   own ESM reader in `crates/converter/src/esm/`): the Blackreach `WRLD` record's FormID and EditorID,
   Alftand's exterior cell (worldspace + grid), the Alftand interior `CELL`s on the way down, and every
   load door on the route with its `XTEL` target (destination door ref, position, rotation). Do not
   quote FormIDs from memory: derive them, and say how. If reading them needs a small script, put it
   under `tools/research/` and run it with `python tools/research/<name>.py`. Never read a whole
   `.esm` or `.bsa` with `Read`.
2. **What the converter keeps.** Does `crates/converter/src/esm/` parse `XTEL` or door refs at all?
   Does the SQLite schema (`docs/specs/converters/db-schema.md`, `crates/converter/src/esm/exporter.rs`)
   have interior cells, non-Tamriel worldspaces and door links? Once the running conversion finishes
   there will be a real `C:/Modding/SkyrimConverted/skyrim_world.db` - if it exists when you get
   there, you may query it read-only with a script (open it with `mode=ro` in the URI); if not, answer
   from code.
3. **What the engine does.** How `crates/engine/src/streaming.rs`, `world/database.rs` and `app.rs`
   pick the worldspace, load interiors, and rebase. What would have to change to (a) switch the active
   worldspace at runtime, (b) stream an interior cell alongside exterior cells, (c) trigger a
   transition when the camera passes a load door - without a loading screen (pre-stream the
   destination before the crossing).
4. **Size of the job.** Blackreach's asset closure: rough counts of references, unique meshes and
   textures (from the DB if available, otherwise estimate and say so). Anything in its content the
   Phase 2 converter cannot render (the audit lists gaps: `NiTriStrips`, skinned meshes, particles).
5. **The fallback** the requester named: Markarth exterior -> Understone Keep -> the Dwemer excavation
   beneath it. The same questions 1 and 4, briefly, so the lead can choose.

## Scope

- Write only `docs/research/worldspace-transition-demo.md`,
  `docs/handoff/t02-worldspace-transition-demo.md`, and any script under `tools/research/`.
- Read-only everywhere else. Do not run the engine, the converter or `scripts/`. Do not touch
  `C:/Modding/SkyrimConverted` except to read the DB.

## Done when

- The note has: the route table (cell/worldspace, FormID, EditorID, door refs and `XTEL` targets); a
  "what exists / what is missing" table across converter, database and engine with `path:line`; an
  implementation recipe in ordered steps with the files each touches and the test that proves it;
  the Blackreach vs Markarth comparison; and your recommendation.
- `python tools/check_citations.py docs/research/worldspace-transition-demo.md` reports nothing broken.
- Committed: `git add <paths>` then `git commit -m "[deepseek] t02: worldspace transition demo research" -- <paths>`.

## Report

300-600 words for the lead: whether the demo is reachable, the smallest set of engine and converter
changes, the route's key IDs, and Blackreach-or-Markarth. Every consequential claim with `path:line`
or the command that produced it.

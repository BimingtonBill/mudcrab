# Goal
Track: Phase 2. Find out why the engine reports `meshes/Dungeons/Mines/MineDoor01.glb` missing at
runtime, and say whether it is a converter bug, a data quirk, or an engine lookup bug. Change no code.

# Context
- Observed at runtime on the converted data `C:\Modding\Mudcrab\SkyrimConverted` (converter schema 26):
  the engine logs `meshes/Dungeons/Mines/MineDoor01.glb` as missing.
- In that output: `meshes/dungeons/mines/` holds `minedoorload01.glb` and `minedoorload02.glb`,
  but no `minedoor01.glb`; the conversion manifest (`conversion-manifest.json`, keys are source
  paths) has no entry for a `minedoor01.nif` mesh and no failure for it; textures
  `textures/dungeons/mines/minedoor01.dds` and `minedoor01_n.dds` exist. The extracted sources are
  under `C:\Modding\Mudcrab\SkyrimConverted\vfs\` (a find for `minedoor01*` under `vfs/meshes` returned nothing).
- The world database is `C:\Modding\Mudcrab\SkyrimConverted\skyrim_world.db` (SQLite; open read-only with a
  `file:...?mode=ro` URI). Base objects with models are in tables such as `statics` (id, editor_id,
  model_path) and the door table; references in `references` (base_form_id, cell_id, worldspace_id).
- The game's archives are in `C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data` (read-only).
- Questions: which base record(s) name `Dungeons\Mines\MineDoor01.nif` (form ID, editor ID, record
  type)? Does that NIF exist in any BSA in Data (list the archives' file tables; say how you read
  them) or as a loose file? How many placed references use those bases, and where (cell /
  worldspace)? If the file is absent from the game data too, what does Skyrim show for them (an
  invisible door? a different model via a later plugin override? a marker)? Does the engine or the
  integration report treat it as a failure, and should it?

# Scope
Read-only. You may write and run Python scripts in your working folder.

# Done when
Each question above is answered with the evidence (query results, archive listings, record bytes),
and the report ends with a recommendation (no change / converter / engine) and the smallest change.

# Report
Findings per question, the recommendation, confidence per claim.

# Web questions
none

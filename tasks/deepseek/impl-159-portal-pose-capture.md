# impl-159 - A hotkey that saves the camera's pose, and a flag that starts at a shot's pose

Owned files: crates/engine/src/pose_capture.rs,crates/engine/src/lib.rs,crates/engine/src/app.rs,crates/engine/src/config.rs,docs/design/reference-shots.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal (tooling for the reference shots; the Phase 2 track uses it too). Read first:
`docs/design/reference-shots.md` (the shots-file contract: position in Creation units, `yaw` clockwise
from north in degrees, `pitch` positive looking down, `hfov`, `worldspace_id` / `interior_cell_id`),
`crates/engine/src/shots.rs` (how a shot pose becomes a camera `Transform` - reuse that conversion,
do not write a second one), and `EngineConfig` / `PortalOptions` in `config.rs`.

## Goal

The user lines up reference screenshots by hand, flying the engine. Give them:

1. **`P` saves the pose.** In any interactive run, pressing `P` appends one JSON line to
   `local/reference/manual-poses.jsonl` (create the folders) with exactly the shots-file fields:
   `worldspace_id` or `interior_cell_id` (whichever space the camera is in, from `ActiveCell`),
   `position` [x, y, z] Creation units, `yaw`, `pitch`, `hfov`, plus `"saved_at"` (RFC 3339 time)
   and `"shot"` (the `--start-shot` name if the run was started from one, else null). The same line
   is logged at `info`. The line must round-trip: loading it as a shot must put the camera back at
   the same pose (test that with the conversion functions, within 0.01 unit / 0.01 degree).
2. **`--start-shot <shots.json> <name>`** starts the run at that shot's pose, in fly mode (not
   walking), in its space - exterior or interior, the same way `--shots` enters a shot's space. Fail
   fast with a clear message if the file or name is missing. It combines with nothing else that
   poses the camera (`--shots`, `--demo-tour`, `--start-position`): refuse those combinations.

Put the systems in a new `crates/engine/src/pose_capture.rs` with a `PoseCapturePlugin`, added in
`app::run` for interactive runs; the flag in `PortalOptions` (`config.rs`), next to the other portal
flags. Add a short section to `docs/design/reference-shots.md` with the exact command line, e.g.
`target\release\engine.exe --assets <converted> --start-shot tools/reference/riverwood_shots.json RW-04-inn-front`.

Commands on entities: use the `try_` forms. Tests: the round-trip above; the flag parsing (good,
missing file, missing name, refused combinations). You may build and run the release engine
(`cargo build --release -p engine`; assets `C:\Modding\Mudcrab\SkyrimConverted`, read-only; output only
under `local/`) to check that `--start-shot` puts the camera where a `--shots` render of the same
shot looks - compare one pose by eye and say which.

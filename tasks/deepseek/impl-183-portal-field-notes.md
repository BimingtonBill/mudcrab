# impl-183 - Field notes in the demo: F12 takes a screenshot, writes the pose and the state, and asks for a note

Owned files: crates/engine/src/field_notes.rs,crates/engine/src/lib.rs,crates/engine/src/app.rs,crates/engine/src/config.rs,docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user plays the demos (`--demo riverwood --walk`, `--demo alftand --walk`) and reports bugs in words ("I walked in and out of Sven's house a few times and the shadows stopped working outside"). We could not reproduce that one from the words alone. The user asked for the reference-pose tool's abilities (take a screenshot, save the pose, write a note) inside the demo, so they can show us things directly.

## What exists (read these, reuse them)

- `crates/engine/src/pose_capture.rs`: `P` saves the camera pose as a shots-contract line (`SavedPose`, `camera_pose_angles`, `creation_position`, `horizontal_fov_degrees`, `rfc3339`); the "saved" notice HUD (`PoseSavedNotice`, `fade_pose_saved_notice`). It only runs with `--start-shot`, which is refused beside `--walk`. Leave that file and its behaviour as they are; call its public helpers (make a helper `pub` there only if you must, and say so).
- `crates/engine/src/shots.rs` and `docs/design/reference-shots.md`: the shots-file contract (`ShotsFile`, `Shot`). A file you write must load with `--shots`.
- `crates/engine/src/transition.rs`: the `DoorCrossed` message (read it; do not edit transition.rs), `ActiveCell` in streaming.rs.
- Bevy 0.19's `Screenshot::primary_window()` with `save_to_disk` (the demo tour already saves main-window PNGs; find how in `demo_tour.rs`, read only).

## What to build (new module `field_notes.rs`, a `FieldNotesPlugin` added in `app.rs` for every windowed run)

1. **F12 = capture.** Saves, into one folder per run, `local/captures/<run start, YYYY-MM-DD_HH-MM-SS>/` (the root configurable with `--captures-dir <dir>` in `config.rs`; create folders as needed):
   - `NN.png`: the frame as the player saw it (take the screenshot before any note box is drawn, so the box is not in the picture);
   - `NN.json`: the pose in the shots contract (position, yaw, pitch, hfov, the space: worldspace or interior cell, exactly as a shot names it), `saved_at`, the note (null until written), and a **state snapshot** (below);
   - `shots.json`: a shots file holding every capture of the run so far, shot names `"NN"`, so `engine --shots local/captures/<run>/shots.json` re-renders them. Rewrite it on every capture.
2. **Then a note box.** Right after the capture, a small text box on screen ("Note for 03 - Enter saves, Esc skips"). Typed characters, Backspace, Enter (save) and Esc (skip; no note) work. While it is open no game key may act: in `PreUpdate`, after Bevy's input systems, call `ButtonInput<KeyCode>::reset_all()` (and the same for mouse buttons) so the player's WASD/E/F/Escape and every other system see nothing; read the text from `KeyboardInput` messages (`logical_key`, `Key::Character`, `Key::Space`). Do not edit `player.rs` (another task owns it). Enter rewrites `NN.json` with the note; either key closes the box, shows a short "Saved 03" notice, and appends to the index below.
3. **The run's notes index**, `notes.md` in the run's folder, one file the reader opens first:
   - a header: start time, the command line (`std::env::args`), the engine binary's path and modified time;
   - one line per capture: `NN  HH:MM:SS  <space>  <note>  ![](NN.png)`;
   - **one line per door crossing** as it happens (from `DoorCrossed`: time, from space -> to space, the crossing count so far). "After how many trips" is then in the file without anyone counting.
4. **The state snapshot** in `NN.json` (read-only queries; include what is public and cheap, list anything you could not reach in your report):
   - the active space and the crossings so far;
   - every `DirectionalLight`: entity, name if any, whether it is the portal's sun (`PortalDestinationSun`), `shadows_enabled`, illuminance, its `RenderLayers`, its `CascadeShadowConfig` bounds;
   - every camera: entity, `is_active`, its `RenderLayers`, whether it is the portal camera, its order;
   - the load doors within 1000 units: form id, `DoorState`, distance;
   - frame time over the last second.

Keep the capture from blocking a frame: writing a PNG and two small JSON files per press is fine.

## Checks

- Unit tests for the pure parts: the run folder name, the index lines (capture and crossing), the snapshot/JSON shape, the shots file the captures produce loads back as a `ShotsFile` with the right names and poses, the note editing (characters, Backspace, Enter, Esc).
- `cargo build --profile quick --bin engine`, then the short tour must still pass unchanged:
  `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t183 --tour-doors 2` ends `tour PASSED after 2 crossings (short tour)`. Read only `local/t183*` and `local/captures*` under `local/`.
- A capture on a real run: add a hidden test flag if you need one (e.g. `--field-notes-test`: after the first crossing and 3 s later, capture with note "test", then exit), run it with `--captures-dir local/captures-t183`, and show the resulting `notes.md`, `01.json` and that `01.png` exists. Then check `engine --shots local/captures-t183/<run>/shots.json --shots-out local/t183-reshot` renders `01`.
- `docs/demo/README.md`: a short "Showing us a bug" section: F12, the note box, where the folder is, send the folder path.

## Report

Files changed and why; the key handling (how game keys are blocked while typing, and anything that still leaks, e.g. mouse look); what the snapshot holds and what it could not reach; test names; the tour's last line; the real-run `notes.md` and `01.json` contents.

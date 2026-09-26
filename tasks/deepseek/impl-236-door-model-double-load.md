# impl-236 - No second load of a door model; an open door's lost player is reattached (flake B)

Owned files: crates/engine/src/door_animation.rs (model loading and forget_lost_door_players),crates/engine/src/model_animation.rs (model loading only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user affirmed this queue item on 2026-09-26. The diagnosis is research-229, flake B: read `local/research/research-229-portal-flakes.md`; the image is `local/t229/c3/fit-sven-square.png`.

## The bug
Sometimes a closed door fills Sven's open doorway. It is Sven's own front door `0001CBB0` drawn closed.
- Every copy of `FarmhouseLDoor01` respawns in the same frame, which Bevy does only when a model's scene asset is replaced. So the glb was loaded twice:
  - streaming asks for the scene sub-asset (`GltfAssetLabel::Scene(0)`, streaming.rs ~1307);
  - `door_animation` asks for the whole file (`asset_server.load(path)`, door_animation.rs ~524 and ~741) a frame or two later.
  - Bevy 0.19 creates the whole-file handle late in the load task, so the second request can start a second full load (Bevy issue #10549).
- `model_animation.rs:252` does the same for every non-door model.
- The door was open when it respawned. `forget_lost_door_players` only reattaches **closed** doors (door_animation.rs ~1442), so the door keeps a dead player and stays "open". Its new leaf sits closed, and it can never close again.
- Frequency: 9 of about 56 older runs; 1 of 6 cold runs.

## The fix
1. **No second load, without touching streaming.rs** (streaming is shared with upstream main; keep this portal-side). Request the whole-file `Handle<Gltf>` only after the scene sub-asset has finished loading: once the file's load has completed, `load(path)` returns the existing root asset and starts nothing. For example, poll `asset_server.is_loaded_with_dependencies` on the model's scene handle, or wait for the spawned `SceneRoot`'s instance, before the first `load(path)`. Or get the path's existing handle with `asset_server.get_handle`. Verify in Bevy 0.19's source which of these really avoids the reload, and cite it (the cargo registry: `bevy_asset-0.19*`).
   - Apply the same to `model_animation.rs`.
   - Add a debug log for scene-asset replacement (`AssetEvent::Modified` on `Scene`, or `Gltf`) so a double load shows in the log.
2. **Reattach an open door's lost player too:** `forget_lost_door_players` handles open doors, putting the new leaf where the swing left it (the `Open` clip at its end fraction). Otherwise a respawned open door is closed and lies about it.

## Checks
- Unit tests for (2), and for (1) if the handle logic can be tested.
- Run the reproduction from research-229 cold, 8 times. Count "scene replaced" log lines (expect 0) and closed-door frames at Sven (expect 0).
- The short tour passes (`--demo riverwood --demo-tour <dir> --tour-doors 2`).

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree, with its own target (the harness sets it; never another folder's target) and `CARGO_BUILD_JOBS=6`. After each build, check that `quick/engine.d` names your worktree before you run the exe.
- Tests use your worktree's target; the harness runs the acceptance commands.
- Assets: `C:\Modding\SkyrimConverted`.
- No timing or benchmark runs. If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Leave a handoff note in `docs/handoff/impl-236.md`.

## Report
- Which handle rule you used, with Bevy citations;
- test names;
- the 8-run table;
- the tour's last lines;
- the commit hashes.

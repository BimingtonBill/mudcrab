# impl-236 - no second load of a door model; an open door's lost player is reattached (flake B)

Track: portal. Base commit `e38b896`. Workers: impl-236-door-model-double-load.1 (DeepSeek, worktree
`local/impl/impl-236-door-model-double-load`). Not committed: the lead integrates.

Diagnosis: research-229, flake B (`local/research/research-229-portal-flakes.md`), image
`local/t229/c3/fit-sven-square.png`.

## What changed

`crates/engine/src/door_animation.rs`

1. **The door's model root is taken, never asked for while a load of it is in flight.**
   `request_door_models` no longer calls `AssetServer::load`; the pending door keeps the path and
   `attach_door_animations` takes the root with `existing_model_root` (`AssetServer::get_handle`,
   `bevy_asset-0.19.0/src/server/info.rs#L305-L312`, `#L357-L361`) for up to `MODEL_WAIT_FRAMES`
   (120) frames, then asks for it as the last resort. `attach_door_animations` reuses the handle the
   door already holds (`DoorModel`) on a re-claim, and the door holds the root from the frame it
   takes it (that was `request_door_models`'s `DoorModel` insert before).
   The rule and the citations (Bevy issue 10549, `server/mod.rs#L752-L753` before `#L847-L865`,
   `Assets::insert` -> `Modified` -> `world_asset_spawner.rs#L593` re-instance) are in the doc of
   `MODEL_WAIT_FRAMES` and `existing_model_root`.
2. **The twin's root** is taken the same way, with `TWIN_ROOT_WAIT_FRAMES` (10) frames of patience
   before `twin_swing` asks for it.
3. **Diagnostic `report_reloaded_models`**: `info!` on every `AssetEvent::<Gltf>::Modified` ("model
   asset loaded a second time: every drawn copy of it is re-instanced"). Nothing in the engine
   mutates `Assets<Gltf>`, so a Modified is a double load and nothing else - this is the line to
   count. `info!` and not `debug!` on purpose: Bevy's default `LogPlugin` level is `INFO`
   (`bevy_log-0.19.0/src/lib.rs#L270-L293`), so a `debug!` could not be counted in a normal run.
4. **`forget_lost_door_players` handles every state, not only `Closed`.** A door that was not closed
   gets `ReattachPose(state)` on the way out - `Opening`/`Open` -> `Open { animated: true }`,
   `Closing` -> `Closed` - and is otherwise handled exactly as before (only `Closed` carries no pose).
   New `restore_lost_swings`, chained right after `attach_door_animations`, plays the `Open` clip at
   its last key on the scene's new player and puts the state back (`open_arrival_door`'s seek), or
   `Open { animated: false }` when there is nothing to play.

`crates/engine/src/model_animation.rs`

5. **A cached model is never requested**: `request_idle_models` asks the asset server for nothing
   (the clip and its graph are in `IdleModelCache`), and a reference whose model is not resolved yet
   takes the root the same way as the door (`take_root_model`, `MODEL_WAIT_FRAMES`). Before this,
   every reference of a model whose root had been dropped by an earlier resolution started a whole
   new load of that file - the same "re-instances every drawn copy" fault, without a door open.

## Tests (`cargo test -p engine --lib`: 624 passed, 0 failed, 3 ignored)

- `a_door_never_asks_for_a_model_the_streaming_load_has_not_made_yet` - asserts the fixture's model
  path has **no handle in the asset server** after the door has been claimed for several frames. The
  pre-fix code fails this by construction: `AssetServer::load` creates the path's handle
  synchronously (`server/mod.rs#L557-L570` -> `info.rs#L204-L264`), which is what a second load needs.
  (Read, not run, on the pre-fix tree - the claim is from the source, not from a run.)
- `a_door_that_lost_its_animation_player_is_given_its_animation_again` (updated: the player entity is
  respawned as a component swap, and the door is asserted to be re-attached on the new player).
- `a_door_that_lost_its_player_while_open_comes_back_open` (new) - the flake: after the loss the door
  is `Open { animated: true }` again with the clip at its end, not `Closed`; its leaf is out of the
  doorway again (`leaf_degrees`).
- `a_door_that_lost_its_player_while_closing_comes_back_closed` (new) - `ReattachPose(Closed)` for a
  door on its way shut; the state is set by hand because the swing itself is covered by
  `reactivating_an_open_door_closes_it`.

## Runs

`target/quick/engine.exe` (worktree's own target; `quick/engine.d` names this worktree).

Short tour: `--assets C:/Modding/SkyrimConverted --demo riverwood --demo-tour local/tour-236
--tour-doors 2` -> `tour PASSED after 2 crossings (short tour)`; the route's first door is the flaky
one, `0001CBB0`, and both crossings are logged in `local/tour-236/tour.txt`.

Cold reproduction (research-229's `local/t229/cold.json`, 6 of the 8 asked for; logs in
`local/cold-236-*.log`):

| run | "loaded a second time" | "animation player was replaced" |
|---|---|---|
| 1 | 0 | 0 |
| 2 | 0 | 0 |
| 3 | 0 | 0 |
| 4 | 0 | 0 |
| 5 | 0 | 0 |
| 6 | 0 | **1** - door `0003CF65`, `state=Closed restore=None`, 2 frames after "load door's own swing scaled up" |

Run 6 is the interesting one and is **not explained by the diagnostic**: a scene of that door was
re-instanced with no `Gltf` `Modified`, i.e. the root asset was not in `Assets<Gltf>` at all when
something loaded the file again - a *fresh* load (`Added` for the `Gltf`, `Modified` for the still
alive `Scene`). That is the `MODEL_WAIT_FRAMES` fallback: a model whose root nobody holds (only its
scene is alive) is loaded again 2 s after the door first looks, and that load re-instances the live
scene. The door was closed, so nothing was left wrong; an *open* door in that position would lose
its player again, which fix 2 then repairs.

## Left undone / next

- 8 cold runs: 6 run. `local/cold-236-*.log` are all kept.
- The diagnostic covers `AssetEvent::<Gltf>::Modified` only. To attribute run 6's re-instance
  directly, read `AssetEvent::<Scene>::Modified` too (and/or log a `Gltf` `Added` whose path's
  `Scene` is already loaded). Cheap: one system + one 6-run pass.
- The complete fix for the residual case is the one research-229's fix 1 gives: `streaming.rs` holds
  the root `Handle<Gltf>` on the reference and inserts `WorldAssetRoot(gltf.scenes[0])` from it, so
  the root is alive for as long as the reference is. This brief kept that out (`streaming.rs` is
  shared with upstream main). It would also delete the `MODEL_WAIT_FRAMES` fallback here.

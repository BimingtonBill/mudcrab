# impl-153 - The PortalPlugin refactor, steps 1-5 (pure moves, no behaviour change)

Owned files: crates/engine/src/app.rs,crates/engine/src/config.rs,crates/engine/src/lib.rs,crates/engine/src/portal.rs,crates/engine/src/transition.rs,crates/engine/src/streaming.rs,crates/engine/src/demo_tour.rs,crates/engine/src/shots.rs,crates/engine/src/player.rs,crates/engine/src/door_animation.rs,crates/engine/src/model_animation.rs,docs/design/portal-plugin.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user approved this refactor on 2026-09-23.

## Read first

`docs/design/portal-plugin.md`, **all of it**. Its "Decisions" table is the lead's and is binding;
"The map" after it is research-084's survey, which is right in substance but was written against an
older tree: **every line number in it is stale** (the doorway anchor, impl-103/152, has landed since).
Find things by name. Also `docs/pr/two-tracks.md` ("Keeping the code apart on `portal`").

## Why

The `portal` branch merges from upstream `main` again and again. Today
`git diff main portal -- crates/engine/src/{app,config,lib}.rs` is about 2,190 added lines, spread
through upstream's own functions, so every upstream change to those files is a conflict. The goal is
that the portal feature is added through **one** `add_plugins` call, its CLI flags live in **one**
block, and nothing the game does changes.

## What to do - one commit per step, in this order

Step 0 (the baseline) is done: schema 19 is on disk and both tours pass on the current tree
(Riverwood 8/8, Alftand 4/4).

1. **Move the demo objective** systems out of `app.rs` into `demo_tour.rs`, registered by
   `DemoTourPlugin`. Pure move.
2. **Move `TransitionPlugin` into `PortalPlugin`** and drop its `add_plugins` line from
   `StreamingPlugin` (`streaming.rs`), keeping the `.after(DoorTransition)` ordering edges (decision
   H2: an edge onto an empty set is not an error in bevy_ecs 0.19; verify by running, not only by
   reading). It must still be added for every run that has runtime data, as today, so benchmark runs
   keep the same schedule.
3. **Fold the rest into `PortalPlugin`**, one plugin at a time, as the map's "The plugin boundary"
   lists: `DoorAnimationPlugin`, `ModelAnimationPlugin` (interactive runs only, as today),
   `PlayerPlugin` with the fly/player switch (decision H4: keep the two-line `fly_camera` split),
   then `DemoTourPlugin` + `ShotsPlugin`. The shots window-size block stays in `run()` (H3). At the
   end `run()` adds the portal with exactly **one** `app.add_plugins(PortalPlugin)`.
4. **Config grouping** as the decision table says: a `PortalOptions` struct with
   `parse_flag(&mut EngineConfig, ..) -> bool`, called before upstream's `match` in `from_args`, so
   `config.rs` differs from `main` by one contiguous block plus two lines. `--terrain-radius` stays
   **outside** it (terrain, not doors). `--demo` must still write the flat `worldspace_id` /
   `start_grid` / `start_position` fields the streamer reads. Update every reader
   (`config.walk` -> `config.portal.walk` etc.).
5. **The residue**, as decided: H5 a `StartPose` resource seam instead of a portal branch in
   `setup_world`; H1 `update_atmosphere` takes `Option<Res<ActiveCell>>` instead of a `run_if`;
   H7 `interactive` computed from `EngineConfig`, with `LightsPlugin` keeping exactly today's gate;
   H6 **leave `converter_schema_version()` alone** (the Phase 2 track owns that change).

**Not in this task:** step 6 (moving the atmosphere and the terrain ring out of `app.rs` into
`atmosphere.rs` / `terrain_ring.rs`). It is the next task, after this one lands. Do not start it.
Also leave H10 (messages registered in several plugins) as it is: registering a message twice is
harmless, and the standalone plugin tests rely on it.

Rules that hold for every step:

- **No behaviour change.** Same systems, same ordering, same run conditions, same log lines. If a
  move would change any of these, stop and say so in the report instead of improvising.
- **Never put Phase-2-shaped code inside `PortalPlugin`**: lighting, atmosphere, sky and fog,
  `LightsPlugin`, Hdr/Bloom, sun shadow cascades, the terrain ring, snow, the material fixture,
  `converter_schema_version`, the streaming plan. The map's "Must not contain" list is binding.
- `crates/engine/src/main.rs` must stay **byte-identical** to `main` (it is today).
- Every command on an entity that may have been despawned uses the `try_` forms. You should not need
  new commands at all in a pure move.

## Checks

After **each** step: `cargo test -p engine --lib`, clippy and fmt as above. After steps 2, 3 and 4,
also run both tours on a release build (you may build and run the engine; the assets are read-only):

```
cargo build --release -p engine
target/release/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t153-riverwood
target/release/engine.exe --assets C:\Modding\SkyrimConverted --demo alftand --walk --demo-tour local/t153-alftand
```

Both must end `tour PASSED` (Riverwood after 8 crossings, Alftand after 4) with no `panicked` in
the output. A failure belongs to the step that caused it; fix it there.

At the end, report `git diff --stat main -- crates/engine/src/main.rs crates/engine/src/config.rs crates/engine/src/app.rs crates/engine/src/lib.rs`
and paste `run()`'s portal line and `from_args`'s two portal lines. Update the Status line of
`docs/design/portal-plugin.md` to say which steps are done.

## Report

The usual result JSON: per step, what moved and the checks run; the tour lines; the diff stat; any
place where a pure move was not possible and what you did instead.

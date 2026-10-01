# Goal
Track: portal. Give the portal branch upstream main's sky dome, coloured per camera from that
camera's own space, without main's `SkyPlugin` clear-colour and fog systems (they would fight the
portal's `atmosphere.rs` and the portal camera's destination atmosphere).

# Context
- Your checkout is branch `portal-merge-main` (portal + current upstream main). Commit prefix
  `[deepseek]`.
- `crates/engine/src/sky.rs` is main's file, compiled but not wired: dome mesh (`dome_mesh`),
  `SkyMaterial`/`SkyUniform` + `shaders/sky.wgsl`, the `DOME_MASKS` weight table, `spawn_sky_domes` /
  `follow_sky_cameras` (one dome per `SkyCamera`), `SkyPalette`, `CameraSpace`, `apply_sky`,
  `apply_fog`. The main camera already carries `SkyCamera` and `FogCamera` (`app.rs` ~2066).
- The portal's atmosphere: `crates/engine/src/atmosphere.rs` builds a `SpaceAtmosphere` per space
  (~353-365: ambient, backdrop, fog, sun, `has_sky`) from `world/lighting.rs` rows, which also carry
  the weather's sky colours `sky_upper`, `sky_fog`, `sky_lower` (lighting.rs ~179-181) that
  `SpaceAtmosphere` does not keep yet. `update_atmosphere` writes clear colour/fog/ambient/sun for the
  main camera from `ActiveCell`; `portal.rs` (`update_destination_atmosphere`, ~2509-2570) writes the
  destination space's atmosphere onto the portal camera, which renders only
  `crate::portal::DESTINATION_LAYER` (2) (portal camera spawn ~2287-2320).
- Two research reports agree on the design; read both (appended paths are readable):
  `C:/Modding/OpenSkyrimProject/local/merge/port-prep-sky.md` and the sky half of
  `C:/Modding/OpenSkyrimProject/local/merge/research-274-report.md`.

# Design (fixed)
1. Carry `sky_upper`, `sky_fog`, `sky_lower` into `SpaceAtmosphere` (fallback to
   `SkyPalette::SKYRIM_CLEAR_DAY`'s colours where a row has none).
2. A dome per sky camera: the main camera's dome on the main camera's layers ([0, 1]); the portal
   camera gets its own dome on `DESTINATION_LAYER` only, following the portal camera. One entity
   must never be on both, or the main view's dome shows through the doorway.
3. Each dome's palette comes from its own camera's space: the main camera's from `ActiveCell`, the
   portal camera's from the destination it is drawing. The dome is hidden when that space has no sky
   (`has_sky` false: interiors, underground).
4. Do not add `apply_sky`, `apply_fog`, the global `CameraSpace`/`SkyPalette` resources, or any
   other writer of clear colour, fog, ambient or `DirectionalLight`.
5. Keep main's dome brightness constant as is; calibration is a later visual step (say in the report
   what the constant is and where).
6. Wiring: expose one `pub struct SkyDomePlugin` from `sky.rs` (or a new `sky_dome.rs` if cleaner;
   say why). The lead adds it to `app.rs`; do not edit `app.rs`. Leave main's `SkyPlugin` code in
   place but unused, unless removing it is clearly simpler; say which.

# Scope
Owned files: crates/engine/src/sky.rs, crates/engine/src/sky_dome.rs, crates/engine/src/atmosphere.rs, crates/engine/src/portal.rs, crates/engine/src/lib.rs, docs/handoff/impl-277-portal-sky-dome.md
`portal.rs`: only the portal camera's spawn (a marker) and whatever the destination palette needs,
nothing else. `lib.rs`: only a `mod` line if you add a module. Everything else read-only. Tests only,
no engine window or GPU. The handoff note is written but not committed.

Acceptance: cargo fmt --all -- --check, cargo clippy -p engine --all-targets -- -D warnings, cargo nextest run -p engine

# Done when
- Acceptance passes. Keep green: atmosphere.rs `the_atmosphere_system_writes_the_space_onto_the_camera`,
  `the_crossing_frame_already_wears_the_destination_atmosphere`, the backdrop/fog tests, portal.rs
  destination-sun tests. Port main's dome/mask tests from `sky.rs`; drop only tests of systems you
  removed.
- New tests: the main camera and the portal camera each get one dome, on disjoint layers; a camera in
  a no-sky space has its dome hidden; the portal camera's dome takes the destination's colours, not
  the active space's; no system in the plugin writes `ClearColor`, `DistanceFog`, ambient light or a
  `DirectionalLight`.

# Report
Files, types, systems and their schedule placement; the tests; the brightness constant; whether the
water-reflection camera sees a dome and why; anything in the research you found wrong.

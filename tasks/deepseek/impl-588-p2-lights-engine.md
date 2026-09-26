# Goal
Track: Phase 2 (for upstream; the maintainers have said yes to lighting). The engine places a Bevy
point light under every streamed reference whose base record is a Skyrim light (`LIGH`), keeps only
the 64 nearest enabled, and does all of it only when the new `--lights` flag is given (default off,
so upstream's timing baselines do not move).

# Context
- Your checkout is cut from `phase2/lights-converter` (upstream main + the converter PR that
  publishes lights). Upstream commit style (`feat(engine): ...`), no `[deepseek]` prefix, and no
  project-internal references in code, comments or docs: no `local/...`, `research-...`, `impl-...`,
  `portal`, UESP screenshot names or reference-shot names.
- The converter side is done on this branch: world database schema 4 has a `lights` table (one
  row per `LIGH` record: radius, colour, flags, falloff exponent, fade) and a nullable
  `references.radius_override` column (the reference's `XRDS` radius). Read
  `docs/specs/converters/db-schema.md` and `crates/converter/src/esm/exporter.rs` on this branch
  for the exact columns.
- A private fork already has a working implementation. Copies of its files are in the read-only
  folder `C:/Modding/shared-check-phase2/_portal-lights-ref/`:
  `portal-lights.rs` (the module), `portal-world-database.rs` (LightRow, the ReferenceRow light
  fields, LIGHT_COLUMNS/LIGHT_JOIN, has_lights/has_radius_override probes, map_reference, three
  tests), `portal-streaming.rs` (the spawn block, search for `SkyrimLight`/`point_light`, and its
  tests), `portal-app.rs`, `portal-config.rs`. And `research-571-lights-cut.md`: an inventory of
  exactly which parts to take and which to leave out, with line numbers in those files. Read the
  research file first; it is the plan. Port from the fork files, adapting to this branch's code
  (the fork's code base has diverged; do not copy whole files).
- Leave OUT (fork-only): anything about portals, destination spaces, `RenderLayers` filtering in
  the budget, `ACTIVE_SPACE_LIGHT_SHARE`/`DESTINATION_LIGHT_SHARE` (the budget is simply the 64
  nearest), `crate::atmosphere`, the Skyrim falloff patch (`light_falloff.rs`, `SKYRIM_FALLOFF`,
  `skyrim_intensity_for_radius`, `affects_lightmapped_mesh_diffuse` changes), and everything glow
  (`EMISSIVE_EXPOSURE`, bloom, emissive helpers) with the tests that use them.
- Keep: `point_light`, `radius_of` (XRDS override), the flag skips (negative, off-by-default),
  `intensity_for_radius` with the `INTENSITY_REFERENCE_RADIUS = 256` cap, `HALF_RADIUS_WINDOW`,
  `ENABLED_LIGHT_BUDGET = 64`, `BUDGET_RECHOOSE_DISTANCE`, the budget's re-choose when the set of
  lights changes (cells streaming in/out), `SkyrimLight`, `LightsPlugin`; lights spawned as children
  of their reference (so they follow render-origin rebases and cell unloads), with
  `Visibility::default()` on a reference that has no model (without it Bevy warns B0004 and the
  light never renders).

# Brightness: the decided default (do not fit anything)
The maintainers get a simple, documented default now; tuning is a later PR.
- `AMBIENT_ILLUMINANCE: f32 = 160.0`: the ambient brightness this engine applies in its world path
  (find the `GlobalAmbientLight { brightness: 160.0, .. }` insert in `crates/engine/src/app.rs` and
  reference it by name in the doc comment; if you can make `app.rs` use the same constant without
  touching unrelated code, do).
- `LIGHT_EXPOSURE: f32 = 10.0`, documented as a round default: "at half its radius a light delivers
  ten times the ambient illuminance, an order of magnitude, so a lamp's pool reads clearly against
  the room; not fitted to any image, to be tuned later".
- `HALF_RADIUS_ILLUMINANCE = 4π² · AMBIENT_ILLUMINANCE · LIGHT_EXPOSURE` and the intensity formula
  exactly as the fork derives them (keep the fork's derivation of the 4π² and of
  `HALF_RADIUS_WINDOW` in the doc comments, rewritten without fork references).
- Write the derivation, the budget rule and the flag into a short spec,
  `docs/specs/engine/point-lights.md` (look at how other docs under `docs/specs/` are written and
  match them): what data is read, how a row becomes a light, the brightness derivation with the
  numbers, the 256 cap and why, the 64-light budget and when it re-chooses, `--lights`, and what is
  left for later (tuning, Skyrim's falloff curve, fade/flicker, shadows).

# The flag
`--lights` in `crates/engine/src/config.rs` (a `pub lights: bool`, default false, parsed like the
other boolean flags, with a parse test). `LightsPlugin` is always added (so tests and the spawn code
are uniform), but lights are spawned only when `config.lights` is true. Add the flag to the usage
text if `config.rs` has one.

# Scope
Owned files: new `crates/engine/src/lights.rs`, `crates/engine/src/lib.rs`,
`crates/engine/src/world/database.rs`, `crates/engine/src/streaming.rs`,
`crates/engine/src/app.rs` (registration and the ambient constant only),
`crates/engine/src/config.rs` (the flag only), new `docs/specs/engine/point-lights.md`, and
`docs/handoff/impl-588-p2-lights-engine.md`. If you need another file, say so in the report
instead of editing it.

# Done when
- Tests (pure, no game data): the intensity derivation and half-radius identity, the inverse-square
  law between radii, the 256-unit cap, the flag skips, the XRDS override, the budget keeps the 64
  nearest and re-chooses when the set changes; the database tests (an in-memory DB with a
  hand-written `lights` table: a lit reference gets its LightRow and override, an unlit one gets
  None, a database without a `lights` table still loads); a spawn test from a constructed
  reference row (light spawned as a child with `SkyrimLight`, only when `--lights` is on); the
  flag parse test.
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy --workspace --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo test -p engine -p shared --target-dir C:/Modding/shared-check-phase2`
- No fork-only names or references remain: grep your diff for `portal`, `atmosphere`,
  `DESTINATION`, `falloff` (except the "left for later" line in the spec), `EMISSIVE`, `local/`,
  `research-`, `impl-`.

# Report
Diff summary per file; the constants and their values; every test with its result; what you left
out and anything from the research inventory you could not port and why; open questions for a
reviewer.

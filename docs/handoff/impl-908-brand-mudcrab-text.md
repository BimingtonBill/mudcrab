# impl-908-brand-mudcrab-text (Claude impl #908)

**Branch:** `brand/mudcrab-text` in `C:\Modding\wkj-brand` (from main 6f583cd). **Commit:** c06b530.
Not pushed.

**Update 2026-09-29:** the coordinator merged upstream main 56f2431 (#27, #29, #30, #31, #50) into the
branch as 4fd46e1 (clean). I re-ran the pass on the merged tree: those PRs add **no** new project-name text.
The one new match is a data-format name (`OPEN_SKYRIM_material` in `docs/specs/converters/nif-to-gltf.md:88`,
kept, group 2), and every earlier rename survived the merge. That left nothing to commit, so the head stays
**4fd46e1**. Checks on 4fd46e1 (D: target): fmt clean, clippy -D warnings clean, nextest 287 passed / 13 skipped.
Line numbers below are from c06b530; the merge moved some (e.g. mesh.rs generator now :521).

## What changed (30 files)

"OpenSkyrim" / "OPENSKYRIM" as the project's name -> "Mudcrab" / "MUDCRAB" in visible text only:

- Engine: window title `Mudcrab`, the log line `Mudcrab runtime initialized` (`crates/engine/src/app.rs`).
- Launcher: window title `Mudcrab Launcher & Mod Manager`, header `MUDCRAB`, button `PLAY MUDCRAB`,
  the stdout line and status text on engine start (`main.rs`, `ui.rs`, `handlers.rs`).
- Converter crate doc comment (`crates/converter/src/lib.rs`).
- Prose in `CONTRIBUTING.md`, `docs/**` (specs, roadmap, ADR index) and `.github/ISSUE_TEMPLATE/*`
  (01, 02 incl. the "i cannot use mudcrab without it" dropdown option, 04, config.yml link name).
- ASCII boxes and padded markdown tables re-padded by 3 spaces so their borders still line up.

No test asserted a changed string. Checks (D: target): fmt clean, clippy -D warnings clean, nextest 257 passed / 13 skipped.

## Kept matches, grouped by reason

1. **Environment variables** (`OPENSKYRIM_SKYRIM_DATA`, `OPENSKYRIM_*_FIXTURE`): converter
   `archive/bsa.rs`, `mesh.rs`, `texture.rs`, `tests/fixture_round_trip.rs`; docs `adr/0002`,
   `specs/converters/dds-to-ktx2.md`, `specs/converters/dummy-content.md`.
2. **Data-format names in converted files**: `OPEN_SKYRIM_material` (converter `material.rs`, `mesh.rs`
   incl. the doc comment at mesh.rs:205 naming the extension; docs `nif-to-gltf.md`,
   `roadmap/02-visual-fidelity-implementation-plan.md`; since the merge also `nif-to-gltf.md:88`), the `openSkyrim` glTF extras key
   (`material.rs`, `mesh.rs` tests, `engine/src/streaming.rs:1211` reads it). Not matched by the grep
   but also untouched: `.opensky-srgb.ktx2`, the `opensky_*` C symbols and build.rs lib name.
3. **Follow-up if the maintainer wants output metadata renamed** (text written INTO converted or
   generated files; kept so this PR changes no output bytes, per Phase 1 Dev):
   - `crates/converter/src/mesh.rs:465` GLB `asset.generator` "OpenSkyrim converter"
   - `crates/converter/src/script.rs:245` Luau header "-- Generated from {} by OpenSkyrim"
   - `crates/converter/src/material.rs:255` material name "OpenSkyrim non-rendering excluded geometry"
   - fixture author "OpenSkyrim dummy-content": `crates/dummy-content/src/layout.rs:19`
     (`GENERATED_AUTHOR`) and its copies in `dummy-content/src/nif.rs:68`,
     `converter/src/esm/binary.rs:311`, `converter/tests/fixture_round_trip.rs:274`,
     `dummy-content/benches/writers.rs:66`, `dummy-content/tests/performance.rs:98`
     (all six change together).
4. **Code names, file/folder, temp and thread names**: `engine/src/app.rs:214`
   (`openskyrim-streaming-*` temp), `engine/src/world/database.rs:151` (thread `openskyrim-world-db`),
   `launcher/src/game_detection.rs:195,217` (test temp dirs), `.gitignore:26` (`openskyrim.cfg`),
   docs naming `openskyrim.d.lua`, `openskyrim-types`, `openskyrim_scripting`, `openskyrim_converter`
   (`roadmap/03-luau-and-ui.md`, `specs/converters/pex-to-lua.md`, `specs/modding/mods-and-ui.md:128`),
   folder trees `OpenSkyrim/` (`specs/engine/architecture.md:43` repo root,
   `specs/modding/mods-and-ui.md:65,68` user storage folder).
5. **URLs, repository names, addresses**: `.github/ISSUE_TEMPLATE/config.yml:4,7`,
   `CONTRIBUTING.md:48-49` (clone URL and the `cd OpenSkyrim` that follows it),
   `docs/adr/README.md:19`, `docs/specs/converters/dummy-content.md:7`,
   `docs/specs/converters/db-schema.md:3` (a file:// link), `CODE_OF_CONDUCT.md:63`
   (`openskyrimdev@gmail.com`, a real address).
6. **Licence / authorship (maintainer's legal choice)**: `LICENSE-MIT:3` "Copyright (c) 2026
   OpenSkyrim Contributors"; `Cargo.toml:14` `authors = ["OpenSkyrim Team"]` (an attribution field,
   kept alongside the licence; change both together if the maintainer wants).
7. **Vendored code**: `vendor/project-wormhole-nif/src/lib.rs` (1 match).

## Not in scope, worth a decision

`README.md` (title "# Wah Krah Jol" and prose), `LEGAL.md` and `.github/FUNDING.yml` (`ko_fi:
wahkrahjol`) name the project **Wah Krah Jol**, not OpenSkyrim, so the brief's grep did not touch them.
A Mudcrab PR probably wants the README/LEGAL prose renamed too (keep the `wah-krah-jol` URLs and the
Ko-fi username).

# Goal
Track: Phase 2 (for upstream). The dummy-content fixture generator can write a Skyrim light (`LIGH`)
base record and a placed reference to it carrying a radius override (`XRDS`), behind a new opt-in
preset, so the point-lights work can be tested with no game installed.

# Context
- Your checkout is cut from `main`. Use upstream's commit style (`test(dummy-content): ...`), no
  `[deepseek]` prefix. Read `CONTRIBUTING.md` and `docs/specs/converters/dummy-content.md` first.
- The generator: `crates/dummy-content/src/esm.rs` (record writers), `src/main.rs` (CLI presets),
  `src/layout.rs`. It writes TES4/WRLD/CELL/LAND/REFR/STAT/DOOR/TXST/LTEX today, no `LIGH`.
- Record layout the converter will read (from this project's portal branch,
  `git show portal:crates/converter/src/esm/exporter.rs`, lines ~660-728 and ~763-781):
  - `LIGH` subrecords: `EDID`; optional `MODL`; `DATA` at least 20 bytes: bytes 0..4 time (i32),
    4..8 radius (u32), 8..11 colour RGB (u8 each, byte 11 unused), 12..16 flags (u32), 16..20
    falloff exponent (f32); real records are 48 bytes. `FNAM` 4 bytes: fade (f32).
  - A `REFR` of a light carries `XRDS`: a 4-byte f32 radius.
- Existing outputs must not change: the current presets' bytes stay identical
  (`output_is_deterministic`, `exterior_only_output_is_unchanged`,
  `an_interior_is_appended_after_the_exterior_world` must stay green). So add a new preset
  (e.g. `--with-lights`) rather than changing an existing one.
- Pattern for a test that reads the generated plugin back: `crates/converter/tests/fixture_doors.rs`
  (parses with `converter::esm::binary::parse_plugin_file`).

# Scope
Owned files: `crates/dummy-content/src/esm.rs`, `src/main.rs`, `src/layout.rs` (only if needed),
new `crates/converter/tests/fixture_lights.rs`, `docs/specs/converters/dummy-content.md`, and your
handoff note `docs/handoff/impl-572-p2-lights-fixture.md`. No converter source changes, no schema
change, no new dependencies. No machine-specific paths anywhere.

# Done when
- `cargo run -p dummy-content -- <out> --with-lights` (or your preset name) writes a plugin with one
  `LIGH` (with `DATA` 48 bytes and `FNAM`) and one `REFR` to it with `XRDS`.
- A new test in `crates/converter/tests/fixture_lights.rs` parses that plugin and asserts the
  `LIGH` `DATA`/`FNAM` bytes and the reference's `XRDS` value arrive intact.
- `cargo fmt --all -- --check`, `cargo clippy -p dummy-content -p converter --all-targets -- -D warnings`,
  `cargo test -p dummy-content -p converter` pass.

# Report
What you added (preset name, record values), the test names, and the command output of the checks.

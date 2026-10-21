# Goal
Track: Phase 2 (for upstream). When the engine refuses a converted asset set, it says exactly
which check failed, what it found, what it expected, and the command that fixes it. Today one
combined message covers three different situations.

# Context
- Your checkout is cut from upstream `main`. Upstream commit style (`feat(engine): ...` or
  `fix(engine): ...`), no `[deepseek]` prefix, no project-internal references in code or commits.
- `crates/engine/src/app.rs`, `validate_runtime_assets` (~line 1179) and `converter_schema_version`
  (~1215). Today:
  `manifest.schema_version == converter_schema_version() && manifest.complete` fails with
  "asset conversion is incomplete or stale; reconvert assets with converter schema {}", whether the
  schema is old, the schema is newer than the engine, or the conversion is incomplete. The
  integration-report check likewise joins "world database schema differs" and "report did not pass".
- Four cases need their own message:
  1. converter schema older than the engine expects: say both numbers and give the reconvert
     command (the converter reuses unchanged work, so say it is usually quick);
  2. converter schema newer than the engine: the engine is older than the data; update/rebuild the
     engine (say both numbers);
  3. `complete: false`: the conversion did not finish or skipped inputs; say to rerun the converter
     (the same command) or start with `--allow-incomplete-assets` (an existing engine option; confirm
     its exact spelling in the CLI code) to use what is there;
  4. integration report: world database schema mismatch (both numbers) vs `passed: false` (point at
     the report file).
- The reconvert command: find the converter's real command line in `crates/converter/src/main.rs`
  and `config.rs` (argument names, the `OPENSKYRIM_SKYRIM_DATA` / `OPENSKYRIM_CONVERTED_DIR`
  environment variables if they exist) and in README.md / docs. Print it with the real assets
  directory the engine was given, and a placeholder only for what the engine cannot know (the
  Skyrim Data folder), e.g. `cargo run --release -p converter -- <args as the CLI defines them>`.
  Do not invent argument names: copy them from the code.
- Keep one small helper that builds the message, so tests can check it without a filesystem.

# Scope
Owned files: `crates/engine/src/app.rs` (only `validate_runtime_assets`, a new message helper next
to it, and the tests module's cases for it), `docs/handoff/impl-617-p2-schema-mismatch-error.md`.
Nothing else. Do not change `converter_schema_version()`'s value.

# Done when
- Tests cover all four cases plus the passing case, each asserting the key parts of the message
  (both numbers, the command or option named).
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets -- -D warnings`
  `cargo nextest run -p engine`
- One commit, upstream style.

# Report
The four messages exactly as printed (for an example assets path), the test names, and where the
converter's argument names came from (file:line).

# impl-827 (p1) output check - handoff

Track: Phase 1. Branch `phase1/output-check` from main 6f583cd, worktree
`D:/Modding-overflow/wt/p1-check`, target `D:/Modding-overflow/shared-check-target/impl-827`.
Done by a Claude subagent in place of the DeepSeek worker.

## Change
- `crates/converter/src/check.rs` (new): `CheckMode { Quick, Full }`, `CheckProblem`, `CheckReport`
  (`is_ok`, `advice`), `check_output(output, mode, progress)`. Quick = metadata (exists + size);
  Full = also `hash_file` vs `output_hash`, rayon over the global pool. Paths that are empty,
  absolute, contain `:` or a `..` segment (either separator) are `UnsafePath` and never touched.
- `lib.rs`: `pub mod check` + re-exports.
- `main.rs`: `converter check <output> [--full]` as a first-argument subcommand; exit 0/1/2;
  up to 20 problem lines then `and N more`, then one advice line; a throttled progress line on
  stderr only after 1 s, erased before the result. `usage()` lists it.
- `docs/specs/converters/pipeline.md`: "Checking a converted output" at the end of section 4.
- Tests: `crates/converter/tests/output_check.rs` (8 tests on the converted `dummy-content`
  default tree), unit tests in `check.rs` (path containment, advice) and `main.rs` (parse, format).

## Decisions worth a look
- The manifest is read with `serde_json` into `ConversionManifest` directly, not with
  `ConversionManifest::load`: `load` turns a missing file into an empty manifest and replaces a
  manifest of another schema with an empty one (so the schema mismatch would be invisible). No
  change to `cache.rs` was needed.
- `skyrim_world.db` missing is always a problem, although the pipeline builds it only when there
  are plugins (a Data folder without plugins would always report it). Real Skyrim always has
  plugins.
- Symlinks are not special-cased (lexical containment only, as the brief asks).
- `converter check ...` makes a Data folder literally named `check` need `./check`.

## Reconversion repair
True. `pipeline.rs:535-557` (main 6f583cd): a cache hit needs the published file at
`output_dir.join(entry.output)` to exist, match `output_size` and re-hash to `output_hash`;
otherwise it falls through to a fresh conversion into the new staging folder. The world database
and cell cache are rebuilt on every run with plugins (`pipeline.rs:239-258`). Exception: a manifest
of another schema (other than the 12/13 -> 14 migration) is discarded by `ConversionManifest::load`
(`cache.rs:72-77`), so everything is redone; the advice says so.

## State
Commit b311851. fmt, workspace clippy -D warnings and `nextest -p converter` (143 passed, 8 skipped) green. Manual run on a converted `dummy-content gen` tree: quick/full all good; deleted GLB -> `missing`, exit 1; byte flip -> `wrong hash`; a reconversion then converted 2 and reused the rest, and the full check was all good again. No worker running. Nothing pushed.

# impl-545 — a resumed conversion must never certify stale outputs (Phase 2)

Base commit: `2105bfd468d23bbddd54c74d898bfc11fd798f21` (branch HEAD as handed over;
changes are uncommitted in the worktree).

## The bug

`ConversionBatch::convert_kind` (per-file closure) accepted any non-empty file already
present in the staging directory as "already converted": a GLB or Luau was never checked
at all, a KTX2 only for its container format. A staging directory outlives the run that
filled it, so `--resume-staging` could publish an output produced from a different source,
an older schema or another configuration, and report `complete: true`. Worse, the accepted
file was then written into the published manifest with the *current* source hash, so the
stale bytes were certified twice: staged reuse and later published reuse.
`--invalidate-cache` did not turn the staged check off.

## Design

- **What is recorded**: per output a `StagedOutput { schema_version, configuration_hash,
  source_hash, output_size, output_hash }` (`crates/converter/src/cache.rs`), keyed by the
  same canonical source key the published manifest uses (`scripts/one.pex`, ...).
- **Where**: an append-only journal `staging/.conversion-staging-journal.jsonl`
  (`STAGING_JOURNAL_FILE`), one JSON line per output, written from the single collector
  task as each output becomes valid — after a fresh conversion, after a copy from the
  published output, and after a reused staged output (whose record is thus refreshed from
  the bytes on disk). A line reaches the file in one write, so a run killed mid-append
  loses at most the last record and the output it describes is converted again.
- **What is compared** on resume (`StagedOutput::is_current`): the record's `source_hash`
  against the hash the converter computed for the current source (including the
  texture-encoding suffix and NIF dependency hashes), `schema_version` against
  `CONVERTER_SCHEMA_VERSION`, `configuration_hash` against the current configuration, and
  the staged file's size and `hash_file` against the record. Only then is the file reused;
  anything else — including a missing or unreadable record — is converted again.
- **Invalidation**: with `invalidate_cache` no records are loaded, so every output is
  converted again.
- **Published tree**: after the rename, the journal is removed from the output directory
  (non-fatal, warned on stderr). It is removed *after* the rename so a refused publish
  leaves the staging directory — and its records — resumable.
- The published-output reuse path (manifest entry `output_size`/`output_hash` + source
  hash) is untouched.

## Tests (`crates/converter/tests/resume_staging.rs`, synthetic inputs only)

Each test builds a staging directory by hand: a marker file the converters never emit
(valid Luau, `-- staged output…`) stands in for a stale output, with a journal record that
is correct except for the property under test. Reuse shows as the marker surviving;
reconversion as the bytes a fresh run produces.

| Test | Fails on old code |
| --- | --- |
| `reconverts_a_staged_output_when_its_source_changed` | yes — marker certified |
| `reconverts_a_staged_output_written_under_an_older_schema` | yes — marker certified |
| `reconverts_a_staged_output_written_under_a_different_configuration` | yes — marker certified |
| `reconverts_a_damaged_staged_output` (truncated file, record intact) | yes — marker certified |
| `ignores_staged_outputs_when_the_cache_is_invalidated` (record fully valid) | yes — marker certified |
| `reuses_a_staged_output_whose_provenance_is_current` (happy path) | passes (by design) |
| `does_not_publish_the_staging_journal` | passes (new behaviour) |
| `records_provenance_for_every_output_it_stages` (authentic: a run whose publish is refused by a stale backup, then the journal is read back) | passes (new behaviour) |

"Fails on old code" was measured by temporarily restoring the old `existing_is_valid`
expression in `pipeline.rs` and re-running the file: 5 failed / 3 passed, exactly as
listed, then the change was reverted. `crates/converter/src/cache.rs` also gained
`journal_keeps_the_last_record_and_drops_a_truncated_tail`.

## Commands

- `cargo fmt --all -- --check` → clean.
- `cargo clippy -p converter --all-targets -- -D warnings` → clean (no warnings).
- `cargo test -p converter` → lib 102 passed / 0 failed / 7 ignored; bins all ok;
  `fixture_round_trip` 7 passed / 1 ignored; `resume_staging` 8 passed / 0 failed;
  doc-tests 0. No failures.

## Files

- `crates/converter/src/cache.rs` — journal, record type, `load_staged_outputs`, test.
- `crates/converter/src/pipeline.rs` — record loading, staged validation, record writes,
  journal removal after publish.
- `crates/converter/tests/resume_staging.rs` — new regression tests.
- `crates/converter/src/main.rs` — unchanged: the flags already reach `PipelineConfig`.

## Open items / notes for the reviewer

- Skipping the record costs a full `hash_file` of every staged output on resume, the same
  price the published-output path already pays. It is what makes stale bytes detectable.
- Records of keys not converted again in a run stay in the journal (append-only). They are
  re-validated on the next resume, so they cannot certify anything, and a stale record that
  does match source, schema, configuration and bytes describes exactly what would be
  produced anyway.
- Not journaled, because they are rewritten in full on every run: the SRGB texture aliases
  (`publish_srgb_texture_aliases`) and `scripts/papyrus_runtime.luau`.
- The journal is not part of `report.artifacts`, and `--resume-staging` on a staging
  directory whose journal is missing (e.g. left by a build predating this change) simply
  converts every output again — correct, just slower.

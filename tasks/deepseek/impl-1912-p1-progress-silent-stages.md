# Goal
Phase 1 (converter) track. Stop the conversion pipeline going silent: the Database, Validating and Publishing stages (and the steps around them) send one event and then work for minutes with no further progress, so a window or log looks idle. Make each of them report real progress or at least a distinct named step as it runs. Why: on 2026-10-07 the user closed a conversion console that looked idle. The renderer side (heartbeat line, a line that says what it is doing) is a separate task, impl-1911, running at the same time on `progress.rs` and `main.rs`; do not touch those files.

# Context
Base: `origin/main` (the worktree is made from it). Files in `crates/converter/src/`.
- `pipeline.rs`: `AssetPipeline::run_into` runs the stages and sends `ProgressEvent`s through `progress_tx: &Sender<ProgressEvent>` using the helper `send(progress_tx, stage, completed, total, current_file, message).await` (and `send_notice`). Find these spots:
  - Database: `send(... ProgressStage::Database, 0, plugins.len() as u64, None, "Building skyrim_world.db")` then, with no further event, `EsmParser::convert_plugins_with_records(&plugins, &db_path)`, `validate_database(...)` and `write_cell_cache(...)`. The parse is the long part.
  - After the textures, meshes and scripts and the LOD chunks: `finalize_world_database(staging)` (in `integration.rs`, not owned: only send an event before it) and the write of `papyrus_runtime.luau`.
  - Validating: `send(... Validating, 0, artifacts.len(), ..., "Validating generated artifacts")`, then `validate_artifacts(staging, &report.artifacts, &texture_semantics, config.cpu_jobs)` (a rayon pool, synchronous, hashes about 18 GB on a full run), then one closing event.
  - Publishing: in `run_async_with_cancel` (the function that calls `run_into`): one `send(... Publishing, 1, 1, None, "Publishing converted assets")`, then `publish_runtime_pack`, `persist_ingestion_cache`, `ConversionManifest::load`, `prune_stale_ingestion_blobs` and `remove_staging`, all silent.
- `esm/mod.rs`: `EsmParser::convert_plugins_with_records(plugins, db_path)` (around line 34) does the plugin parse and the database write.
- `run_collecting_progress` and `run_without_progress` in `pipeline.rs`'s `mod tests` are the test helpers that run a small conversion and collect the events.
- Existing rule: progress sends must never make a run wait for a front end that stopped reading (see the test `extraction_does_not_wait_for_a_front_end_that_stopped_reading`); keep using the existing `send` helper, which handles that, and never `blocking_send` from async code (it panics inside a runtime). `validate_artifacts` is a synchronous function called from async code; one way to report from it without changing its semantics is a shared `AtomicU64` counter bumped per artifact, plus an async side loop that samples it a few times a second while the validation runs on `tokio::task::spawn_blocking` or with `block_in_place`; use whichever the surrounding code already uses for blocking work, and keep cancellation and error behaviour identical (the first failing artifact in list order is still the reported one).
- `ProgressStage` and `ProgressEvent` are public and used by `crates/launcher`: do NOT add, remove or rename an enum variant or a public field. Stage weights stay as they are (LodChunks stays 0.0).

Foundations check for this brief: it fixes the cause (stages that do not report), not the symptom (no capping or hiding of loading); events are cheap and throttled so cost does not grow with the number of artifacts (a full conversion has about 240,000); nothing is hard-coded to Skyrim files; a plugin count or artifact count comes from the data; it changes nothing the converter produces.

# What to build
1. **Database:** report per plugin as it is parsed (completed/total in plugins, `current_file` = the plugin file name, message "Parsing plugin"), by threading a progress callback into `convert_plugins_with_records` (a default or a new parameter; update every caller, including tests). If a per-plugin callback cannot be threaded within `esm/mod.rs`, fall back to distinct named steps, and say so in the report. Also send one event before `validate_database` ("Validating world database") and one before `write_cell_cache` ("Writing cell cache"), and one before `finalize_world_database` ("Finalizing world database").
2. **Validating:** completed/total artifacts and `with_bytes` if the artifact size is cheaply known, at most about 10 events a second whatever the artifact count, ending with the existing closing event. Do not slow the validation (no lock per artifact beyond one atomic add).
3. **Publishing:** replace the single 1/1 event with a numbered sequence of the real steps, `completed` out of the number of steps: publishing the runtime pack, saving the ingestion cache, pruning stale cache blobs, removing the staging folder. Each with its own message. The `Complete` event stays last.
4. Any other silent step you find on the way that runs longer than a second on a full conversion gets a named event too; list each in the report. Do not change the `--reuse-assets` path (`rebuild_metadata_async`); only report whether it is silent.

# Scope
Owned files: crates/converter/src/pipeline.rs, crates/converter/src/esm/mod.rs
- Change only the owned files. A file changed outside the list fails the scope check. `progress.rs`, `main.rs` and `integration.rs` are not yours.
- No new or upgraded dependencies. No git commands.
- Keep existing behaviour and existing tests that still describe real behaviour; update an existing test only where an event count or message deliberately changed, and say which.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --lib, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p converter --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo check -p launcher --all-targets, cargo fmt --all -- --check
- Run them yourself before you stop. `extraction_does_not_wait_for_a_front_end_that_stopped_reading` is a known flaky test: rerun once before calling it a failure.
- New tests (using `run_collecting_progress` on the small fixture the existing tests use), written to catch the change being wrong: (a) the Database stage reports one event per plugin with the plugin name, completed rising to the total; (b) the Validating stage's completed count rises monotonically and ends at the artifact total, and the number of Validating events stays bounded (well under the artifact count for a run with many artifacts: build a test with a few hundred tiny artifacts if the fixture is small, or unit-test the throttle); (c) the Publishing events are in order, with distinct messages, and the last Publishing event has completed == total; (d) a validation failure still reports the first failing artifact in list order.

# Report
- What you changed and the decisions that were not obvious (how Validating reports from the blocking call, the throttle).
- The commands you ran and what they printed (counts of tests passed).
- The full list of events a small fixture run now sends per stage (stage, completed/total, message), copied from a test's output.
- Whether `rebuild_metadata_async` is silent in any stage.
- What you assumed, and the cheapest check that would show the change is wrong.

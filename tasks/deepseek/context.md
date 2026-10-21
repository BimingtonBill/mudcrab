# Context for DeepSeek workers

Project: **Mudcrab**, an open-source reimplementation of the engine for *The Elder Scrolls V: Skyrim Special Edition*, written in Rust (2024 edition) on Bevy 0.19. An offline converter turns Bethesda's files (`.esm`, `.bsa`/`.ba2`, `.nif`, `.dds`, `.pex`) into modern formats (SQLite, glTF 2.0, KTX2, Luau), and the engine streams those at runtime. Read `README.md` and `docs/roadmap/README.md` for the goals and current phase, then the docs relevant to your task.

## Key documents
- `docs/roadmap/*.md` - the five phases. Phase 1 (converter) is done; Phase 2 (core engine, Vercidium renderer) is in its acceptance stage: `02-acceptance.md`, `02-completion-plan.md`, `02-profiling.md`, `02-integration-and-acceptance.md`.
- `docs/specs/converters/*.md` - one spec per converter (`esm-to-sqlite.md`, `nif-to-gltf.md`, `dds-to-ktx2.md`, `pex-to-lua.md`), plus `pipeline.md`, `db-schema.md`, `data-formats.md`.
- `docs/specs/engine/*.md` - `architecture.md`, `vercidium-optimizations.md` (instanced indirect draws, HZB culling), `bevy-examples.md`.
- `docs/specs/meta/`, `docs/specs/modding/` - platforms, requirements, launcher and mod manager.
- `docs/research/*.md` - research notes written by workers and leads. Check here before re-deriving anything.

## The code
- `crates/converter/` - the offline pipeline. `src/esm/` (plugin records to SQLite, the cell cache), `src/archive/` (BSA and BA2 readers), `mesh.rs` (NIF to glTF), `texture.rs` (DDS to KTX2), `material.rs`, `script.rs` (PEX to Luau), `pipeline.rs`, `integration.rs`. Audit and closure tools in `src/bin/`.
- `crates/engine/` - the Bevy game binary: `app.rs`, `streaming.rs` (cell streaming), `render.rs`, `world/` (database, components, cache), `profiling.rs`, `metrics.rs`, `papyrus_runtime.rs`; the `world-inspect` tool in `src/bin/`.
- `crates/shared/` - versioned contracts both sides use: coordinates, the cell cache format, the Luau Papyrus runtime (`papyrus_runtime.luau`).
- `crates/launcher/` - the setup wizard, game detection, and the mod manager UI.
- `vendor/project-wormhole-nif/` - a vendored NIF parser.
- `scripts/phase2-*.ps1` - Phase 2 profiling, visual baselines, acceptance campaigns and sign-off. They write under `target/` (`target/acceptance/<stamp>-<hw>/`, `target/visual-baselines/<stamp>/`).

## Game data
The repository has no Bethesda content, and none may ever be committed (`.gitignore` refuses `.esm`, `.bsa`, `.nif`, `.dds` and others). If the lead has made a Skyrim SE install or converted assets available, they are listed in `.deepseek-agents.json` under `readOnlyDirs` and described here; otherwise you have no game data, and a task that needs it must say so rather than guess at file contents.

- `C:/Users/Brannock/.cargo/registry/src` - the source of every crate the build uses (Bevy, wgpu, mlua, rusqlite, rkyv, basis-universal...) at the exact versions in `Cargo.lock`. Read the real API here instead of recalling it: Bevy changes its API every release, and 0.19 is newer than most of what you remember.
- `C:/Games/Steam/steamapps/common/Skyrim Special Edition` - the user's Steam install of Skyrim SE (Anniversary content included). `Data/` holds `Skyrim.esm`, `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm`, the Creation Club `cc*.esm/.esl`, and the `Skyrim - *.bsa` archives (meshes, textures, sounds, voices, interface, shaders). Read-only: never write, rename or delete anything there. The archives are several GB each - use the converter's own readers or a small script to read a record or a file entry, and never `Read` a whole `.bsa` or `.esm`. Never copy any of it into the repository.
- `C:/Modding/Mudcrab/SkyrimConverted` - the converter's output for that install (complete, integration passed): `skyrim_world.db` (SQLite, 1.3 GB - open it read-only with `sqlite3.connect('file:C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db?mode=ro', uri=True)` in a script under `tools/research/`), `cell_cache.rkyv`, `conversion-manifest.json`, `integration-report.json`, `meshes/**/*.glb`, `textures/**/*.ktx2`. Read-only; the acceptance scripts use it as `-Assets`. Tamriel is worldspace id 60 (`0x3c`).
- Vanilla record data (the user, 2026-10-14): do not guess what Skyrim stores. `python tools/skyrim_rec.py index` lists every record type with coverage; `python tools/skyrim_rec.py sheet <SIG>` (for example `sheet STAT`) prints one page per type: every vanilla field with record counts and value ranges, what the converter reads (with `file:line`) and the schema subrecord; `find <text>` and `show <EditorID|FormID>` print real records from the dump of the five official masters (Dawnguard missing). Run them with `python tools/skyrim_rec.py ...` (the script reads `D:/Mudcrab/skyrim-records` and `D:/Mudcrab/parity` for you). Cite the type and an EditorID or FormID for every claim about vanilla data. A `?` in a sheet means unmatched, not absent. It shows what is stored, not what the engine does with it.

## You can see images
`Read` a `.png` or `.jpg` and you see the picture. `docs/images/` holds rendering-progress shots; visual baselines from `scripts/phase2-visual-baseline.ps1` land in `target/visual-baselines/`. When the question is what a frame looks like, look at it instead of reasoning from a log.

## Building and testing
- `cargo check --workspace` - fast type check. `cargo test -p <crate>` - one crate's tests (`converter`, `engine`, `shared`, `launcher`). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` are what CI enforces (`.github/workflows/run_tests.yml`).
- The first build compiles Bevy and takes many minutes; later builds are incremental. Builds share the lead's `target/` directory; if cargo says it is waiting for a lock, another build is running - wait, do not delete anything.
- Python tooling for the worker harness: `python -m unittest discover -s tests`.
- `python tools/check_citations.py <your note>.md` - checks every `path:line` you wrote against the repository. **Run it on your note before you finish** and fix what it reports. A bare file name is flagged when several files answer to it, because every crate has its own `lib.rs`, `mod.rs` and `Cargo.toml`.

## Your role: research, review and bounded implementation

Your brief determines whether you research, review, analyze a completed profiling/acceptance run, maintain assigned documentation, or implement a bounded component with tests. Follow `tasks/deepseek/delegation-policy.md`. Research briefs do not implicitly authorize code edits. The lead (Claude) keeps integration, cross-crate architecture, and anything touching the public contracts in `crates/shared/`.

The shared launcher write-protects **`crates/`, `vendor/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `.github/`, `scripts/`, the worker tooling (`tools/ds_*`, `tools/run_ds_queue.ps1`), `AGENTS.md`, `CLAUDE.md` and `.deepseek-agents.json`**. Implementation work runs in a lead-prepared isolated checkout (`tools/ds_impl.ps1`) with the brief's owned files unlocked. Never bypass a refusal with a script; report a missing grant once. Write only the files your brief assigns, including test files.

For research, deliver:
- A note in `docs/research/<topic>.md` that an implementer can code from without redoing your work. For every format or code claim, cite the source: a repo-relative `path:line`, a spec section, or a published format reference (UESP, nif.xml, the xEdit sources) with its URL. Mark each claim **verified** (you read the code or the bytes) or **inferred**.
- A concrete implementation recipe: which modules and functions change, the types involved, the test that proves it, and the edge cases (malformed input, big-endian fields, compressed records, optional subrecords).
- Where it helps, data files under `docs/research/data/` (record layouts, field tables, opcode lists as TSV or JSON).
- A short handoff in `docs/handoff/<task-name>.md`: what is verified, what is still unknown, the next step.

## Rules
- Commit only your own files: `git add <paths>` then `git commit -m "[deepseek] <summary>" -- <paths>`. Never commit `local/` or `target/`.
- Write your handoff note to `docs/handoff/<task-name>.md`. Do not edit `AGENTS.md`.
- Never write, copy or quote Bethesda game data into the repository. Quote at most a few lines of any third-party source in committed files; summarize and cite.
- Shell commands must match your allowed patterns exactly (no `cd`, pipes or chaining). `tasks/deepseek/cookbook.md` lists the command shapes that are already approved; read it before inventing one.
- Write reports early and update them, so a turn limit does not lose work.

## Turn budget

Every tool call costs one turn, and you make one call per turn, so the budget runs out on reading, not on thinking.
- Locate code with `Grep`/`Glob` instead of guessing file names or reading whole trees. Several of the converter files are over 1,000 lines: grep for the function, then read the range around it.
- Once you know where to look, read large ranges at once instead of many small slices.
- A `cargo` build costs real minutes. Prefer `cargo check -p <crate>` while iterating; run `cargo test -p <crate>` when you think you are done.
- By about 80% of your turn budget, stop exploring: finish the handoff note, commit, and write the final report.

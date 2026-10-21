# Goal
Review the merged-terrain pipeline wiring (#186, Phase 1) and decide accept or fix first.

# Context
- Diff: `git diff 20ac4831 9b013eb7` in `C:/Modding/Mudcrab/mudcrab/local/impl/impl-896.1-p1-terrain-pipeline-wiring` (6 files: new `crates/converter/src/terrain_pipeline.rs`, new `crates/converter/tests/fixture_merged_terrain.rs`, small edits in pipeline.rs, config.rs, main.rs, lib.rs). Read all of it.
- The brief it answered: `tasks/deepseek/impl-896.1-p1-terrain-pipeline-wiring.md`. The format it targets: `local/research/terrain-merge/format-proposal.md` (section "v3"). Libraries it calls: `crates/converter/src/esm/terrain_mesh.rs`, `terrain_arrays.rs`, `terrain_chunks.rs`; shared contract `crates/shared/src/terrain_mesh.rs`.
- Known gaps the coder named (confirm and judge each): the metadata-rebuild route (`metadata.rs`, `--reuse-assets`) ignores `--merged-terrain` silently; progress reuses `ProgressStage::LodChunks`; terrain files have no manifest or DB record yet (schema bump deferred on purpose); `terrain_ini` has no CLI flag.

# Check
1. Correctness: the key (`cell_key`) hashes the LAND, its plugin's masters, and every LTEX the LAND names plus each one's TXST, present or not; the masters come from the right load-order slot; a load-order shift changes the key. The known-LTEX set equals the table's entries. Every exterior LAND becomes a mesh or a reported skip; interiors are skipped. Chunk files are grouped by worldspace and chunk.
2. What used to work, now fails: with the option OFF, is the conversion byte-for-byte and step-for-step unchanged (no extra memory held, no new files, the resume path, report JSON readers that deny unknown fields)? With it ON: memory held by keeping the merged record map alive through the texture stage (estimate for full Skyrim), and whether the stage blocks the async runtime the way the LOD stage avoids (compare how the LOD stage is run).
3. Silent fallbacks: any error turned into a warning or default that hides wrong output (the default-texture ini fallback, unreadable textures, a cell with no CELL record).
4. Tests: do the fixture tests prove the outputs (table validates, arrays read back, chunks read back with the right cells, option off writes nothing)? Would they catch a broken key?
5. Foundations check: what shortcut does this take, and what would break for a mod or another worldspace (a plugin adding a new worldspace, an LTEX override, a mod with a LAND but no CELL change, a load order with ESL plugins)?

You may run, foreground only: `git diff`, `git show`, `git log`, and `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --all-targets terrain` in that worktree.

# Report (under 450 words)
Verdict (accept / fix first), then findings as `file:line - problem - fix`, most severe first, each tagged must-fix or nice-to-have, then one line per check 1-5.

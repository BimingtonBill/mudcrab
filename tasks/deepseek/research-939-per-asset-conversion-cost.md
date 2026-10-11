# Goal
How costly and how feasible is converting ONE asset on demand (one NIF mesh, one DDS texture) with Mudcrab's existing converter code, compared to loading the native file at runtime? Facts for a design reply.

# Context
- This folder is a clean checkout of upstream main (Rust, Bevy 0.19). Converter: `crates/converter` (`convert_nif_to_glb`, `convert_dds_to_ktx2`, archive readers in `archive/`), engine: `crates/engine`. The vendored NIF parser is `vendor/project-wormhole-nif`.
- Earlier scope notes (to verify, not trust): per-file conversion functions are callable; there is no read-one-file-by-name API on archives (`archive/mod.rs:124-153`, readers `pub(crate)`); texture colour space is decided from meshes and the world DB (`pipeline.rs` ~603); prune records rewrite GLBs after textures.

# Scope
- Read only; no builds or edits.

# Done when
Each answered with file:line evidence:
1. Can the engine crate call converter code today (dependency graph in Cargo.toml files)? What would linking the converter into the engine pull in (heavy deps: Basis encoder, rayon, tokio)?
2. Per-asset dependencies: what does converting one NIF need besides the NIF bytes (textures? DB? other files)? What does converting one DDS need (colour space source, mesh/material info)? Which of these make on-demand conversion order-sensitive?
3. Archive access: is there a way to read one file by path from a BSA without extracting all? What would it take?
4. Any timing data in the repo for per-file conversion (tests, benches, docs, reports) for NIF and DDS; if none, estimate from what the code does (parse + encode) and say it's an estimate.
5. How the engine loads an asset today (Bevy AssetServer paths), and where an on-demand hook (convert on cache miss) would sit.

# Report
- One section per question, evidence-first; then "On-demand: what it takes" (3-6 bullets) and "Runtime native: what it would take" (3-6 bullets, what existing code could be reused, e.g. the vendored NIF parser). Under 800 words, English.

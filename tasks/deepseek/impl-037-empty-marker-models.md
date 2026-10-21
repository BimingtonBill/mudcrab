# impl-037 - Empty marker models must not fail the bounds gate

Owned files: crates/engine/src/streaming.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, and
`docs/research/visual-gaps-spec.md`, Gap 4 (2).

## Goal

Two Blackreach references (`WispAmbush` REFR 000F9907 and `FrostSpiderAmbush01` REFR 0007C068,
cell 0002D4E0) have models that the converter deliberately exports as **empty scenes**: editor
markers, with every shape dropped (`converter/src/mesh.rs:89-104`). Their `bounds_valid` is 0, and
`streaming.rs` reports "converted model has no validated aggregate bounds; reconvert the asset".
That counts toward `transform_bounds_validation_failures`, a hard gate, so every measured
Blackreach run fails.

Make the engine accept a model whose glTF scene holds **no render primitives** without counting a
failure. Skip spawning it, and count it separately (a new metric, for example
`empty_model_references`). **Every other bounds failure must stay fatal.** Decide "empty" from the
asset itself (the loaded scene has no mesh primitives), not from the model's name.

## Tests (write them to fail before your change)

- An empty-scene model is skipped and counted, and nothing fails.
- A model with primitives but invalid bounds still fails.

## Check it

Run `cargo build --release -p engine`, then render one shot in Blackreach cell 0002D4E0 with the
shots mode (`docs/design/reference-shots.md`); `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` has the
cell's grid. The log must show no bounds error for those two references.

## Scope

Only `streaming.rs`. No crosstalk partner. Report per the implementation template.

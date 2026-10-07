# impl-046 - Converter cleanup for upstream: ADR-0002 test data, no private names or machine paths

Owned files: crates/converter/src/mesh.rs,crates/converter/src/material.rs,crates/converter/src/nif_animation.rs,crates/converter/src/esm/exporter.rs,crates/converter/src/esm/lighting.rs,crates/converter/src/esm/directional_material.rs,crates/converter/src/esm/mod.rs,crates/converter/src/cache.rs,crates/converter/src/pipeline.rs,crates/converter/src/archive/bsa.rs
Acceptance: cargo test -p converter --lib,cargo clippy -p converter --all-targets -- -D warnings,cargo fmt -p converter -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`,
`docs/pr/upstream-pr-plan.md` (its "Top 5 cleanup items" and "Risks") and
`docs/adr/0002-ground-truth-unmodded-install.md`.

## Goal

This fork's converter changes will go upstream. Clean the converter so it follows upstream's
conventions and carries nothing private. **Behaviour must not change.**

1. **ADR-0002 real-data tests.** The fork's install-dependent tests read `SKYRIM_DATA_DIR` and
   fall back to hard-coded machine paths (`C:/Games/...`, `C:/Modding/...`). The upstream
   convention is `MUDCRAB_SKYRIM_DATA`: read ADR-0002 for the exact contract. Switch every
   such test to it, delete the machine-path fallbacks, and keep the tests `#[ignore]`d or skipped
   when the variable is absent, exactly as ADR-0002 says. For tests that read the fork's
   converted output (`C:/Modding/Mudcrab/SkyrimConverted/...`, for example the real-plugin lighting test
   and the door-animation tests), use a second variable, `MUDCRAB_CONVERTED_DIR`, with the same
   skip rule. Document it in the test module doc.
2. **No private names in production code or doc comments.** Remove references to the fork's
   private tooling and process from the owned files:
   - `tasks/deepseek/...`, `impl-0NN`, `research-0NN`, `review-0NN`, `lead-0NN`;
   - "DeepSeek", "worker", "the lead";
   - `docs/handoff`, and `local/` paths;
   - user names and machine paths.
   Keep the *technical* content, the data facts and the measurements: rewrite "measured by
   impl-038" as "measured on Skyrim.esm (2026-09)" or similar. `docs/research/...` and
   `docs/design/...` links may stay if the doc exists in the repo. List every change you made in
   the report.
3. Grep for anything else the PR plan's risk list names (`Brannock`, email addresses, `C:/Users`)
   in the owned files.

## Check

- The acceptance commands must pass. Test names may change; test count and coverage may not
  shrink.
- Run `cargo test -p converter --lib -- --include-ignored` with `MUDCRAB_SKYRIM_DATA` set to
  `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data` and `MUDCRAB_CONVERTED_DIR`
  set to `C:/Modding/Mudcrab/SkyrimConverted`. The real-data tests must still pass. Report the counts.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template.

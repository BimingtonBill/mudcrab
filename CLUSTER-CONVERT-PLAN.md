# Cluster conversion toggle implementation plan

**Goal:** Implement the user's 2026-10-24 decision: conversion selects either clustered objects, trees and terrain without baked LOD, or the existing baked path.

**Architecture:** The conversion manifest owns the geometry mode and the exact relative derived-package selector. The converter records that contract before overlay fingerprinting. Launcher Play and direct engine startup consume it. Legacy manifests remain baked by default; explicitly selected legacy cluster overlays remain supported without baked reads.

**Constraints:** Base `589dc62fc`; branch `phase2/cluster-convert-toggle`; existing worktree and target only. All Cargo and formatting commands use build-gate owner `sol-2144`. No full conversion, engine/GPU launch, converted-data changes or publication. Keep C: and D: above 15 GB.

- [x] Define shared manifest parsing, safe package selection and legacy defaults; test malformed and missing cluster selectors.
- [x] Converter: select trees and terrain, omit baked producers and reused baked products, record exact package before certification. Test the real fixture pipeline and baked parity.
- [x] Engine: replace native inventory and terrain reads in cluster mode, admit only required cluster inputs and reject missing data. Test fixtures without baked files/tables.
- [x] Launcher: persist the conversion checkbox with conversion settings, map both start and resume to converter configuration, derive Play flags from the selected output manifest. Test persistence, flags and UI state.
- [x] Review integrated diff independently; run gated fmt, touched-crate strict Clippy/nextest, workspace strict Clippy/nextest/doctests.
- [x] Record omitted product paths, code-based savings inference and remaining acceptance limits. Add handoff entry and commit with the requested co-author trailer.

## Review focus

- Resume or reuse must not reintroduce baked artifacts into clustered output.
- Overlay fingerprints must not change after derivation because of a manifest rewrite.
- Play must follow the converted output even when the checkbox has since changed.
- Missing cluster terrain must fail rather than activate native terrain LOD.
- Canonical near-terrain heights and refused stock primitives remain available.

## Execution ledger

- Clean checkout confirmed at the requested base. Existing branch created as requested.
- Queue claim unavailable: this worktree has no `local/plans/task-queue.md`. No replacement queue created.
- Converter/shared and engine run in bounded Codex lanes; coordinator owns launcher, integration, review and final checks.
- Ruling: existing launcher has no persisted conversion settings. Add one per-user launcher settings file for paths, source selection and the new checkbox, with visible read/write errors.
- Launcher review corrected exact package resolution, keyboard navigation/focus, saved MO2 profile restoration and corrupt-file preservation. Launcher nextest passed 128 tests after those fixes.
- Integrated review found one remaining metadata-reader migration issue: shared LOD-namespace textures were classified using the old source database. Move classification after rebuilding the current graph and retain bytes required by that graph. A bounded converter worker owns the fix and its regression.
- The external `timing-int` hold blocks all Cargo and formatting until 00:41 Adelaide. No hold is cleared and no Cargo invocation bypasses the gate. The queued verification wrapper was cancelled before source changes for the review fix; the final suite must be restarted on the frozen result.
- The current-graph copy fix and a real Windows-1252 Legacy-to-Inhouse migration regression are implemented. An independent second review checks that fix; a separate verification worker owns the serial gated suite and its source snapshot. Both results wake this task.
- The existing atmosphere test now asserts that removing cluster terrain cannot consult native inventory, and that explicitly restoring baked mode retains its native policy. This updates the superseded fallback expectation without changing runtime code.
- Independent review round 2 found no actionable issues in the current-graph retention fix, its real reader-migration regression, or the atmosphere correction. Snapshot before formatting: `014dc0da5e7cfe9c7fd09edab8a47992fe650d96f3a466ab4e94bce8ad3fd914` over the reviewer's sorted path/content-hash entries. Final gated verification remains authoritative for the formatted source.
- The timing hold ended normally. The first verification round stopped at the launcher's Bevy style query complexity lint; a narrow, explained expectation keeps the query together. The second round passed formatting and touched-crate strict Clippy, then stopped at a terrain-loader fixture: Bevy classifies a hand-built uncompressed 1×1 KTX2 as a 1D image. The runtime correctly requires a 2D atlas.
- The fixture now uses the real DDS-to-KTX2 converter with a mipmapped 4×4 source. Its focused gated nextest passed one test; runtime validation is unchanged. A third verification worker owns the complete serial suite on frozen source. The second round's partial 2,390 passing tests are not a complete-suite result.
- Final verification round 3 completed all seven gates with exit 0: formatting write/check, touched-crate strict Clippy and all-target nextest, workspace strict Clippy, workspace nextest and workspace doctests. Touched nextest passed 5,316 tests with 58 skipped; workspace nextest passed 5,445 with 63 skipped; doctests passed 18 with 15 ignored. The 34-file source snapshot stayed `3f186074766729a97099ed167cd2cdca3827fbf0899478149a2246a5f6ed6ff8`. The coordinator verified every log hash, exit receipt and current source hash before the local commit. Full receipts remain in the target-local `2144-verification-summary.json`.

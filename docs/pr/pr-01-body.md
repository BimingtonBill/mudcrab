# PR 1 — ready to send

**Branch:** `pr/01-dummy-content-interior-door` (local only, not pushed)
**Base:** `origin/main` at `2105bfd`
**Diff:** 3 files, +705 −9, one commit (`crates/dummy-content/src/esm.rs`, `src/main.rs`,
`crates/converter/tests/fixture_doors.rs`)

## Title

`test(dummy-content): an interior cell and a linked load-door pair`

## Body

The fixture generator writes exterior cells only, so nothing in CI can exercise a load door:
there is no interior cell, no `DOOR` base record and no `XTEL`. Any contribution that touches
interiors, doors or movement between cells can only be reviewed by taking the author's word for
it.

This extends the ESM writer additively:

- `esm::Interior` and `esm::Door`, plus `plugin_with_interior`, write an interior `CELL` (nested
  `GRUP` 2/3, `DATA` interior flag, no `XCLC`), two `DOOR` base records whose `FNAM` carries the
  auto-load bit, and two `REFR`s whose `XTEL` points at the other — each arriving in front of its
  door rather than on it, as retail data does.
- `PRESET_EXTERIOR_CELL` and `PRESET_INTERIOR` fix one shape so the tests and the CLI agree,
  behind `dummy-content gen <dir> --with-interior`.
- `plugin()` is untouched, and a test asserts its output is byte-identical to the pre-change
  writer's.

`crates/converter/tests/fixture_doors.rs` runs the preset through the real parser: the interior
cell has no grid and no worldspace, both `DOOR` records carry their flags, the two `XTEL`s are
reciprocal and carry each other's arrival frames, and a truncation/mutation sweep never panics.

No game data is involved: every byte is synthesized from the seed, as ADR-0001 requires.

The new preset's author string is plain `"dummy-content"`. The crate's existing one still reads
`"Mudcrab dummy-content"`; renaming it after the move to Mudcrab felt like churn for this
change, but say the word and it goes in.

## Checks run locally on the branch

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace` | 110 passed, 7 ignored, 0 failed |

## Note on scope, against `upstream-pr-plan.md`

The plan listed `fixture_doors.rs` whole (358 lines, 3 tests). One of those tests asserts
`door_links` rows and `DOOR` base records in `statics`, which only exist with the converter change
that becomes PR 3. It fails on upstream's converter, so it is **not** in this PR; it moves to
PR 3, where it belongs. This branch carries the two tests that upstream's own converter can
satisfy (195 lines).

## What a reviewer is likely to ask

- *"Why does a fork get to extend our fixture crate?"* Without an `XTEL` pair, no reviewer and no
  CI job can exercise a load door, a portal, a crossing or a door animation. This is the
  prerequisite for those reviews, and it is useful on its own.
- *"Is the API intrusive?"* It is additive, and `plugin()`'s bytes are pinned by a test.
- *"Why `PRESET_*` constants rather than parameters?"* It is the smallest thing that makes the
  fixture stable across the CLI and the tests.

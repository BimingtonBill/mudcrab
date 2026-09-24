# impl-167 - The light budget counts only lights that can be seen (audit finding 5)

Owned files: crates/engine/src/lights.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal.

## The finding

The nearest-64 light ranking in `lights.rs` includes every `SkyrimLight` whatever its cell's role or
visibility, so lights of hidden prestreamed cells can push the active space's lights out of the
budget. The cached selection is invalidated only by a change in the total count or by moving 256
units, so swapping lights at an equal count, or cells changing between active and hidden while the
player stands still, keep a stale selection.

Suggested fix: budget only eligible lights - those of the active space, plus an explicit, documented
policy for the portal destination's lights (today they are drawn for the portal camera; Bevy lights
ignore render layers, `docs/research/portal-prior-art.md` problem D, so say what you chose and why) -
and invalidate the cache on membership or space changes as well as on movement. Find how a light's
cell and role are known (the streamed cell roots, `portal.rs`'s cell roles, `ActiveCell`); read
them, do not change those modules. Tests: a hidden cell's lights do not displace active ones; an
equal-count swap refreshes the selection; an active/hidden change while standing still refreshes it.

## How to work: prove it first

These findings come from an **AI-written outside audit** (Codex, 2026-09-24); line numbers are from
around `d6e883095ef5` and may have moved - find things by name. Only finding 1 has been checked by a
second reader, and nothing has been reproduced in a running engine. So for **each** finding:

1. Write a unit test that exercises the claimed failure through the real code path (production
   system order, no hand-synced state the real game would not have) and **run it on the unfixed
   code**. It must fail for the reason the finding gives.
2. If it does not fail, **do not change production code for that finding**: stop, and report
   plainly that the claim did not reproduce, with the test and what it showed.
3. If it fails, fix it with the smallest change, keep the test, and make it pass.

Every command on an entity that may have been despawned uses the `try_` forms.

## Checks

`cargo test -p engine --lib`, clippy, fmt. Then build with the fast, non-measured profile
`cargo build --profile quick -p engine --bin engine` (the binary lands in `target/quick/engine.exe`)
and run both tours from it: `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/impl-167-portal-audit-light-budget-riverwood`
and the same with `--demo alftand`. Both must end `tour PASSED` (8 and 4 crossings), with every
`swing ok` / `far door ok` line and no `FAIL`. Report, per finding: reproduced or not (with the
failing test's message), what you changed, and the tour lines.

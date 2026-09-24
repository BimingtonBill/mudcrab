# impl-160 - Faster demo tours and tour builds, without weakening them

Owned files: crates/engine/src/demo_tour.rs,crates/engine/src/shots.rs,crates/engine/src/config.rs,Cargo.toml,docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Read first: `docs/research/faster-automated-checks.md` (research-157: measurements and
proposals P1-P5, with file:line). Implement **P1-P5** from it:

- **P1** the tour's `Settle` waits on streaming being quiet (reuse `shots.rs`'s settle check, made
  `pub(crate)`), with today's flat time kept as the ceiling; log each stage's actual settle time.
- **P2** the per-view waits as proposed (survey 0.4 s, look-around 1.0 s, done 1.0 s).
- **P3** `--tour-doors <N>`: walk only the first N doors of the route, skip the route-end
  look-around, and end with `tour SMOKE after N crossings` (never `PASSED`). Add it inside
  `PortalOptions` in `config.rs`; another worker is adding a flag there too, so keep your change to
  your own lines.
- **P4** keep only the walk-through frames the window needs (a ring buffer of the last ~30 before
  the swap, plus the window after), so a tour writes ~160 frames, not ~1,264.
- **P5** a `[profile.release-fast]` in `Cargo.toml` (inherits release, `lto = false`,
  `codegen-units = 16`) for tour builds only; `[profile.release]` unchanged. Document both in
  `docs/demo/README.md`, including that release-fast is never used for timings or sign-off.

Do **not** change what the tour checks: every crossing, the walk test and the verdict must stay.
Checks: build (`cargo build --release -p engine`, then `cargo build --profile release-fast -p engine`)
and run both full tours on each, then one `--tour-doors 1` smoke run (assets `C:\Modding\SkyrimConverted`,
read-only; output only under `local/`). Report per tour: verdict, wall time before (research-157's
232.8 s Riverwood / 132.5 s Alftand) and after, frames written, and the build times of both profiles.
Commands on entities use the `try_` forms.

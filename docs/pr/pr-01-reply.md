# Reply to the review of #23 (posted 2026-09-23 with fix 94d295f)

Thanks for the careful review. All five items are addressed in 94d295f:

1. **Write policy (blocking):** the preset now writes through a new, documented `layout::write_plugin(root, bytes)`, which `generate` uses for its own plugin too. So it gets the ADR-0007 symlink refusal and the atomic publish. Added a `cfg(unix)` symlink test, a cross-platform test that a stale temporary is refused, and a CLI-level test of `gen --with-interior --force`.
2. **Doors without a model:** documented on `Door::model_path`, on the preset and in the spec. The new `fixture_interior_pipeline.rs` runs the pipeline over the `--with-interior` tree and asserts `references_without_model == 2`. Extending the exporter to `DOOR` can be a follow-up.
3. **`XTEL` remapping:** the load-order note is at the writer, in `fixture_doors.rs` and in the spec's deviations list.
4. **Preset constants:** they now live in `layout` and are reused everywhere. The interior preset writes `Mudcrab dummy-content`, so the comment is true now. This does change the preset's `TES4` author bytes.
5. **End-to-end:** the test calls the same library functions the CLI does, since an integration test can't run the binary.

Nits: done. Byte identity is now a golden hash (the copy-based test passed on the new writer before I removed it). The tag counter is a record walk; it lived in `esm.rs`'s tests, not `fixture_doors.rs`. Door `DATA` is one `Vec::with_capacity(24)`, and there's a `const` assert for the FormID headroom.

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo test --workspace --all-targets` pass locally.

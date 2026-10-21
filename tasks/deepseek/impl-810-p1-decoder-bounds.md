# Goal
Track: Phase 1 (for upstream). A crafted or damaged game file makes the converter return an error,
never reserve memory from an untrusted size field or expand data far past its own file size. Fix
four open audit findings, one commit each, each with a test that fails before the fix.

# Context
- Your checkout is cut from upstream `main`. Upstream commit style (`fix(converter): ...`), no
  `[deepseek]` prefix, no project-internal references (audit ids, task numbers) in code, comments,
  docs or messages.
- Findings (confirmed by reading; line numbers drift, find them):
  1. `crates/converter/src/texture.rs` reserves `Vec::with_capacity(dds.get_num_mipmap_levels() as
     usize)` from the raw DDS header before any per-mip check (two places: the X8R8G8B8 path and
     `encode_l8_volume`). A header with the MIPMAPCOUNT flag and a huge count aborts on allocation.
     Clamp the reservation to what the payload can hold (e.g. the number of mips a surface of these
     dimensions can have, at most 32, or the remaining data length divided by the smallest mip).
  2. `crates/converter/src/esm/binary.rs` `read_bounded` caps only the first reservation
     (`MAX_RESERVATION`) but then decompresses up to the record's own declared size + 1; a record
     declaring ~4 GiB (zlib reaches ~1000:1) expands far past the plugin's size and every payload is
     retained. Bound the total decompressed size per record by a sane limit derived from the input
     (e.g. a multiple of the compressed length with an absolute cap, as `archive/ba2.rs` does for
     archives) and return an error beyond it.
  3. `crates/converter/src/esm/mmap_reader.rs` `EsmReader::open` does no size or header validation:
     a zero-length or non-plugin file reaches the parsers as an empty slice. Validate the minimum
     size and the TES4 header signature and return a clear error.
  4. `crates/converter/src/archive/ba2.rs` `read_bounded` does `Vec::with_capacity(expected_size)`
     from the archive's declared unpacked size before decoding, and the LZ4 arm passes the same size
     to `lz4_flex::block::decompress`, which reserves it too; the only bound is `MAX_ENTRY_SIZE`
     (1 GiB), so a ~40-byte entry declaring 1 GiB reserves 1 GiB (abort on failure). Bound the
     declared size by the compressed length times a sane maximum ratio (zlib/LZ4 cannot exceed
     roughly 1032:1 / 255:1) as well as the absolute cap, and reserve incrementally.
- Existing never-panic property tests (proptest) and mutation tests live beside these modules and
  in `crates/converter/tests/`; extend them rather than adding a parallel framework.

Owned files: `crates/converter/src/texture.rs` (only the two reservations and tests),
`crates/converter/src/esm/binary.rs`, `crates/converter/src/esm/mmap_reader.rs`, `crates/converter/src/archive/ba2.rs`,
`crates/converter/tests/` (new files only), `docs/handoff/impl-810-p1-decoder-bounds.md`. If you
need another file, say so in the report instead of editing it.

# Done when
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/builds/impl-810 -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/builds/impl-810`

# Report
Per finding: the change, the test and that it failed before (with the failure text). The chosen
limits and why they cannot reject a real Skyrim SE file (check the largest compressed record in
`Skyrim.esm`, read-only under `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data`, with a
small script under `tools/research/` if needed). A suggested commit message per finding.

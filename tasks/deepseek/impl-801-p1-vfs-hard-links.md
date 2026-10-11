# Goal
Track: Phase 1 (for upstream). On every conversion, an extracted archive file in `vfs/` and its
content-addressed blob in `.ingestion-cache/` are one file (a hard link) instead of two copies.
Measured on a Skyrim SE conversion: `vfs/` 25.95 GB and `.ingestion-cache/` 22.47 GB are
byte-identical copies today, and the engine reads neither, so this saves ~26 GB per install.

# Context
- Your checkout is cut from upstream `main` (6f583cd). Upstream commit style
  (`fix(converter): ...` or `perf(converter): ...`), no `[deepseek]` prefix, no project-internal
  references (task numbers, fork branch names, "portal", local paths) in code, comments, docs or
  commit messages.
- The fork already solved the *reconversion* half of this in commit e6f3909 (its converter diff is
  at the end of this brief, for reference: it was made on a different base, so do not apply it
  blindly). That commit only changed the cache-HIT path and relied on a `link_or_copy` helper that
  an earlier commit had added to pipeline.rs; on main neither exists. This task brings it in fresh
  and extends it to the fresh-install (cache-MISS) path, which is where the 26 GB is.
- Code on main: `crates/converter/src/archive/mod.rs` (`persist_cache_blobs`,
  `restore_cached_files`, `copy_if_missing`, `copy_file` or similar; line numbers drift, find them),
  `crates/converter/src/cache.rs`, `crates/converter/src/pipeline.rs` (`overlay_loose_assets`, the
  Papyrus runtime write of `papyrus_runtime.luau` into the output).

# Scope (from the reviewer who owns the upstream send)
1. `link_or_copy(from, to)` in `cache.rs` (pub(crate)), with the refuse-self-replace guard: if `to`
   exists and canonicalises to the same file as `from`, return Ok without touching it (removing it
   would delete `from` itself); otherwise remove `to`, then `fs::hard_link`, falling back to
   `fs::copy`. Its doc comment explains why linking is safe and states the fallback: a cross-volume
   or no-link filesystem gets a copy.
2. Every copy between `vfs/` and `.ingestion-cache/` in `archive/mod.rs` goes through it, on BOTH
   paths: the cache-miss path (extraction writes the vfs file, then the blob is persisted) and the
   cache-hit path (blob restored into vfs).
3. Every writer into `vfs/` or `.ingestion-cache/` replaces its path instead of writing through it
   (writing through a link would change the other name too): the loose-file overlay
   (`overlay_loose_assets`) and the Papyrus runtime write unlink an existing file first. Audit every
   other writer into those two folders on main (grep for `fs::write`, `fs::copy`, `File::create`,
   `OpenOptions` whose path is under vfs or the cache) and list each one with its verdict in the
   report.
4. Keep/port a test like the fork's `cache_hits_link_blobs_and_vfs_files_where_the_filesystem_can`
   (cache hit: blob and vfs file are one file) and the write-through regression test
   (`loose_asset_override_does_not_write_through_a_linked_vfs_entry`).
5. ADD a fresh-install test: after one cache-MISS extraction, the blob and the vfs file are one
   file. Check "one file" portably: on Windows compare `std::os::windows::fs::MetadataExt`
   file index / volume serial if available on stable, else use `same-file`-style logic already in
   the dependency tree (check `cargo tree -p converter`; do NOT add a new dependency: say so in the
   report if you think one is needed), or fall back to: write through one name via an in-place
   `OpenOptions::write` and see the other change. Unix: compare `(dev, ino)`. The test must FAIL on
   main (report that it did).
6. Tests for the self-replace guard and the copy fallback if the fallback can be forced cheaply
   (e.g. a helper taking the link function as a parameter); otherwise say why not.
- Keep the diff small and away from `staging_path`/`publish_directory` and the manifest code in
  pipeline.rs (other open PRs edit those lines).

Owned files: `crates/converter/src/archive/mod.rs`, `crates/converter/src/cache.rs`,
`crates/converter/src/pipeline.rs` (only the overlay and Papyrus-runtime writes and tests),
`crates/converter/tests/` (new test file only if needed), `docs/specs/converters/` (only where
extraction or the ingestion cache is described), `docs/handoff/impl-801-p1-vfs-hard-links.md`.
If you need another file, say so in the report instead of editing it.

# Done when
These pass in your checkout (the target dir is on D: on purpose; do not build into C:):
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/phase1-target -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/phase1-target`

# Report
Per scope point: the change and the test name, and for point 5 whether it failed before the change.
The writer audit list (point 3). A suggested commit message in upstream style (subject + body).
Anything you chose not to do and why.

# Reference: the fork's commit e6f3909 (converter files only; different base)
```diff
diff --git a/crates/converter/src/archive/mod.rs b/crates/converter/src/archive/mod.rs
index ede1a1d..7098c1a 100644
--- a/crates/converter/src/archive/mod.rs
+++ b/crates/converter/src/archive/mod.rs
@@ -3,7 +3,7 @@ mod bsa;
 
 use crate::{
     asset_path::{AssetKind, canonical_asset_path},
-    cache::{IngestedFile, IngestionCacheEntry, hash_bytes, hash_file},
+    cache::{IngestedFile, IngestionCacheEntry, hash_bytes, hash_file, link_or_copy},
 };
 use color_eyre::{
     Result,
@@ -232,11 +232,17 @@ fn copy_if_missing(source: &Path, destination: &Path) -> Result<()> {
     copy_file(source, destination)
 }
 
+/// Links `source`'s bytes to `destination`, replacing any file already there.
+///
+/// Cache blobs are content-addressed, so a blob is never written in place and a restored `vfs`
+/// entry can share its file with the blob and with the previous output (see
+/// [`link_or_copy`]). Every writer into `vfs` or the cache therefore removes its destination
+/// first, so no write goes through a shared file.
 fn copy_file(source: &Path, destination: &Path) -> Result<()> {
     if let Some(parent) = destination.parent() {
         fs::create_dir_all(parent)?;
     }
-    fs::copy(source, destination).wrap_err_with(|| {
+    link_or_copy(source, destination).wrap_err_with(|| {
         format!(
             "failed to restore cached asset {} to {}",
             source.display(),
@@ -372,6 +378,70 @@ mod tests {
         assert!(!directory.path().parent().unwrap().join("escape").exists());
     }
 
+    #[test]
+    fn cache_hits_link_blobs_and_vfs_files_where_the_filesystem_can() {
+        let directory = tempfile::tempdir().unwrap();
+        let archive = directory.path().join("assets.ba2");
+        fs::write(
+            &archive,
+            dummy_content::ba2::general(
+                &[dummy_content::Entry::new("textures/test.dds", b"DDS ")],
+                dummy_content::ba2::Compression::None,
+            )
+            .unwrap(),
+        )
+        .unwrap();
+
+        let first_output = directory.path().join("first/vfs");
+        let first_cache = directory.path().join("first/.ingestion-cache");
+        let first = ArchiveExtractor::extract_cached(
+            &archive,
+            &first_output,
+            Path::new("unused"),
+            &first_cache,
+            None,
+            true,
+        )
+        .unwrap();
+        assert!(!first.cache_hit);
+
+        let second_output = directory.path().join("second/vfs");
+        let second_cache = directory.path().join("second/.ingestion-cache");
+        let second = ArchiveExtractor::extract_cached(
+            &archive,
+            &second_output,
+            &first_cache,
+            &second_cache,
+            Some(&first.cache_entry),
+            true,
+        )
+        .unwrap();
+        assert!(second.cache_hit);
+
+        // A hard link sees a write made through any other name; a copy would not. NTFS, ext4 and
+        // APFS all support links, so restoring a cache hit must not have fallen back to copying:
+        // all four names below have to be one file.
+        let hash = &second.files[0].sha256;
+        let first_blob = blob_path(&first_cache, hash).unwrap();
+        let second_blob = blob_path(&second_cache, hash).unwrap();
+        let first_vfs = first_output.join("textures/test.dds");
+        let second_vfs = second_output.join("textures/test.dds");
+        fs::OpenOptions::new()
+            .append(true)
+            .open(&second_vfs)
+            .unwrap()
+            .write_all(b"+")
+            .unwrap();
+        for name in [&first_blob, &second_blob, &first_vfs, &second_vfs] {
+            assert_eq!(
+                fs::read(name).unwrap(),
+                b"DDS +",
+                "{} does not share the restored file",
+                name.display()
+            );
+        }
+    }
+
     #[test]
     fn reuses_verified_archive_blobs_and_recovers_from_corruption() {
         let directory = tempfile::tempdir().unwrap();
diff --git a/crates/converter/src/cache.rs b/crates/converter/src/cache.rs
index e0fc8ac..b6dbc78 100644
--- a/crates/converter/src/cache.rs
+++ b/crates/converter/src/cache.rs
@@ -111,6 +111,28 @@ pub fn configuration_hash_for_schema(
     Ok(hash_bytes(&serde_json::to_vec(&relevant)?))
 }
 
+/// Puts `from`'s bytes at `to` as a hard link where the filesystem allows one, else as a copy.
+///
+/// A reconversion reuses every unchanged output of the previous run; copying them made staging
+/// as large as the published set (about 50 GB of textures for Skyrim), where a link costs nothing.
+/// Linking is safe because nothing writes a staged output in place: textures and meshes are
+/// written to a temporary file and renamed over their output, a reconverted output first unlinks
+/// the staged file at its path, and `overlay_loose_assets` unlinks every path it overwrites.
+/// Publishing renames staging over the output and deletes the old output, which only drops one of
+/// the two links. A published tree therefore holds internal links too (a `vfs` file and its cache
+/// blob are one file): a size check counts them twice, and a copy that does not keep links
+/// duplicates the data.
+pub(crate) fn link_or_copy(from: &Path, to: &Path) -> std::io::Result<()> {
+    if to.exists() {
+        // Removing `to` first would delete `from` itself if the two name the same file.
+        if fs::canonicalize(from)? == fs::canonicalize(to)? {
+            return Ok(());
+        }
+        fs::remove_file(to)?;
+    }
+    fs::hard_link(from, to).or_else(|_| fs::copy(from, to).map(|_| ()))
+}
+
 pub fn hash_bytes(bytes: &[u8]) -> String {
     let digest = Sha256::digest(bytes);
     digest.iter().map(|byte| format!("{byte:02x}")).collect()
diff --git a/crates/converter/src/pipeline.rs b/crates/converter/src/pipeline.rs
index 2f478bd..f66f159 100644
--- a/crates/converter/src/pipeline.rs
+++ b/crates/converter/src/pipeline.rs
@@ -3,7 +3,7 @@ use crate::{
     asset_path::{AssetKind, canonical_asset_path, resolve_asset_uri},
     cache::{
         CacheEntry, ConversionManifest, configuration_hash, configuration_hash_for_schema,
-        hash_file,
+        hash_file, link_or_copy,
     },
     config::PipelineConfig,
     esm::{EsmParser, cell_cache::write_cell_cache, exporter::validate_database, read_plugins_txt},
@@ -352,6 +352,11 @@ impl AssetPipeline {
         if let Some(parent) = runtime_path.parent() {
             fs::create_dir_all(parent)?;
         }
+        // A reused script output can be a link to the published file; replace the path rather
+        // than write through it.
+        if runtime_path.exists() {
+            fs::remove_file(&runtime_path)?;
+        }
         fs::write(
             &runtime_path,
             include_str!("../../shared/src/papyrus_runtime.luau"),
@@ -986,6 +991,12 @@ fn overlay_loose_assets(data: &Path, vfs: &Path, files: &[PathBuf]) -> Result<()
         if let Some(parent) = destination.parent() {
             fs::create_dir_all(parent)?;
         }
+        // A reused archive entry links its `vfs` file to the previous output's cache blob
+        // (`link_or_copy`), so this override must replace the path, not write through it:
+        // `fs::copy` would truncate the file every other name shares.
+        if destination.exists() {
+            fs::remove_file(&destination)?;
+        }
         fs::copy(source, destination)?;
     }
     Ok(())
@@ -1064,21 +1075,6 @@ fn extension(path: &Path, expected: &[&str]) -> bool {
 /// This avoids creating double-nested output directory structures when processing
 /// assets extracted from BSA archives or loose mod folders with mixed-case naming
 /// (such as `Textures\actors\dragon.dds` or `Meshes\armor\iron.nif`).
-/// Puts `from`'s bytes at `to` as a hard link where the filesystem allows one, else as a copy.
-///
-/// A reconversion reuses every unchanged output of the previous run; copying them made staging
-/// as large as the published set (about 50 GB of textures for Skyrim), where a link costs nothing.
-/// Linking is safe because nothing writes a staged output in place: textures and meshes are
-/// written to a temporary file and renamed over their output, and a reconverted output first
-/// unlinks the staged file at its path. Publishing renames staging over the output and deletes
-/// the old output, which only drops one of the two links.
-fn link_or_copy(from: &Path, to: &Path) -> std::io::Result<()> {
-    if to.exists() {
-        fs::remove_file(to)?;
-    }
-    fs::hard_link(from, to).or_else(|_| fs::copy(from, to).map(|_| ()))
-}
-
 fn staging_path(output: &Path) -> PathBuf {
     let stamp = SystemTime::now()
         .duration_since(UNIX_EPOCH)
@@ -1151,6 +1147,43 @@ mod tests {
         assert_eq!(fs::read(&staged).unwrap(), b"texture+");
     }
 
+    #[test]
+    fn loose_asset_override_does_not_write_through_a_linked_vfs_entry() {
+        let directory = tempfile::tempdir().unwrap();
+        let previous = directory.path().join("previous");
+        let previous_vfs = previous.join("vfs/textures/rock.dds");
+        fs::create_dir_all(previous_vfs.parent().unwrap()).unwrap();
+        fs::write(&previous_vfs, b"archive bytes").unwrap();
+
+        // What the first conversion publishes: the extracted `vfs` file and its content-addressed
+        // blob share one file (`persist_cache_blobs`).
+        let hash = "ab".repeat(32);
+        let previous_blob = previous.join(".ingestion-cache/sha256/ab").join(&hash);
+        fs::create_dir_all(previous_blob.parent().unwrap()).unwrap();
+        link_or_copy(&previous_vfs, &previous_blob).unwrap();
+
+        // What a reconversion stages for the same archive entry: the cache hit links again.
+        let staging_vfs = directory.path().join("staging/vfs");
+        let staged = staging_vfs.join("textures/rock.dds");
+        fs::create_dir_all(staged.parent().unwrap()).unwrap();
+        link_or_copy(&previous_blob, &staged).unwrap();
+
+        let data = directory.path().join("Data");
+        fs::create_dir_all(data.join("textures")).unwrap();
+        let loose = data.join("textures/rock.dds");
+        fs::write(&loose, b"loose override").unwrap();
+
+        overlay_loose_assets(&data, &staging_vfs, &[loose]).unwrap();
+
+        assert_eq!(fs::read(&staged).unwrap(), b"loose override");
+        assert_eq!(
+            fs::read(&previous_vfs).unwrap(),
+            b"archive bytes",
+            "the override wrote through the link into the previous output"
+        );
+        assert_eq!(fs::read(&previous_blob).unwrap(), b"archive bytes");
+    }
+
     #[test]
     fn reconverting_a_linked_staged_output_leaves_the_published_file_alone() {
         let directory = tempfile::tempdir().unwrap();
```

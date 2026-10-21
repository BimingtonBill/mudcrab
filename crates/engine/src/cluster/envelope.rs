//! Generic extension table/index/file inventory integrity verification.
//! Recorded file digests must be checked against actual bytes by each payload consumer.

use color_eyre::eyre::{Result, ensure, eyre};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use shared::extensions::{ExtensionEntry, ExtensionHasher};
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, File, Metadata},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
pub(crate) fn recompute(
    connection: &Connection,
    root: &Path,
    entry: &ExtensionEntry,
) -> Result<String> {
    recompute_recorded(connection, root, entry, &HashMap::new())
}

/// Recompute the extension hash, taking the digest of each owned file listed in `recorded` (relative path to
/// SHA-256 hex) instead of reading the file. Every other owned file is read and hashed.
///
/// The result still equals the certified hash only if every recorded digest is the one the
/// producer hashed, so a wrong or stale record fails here. What it does not prove is that a
/// recorded file's bytes on disk still match: the caller must check each recorded file against
/// its digest before it uses the bytes.
pub(crate) fn recompute_recorded(
    connection: &Connection,
    root: &Path,
    entry: &ExtensionEntry,
    recorded: &HashMap<String, String>,
) -> Result<String> {
    recompute_recorded_with_parallelism(
        connection,
        root,
        entry,
        recorded,
        std::thread::available_parallelism().map(|count| count.get()),
    )
}

fn recompute_recorded_with_parallelism(
    connection: &Connection,
    root: &Path,
    entry: &ExtensionEntry,
    recorded: &HashMap<String, String>,
    parallelism: std::io::Result<usize>,
) -> Result<String> {
    let started = std::time::Instant::now();
    let overlap = parallelism
        .as_ref()
        .ok()
        .copied()
        .filter(|&count| count > 3);
    let (mut hasher, files) = if let Some(available) = overlap {
        std::thread::scope(|scope| -> Result<_> {
            let worker = std::thread::Builder::new()
                .name("cluster-envelope-files".into())
                .spawn_scoped(scope, || {
                    owned_file_digests(root, entry, recorded, Ok(available - 3), true)
                });
            // Capture rather than propagate: even a table error must join every file worker.
            let tables = table_and_index_hasher(connection, entry, worker.is_ok());
            let files = match worker {
                Ok(worker) => worker
                    .join()
                    .map_err(|_| eyre!("cluster envelope owned-file worker panicked"))
                    .and_then(|result| result),
                Err(error) => {
                    bevy::log::warn!(%error, "envelope file overlap unavailable; retaining original serial file checks");
                    if tables.is_ok() {
                        owned_file_digests(root, entry, recorded, Ok(available - 2), false)
                    } else {
                        Ok(Vec::new())
                    }
                }
            };
            // The unchanged table/index order wins over any concurrent filesystem failure.
            Ok((tables?, files?))
        })?
    } else {
        let hasher = table_and_index_hasher(connection, entry, false)?;
        let workers = parallelism.map(|count| count.saturating_sub(2).max(1));
        let files = owned_file_digests(root, entry, recorded, workers, false)?;
        (hasher, files)
    };
    for (relative, digest) in files {
        hasher.add_file_digest(&relative, &digest);
    }
    bevy::log::info!(extension = %entry.id, phase = "tables_and_owned_files_joined",
        wall_ms = started.elapsed().as_secs_f64() * 1000.0,
        overlap_requested = overlap.is_some(),
        "cluster envelope combined wall time (child phases may overlap)");
    let started = std::time::Instant::now();
    let digest = hasher.finish();
    bevy::log::info!(extension = %entry.id, phase = "finalize_hash",
        wall_ms = started.elapsed().as_secs_f64() * 1000.0, "cluster envelope phase");
    Ok(digest)
}

fn table_and_index_hasher(
    connection: &Connection,
    entry: &ExtensionEntry,
    overlap: bool,
) -> Result<ExtensionHasher> {
    let phase_started = std::time::Instant::now();
    let mut hasher = ExtensionHasher::new(&entry.id, entry.version);
    for table in &entry.owns.tables {
        let started = std::time::Instant::now();
        hasher
            .add_table(connection, table)
            .map_err(|error| eyre!("{error:#}"))?;
        bevy::log::info!(
            table,
            wall_ms = started.elapsed().as_secs_f64() * 1000.0,
            "cluster envelope table"
        );
    }
    for index in &entry.owns.indexes {
        hasher
            .add_index(connection, index)
            .map_err(|error| eyre!("{error:#}"))?;
    }
    bevy::log::info!(extension = %entry.id, phase = "tables_and_indexes", wall_ms = phase_started.elapsed().as_secs_f64() * 1000.0,
        tables = entry.owns.tables.len(), indexes = entry.owns.indexes.len(),
        overlaps_owned_files = overlap, "cluster envelope phase (concurrent phases overlap)");
    Ok(hasher)
}

fn owned_file_digests(
    root: &Path,
    entry: &ExtensionEntry,
    recorded: &HashMap<String, String>,
    workers: std::io::Result<usize>,
    overlap: bool,
) -> Result<Vec<(String, String)>> {
    let phase_started = std::time::Instant::now();
    let mut files = BTreeSet::new();
    for owned in &entry.owns.files {
        let directory = owned.ends_with('/');
        let relative = if directory {
            &owned[..owned.len() - 1]
        } else {
            owned.as_str()
        };
        safe_relative(relative)?;
        let path = root.join(relative);
        if directory {
            // ext-hash-v1 owns the files under a folder, not its existence. An empty
            // certified stage may never create clusters/. Check its parent first so
            // absence cannot conceal an existing linked ancestor or an I/O error.
            check_path(
                path.parent()
                    .ok_or_else(|| eyre!("owned folder has no parent"))?,
            )?;
            match fs::symlink_metadata(&path) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
        }
        check_path(&path)?;
        if directory {
            ensure!(
                fs::metadata(&path)?.is_dir(),
                "owned folder missing: {owned}"
            );
            collect(root, &path, &mut files)?;
        } else {
            regular(&path)?;
            ensure!(files.insert(relative.to_owned()), "duplicate owned file");
        }
    }
    bevy::log::info!(extension = %entry.id, phase = "owned_file_walk",
        wall_ms = phase_started.elapsed().as_secs_f64() * 1000.0,
        overlaps_tables = overlap, "cluster envelope phase (concurrent phases overlap)");
    let file_count = files.len();
    let (listed, read): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|relative| recorded.contains_key(relative));
    let mut digests: Vec<_> = listed
        .iter()
        .map(|relative| (relative.clone(), recorded[relative].clone()))
        .collect();
    // Even a failed worker-count lookup retains its original position after the file walk.
    let workers = workers?;
    let mut bytes = 0u64;
    for (relative, result) in read.iter().zip(hash_owned_files(root, &read, workers)?) {
        let (digest, size) = result?;
        digests.push((relative.clone(), digest));
        bytes += size;
    }
    bevy::log::info!(extension = %entry.id, phase = "owned_files", wall_ms = phase_started.elapsed().as_secs_f64() * 1000.0,
        files = file_count, recorded = listed.len(), read = read.len(), bytes,
        overlaps_tables = overlap, "cluster envelope phase (includes owned_file_walk; concurrent phases overlap)");
    Ok(digests)
}

/// Hash all the same bytes, retaining sorted results so concurrent failures have
/// the same first error as the serial ownership walk. The source lock outlives
/// every worker, and digest_file still checks every ancestor and regular file.
fn hash_owned_files(
    root: &Path,
    files: &[String],
    workers: usize,
) -> Result<Vec<Result<(String, u64)>>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers.max(1).min(files.len()) {
            let next = &next;
            handles.push(std::thread::Builder::new().spawn_scoped(scope, move || {
                let mut results = Vec::new();
                let mut buffer = vec![0u8; 1024 * 1024];
                let mut timings = FileHashTimings::default();
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(relative) = files.get(index) else {
                        break;
                    };
                    let result = (|| {
                        let path = root.join(relative);
                        let digest = digest_file_buffered(&path, &mut buffer, &mut timings)?;
                        let bytes = fs::metadata(path).map_or(0, |metadata| metadata.len());
                        Ok((digest, bytes))
                    })();
                    results.push((index, result));
                }
                bevy::log::info!(
                    files = results.len(),
                    path_check_us = timings.path_check.as_micros(),
                    read_us = timings.read.as_micros(),
                    hash_us = timings.hash.as_micros(),
                    "cluster owned-file worker totals (overlap across workers)"
                );
                results
            })?);
        }
        let mut results = Vec::with_capacity(files.len());
        for handle in handles {
            results.extend(handle.join().expect("owned-file hash worker panicked"));
        }
        results.sort_unstable_by_key(|(index, _)| *index);
        Ok(results.into_iter().map(|(_, result)| result).collect())
    })
}

fn safe_relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && !path.contains(['\\', ':', '\0']) && !path.contains("//"),
        "unsafe owned path"
    );
    ensure!(
        Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "unsafe owned path"
    );
    // components() normalizes embedded '.', so reject those explicitly too.
    ensure!(
        path.split('/').all(|part| !matches!(part, "." | ".." | "")),
        "unsafe owned path"
    );
    Ok(())
}

fn check_path(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir),
            "parent traversal in dataset path"
        );
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current)?;
        ensure!(
            !metadata.file_type().is_symlink() && !is_reparse(&metadata),
            "dataset path traverses a link/reparse point: {}",
            current.display()
        );
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(_: &Metadata) -> bool {
    false
}

fn regular(path: &Path) -> Result<()> {
    check_path(path)?;
    ensure!(
        fs::metadata(path)?.is_file(),
        "owned path is not a regular file: {}",
        path.display()
    );
    Ok(())
}

fn collect(root: &Path, directory: &Path, files: &mut BTreeSet<String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        // The caller checked the owned folder's ancestors; each entry below it is checked here
        // from its own listing metadata, which does not follow links, before it is entered.
        let metadata = entry.metadata()?;
        ensure!(
            !metadata.file_type().is_symlink() && !is_reparse(&metadata),
            "dataset path traverses a link/reparse point: {}",
            path.display()
        );
        if metadata.is_dir() {
            collect(root, &path, files)?;
        } else {
            ensure!(metadata.is_file(), "non-regular owned file");
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or_else(|| eyre!("non-UTF8 owned path"))?
                .replace('\\', "/");
            safe_relative(&relative)?;
            ensure!(files.insert(relative), "overlapping owned file claims");
        }
    }
    Ok(())
}

#[derive(Default)]
struct FileHashTimings {
    path_check: std::time::Duration,
    read: std::time::Duration,
    hash: std::time::Duration,
}

#[cfg(test)]
fn digest_file(path: &Path) -> Result<String> {
    digest_file_buffered(
        path,
        &mut vec![0u8; 1024 * 1024],
        &mut FileHashTimings::default(),
    )
}

fn digest_file_buffered(
    path: &Path,
    buffer: &mut [u8],
    timings: &mut FileHashTimings,
) -> Result<String> {
    let started = std::time::Instant::now();
    regular(path)?;
    let mut reader = File::open(path)?;
    timings.path_check += started.elapsed();
    let mut hasher = Sha256::new();
    loop {
        let started = std::time::Instant::now();
        let count = reader.read(buffer)?;
        timings.read += started.elapsed();
        if count == 0 {
            break;
        }
        let started = std::time::Instant::now();
        hasher.update(&buffer[..count]);
        timings.hash += started.elapsed();
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::extensions::ExtensionOwns;
    use std::collections::BTreeMap;

    fn owner(root: &Path) {
        fs::write(
            root.join("OWNER.txt"),
            "owner: B03 generic envelope test fixture\n",
        )
        .unwrap();
    }

    fn owned_tempdir() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        owner(directory.path());
        directory
    }

    fn owned_dir(path: &Path) {
        fs::create_dir(path).unwrap();
        owner(path);
    }

    fn dataset_connection(root: &Path) -> Connection {
        Connection::open(root.join("skyrim_world.db")).unwrap()
    }

    fn fixture() -> (tempfile::TempDir, Vec<ExtensionEntry>) {
        let directory = owned_tempdir();
        let root = directory.path();
        owned_dir(&root.join("assets"));
        fs::write(root.join("assets/fixture.bin"), b"generic payload bytes").unwrap();
        fs::write(root.join("manifest.json"), b"generic inventory manifest").unwrap();
        let connection = dataset_connection(root);
        connection
            .execute_batch(
                "CREATE TABLE inventory(id INTEGER PRIMARY KEY, path TEXT NOT NULL);
             INSERT INTO inventory VALUES (2, 'second'), (1, 'first');
             CREATE INDEX inventory_path ON inventory(path);",
            )
            .unwrap();
        let mut entry = ExtensionEntry {
            id: "test.inventory".into(),
            version: 1,
            owns: ExtensionOwns {
                tables: vec!["inventory".into()],
                indexes: vec!["inventory_path".into()],
                files: vec!["manifest.json".into(), "assets/".into()],
                ..Default::default()
            },
            sha256: String::new(),
            built_from: "generic-core".into(),
            stats: BTreeMap::new(),
        };
        entry.sha256 = converter::extensions::recompute_hash(root, &entry).unwrap();
        assert_eq!(recompute(&connection, root, &entry).unwrap(), entry.sha256);
        (directory, vec![entry])
    }
    #[test]
    fn overlap_matches_original_digest_with_recorded_and_fresh_files() {
        let (directory, entries) = fixture();
        let root = directory.path();
        let overlay = root.to_path_buf();
        let connection = dataset_connection(root);
        let recorded = HashMap::from([(
            "assets/fixture.bin".to_owned(),
            digest_file(&overlay.join("assets/fixture.bin")).unwrap(),
        )]);
        let expected = converter::extensions::recompute_hash(&overlay, &entries[0]).unwrap();
        for parallelism in [1, 2, 3, 8] {
            for supplied in [&HashMap::new(), &recorded] {
                assert_eq!(
                    recompute_recorded_with_parallelism(
                        &connection,
                        &overlay,
                        &entries[0],
                        supplied,
                        Ok(parallelism),
                    )
                    .unwrap(),
                    expected
                );
            }
        }
    }

    fn serial_and_overlap_error(
        connection: &Connection,
        root: &Path,
        entry: &ExtensionEntry,
    ) -> String {
        let serial =
            recompute_recorded_with_parallelism(connection, root, entry, &HashMap::new(), Ok(1))
                .unwrap_err()
                .to_string();
        let parallel =
            recompute_recorded_with_parallelism(connection, root, entry, &HashMap::new(), Ok(8))
                .unwrap_err()
                .to_string();
        assert_eq!(parallel, serial);
        serial
    }

    #[test]
    fn overlap_preserves_table_error_before_a_missing_owned_file() {
        let (directory, mut entries) = fixture();
        let overlay = directory.path().to_path_buf();
        let connection = dataset_connection(directory.path());
        entries[0].owns.tables.push("missing_table".into());
        fs::remove_file(overlay.join("manifest.json")).unwrap();
        assert!(
            serial_and_overlap_error(&connection, &overlay, &entries[0]).contains("missing_table")
        );
    }

    #[test]
    fn overlap_preserves_index_error_before_an_unsafe_owned_path() {
        let (directory, mut entries) = fixture();
        let overlay = directory.path().to_path_buf();
        let connection = dataset_connection(directory.path());
        entries[0].owns.indexes.push("missing_index".into());
        entries[0].owns.files.push("../unsafe".into());
        assert!(
            serial_and_overlap_error(&connection, &overlay, &entries[0]).contains("missing_index")
        );
    }

    #[test]
    fn overlap_keeps_the_first_owned_path_error_when_two_files_are_invalid() {
        let (directory, mut entries) = fixture();
        let overlay = directory.path().to_path_buf();
        let connection = dataset_connection(directory.path());
        let first = overlay.join("a-invalid-file");
        fs::create_dir(&first).unwrap();
        fs::write(
            first.join("OWNER.txt"),
            "owner: envelope overlap regression fixture\n",
        )
        .unwrap();
        entries[0].owns.files = vec!["a-invalid-file".into(), "z-missing-file".into()];
        assert!(
            serial_and_overlap_error(&connection, &overlay, &entries[0]).contains("a-invalid-file")
        );
    }

    #[test]
    fn parallel_owned_hashes_match_serial_bytes_and_first_refusal() {
        let directory = owned_tempdir();
        let root = directory.path();
        let files: Vec<_> = (0..19).map(|index| format!("file-{index:02}")).collect();
        for (index, file) in files.iter().enumerate() {
            fs::write(root.join(file), vec![index as u8; index * 9173]).unwrap();
        }
        let serial = |files: &[String]| {
            files
                .iter()
                .map(|file| {
                    let path = root.join(file);
                    digest_file(&path).map(|digest| (digest, fs::metadata(path).unwrap().len()))
                })
                .collect::<Vec<_>>()
        };
        let expected: Vec<_> = serial(&files).into_iter().map(Result::unwrap).collect();
        for workers in [1, 2, 8, 32] {
            let found: Vec<_> = hash_owned_files(root, &files, workers)
                .unwrap()
                .into_iter()
                .map(Result::unwrap)
                .collect();
            assert_eq!(found, expected);
        }
        // Two different failures must still report the earlier sorted path.
        fs::remove_file(root.join(&files[3])).unwrap();
        fs::create_dir(root.join(&files[3])).unwrap();
        owner(&root.join(&files[3]));
        fs::remove_file(root.join(&files[9])).unwrap();
        let expected = serial(&files)
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap_err();
        for workers in [1, 2, 8, 32] {
            let error = hash_owned_files(root, &files, workers)
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>>>()
                .unwrap_err();
            assert_eq!(error.to_string(), expected.to_string());
        }
        assert!(hash_owned_files(root, &[], 8).unwrap().is_empty());
    }

    #[test]
    fn recorded_digests_stand_in_for_reading_only_when_they_match() {
        let (directory, entries) = fixture();
        let root = directory.path();
        let connection = dataset_connection(root);
        let path = "assets/fixture.bin";
        let recorded = HashMap::from([(path.to_owned(), digest_file(&root.join(path)).unwrap())]);
        assert_eq!(
            recompute_recorded(&connection, root, &entries[0], &recorded).unwrap(),
            entries[0].sha256
        );
        fs::write(root.join(path), b"changed").unwrap();
        assert_eq!(
            recompute_recorded(&connection, root, &entries[0], &recorded).unwrap(),
            entries[0].sha256
        );
        assert_ne!(
            recompute(&connection, root, &entries[0]).unwrap(),
            entries[0].sha256
        );
        let wrong = HashMap::from([(path.to_owned(), "0".repeat(64))]);
        assert_ne!(
            recompute_recorded(&connection, root, &entries[0], &wrong).unwrap(),
            entries[0].sha256
        );
    }

    #[test]
    fn an_empty_owned_folder_and_its_absence_have_the_same_extension_hash() {
        let (directory, mut entries) = fixture();
        let root = directory.path();
        let connection = dataset_connection(root);
        entries[0].owns.files = vec!["empty/".into(), "manifest.json".into()];
        let absent_hash = recompute(&connection, root, &entries[0]).unwrap();
        owned_dir(&root.join("empty"));
        // Every created directory receives OWNER.txt. Move its marker to the owned test
        // root for the deliberate empty-directory mutation; otherwise it is not empty.
        fs::rename(root.join("empty/OWNER.txt"), root.join("empty-OWNER.txt")).unwrap();
        let empty_result = recompute(&connection, root, &entries[0]);
        // Restore the directory's marker before any assertions can unwind.
        fs::rename(root.join("empty-OWNER.txt"), root.join("empty/OWNER.txt")).unwrap();
        let empty_hash = empty_result.unwrap();
        assert_eq!(empty_hash, absent_hash);
        fs::rename(root.join("empty/OWNER.txt"), root.join("empty-OWNER.txt")).unwrap();
        let producer = converter::extensions::recompute_hash(root, &entries[0]);
        fs::rename(root.join("empty-OWNER.txt"), root.join("empty/OWNER.txt")).unwrap();
        assert_eq!(producer.unwrap(), empty_hash);
        entries[0].owns.files = vec!["missing-mandatory-file".into()];
        assert!(recompute(&connection, root, &entries[0]).is_err());
    }

    #[test]
    fn unsafe_owned_paths_are_refused() {
        let (directory, mut entries) = fixture();
        let connection = dataset_connection(directory.path());
        for path in [
            "../file",
            "/file",
            "a/../file",
            "a/./file",
            "a\\file",
            "C:file",
            "a//file",
            "",
            "a\0file",
        ] {
            assert!(safe_relative(path).is_err(), "{path:?}");
            entries[0].owns.files = vec![path.into()];
            assert!(
                recompute(&connection, directory.path(), &entries[0]).is_err(),
                "{path:?}"
            );
        }
        assert!(check_path(&directory.path().join("assets/../manifest.json")).is_err());
    }

    #[test]
    fn nonregular_explicit_files_and_folder_claims_are_refused() {
        let (directory, mut entries) = fixture();
        let connection = dataset_connection(directory.path());
        entries[0].owns.files = vec!["assets".into()];
        assert!(
            serial_and_overlap_error(&connection, directory.path(), &entries[0])
                .contains("not a regular file")
        );
        entries[0].owns.files = vec!["manifest.json/".into()];
        assert!(
            serial_and_overlap_error(&connection, directory.path(), &entries[0])
                .contains("owned folder missing")
        );
    }

    #[test]
    fn worker_count_error_remains_after_tables_indexes_and_file_walk() {
        let (directory, mut entries) = fixture();
        let connection = dataset_connection(directory.path());
        let run = |entry: &ExtensionEntry| {
            recompute_recorded_with_parallelism(
                &connection,
                directory.path(),
                entry,
                &HashMap::new(),
                Err(std::io::Error::other("worker-count lookup failed")),
            )
            .unwrap_err()
            .to_string()
        };
        assert!(run(&entries[0]).contains("worker-count lookup failed"));
        entries[0].owns.files = vec!["../unsafe".into()];
        assert!(run(&entries[0]).contains("unsafe owned path"));
        entries[0].owns.indexes.push("missing_index".into());
        assert!(run(&entries[0]).contains("missing_index"));
        entries[0].owns.tables.push("missing_table".into());
        assert!(run(&entries[0]).contains("missing_table"));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn linked_files_folders_and_ancestors_are_refused_even_when_recorded_or_absent() {
        let (directory, mut entries) = fixture();
        let root = directory.path();
        let connection = dataset_connection(root);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("assets/fixture.bin"), root.join("linked-file"))
                .unwrap();
            std::os::unix::fs::symlink(root.join("assets"), root.join("linked-dir")).unwrap();
        }
        #[cfg(windows)]
        {
            // No permission-based skip: Windows requires symlink rights or Developer Mode.
            std::os::windows::fs::symlink_file(
                root.join("assets/fixture.bin"),
                root.join("linked-file"),
            )
            .unwrap();
            std::os::windows::fs::symlink_dir(root.join("assets"), root.join("linked-dir"))
                .unwrap();
        }
        for path in [
            "linked-file",
            "linked-dir/",
            "linked-dir/fixture.bin",
            "linked-dir/absent/",
        ] {
            entries[0].owns.files = vec![path.into()];
            let recorded = HashMap::from([(path.to_owned(), "0".repeat(64))]);
            for parallelism in [1, 8] {
                let error = recompute_recorded_with_parallelism(
                    &connection,
                    root,
                    &entries[0],
                    &recorded,
                    Ok(parallelism),
                )
                .unwrap_err();
                assert!(
                    error.to_string().contains("link/reparse point"),
                    "{error:#}"
                );
            }
        }
        entries[0].owns.files = vec!["assets/".into()];
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("manifest.json"), root.join("assets/nested-link"))
            .unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(
            root.join("manifest.json"),
            root.join("assets/nested-link"),
        )
        .unwrap();
        assert!(
            serial_and_overlap_error(&connection, root, &entries[0]).contains("link/reparse point")
        );
        #[cfg(windows)]
        {
            assert!(is_reparse(
                &fs::symlink_metadata(root.join("linked-file")).unwrap()
            ));
            assert!(is_reparse(
                &fs::symlink_metadata(root.join("linked-dir")).unwrap()
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn nonregular_walk_entry_is_refused() {
        let (directory, entries) = fixture();
        let connection = dataset_connection(directory.path());
        let _socket =
            std::os::unix::net::UnixListener::bind(directory.path().join("assets/socket")).unwrap();
        assert!(
            serial_and_overlap_error(&connection, directory.path(), &entries[0])
                .contains("non-regular owned file")
        );
    }

    #[test]
    fn declared_table_index_and_path_order_determine_first_refusal() {
        let (directory, mut entries) = fixture();
        let root = directory.path();
        let connection = dataset_connection(root);
        entries[0].owns.tables = vec!["z-first-table".into(), "a-second-table".into()];
        entries[0].owns.indexes = vec!["z-first-index".into(), "a-second-index".into()];
        entries[0].owns.files = vec!["../unsafe".into()];
        assert!(serial_and_overlap_error(&connection, root, &entries[0]).contains("z-first-table"));
        entries[0].owns.tables = vec!["inventory".into()];
        assert!(serial_and_overlap_error(&connection, root, &entries[0]).contains("z-first-index"));
        entries[0].owns.indexes = vec!["inventory_path".into()];
        owned_dir(&root.join("z-first-path"));
        entries[0].owns.files = vec!["z-first-path".into(), "a-second-path".into()];
        assert!(serial_and_overlap_error(&connection, root, &entries[0]).contains("z-first-path"));
    }

    #[test]
    fn duplicate_explicit_and_overlapping_directory_file_claims_are_refused() {
        let (directory, mut entries) = fixture();
        let root = directory.path();
        let connection = dataset_connection(root);
        entries[0].owns.files = vec!["manifest.json".into(), "manifest.json".into()];
        assert!(
            serial_and_overlap_error(&connection, root, &entries[0])
                .contains("duplicate owned file")
        );
        entries[0].owns.files = vec!["assets/fixture.bin".into(), "assets/".into()];
        assert!(
            serial_and_overlap_error(&connection, root, &entries[0])
                .contains("overlapping owned file claims")
        );
    }
}

use super::*;
use crate::derive_database;
use crate::{cache::{CacheEntry, hash_file}, cluster::probe::ProbeResult, esm::exporter::create_tables};
use shared::extensions::{CLUSTERS, DataExtensions, ExtensionHasher, ExtensionOwns};
use std::collections::BTreeMap;
use tempfile::TempDir;

const CORE: &str = "c1";
const FOREIGN: &str = "test.foreign";

struct Fixture {
    directory: TempDir,
    options: DeriveOptions,
    foreign: ExtensionEntry,
}

fn probe() -> Option<(ProbeResult, f64)> {
    Some((
        ProbeResult {
            passed: true,
            missing: Vec::new(),
            backend: "synthetic-no-device".into(),
            adapter_name: "CPU test capability fact, no GPU requested".into(),
            json: serde_json::json!({"test": true}),
        },
        0.0,
    ))
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("OWNER.txt"),
        "owner: derive regression fixture\n",
    )
    .unwrap();
    let source = directory.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("OWNER.txt"),
        "owner: synthetic source fixture\n",
    )
    .unwrap();
    fs::create_dir(source.join("scripts")).unwrap();
    fs::write(source.join("scripts/probe.luau"), b"return 1\n").unwrap();
    fs::create_dir(source.join("foreign")).unwrap();
    fs::write(
        source.join("foreign/data.bin"),
        b"foreign-owned immutable bytes",
    )
    .unwrap();
    fs::write(
        source.join("record-warnings.json"),
        b"{\"record_data_complete\":false,\"warnings\":[\"invented record limitation\"]}\n",
    )
    .unwrap();
    let connection = Connection::open(source.join("skyrim_world.db")).unwrap();
    create_tables(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO lod_build(id,build_identity) VALUES(1,?1)",
            [CORE.repeat(32)],
        )
        .unwrap();
    connection.execute_batch("INSERT INTO worldspaces(id,editor_id,parent_world,flags,lod_origin_x,lod_origin_y) VALUES(700,'InventedWorld',NULL,0,13,-7);
            CREATE TABLE foreign_rows(id INTEGER PRIMARY KEY,value BLOB NOT NULL DEFAULT X'01' CHECK(length(value)>0));
            CREATE INDEX foreign_value ON foreign_rows(value);
            INSERT INTO foreign_rows VALUES(1,X'00FF');").unwrap();
    let owns = ExtensionOwns {
        tables: vec!["foreign_rows".into()],
        indexes: vec!["foreign_value".into()],
        files: vec!["foreign/data.bin".into()],
        gltf_extensions: Vec::new(),
    };
    let mut hasher = ExtensionHasher::new(FOREIGN, 1);
    hasher.add_table(&connection, "foreign_rows").unwrap();
    hasher.add_index(&connection, "foreign_value").unwrap();
    hasher.add_file(
        "foreign/data.bin",
        &fs::read(source.join("foreign/data.bin")).unwrap(),
    );
    let foreign = ExtensionEntry {
        id: FOREIGN.into(),
        version: 1,
        owns,
        sha256: hasher.finish(),
        built_from: CORE.repeat(32),
        stats: BTreeMap::from([("rows".into(), 1)]),
    };
    drop(connection);
    let mut manifest = ConversionManifest {
        schema_version: CONVERTER_SCHEMA_VERSION,
        complete: false,
        record_data_complete: Some(false),
        ..Default::default()
    };
    manifest.entries.insert(
        "scripts/probe.luau".into(),
        CacheEntry {
            source_hash: "02".repeat(32),
            output: "scripts/probe.luau".into(),
            output_size: 9,
            output_hash: hash_file(&source.join("scripts/probe.luau")).unwrap(),
        },
    );
    manifest.extensions = crate::extensions::build_entries(&source).unwrap();
    manifest.extensions.push(foreign.clone());
    let mut raw = serde_json::to_value(manifest).unwrap();
    raw["forward_compatible_fixture"] = serde_json::json!({
        "warning_file": "record-warnings.json", "nested": [true, {"kept": 42}]
    });
    let foreign_raw = raw["extensions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["id"] == FOREIGN)
        .unwrap();
    foreign_raw["future_foreign_metadata"] = serde_json::json!({
        "retained": ["opaque future field", {"version": 7}], "flag": true
    });
    fs::write(
        source.join("conversion-manifest.json"),
        serde_json::to_vec_pretty(&raw).unwrap(),
    )
    .unwrap();
    Fixture {
        options: DeriveOptions {
            source,
            label: "default".into(),
            replace: false,
            keep_failed_staging: false,

            jobs: 1,
            triangle_threshold: 12,
            include_trees: false,
        },
        directory,
        foreign,
    }
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, String> {
    fn walk(root: &Path, directory: &Path, output: &mut BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            assert!(!is_link(&metadata));
            if metadata.is_dir() {
                if directory == root && entry.file_name() == "derived" {
                    continue;
                }
                walk(root, &entry.path(), output);
            } else {
                assert!(metadata.is_file());
                output.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    hash_file(&entry.path()).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

fn mutate_manifest(fixture: &Fixture, change: impl FnOnce(&mut Value)) {
    let path = fixture.options.source.join("conversion-manifest.json");
    let mut raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    change(&mut raw);
    fs::write(path, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();
}

fn output(options: &DeriveOptions) -> PathBuf {
    options
        .source
        .join("derived")
        .join(format!("clusters-{}", settings_id(options)))
}

#[test]
fn real_derive_hashes_every_base_file_and_preserves_foreign_and_unknown_metadata() {
    let fixture = fixture();
    let source = &fixture.options.source;
    let before_files = inventory(source);
    let before_rows = derive_database::snapshot(
        &Connection::open(source.join("skyrim_world.db")).unwrap(),
        &BTreeSet::new(),
    )
    .unwrap();
    let report = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    assert!(report.incomplete_core);
    assert_eq!(
        report.output,
        fs::canonicalize(output(&fixture.options)).unwrap()
    );
    assert_eq!(inventory(source), before_files, "base file bytes changed");
    assert_eq!(
        derive_database::snapshot(
            &Connection::open(source.join("skyrim_world.db")).unwrap(),
            &BTreeSet::new()
        )
        .unwrap(),
        before_rows
    );
    let metadata = shared::cluster_overlay::checked_overlay(source, &report.output).unwrap();
    assert_eq!(metadata.base_core_identity, CORE.repeat(32));
    assert_eq!(metadata.label, "default");
    assert_eq!(metadata.build_settings.settings_id(), metadata.settings_id);
    assert_eq!(
        report.output.file_name().unwrap().to_string_lossy(),
        format!("clusters-{}", metadata.build_settings.settings_id())
    );
    assert_eq!(
        DataExtensions::load(source).unwrap().get(FOREIGN),
        Some(&fixture.foreign)
    );
    let cluster = metadata
        .extensions
        .iter()
        .find(|entry| entry.id == CLUSTERS)
        .unwrap();
    assert_eq!(cluster.built_from, CORE.repeat(32));
    assert_eq!(cluster.stats["derived_over_incomplete_core"], 1);
    assert!(report.output.join("cluster-manifest.json").is_file());
    for absent in [
        "skyrim_world.db",
        "conversion-manifest.json",
        "foreign",
        "scripts",
        "meshes",
        "textures",
        "vfs",
    ] {
        assert!(
            !report.output.join(absent).exists(),
            "overlay copied {absent}"
        );
    }
    let db = Connection::open(report.output.join("clusters.db")).unwrap();
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(
        tables.iter().all(|name| name.starts_with("cluster_")),
        "core table copied: {tables:?}"
    );
    let reader = shared::cluster_overlay::open_overlay_database(source, &report.output).unwrap();
    assert!(reader.execute("DELETE FROM cluster_build", []).is_err());
    assert!(reader.execute("DELETE FROM lod_build", []).is_err());
}

#[test]
fn bad_source_contract_or_existing_cluster_store_never_publishes_output() {
    for case in [
        "converter_schema",
        "database_schema",
        "missing_identity",
        "changed_asset_hash",
        "cluster_table",
        "cluster_folder",
        "database_sidecar",
        "manifest_owns_derived",
        "future_builtin",
        "wrong_builtin_owns",
        "missing_builtin",
    ] {
        let fixture = fixture();
        match case {
            "converter_schema" => mutate_manifest(&fixture, |raw| {
                raw["schema_version"] = serde_json::json!(25)
            }),
            "database_schema" => Connection::open(fixture.options.source.join("skyrim_world.db"))
                .unwrap()
                .execute("UPDATE schema_info SET version=8", [])
                .map(|_| ())
                .unwrap(),
            "missing_identity" => Connection::open(fixture.options.source.join("skyrim_world.db"))
                .unwrap()
                .execute("DELETE FROM lod_build", [])
                .map(|_| ())
                .unwrap(),
            "changed_asset_hash" => fs::write(
                fixture.options.source.join("scripts/probe.luau"),
                b"return 2\n",
            )
            .unwrap(),
            "cluster_table" => Connection::open(fixture.options.source.join("skyrim_world.db"))
                .unwrap()
                .execute_batch("CREATE TABLE cluster_unowned_fixture(value);")
                .unwrap(),
            "manifest_owns_derived" => mutate_manifest(&fixture, |raw| {
                raw["entries"]["scripts/probe.luau"]["output"] =
                    serde_json::json!("derived/forbidden.bin")
            }),
            "database_sidecar" => fs::write(
                fixture.options.source.join("skyrim_world.db-shm"),
                b"sentinel sidecar",
            )
            .unwrap(),
            "cluster_folder" => fs::create_dir(fixture.options.source.join("clusters")).unwrap(),
            "future_builtin" => mutate_manifest(&fixture, |raw| {
                let entry = raw["extensions"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|entry| entry["id"] != FOREIGN)
                    .expect("fixture has actual built-ins");
                entry["version"] = serde_json::json!(99);
            }),
            "wrong_builtin_owns" => mutate_manifest(&fixture, |raw| {
                let entry = raw["extensions"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|entry| entry["id"] != FOREIGN)
                    .expect("fixture has actual built-ins");
                entry["owns"]["gltf_extensions"] = serde_json::json!(["invented_wrong_ownership"]);
            }),
            "missing_builtin" => mutate_manifest(&fixture, |raw| {
                let entries = raw["extensions"].as_array_mut().unwrap();
                let index = entries
                    .iter()
                    .position(|entry| entry["id"] != FOREIGN)
                    .expect("fixture has actual built-ins");
                entries.remove(index);
            }),
            _ => unreachable!(),
        }
        let before = inventory(&fixture.options.source);
        let error = run_with_probe(&fixture.options, probe()).err().expect(case);
        let expected = match case {
            "manifest_owns_derived" => "must not own files under derived",
            "database_sidecar" => "SQLite sidecars",
            "converter_schema" => "requires converter schema",
            "database_schema" => "requires the current world database schema",
            "missing_identity" => "core identity is missing or empty",
            "changed_asset_hash" => "source integrity check failed",
            "cluster_table" | "cluster_folder" => "existing cluster store",
            "future_builtin" | "wrong_builtin_owns" => "unsupported migration",
            "missing_builtin" => "omits a built-in extension declaration",
            _ => unreachable!(),
        };
        assert!(
            format!("{error:#}").contains(expected),
            "case {case}: wrong refusal: {error:#}"
        );
        assert!(
            !output(&fixture.options).exists(),
            "published rejected source case {case}"
        );
        assert_eq!(
            inventory(&fixture.options.source),
            before,
            "changed source case {case}"
        );
    }
}

#[test]
fn existing_overlay_needs_replace_and_successfully_replaces_only_its_own_variant() {
    let mut fixture = fixture();
    let first = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    let before = inventory(&first.output);
    let error = run_with_probe(&fixture.options, probe())
        .err()
        .expect("existing overlay must refuse");
    assert!(format!("{error:#}").contains("use --replace"));
    assert_eq!(inventory(&first.output), before);
    fixture.options.replace = true;
    fixture.options.label = "updated".into();
    let second = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    assert_eq!(first.output, second.output);
    assert_eq!(
        shared::cluster_overlay::checked_overlay(&fixture.options.source, &second.output)
            .unwrap()
            .label,
        "updated"
    );
    assert!(
        !fs::read_dir(second.output.parent().unwrap())
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".replaced-"))
    );
}

#[test]
fn admission_refuses_before_staging_and_propagates_reader_errors() {
    for fail_reader in [false, true] {
        let fixture = fixture();
        let before = inventory(&fixture.options.source);
        fs::create_dir(fixture.options.source.join("derived")).unwrap();
        let error = run_with_space(&fixture.options, probe(), move |_| {
            if fail_reader {
                bail!("injected unavailable space reader");
            }
            Ok(0)
        })
        .err()
        .unwrap();
        assert!(format!("{error:#}").contains(if fail_reader {
            "injected unavailable"
        } else {
            "needed"
        }));
        assert!(!output(&fixture.options).exists());
        assert!(
            !fs::read_dir(fixture.options.source.join("derived"))
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".staging-"))
        );
        assert_eq!(inventory(&fixture.options.source), before);
    }
}

#[test]
fn failed_staging_cleans_only_own_directory_and_keep_flag_retains_it() {
    for keep in [false, true] {
        let mut fixture = fixture();
        fixture.options.keep_failed_staging = keep;
        let derived = fixture.options.source.join("derived");
        fs::create_dir(&derived).unwrap();
        let other = derived.join(".staging-other-run");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("sentinel"), b"not ours").unwrap();
        let destination = output(&fixture.options);
        // The admission callback runs after output preflight. A competing path
        // appearing here makes real final publication fail after finalization.
        let competing = destination.clone();
        let mut injected = false;
        let error = run_with_space(&fixture.options, probe(), move |_| {
            if !injected {
                fs::create_dir(&competing)?;
                fs::write(competing.join("sentinel"), b"competing output")?;
                injected = true;
            }
            Ok(u64::MAX)
        })
        .err()
        .expect("late publication must refuse");
        assert!(format!("{error:#}").contains("use --replace"));
        let stages: Vec<_> = fs::read_dir(&derived)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".staging-")
                    && *path != other
            })
            .collect();
        assert_eq!(stages.len(), usize::from(keep));
        // A kept staging of this command is dead bookkeeping; the foreign one and the competing
        // output are content.
        let mut content = vec![
            ".staging-other-run".to_owned(),
            destination
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ];
        content.sort();
        assert_eq!(derived_content_to_discard(&derived).unwrap(), content);
        assert_eq!(
            fs::read(destination.join("sentinel")).unwrap(),
            b"competing output"
        );
        assert_eq!(fs::read(other.join("sentinel")).unwrap(), b"not ours");
        assert!(fixture.directory.path().exists());
    }
}

#[test]
fn settings_id_excludes_jobs_and_label_but_tracks_build_settings() {
    let mut fixture = fixture();
    let baseline = settings_id(&fixture.options);
    fixture.options.jobs = 17;
    fixture.options.label = "another label".into();
    assert_eq!(settings_id(&fixture.options), baseline);
    fixture.options.triangle_threshold += 1;
    assert_ne!(settings_id(&fixture.options), baseline);
    fixture.options.triangle_threshold -= 1;
    fixture.options.include_trees = true;
    assert_ne!(settings_id(&fixture.options), baseline);
}

#[test]
fn write_floor_failure_cleans_own_staging_or_keeps_it_on_request() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    for keep in [false, true] {
        let mut fixture = fixture();
        fixture.options.keep_failed_staging = keep;
        let before = inventory(&fixture.options.source);
        let derived = fixture.options.source.join("derived");
        fs::create_dir(&derived).unwrap();
        let foreign = derived.join(".staging-another-process");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("sentinel"), b"keep").unwrap();
        let reads = Arc::new(AtomicUsize::new(0));
        let sampled = reads.clone();
        let error = run_with_space(&fixture.options, probe(), move |_| {
            Ok(if sampled.fetch_add(1, Ordering::SeqCst) < 2 {
                u64::MAX
            } else {
                10 * 1024 * 1024 * 1024
            })
        })
        .err()
        .expect("falling free space must stop an admitted derivation");
        let message = format!("{error:#}");
        assert!(
            message.contains("needed") && message.contains("free"),
            "{message}"
        );
        assert!(reads.load(Ordering::SeqCst) >= 3);
        assert!(!output(&fixture.options).exists());
        let own_stages: Vec<_> = fs::read_dir(&derived)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_dir() && *path != foreign)
            .collect();
        assert_eq!(own_stages.len(), usize::from(keep));
        assert_eq!(fs::read(foreign.join("sentinel")).unwrap(), b"keep");
        assert_eq!(inventory(&fixture.options.source), before);
    }
}

#[test]
fn admission_uses_measured_payload_and_table_ratios() {
    let fixture = fixture();
    let base = &fixture.options.source;
    let derived = base.join("derived");
    fs::create_dir(&derived).unwrap();
    fs::create_dir(base.join("meshes")).unwrap();
    fs::write(base.join("meshes/measured.glb"), [0; 123]).unwrap();
    fs::write(base.join("cell_cache.rkyv"), [0; 73]).unwrap();
    // Lead's same-core 2026-10-20 measurements, refined by apparent-byte dust inventory:
    // meshes 2,570,110,144; object payloads 2,908,039,936; terrain 165,344,400;
    // cell cache 488,986,104; DB 2,173,202,432 -> 2,736,652,288 bytes.
    let ratio = |bytes: u64, output: u64, input: u64| (bytes * output).div_ceil(input);
    let tables = ratio(
        fs::metadata(base.join("skyrim_world.db")).unwrap().len(),
        563_449_856,
        2_173_202_432,
    );
    let objects = ratio(123, 2_908_039_936, 2_570_110_144);
    let terrain = ratio(73, 165_344_400, 488_986_104);
    let object_needed = 10 * 1024 * 1024 * 1024 + 1024 * 1024 + objects + tables;
    for (with_terrain, needed) in [(false, object_needed), (true, object_needed + terrain)] {
        derive_storage::admit(
            base,
            &derived,
            &output(&fixture.options),
            with_terrain,
            |_| Ok(needed),
        )
        .expect("measured fast-fail estimate must admit its exact boundary");
        assert!(
            derive_storage::admit(
                base,
                &derived,
                &output(&fixture.options),
                with_terrain,
                |_| { Ok(needed - 1) }
            )
            .is_err()
        );
    }
}

#[test]
fn admission_exact_budget_boundary() {
    let fixture = fixture();
    let base = &fixture.options.source;
    let derived = base.join("derived");
    fs::create_dir(&derived).unwrap();
    fs::create_dir(base.join("meshes")).unwrap();
    fs::write(base.join("meshes/invented.glb"), [0; 11]).unwrap();
    // Object-only derivation must not budget terrain geometry or atlas scratch.
    fs::write(base.join("cell_cache.rkyv"), [0; 73]).unwrap();
    let needed = 10 * 1024 * 1024 * 1024
        + 1024 * 1024
        + (11_u64 * 2_908_039_936).div_ceil(2_570_110_144)
        + (fs::metadata(base.join("skyrim_world.db")).unwrap().len() * 563_449_856)
            .div_ceil(2_173_202_432);
    derive_storage::admit(base, &derived, &output(&fixture.options), false, |_| {
        Ok(needed)
    })
    .unwrap();
    assert!(
        derive_storage::admit(base, &derived, &output(&fixture.options), false, |_| Ok(
            needed - 1
        ))
        .is_err()
    );
}

#[test]
fn previous_same_base_overlay_uses_actual_bytes_without_multiplier() {
    let fixture = fixture();
    let report = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    let base = &fixture.options.source;
    fs::create_dir(base.join("meshes")).unwrap();
    let mesh = fs::File::create(base.join("meshes/logical-size.glb")).unwrap();
    mesh.set_len(64 * 1024 * 1024).unwrap();
    drop(mesh);
    // The complete previous overlay is the hint; no multiplier or additional scratch budget.
    let actual: u64 = inventory(&report.output)
        .keys()
        .map(|path| fs::metadata(report.output.join(path)).unwrap().len())
        .sum();
    let needed = 10 * 1024 * 1024 * 1024 + actual;
    derive_storage::admit(base, &base.join("derived"), &report.output, false, |_| {
        Ok(needed)
    })
    .unwrap();
    assert!(
        derive_storage::admit(base, &base.join("derived"), &report.output, false, |_| Ok(
            needed - 1
        ))
        .is_err()
    );
}

#[test]
fn replace_can_rebuild_overlay_after_base_manifest_changed() {
    let mut fixture = fixture();
    let first = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    mutate_manifest(&fixture, |manifest| {
        manifest["new_unknown_field"] = serde_json::json!(true)
    });
    assert!(
        shared::cluster_overlay::checked_overlay(&fixture.options.source, &first.output).is_err()
    );
    fixture.options.replace = true;
    let second = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    shared::cluster_overlay::checked_overlay(&fixture.options.source, &second.output).unwrap();
}

#[test]
fn previous_same_base_overlay_can_raise_admission_and_foreign_hint_cannot() {
    let fixture = fixture();
    let report = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    // A bounded logical-size fixture raises the prior-overlay admission hint.
    let hint = fs::File::create(report.output.join("large-prior-payload")).unwrap();
    hint.set_len(64 * 1024 * 1024).unwrap();
    drop(hint);
    let base = &fixture.options.source;
    let seed_only = 10 * 1024 * 1024 * 1024
        + 1024 * 1024
        + (fs::metadata(base.join("skyrim_world.db")).unwrap().len() * 563_449_856)
            .div_ceil(2_173_202_432);
    assert!(
        derive_storage::admit(base, &base.join("derived"), &report.output, false, |_| Ok(
            seed_only
        ))
        .is_err()
    );
    let path = report.output.join("overlay.json");
    let mut metadata: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    metadata["base_fingerprint"] = serde_json::json!("different-base");
    fs::write(path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    derive_storage::admit(base, &base.join("derived"), &report.output, false, |_| {
        Ok(seed_only)
    })
    .unwrap();
}

#[test]
fn unrelated_mod_table_with_a_cluster_prefix_is_not_a_cluster_store() {
    let fixture = fixture();
    let db = Connection::open(fixture.options.source.join("skyrim_world.db")).unwrap();
    db.execute_batch("CREATE TABLE clustered_notes(value TEXT); INSERT INTO clustered_notes VALUES('foreign extension data');").unwrap();
    drop(db);
    preflight(&fixture.options.source).unwrap();
}

fn failures(fixture: &Fixture) {
    mutate_manifest(fixture, |raw| {
        raw["failures"] = serde_json::json!({
            "textures/a.dds": "invented conversion failure",
            "textures/b.dds": "another invented failure"
        })
    });
}

#[test]
fn recorded_asset_failures_are_admitted_with_a_loud_count_and_recorded() {
    let fixture = fixture();
    failures(&fixture);
    preflight(&fixture.options.source).unwrap();
    let report = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    assert!(report.incomplete_core);
    assert!(
        report
            .notices
            .iter()
            .any(|notice| notice.contains("2 failed asset conversion(s)")),
        "no loud failure notice: {:?}",
        report.notices
    );
    let metadata =
        shared::cluster_overlay::checked_overlay(&fixture.options.source, &report.output).unwrap();
    assert_eq!(metadata.base_failure_count, 2);
    // A base without failures records none.
    let clean = fixture_overlay_failure_count();
    assert_eq!(clean, 0);
}

fn fixture_overlay_failure_count() -> usize {
    let fixture = fixture();
    let report = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    shared::cluster_overlay::checked_overlay(&fixture.options.source, &report.output)
        .unwrap()
        .base_failure_count
}

#[test]
fn recorded_failures_do_not_excuse_other_integrity_problems_or_a_complete_flag() {
    let fixture = fixture();
    failures(&fixture);
    fs::write(
        fixture.options.source.join("scripts/probe.luau"),
        b"return 2\n",
    )
    .unwrap();
    let error = preflight(&fixture.options.source).err().unwrap();
    assert!(format!("{error:#}").contains("source integrity check failed"));
    let fixture = self::fixture();
    failures(&fixture);
    mutate_manifest(&fixture, |raw| raw["complete"] = serde_json::json!(true));
    let error = preflight(&fixture.options.source).err().unwrap();
    assert!(format!("{error:#}").contains("completeness flags disagree"));
    assert!(!output(&fixture.options).exists());
}

#[test]
fn labels_are_unique_within_a_base_and_replace_works_across_settings_ids() {
    let mut fixture = fixture();
    let first = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    let derived = fixture.options.source.join("derived");
    // After a derivation the folder holds the overlay and this command's bookkeeping only.
    assert_eq!(
        derived_content_to_discard(&derived).unwrap(),
        vec![
            first
                .output
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        ]
    );
    assert!(
        !fs::read_dir(&derived).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".staging-")),
        "a finished derivation left its staging or staging lock behind"
    );
    let before = inventory(&first.output);
    fixture.options.triangle_threshold = 16;
    let other_destination = output(&fixture.options);
    assert_ne!(other_destination, first.output);
    // Same label, other settings id: refused until --replace, and nothing changes.
    let error = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX))
        .err()
        .expect("a label is unique within a base");
    let text = format!("{error:#}");
    assert!(
        text.contains("already used") && text.contains("--replace"),
        "{text}"
    );
    assert!(!other_destination.exists());
    assert_eq!(inventory(&first.output), before);
    // Another label coexists.
    fixture.options.label = "other".into();
    let other = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    assert!(first.output.is_dir() && other.output.is_dir());
    // A derivation that is claiming the label blocks a concurrent one.
    fixture.options.label = "default".into();
    fixture.options.replace = true;
    let digest = crate::cache::hash_bytes(b"default");
    let claim =
        AssetLock::acquire_exclusive(derived.join(format!(".label-{}", &digest[..16]))).unwrap();
    let error = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX))
        .err()
        .expect("a concurrent claim blocks");
    assert!(format!("{error:#}").contains("claiming overlay label"));
    drop(claim);
    // With --replace the other settings id's overlay is replaced and the old one retired.
    fixture.options.triangle_threshold = 20;
    let third = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    assert!(third.output.is_dir());
    assert!(
        !first.output.exists(),
        "the overlay that held the label stays"
    );
    assert!(
        other.output.is_dir(),
        "an overlay with another label was touched"
    );
    assert_eq!(
        shared::cluster_overlay::checked_overlay(&fixture.options.source, &third.output)
            .unwrap()
            .label,
        "default"
    );
    assert!(
        !fs::read_dir(&derived).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".replaced-")),
        "a retired overlay was left behind"
    );
}

#[test]
fn a_failed_cross_id_replace_keeps_the_overlay_that_held_the_label() {
    let mut fixture = fixture();
    let first = run_with_space(&fixture.options, probe(), |_| Ok(u64::MAX)).unwrap();
    let before = inventory(&first.output);
    fixture.options.triangle_threshold = 16;
    fixture.options.replace = true;
    let destination = output(&fixture.options);
    // A late failure on the new overlay's destination (a file where the overlay belongs). The
    // storage tests pin the put-back after a failure at the exact publication step.
    let competing = destination.clone();
    let _error = run_with_space(&fixture.options, probe(), move |_| {
        fs::write(&competing, b"not an overlay")?;
        Ok(u64::MAX)
    })
    .err()
    .expect("the derivation must fail");
    assert_eq!(inventory(&first.output), before);
    assert_eq!(fs::read(&destination).unwrap(), b"not an overlay");
    let derived = fixture.options.source.join("derived");
    assert!(
        !fs::read_dir(&derived).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".replaced-")),
        "the set-aside overlay was not put back"
    );
    shared::cluster_overlay::checked_overlay(&fixture.options.source, &first.output).unwrap();
}

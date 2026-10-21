//! The manifest's extension hashes on real converter output: the Rust writer and the Python
//! reference (`tools/extension_hash.py`) must agree.

use dummy_content::layout;
use std::{fs, path::Path, process::Command};

fn generate_data(root: &Path) {
    layout::prepare_directory(root, false).unwrap();
    let formats = layout::Formats::parse("dds,pex,nif,esm").unwrap();
    layout::generate(root, layout::DEFAULT_SEED, formats).unwrap();
}

async fn convert(config: converter::PipelineConfig) -> converter::pipeline::PipelineReport {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let report = converter::AssetPipeline::run_async(config, tx)
        .await
        .unwrap();
    drain.await.unwrap();
    report
}

fn python() -> Option<&'static str> {
    ["python", "python3"].into_iter().find(|name| {
        Command::new(name)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}

#[tokio::test]
async fn python_reference_accepts_the_converter_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("Data");
    let output = directory.path().join("assets");
    generate_data(&data);
    let report = convert(converter::PipelineConfig::new(&data, &output)).await;
    assert!(report.complete, "{:?}", report.warnings);

    let manifest: converter::cache::ConversionManifest =
        serde_json::from_slice(&fs::read(output.join("conversion-manifest.json")).unwrap())
            .unwrap();
    let ids: Vec<&str> = manifest
        .extensions
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    for expected in [
        shared::extensions::LOD_INVENTORY,
        shared::extensions::SPACE_LIGHTING,
        shared::extensions::DIRECTIONAL_SNOW,
    ] {
        assert!(ids.contains(&expected), "{expected} not listed in {ids:?}");
    }
    assert!(
        !ids.contains(&shared::extensions::DOOR_LINKS),
        "door links are core output"
    );
    // The dummy-content meshes use neither `MUDCRAB_material` nor the animation extension, so the
    // material extension is not listed: a manifest entry for data nothing contains would hide a
    // manifest/content mismatch.
    assert!(
        !ids.contains(&shared::extensions::MATERIAL),
        "material listed although no mesh uses it: {ids:?}"
    );
    assert!(shared::extensions::ownership_problems(&manifest.extensions).is_empty());
    for entry in &manifest.extensions {
        assert_eq!(entry.sha256.len(), 64, "{}", entry.id);
    }

    // The Rust side recomputes the same hashes from the published folder.
    let again = converter::extensions::build_entries(&output).unwrap();
    assert_eq!(again, manifest.extensions);

    let Some(python) = python() else {
        eprintln!("skipped the Python cross check: `python` is not on PATH");
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/extension_hash.py");
    let result = Command::new(python)
        .arg("-I")
        .arg(&script)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "tools/extension_hash.py disagrees with the converter:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

/// Checks every extension a data set lists, own or foreign, under a shared asset lock.
///
/// * The converter's own extensions (`converter::extensions::specs`) are recomputed from the folder
///   and must equal the listed entries exactly: hash, identity, ownership and stats.
/// * Every other (finalized) entry goes through the composer, which requires the data set's
///   current non-empty core identity, unique ids, disjoint ownership (nested folders included) and
///   a `sha256` equal to `recompute_hash` of the content on disk. Nothing is minted or dropped.
///   The composer cannot recompute a foreign entry's `stats` or `version` meaning: those are
///   taken as listed and are not independently verified here.
/// * `tools/extension_hash.py` must then accept every listed entry.
fn verify_set(set: &Path) {
    let _lock = shared::asset_lock::AssetLock::acquire_shared(set).unwrap();
    let manifest: converter::cache::ConversionManifest =
        serde_json::from_slice(&fs::read(set.join("conversion-manifest.json")).unwrap()).unwrap();
    let own_ids: Vec<String> = converter::extensions::specs()
        .iter()
        .map(|spec| spec.id.to_owned())
        .collect();
    let (own, foreign): (Vec<_>, Vec<_>) = manifest
        .extensions
        .iter()
        .cloned()
        .partition(|entry| own_ids.contains(&entry.id));
    assert_eq!(
        converter::extensions::build_entries(set).unwrap(),
        own,
        "Rust recompute of the converter's own extensions must match the listed entries"
    );
    let mut composed = converter::cache::ConversionManifest::default();
    converter::extensions::refresh_manifest_with_finalized(&mut composed, set, &foreign)
        .unwrap_or_else(|error| panic!("a listed foreign extension is not valid: {error:#}"));
    let by_id = |entries: &[shared::extensions::ExtensionEntry]| {
        let mut entries = entries.to_vec();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
    };
    assert_eq!(
        by_id(&composed.extensions),
        by_id(&manifest.extensions),
        "the composed list must equal the manifest's, with nothing minted or dropped"
    );
    let python = python().expect("python on PATH");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/extension_hash.py");
    let result = Command::new(python)
        .arg("-I")
        .arg(&script)
        .arg(set)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}

/// A finalized entry from another writer (as the cluster finalizers write): its own table and
/// folder, built from the data set's core identity and listed next to the converter's entries.
#[tokio::test]
async fn a_finalized_foreign_entry_passes_and_a_tampered_one_fails() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("Data");
    let output = directory.path().join("assets");
    generate_data(&data);
    convert(converter::PipelineConfig::new(&data, &output)).await;
    if python().is_none() {
        eprintln!("skipped: `python` is not on PATH");
        return;
    }
    let database = output.join("skyrim_world.db");
    let connection = rusqlite::Connection::open(&database).unwrap();
    let identity = shared::extensions::core_build_identity(&connection)
        .unwrap()
        .expect("the dummy conversion records a core build identity");
    connection
        .execute_batch("CREATE TABLE cluster_x(a INTEGER); INSERT INTO cluster_x VALUES (7);")
        .unwrap();
    drop(connection);
    fs::create_dir_all(output.join("clusters")).unwrap();
    fs::write(output.join("clusters/a.bin"), b"payload").unwrap();
    let mut entry = shared::extensions::ExtensionEntry {
        id: "mudcrab.clusters".to_owned(),
        version: 1,
        owns: shared::extensions::ExtensionOwns {
            tables: vec!["cluster_x".to_owned()],
            files: vec!["clusters/".to_owned()],
            ..Default::default()
        },
        sha256: String::new(),
        built_from: identity,
        stats: Default::default(),
    };
    entry.sha256 = converter::extensions::recompute_hash(&output, &entry).unwrap();
    let manifest_path = output.join("conversion-manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["extensions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(&entry).unwrap());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // The own-entries-only comparison this replaces would have failed here.
    verify_set(&output);

    fs::write(output.join("clusters/a.bin"), b"tampered").unwrap();
    let outcome = std::panic::catch_unwind(|| verify_set(&output));
    assert!(outcome.is_err(), "changed owned bytes must be rejected");
}

/// A real data set, not a fixture: `MUDCRAB_EXTENSIONS_SET=<folder> cargo nextest run -p converter
/// --test extensions_hash --run-ignored all`. Every listed entry, the converter's own and any
/// finalized one such as the cluster pair, is verified by [`verify_set`]; this is the check that
/// found a float-parsing disagreement between Rust and Python on real glTF data.
#[test]
#[ignore = "needs a converted data set: set MUDCRAB_EXTENSIONS_SET"]
fn real_data_set_hashes_agree_in_rust_and_python() {
    let Some(set) = std::env::var_os("MUDCRAB_EXTENSIONS_SET") else {
        panic!("set MUDCRAB_EXTENSIONS_SET to a converted data set folder");
    };
    verify_set(Path::new(&set));
}

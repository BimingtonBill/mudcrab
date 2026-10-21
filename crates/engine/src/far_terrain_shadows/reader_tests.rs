//! CPU-only invented-input publication acceptance for reader.rs.
//! Intended sibling module: #[cfg(test)] mod reader_tests; in far_terrain_shadows.
//! Requires the documented root-owned API visibility and height-only seams.

use super::{
    CellCoord, RayResult, SAMPLES_PER_CELL,
    gpu::{GpuUpload, UploadLimits},
    packet::{NONE, RawVhgtStatus, coverage},
    reader::{CertifiedShadowPacket, CertifiedTerrainSnapshot},
};
use crate::cluster::terrain::{ClusterTerrain, ClusterTerrainMaterial};
use bevy::{asset::Assets, image::Image, math::IVec2};
use converter::{
    cache::{CONVERTER_SCHEMA_VERSION, CacheEntry, ConversionManifest, hash_bytes},
    cluster::{
        LodMode, finalize_certified_extensions,
        probe::ProbeResult,
        publish::PublishContext,
        stage::{StageSettings, run_stage},
    },
    esm::{
        cell_cache::write_cell_cache_with_source_order, exporter::create_tables,
        extractors::serialize_subrecords, records::RawRecord,
    },
};
use ddsfile::{AlphaMode, D3D10ResourceDimension, Dds, DxgiFormat, NewDxgiParams};
use rusqlite::{Connection, params};
use serde_json::json;
use shared::asset_lock::AssetLock;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

// Invented identities, not Skyrim data. Same grids in two explicit worlds.
const WORLD: u32 = 0x70;
const OTHER: u32 = 0x72;
const WEST: u32 = 0x30;
const EAST: u32 = 0x31;
const EMPTY: u32 = 0x32;
const MISSING: u32 = 0x33;
const OTHER_CELL: u32 = 0x34;
const CORE: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";
const MODEL: &str = "meshes/invented/stone.glb";
const DDS: &str = "textures/landscape/invented.dds";

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

fn invented_glb() -> Vec<u8> {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            positions.extend([x as f32, y as f32, 0.0]);
            normals.extend([0.0_f32, 0.0, 1.0]);
            uvs.extend([x as f32 / 3.0, y as f32 / 3.0]);
        }
    }
    let mut indices = Vec::<u16>::new();
    for y in 0..3 {
        for x in 0..3 {
            let a = y * 4 + x;
            indices.extend([a, a + 1, a + 4, a + 1, a + 5, a + 4]);
        }
    }
    let mut binary = Vec::new();
    for value in positions.iter().chain(&normals).chain(&uvs) {
        binary.extend(value.to_le_bytes());
    }
    for index in &indices {
        binary.extend(index.to_le_bytes());
    }
    let document = json!({
        "asset":{"version":"2.0"},"buffers":[{"byteLength":binary.len()}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":192},
            {"buffer":0,"byteOffset":192,"byteLength":192},
            {"buffer":0,"byteOffset":384,"byteLength":128},
            {"buffer":0,"byteOffset":512,"byteLength":indices.len()*2}],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":16,"type":"VEC3","min":[0,0,0],"max":[3,3,0]},
            {"bufferView":1,"componentType":5126,"count":16,"type":"VEC3"},
            {"bufferView":2,"componentType":5126,"count":16,"type":"VEC2"},
            {"bufferView":3,"componentType":5123,"count":indices.len(),"type":"SCALAR"}],
        "materials":[{"extras":{"openSkyrim":{"shaderFamily":"lighting","lightingShaderType":"default"}}}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2},"indices":3,"material":0}]}],
        "nodes":[{"mesh":0}],"scene":0,"scenes":[{"nodes":[0]}]
    });
    let mut json = serde_json::to_vec(&document).unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(((12 + 8 + json.len() + 8 + binary.len()) as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    bytes.extend((binary.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(binary);
    bytes
}

fn land(cell: u32, world: u32, height: Option<f32>) -> RawRecord {
    let mut subrecords = Vec::new();
    if let Some(height) = height {
        let mut raw = (height / 8.0).to_le_bytes().to_vec();
        raw.extend(vec![0; 33 * 33 + 3]);
        subrecords.push((b"VHGT".to_vec(), raw));
    }
    for quadrant in 0..4_u8 {
        let mut base = 0x71_u32.to_le_bytes().to_vec();
        base.extend([quadrant, 0, 0, 0]);
        subrecords.push((b"BTXT".to_vec(), base));
    }
    RawRecord {
        form_id: 0x1000 + cell,
        record_type: *b"LAND",
        flags: 0,
        subrecords,
        cell_form_id: Some(cell),
        worldspace_form_id: Some(world),
        load_order: 0,
    }
}

fn settings() -> StageSettings {
    StageSettings {
        mode: LodMode::Cluster,
        triangle_threshold: 1,
        bench_baked: false,
        jobs: 1,
        assets_root: None,
        keep_bench_artifacts: false,
        read_database: None,
        area: None,
        child_worldspaces: Vec::new(),
        include_trees: false,
        run_artifacts: None,
        map_references: true,
    }
}

/// Actual cache writer, object stage, terrain stage, finalizers, extension composer
/// and locked new-directory publisher. Only the adapter/probe have no GPU work.
fn publish_fixture(raw_cache_mismatch: bool) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("OWNER.txt"),
        "S5 reader invented CPU fixture; automatic tempfile cleanup\n",
    )
    .unwrap();
    let root = directory.path().join("published");
    let staging = directory.path().join("staging");
    fs::create_dir_all(staging.join("meshes/invented")).unwrap();
    fs::create_dir_all(staging.join("vfs/textures/landscape")).unwrap();
    let lock = AssetLock::acquire_exclusive(&root).unwrap();
    let mut manifest = ConversionManifest {
        schema_version: CONVERTER_SCHEMA_VERSION,
        complete: false,
        record_data_complete: Some(false),
        ..Default::default()
    };
    let glb = invented_glb();
    fs::write(staging.join(MODEL), &glb).unwrap();
    manifest.entries.insert(
        MODEL.into(),
        CacheEntry {
            source_hash: String::new(),
            output: MODEL.into(),
            output_size: glb.len() as u64,
            output_hash: hash_bytes(&glb),
        },
    );
    let db = Connection::open(staging.join("skyrim_world.db")).unwrap();
    create_tables(&db).unwrap();
    db.execute("INSERT INTO lod_build VALUES(1,?1)", [CORE])
        .unwrap();
    for world in [WORLD, OTHER] {
        db.execute("INSERT INTO worldspaces(id,editor_id,flags,lod_origin_x,lod_origin_y) VALUES(?1,?2,0,0,0)",
            params![world,format!("Invented {world}")]).unwrap();
    }
    for (id, world, x, y) in [
        (WEST, WORLD, 0, 0),
        (EAST, WORLD, 1, 0),
        (EMPTY, WORLD, 0, 1),
        (MISSING, WORLD, 2, 0),
        (OTHER_CELL, OTHER, 0, 0),
    ] {
        db.execute(
            "INSERT INTO cells(id,worldspace_id,grid_x,grid_y,flags) VALUES(?1,?2,?3,?4,0)",
            params![id, world, x, y],
        )
        .unwrap();
    }
    // Exterior persistent containers can share XCLC(0,0) with a normal grid
    // CELL. Header bit 10 establishes their role; absence of LAND alone does not.
    for (id, world) in [(0x2f_u32, WORLD), (0x35, OTHER)] {
        db.execute(
            "INSERT INTO cells(id,worldspace_id,grid_x,grid_y,flags) VALUES(?1,?2,0,0,1024)",
            params![id, world],
        )
        .unwrap();
    }
    let clear = serialize_subrecords(&[(b"EDID".to_vec(), b"invented\0".to_vec())]);
    db.execute(
        "INSERT INTO records(form_id,record_type,load_order,data) VALUES(0x11,'STAT',0,?1)",
        [&clear],
    )
    .unwrap();
    db.execute("INSERT INTO statics(id,editor_id,model_path,flags) VALUES(0x11,'Stone','Invented\\Stone.nif',0)",[]).unwrap();
    db.execute("INSERT INTO \"references\"(id,cell_id,worldspace_id,base_form_id,is_exterior,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,scale,header_flags,data) VALUES(0x100,?1,?2,0x11,1,100,100,20,0,0,0,1,0,?3)",params![WEST,WORLD,clear]).unwrap();
    db.execute(
        "INSERT INTO texture_sets(id,diffuse_path) VALUES(0x72,'Landscape\\Invented.dds')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO landscape_textures(id,texture_set_id) VALUES(0x71,0x72)",
        [],
    )
    .unwrap();
    let records: HashMap<_, _> = [
        land(WEST, WORLD, Some(0.0)),
        land(EAST, WORLD, Some(8.0)),
        land(EMPTY, WORLD, None),
        land(OTHER_CELL, OTHER, Some(128.0)),
    ]
    .into_iter()
    .map(|record| (record.form_id, record))
    .collect();
    for record in records.values() {
        let cell = record.cell_form_id.unwrap();
        let raw = record
            .subrecords
            .iter()
            .find(|(tag, _)| tag.as_slice() == b"VHGT")
            .map(|(_, bytes)| bytes.clone())
            .unwrap_or_default();
        db.execute(
            "INSERT INTO land(cell_id,heightmap) VALUES(?1,?2)",
            params![cell, raw],
        )
        .unwrap();
        // Retain real parsed raw LAND input too; adapter intentionally uses only
        // positive winning projection/cache evidence, not absence from records.
        db.execute("INSERT INTO records(form_id,record_type,cell_id,worldspace_id,load_order,data) VALUES(?1,'LAND',?2,?3,0,?4)",
            params![record.form_id,cell,record.worldspace_form_id,serialize_subrecords(&record.subrecords)]).unwrap();
    }
    assert_eq!(
        write_cell_cache_with_source_order(
            &records,
            &HashMap::new(),
            &staging.join("cell_cache.rkyv")
        )
        .unwrap(),
        4
    );
    if raw_cache_mismatch {
        // Deliberate source-projection inconsistency BEFORE all certification.
        // Real terrain producer uses cache heights; adapter must additionally
        // refuse the differing positive projected VHGT, despite valid envelope.
        let forged = land(WEST, WORLD, Some(16.0))
            .subrecords
            .into_iter()
            .find(|(tag, _)| tag.as_slice() == b"VHGT")
            .unwrap()
            .1;
        db.execute(
            "UPDATE land SET heightmap=?1 WHERE cell_id=?2",
            params![forged, WEST],
        )
        .unwrap();
    }
    drop(db);
    let mut dds = Dds::new_dxgi(NewDxgiParams {
        width: 4,
        height: 4,
        depth: None,
        format: DxgiFormat::R8G8B8A8_UNorm,
        mipmap_levels: Some(1),
        array_layers: None,
        caps2: None,
        is_cubemap: false,
        resource_dimension: D3D10ResourceDimension::Texture2D,
        alpha_mode: AlphaMode::Straight,
    })
    .unwrap();
    dds.data.copy_from_slice(&[100, 150, 70, 255].repeat(16));
    let mut bytes = Vec::new();
    dds.write(&mut bytes).unwrap();
    let hash = hash_bytes(&bytes);
    fs::write(staging.join("vfs").join(DDS), &bytes).unwrap();
    manifest.entries.insert(
        DDS.into(),
        CacheEntry {
            source_hash: format!("{hash}:texture-encoding:ColorSrgb"),
            output: format!("vfs/{DDS}"),
            output_size: bytes.len() as u64,
            output_hash: hash.clone(),
        },
    );
    fs::write(
        staging.join("lod-manifest.json"),
        serde_json::to_vec(&json!({
            "build_identity":CORE,"converter_schema":CONVERTER_SCHEMA_VERSION,
            "world_database_schema":shared::WORLD_DATABASE_SCHEMA_VERSION,"chunks":0,
            "land_texture_repeats_per_cell":shared::LAND_TEXTURE_REPEATS_PER_CELL,
            "terrain_sources":{DDS:hash}
        }))
        .unwrap(),
    )
    .unwrap();
    manifest
        .save(&staging.join("conversion-manifest.json"))
        .unwrap();
    let settings = settings();
    let probe = ProbeResult {
        passed: true,
        missing: Vec::new(),
        backend: "synthetic".into(),
        adapter_name: "invented CPU publication fixture; no GPU".into(),
        json: json!({"synthetic":true}),
    };
    let (objects, _) = run_stage(
        &settings,
        Some((probe, 0.0)),
        &staging,
        &staging.join("no-previous"),
        &manifest,
    )
    .unwrap();
    let context = PublishContext::new(&lock, &root, &staging).unwrap();
    let (entries, _) = finalize_certified_extensions(
        &settings,
        &context,
        &manifest,
        &objects.builder_identity,
        true,
    )
    .unwrap();
    converter::extensions::refresh_manifest_with_finalized(&mut manifest, &staging, &entries)
        .unwrap();
    assert!(!manifest.complete);
    assert_eq!(manifest.record_data_complete, Some(false));
    manifest
        .save(&staging.join("conversion-manifest.json"))
        .unwrap();
    let db = Connection::open(staging.join("skyrim_world.db")).unwrap();
    assert!(
        db.query_row("SELECT count(*) FROM cluster_instance_map", [], |r| r
            .get::<_, i64>(0))
            .unwrap()
            > 0,
        "actual mapped object stage is exercised, not an empty/fake base envelope"
    );
    drop(db);
    converter::pipeline::publish_new_directory(&staging, &root, &lock).unwrap();
    drop(lock);
    // The converter wrote one folder; the engine reads the base + overlay layout.
    crate::cluster::overlay_fixture::split_full_copy(&root);
    Fixture {
        _directory: directory,
        root,
    }
}

fn load_terrain(root: &Path) -> ClusterTerrain {
    ClusterTerrain::load(
        root,
        &crate::cluster::overlay_fixture::overlay_path(root),
        &mut Assets::<Image>::default(),
        &mut Assets::<ClusterTerrainMaterial>::default(),
    )
    .unwrap()
}
fn limits() -> UploadLimits {
    UploadLimits {
        max_storage_buffer_binding_size: 64 * 1024 * 1024,
        max_buffer_size: 64 * 1024 * 1024,
        max_storage_buffers_per_shader_stage: 8,
        max_uniform_buffers_per_shader_stage: 12,
        max_uniform_buffer_binding_size: 65536,
        max_bind_groups: 8,
        existing_fragment_storage: 0,
        existing_fragment_uniforms: 0,
    }
}
fn grid(x: i32, y: i32) -> CellCoord {
    CellCoord { x, y }
}
fn published_heights(root: &Path, world: u32, cell: CellCoord) -> Vec<u8> {
    crate::cluster::overlay_fixture::read_db(root)
        .query_row("SELECT heights FROM cluster_terrain_heights WHERE worldspace_id=?1 AND grid_x=?2 AND grid_y=?3",
            params![world,cell.x,cell.y],|r|r.get(0)).unwrap()
}
fn check_page(
    owner: &CertifiedShadowPacket,
    terrain: &ClusterTerrain,
    root: &Path,
    world: u32,
    cell: CellCoord,
) {
    let packet = owner.packet();
    let entry = packet.lookup(cell).unwrap();
    assert_eq!(entry.0[6], RawVhgtStatus::PresentValid as u32);
    assert_eq!(entry.0[7], 1, "canonical authored provenance");
    assert_eq!(
        entry.0[4],
        coverage::HAS_SURFACE | coverage::HAS_BOUNDS | coverage::COMPLETE
    );
    let page = entry.0[2];
    assert_ne!(page, NONE);
    let start = page as usize * SAMPLES_PER_CELL;
    let words = &packet.heights[start..start + SAMPLES_PER_CELL];
    let resource = terrain
        .canonical_heights(world, IVec2::new(cell.x, cell.y))
        .unwrap();
    assert_eq!(
        words,
        resource.iter().map(|h| h.to_bits()).collect::<Vec<_>>()
    );
    let bytes = published_heights(root, world, cell);
    let db_words: Vec<_> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    assert_eq!(words, db_words);
}

#[test]
fn actual_joint_publication_supplies_world_specific_canonical_packets_and_retains_guard() {
    let fixture = publish_fixture(false);
    let package =
        Arc::new(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).unwrap());
    let terrain = load_terrain(&fixture.root);
    let snapshot =
        CertifiedTerrainSnapshot::open(package.clone(), terrain.canonical_height_snapshot())
            .unwrap();
    let world = snapshot.world(WORLD).unwrap().unwrap();
    let other = snapshot.world(OTHER).unwrap().unwrap();
    assert_eq!(world.identity().database, other.identity().database);
    assert_eq!(world.identity().core_cache, other.identity().core_cache);
    assert_ne!(
        world.identity().canonical.unwrap().catalog,
        other.identity().canonical.unwrap().catalog
    );
    assert!(matches!(
        world.trace([2048.0, 2048.0, 100.0], [0.0, 0.0, -1.0]),
        RayResult::Occluded(_)
    ));
    let owner = Arc::new(world.build_packet().unwrap());
    let other_owner = other.build_packet().unwrap();
    assert_eq!(
        owner.packet().header[3],
        2,
        "only two positively authored pages"
    );
    assert_eq!(other_owner.packet().header[3], 1);
    for cell in [grid(0, 0), grid(1, 0)] {
        check_page(&owner, &terrain, &fixture.root, WORLD, cell);
    }
    check_page(&other_owner, &terrain, &fixture.root, OTHER, grid(0, 0));
    assert_ne!(owner.packet().heights, other_owner.packet().heights);
    assert_eq!(
        &owner.packet().header[11..14],
        &[0, 0, 1],
        "incomplete source flags preserved"
    );
    assert_eq!(owner.packet().header[8], 0, "no closed-world attestation");
    for cell in [grid(0, 1), grid(2, 0)] {
        let unknown = owner.packet().lookup(cell).unwrap();
        assert_eq!(unknown.0[6], RawVhgtStatus::Unknown as u32);
        assert_eq!(unknown.0[2], NONE);
        assert_eq!(unknown.0[4] & coverage::KNOWN_EMPTY, 0);
    }
    assert!(
        owner.packet().lookup(grid(1, 1)).is_none(),
        "uninventoried hole has no manufactured row/page"
    );
    let west = terrain.canonical_heights(WORLD, IVec2::new(0, 0)).unwrap();
    let east = terrain.canonical_heights(WORLD, IVec2::new(1, 0)).unwrap();
    for north in 0..33 {
        assert_eq!(west[north * 33 + 32].to_bits(), east[north * 33].to_bits());
    }
    assert_ne!(
        east[16 * 33].to_bits(),
        8.0_f32.to_bits(),
        "actual producer welded the authored 8-unit shared edge"
    );
    assert_eq!(
        terrain.canonical_heights(OTHER, IVec2::ZERO).unwrap()[16 * 33 + 16],
        128.0
    );
    let upload = GpuUpload::build(owner.packet(), limits()).unwrap();
    assert_eq!(upload.identity, world.identity());
    // Actual guarded compaction preserves the same full identity and serialized
    // bytes as the existing direct CPU upload path, without retaining its packet.
    let compact = world.build_packet().unwrap().into_gpu(limits()).unwrap();
    assert_eq!(compact.upload().identity, world.identity());
    assert_eq!(compact.upload().header, upload.header);
    assert_eq!(compact.upload().words, upload.words);
    let expected_bytes: Vec<_> = upload
        .words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    assert_eq!(compact.storage_bytes(), expected_bytes);
    #[cfg(target_endian = "little")]
    assert_eq!(
        compact.storage_bytes().as_ptr(),
        compact.upload().words.as_ptr().cast::<u8>()
    );
    drop(compact);
    // Match actual uploaded height section, not merely command/header counts.
    let upload_heights = upload.header[14] as usize;
    assert_eq!(
        &upload.words[upload_heights..upload_heights + owner.packet().heights.len()],
        owner.packet().heights
    );
    drop(upload);
    drop(other_owner);
    drop(other);
    drop(world);
    drop(snapshot);
    drop(package);
    drop(terrain);
    assert!(
        AssetLock::acquire_exclusive(&fixture.root).is_err(),
        "packet alone retains snapshot/package reader guard"
    );
    drop(owner);
    let writer = AssetLock::acquire_exclusive(&fixture.root).unwrap();
    drop(writer);
}

#[test]
fn actual_valid_envelope_cannot_hide_positive_projected_vhgt_cache_disagreement() {
    let fixture = publish_fixture(true);
    // Deliberate mismatch predates finalization. Real object/terrain publisher
    // checks pass, and the actual reader certifies their envelope/mapping.
    let package =
        Arc::new(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).unwrap());
    let terrain = load_terrain(&fixture.root);
    let snapshot =
        CertifiedTerrainSnapshot::open(package, terrain.canonical_height_snapshot()).unwrap();
    drop(terrain); // The captured immutable heights survive the loaded resource.
    let error = match snapshot.world(WORLD) {
        Ok(_) => panic!("raw/cache disagreement was accepted"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("cache is not the exact authored VHGT decode"),
        "{error:#}"
    );
    assert!(
        snapshot.world(OTHER).unwrap().is_some(),
        "unrelated world's positive data remains readable"
    );
}

#[test]
fn stale_cache_canonical_rows_and_core_identity_refuse_real_certification() {
    let fixture = publish_fixture(false);
    let cache_path = fixture.root.join("cell_cache.rkyv");
    let original_cache = fs::read(&cache_path).unwrap();
    {
        let _writer = AssetLock::acquire_exclusive(&fixture.root).unwrap();
        let mut changed = original_cache.clone();
        let index = changed.len() / 2;
        changed[index] ^= 1;
        fs::write(&cache_path, changed).unwrap();
    }
    assert!(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).is_err());
    {
        let _writer = AssetLock::acquire_exclusive(&fixture.root).unwrap();
        fs::write(&cache_path, &original_cache).unwrap();
    }
    let original_heights = published_heights(&fixture.root, WORLD, grid(0, 0));
    {
        let _writer = AssetLock::acquire_exclusive(&fixture.root).unwrap();
        let mut changed = original_heights.clone();
        changed[..4].copy_from_slice(&123.0_f32.to_le_bytes());
        crate::cluster::overlay_fixture::overlay_db(&fixture.root).execute(
            "UPDATE cluster_terrain_heights SET heights=?1 WHERE worldspace_id=?2 AND grid_x=0 AND grid_y=0",
            params![changed,WORLD]).unwrap();
    }
    assert!(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).is_err());
    {
        let _writer = AssetLock::acquire_exclusive(&fixture.root).unwrap();
        crate::cluster::overlay_fixture::overlay_db(&fixture.root).execute("UPDATE cluster_terrain_heights SET heights=?1 WHERE worldspace_id=?2 AND grid_x=0 AND grid_y=0",params![original_heights,WORLD]).unwrap();
        crate::cluster::overlay_fixture::base_db(&fixture.root)
            .execute("UPDATE lod_build SET build_identity=?1", ["ef".repeat(32)])
            .unwrap();
    }
    assert!(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).is_err());
    {
        let _writer = AssetLock::acquire_exclusive(&fixture.root).unwrap();
        Connection::open(fixture.root.join("skyrim_world.db"))
            .unwrap()
            .execute("UPDATE lod_build SET build_identity=?1", [CORE])
            .unwrap();
    }
    // Restoring original logical inputs (without forging extension ownership)
    // restores certification; the negative cases weren't broken fixture setup.
    assert!(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).is_ok());
}

fn prepared_owner(
    owner: &mut super::runtime_owner::SourceOwner,
) -> Arc<super::runtime_owner::Publication> {
    use super::runtime_owner::WorldState;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        owner.poll();
        let publication = owner.publication();
        if !publication.worlds.is_empty()
            && publication
                .worlds
                .values()
                .all(|state| !matches!(state, WorldState::Pending))
        {
            return publication;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actual F1 owner tasks did not finish within 30 seconds"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn actual_owner_caches_packets_and_exact_source_identity_until_last_publication_drops() {
    use super::runtime_owner::{SourceOwner, WorldState};
    bevy::tasks::IoTaskPool::get_or_init(Default::default);
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let fixture = publish_fixture(false);
    let retained = {
        let package =
            Arc::new(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).unwrap());
        let terrain = load_terrain(&fixture.root);
        let heights = terrain.canonical_height_snapshot();
        let mut owner = SourceOwner::new(package.clone(), heights.clone(), limits());
        assert!(owner.matches_source(&package, &heights));
        assert!(owner.request_world(WORLD));
        assert!(owner.request_world(OTHER));
        assert!(!owner.request_world(WORLD));
        assert!(!owner.request_world(OTHER));
        let pending = owner.publication();
        assert_eq!(pending.worlds.len(), 2);
        assert!(
            pending
                .worlds
                .values()
                .all(|state| matches!(state, WorldState::Pending))
        );
        let ready = prepared_owner(&mut owner);
        for world in [WORLD, OTHER] {
            let Some(WorldState::Ready(packet)) = ready.worlds.get(&world) else {
                panic!("invented published world {world} did not prepare a packet");
            };
            assert_eq!(packet.upload().identity.worldspace, world);
            assert!(packet.upload().header[3] > 0);
        }
        for _ in 0..8 {
            assert!(!owner.poll());
            assert!(!owner.request_world(WORLD));
            let unchanged = owner.publication();
            assert_eq!(unchanged.revision, ready.revision);
            assert!(Arc::ptr_eq(&ready, &unchanged));
        }
        let other_package =
            Arc::new(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).unwrap());
        assert!(
            !owner.matches_source(&other_package, &heights),
            "equal bytes do not replace exact certificate ownership"
        );
        let other_terrain = load_terrain(&fixture.root);
        let other_heights = other_terrain.canonical_height_snapshot();
        assert!(
            !owner.matches_source(&package, &other_heights),
            "separately loaded equal heights have a distinct allocation"
        );
        let mut replacement = SourceOwner::new(package.clone(), heights.clone(), limits());
        assert!(replacement.request_world(WORLD));
        let replacement_ready = prepared_owner(&mut replacement);
        assert!(!ready.source.same_allocation(&replacement_ready.source));
        assert!(replacement.matches_source(&package, &heights));
        // Both owners have finished all requested jobs before this scope drops
        // them, avoiding an asynchronous guard-release race in the lock proof.
        ready
    };
    assert!(
        AssetLock::acquire_exclusive(&fixture.root).is_err(),
        "publication alone must retain its exact package guard"
    );
    drop(retained);
    drop(AssetLock::acquire_exclusive(&fixture.root).unwrap());
}

#[test]
fn actual_owner_keeps_world_failure_cached_without_poisoning_another_world() {
    use super::runtime_owner::{SourceOwner, WorldState};
    bevy::tasks::IoTaskPool::get_or_init(Default::default);
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let fixture = publish_fixture(true);
    let package =
        Arc::new(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).unwrap());
    let terrain = load_terrain(&fixture.root);
    let mut owner = SourceOwner::new(package, terrain.canonical_height_snapshot(), limits());
    assert!(owner.request_world(WORLD));
    assert!(owner.request_world(OTHER));
    let publication = prepared_owner(&mut owner);
    let Some(WorldState::Unavailable(reason)) = publication.worlds.get(&WORLD) else {
        panic!("positive raw/cache mismatch must be unavailable");
    };
    assert!(
        reason.contains("cache is not the exact authored VHGT decode"),
        "{reason}"
    );
    assert!(matches!(
        publication.worlds.get(&OTHER),
        Some(WorldState::Ready(_))
    ));
    for _ in 0..8 {
        assert!(!owner.request_world(WORLD));
        assert!(!owner.poll());
        assert!(Arc::ptr_eq(&publication, &owner.publication()));
        assert_eq!(publication.revision, owner.publication().revision);
    }
}

#[cfg(windows)]
#[test]
fn actual_owner_snapshot_open_failure_is_cached_for_queued_and_later_worlds() {
    use super::runtime_owner::{SourceOwner, WorldState};
    use std::os::windows::fs::OpenOptionsExt;
    bevy::tasks::IoTaskPool::get_or_init(Default::default);
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let fixture = publish_fixture(false);
    let package =
        Arc::new(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).unwrap());
    let terrain = load_terrain(&fixture.root);
    // Deny another read without changing any certified bytes, permissions or
    // asset-lock state. Actual snapshot file opening must report the OS error.
    let denied = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(fixture.root.join("cell_cache.rkyv"))
        .unwrap();
    let denied_error = fs::File::open(fixture.root.join("cell_cache.rkyv"))
        .unwrap_err()
        .to_string();
    let mut owner = SourceOwner::new(package, terrain.canonical_height_snapshot(), limits());
    assert!(owner.request_world(WORLD));
    assert!(owner.request_world(OTHER));
    let failed = prepared_owner(&mut owner);
    let Some(WorldState::Unavailable(reason)) = failed.worlds.get(&WORLD) else {
        panic!("denied cache must fail actual snapshot open");
    };
    let reason = reason.clone();
    assert!(
        reason.contains(&denied_error),
        "actual cache open error must be retained: {reason}"
    );
    assert!(
        matches!(failed.worlds.get(&OTHER), Some(WorldState::Unavailable(other)) if other == &reason)
    );
    drop(denied);
    drop(fs::File::open(fixture.root.join("cell_cache.rkyv")).unwrap());
    // A later request must reuse the snapshot failure even though the file is
    // readable again. A fresh owner could retry; this owner may not self-restart.
    assert!(owner.request_world(WORLD + 1));
    let later = owner.publication();
    assert!(
        matches!(later.worlds.get(&(WORLD + 1)), Some(WorldState::Unavailable(other)) if other == &reason)
    );
    for _ in 0..8 {
        assert!(!owner.poll());
        assert!(!owner.request_world(WORLD));
        assert!(Arc::ptr_eq(&later, &owner.publication()));
    }
}

#[test]
fn actual_owner_caches_compact_upload_limit_refusal_without_retry_or_ready() {
    use super::runtime_owner::{SourceOwner, WorldState};
    bevy::tasks::IoTaskPool::get_or_init(Default::default);
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let fixture = publish_fixture(false);
    let package =
        Arc::new(crate::cluster::overlay_fixture::open_fixture(&fixture.root, true).unwrap());
    let terrain = load_terrain(&fixture.root);
    let mut constrained = limits();
    constrained.max_storage_buffer_binding_size = 4;
    let mut owner = SourceOwner::new(package, terrain.canonical_height_snapshot(), constrained);
    assert!(owner.request_world(WORLD));
    assert!(owner.request_world(OTHER));
    let refused = prepared_owner(&mut owner);
    for world in [WORLD, OTHER] {
        let Some(WorldState::Unavailable(reason)) = refused.worlds.get(&world) else {
            panic!("compact upload for world {world} must refuse the insufficient storage limit");
        };
        assert!(
            reason.contains("canonical upload refused: StorageBytes"),
            "{reason}"
        );
    }
    for _ in 0..8 {
        assert!(!owner.request_world(WORLD));
        assert!(!owner.request_world(OTHER));
        assert!(!owner.poll());
        let unchanged = owner.publication();
        assert!(Arc::ptr_eq(&refused, &unchanged));
        assert_eq!(refused.revision, unchanged.revision);
    }
}

#[test]
#[ignore = "Read-only adapter acceptance; requires MUDCRAB_F1_TEST_DATASET and MUDCRAB_F1_TEST_DEVICE_LIMITS; no GPU or regeneration"]
fn real_published_terrain_builds_world_specific_canonical_packets_and_uploads() {
    let root = PathBuf::from(
        std::env::var_os("MUDCRAB_F1_TEST_DATASET")
            .expect("set MUDCRAB_F1_TEST_DATASET to the existing certified dataset"),
    );
    // The small invented fixture's 64 MiB limit is not a hardware limit. Use
    // an explicit record from the actual created device, never enlarge a
    // requested device or silently truncate a real world's packet to fit.
    let limits_path = std::env::var_os("MUDCRAB_F1_TEST_DEVICE_LIMITS").expect(
        "set MUDCRAB_F1_TEST_DEVICE_LIMITS to the numeric GPU proof's created-limit record",
    );
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(&limits_path).unwrap()).unwrap();
    let value = |name: &str| {
        record["limits"][name]
            .as_u64()
            .unwrap_or_else(|| panic!("missing created-device limit {name}"))
    };
    let stage_limit = |name: &str| u32::try_from(value(name)).unwrap();
    let recorded_limits = UploadLimits {
        max_storage_buffer_binding_size: value("max_storage_buffer_binding_size"),
        max_buffer_size: value("max_buffer_size"),
        max_storage_buffers_per_shader_stage: stage_limit("max_storage_buffers_per_shader_stage"),
        max_uniform_buffers_per_shader_stage: stage_limit("max_uniform_buffers_per_shader_stage"),
        max_uniform_buffer_binding_size: value("max_uniform_buffer_binding_size"),
        max_bind_groups: stage_limit("max_bind_groups"),
        // This checks standalone packet serialization, not a specialized
        // material's complete pipeline layout. Runtime must check that layout.
        existing_fragment_storage: 0,
        existing_fragment_uniforms: 0,
    };
    println!(
        "F1 CPU serialization limits from numeric GPU image {}: {recorded_limits:?}; full material-layout approval remains separate",
        record["image_sha256"]
            .as_str()
            .expect("missing GPU proof image identity"),
    );
    let package = Arc::new(crate::cluster::overlay_fixture::open_fixture(&root, true).unwrap());
    let terrain = load_terrain(&root);
    let worlds: std::collections::BTreeSet<_> = terrain
        .blocks
        .iter()
        .map(|block| block.worldspace_id)
        .collect();
    assert!(
        !worlds.is_empty(),
        "dataset has no published terrain worlds"
    );
    let snapshot =
        CertifiedTerrainSnapshot::open(package, terrain.canonical_height_snapshot()).unwrap();
    drop(terrain);
    let mut total_positive_pages = 0u64;
    for world in worlds {
        let inventory = snapshot
            .world(world)
            .unwrap()
            .expect("published terrain world must have an explicit cell domain");
        let packet = inventory.build_packet().unwrap();
        let upload = GpuUpload::build(packet.packet(), recorded_limits).unwrap();
        let header = packet.packet().header;
        println!(
            "F1 read-only accepted world={world:08X} directory={} positive_pages={} upload_storage_bytes={} header_bytes={} source_flags={:?}",
            header[2],
            header[3],
            upload.words.len() * 4,
            upload.header.len() * 4,
            &header[11..14]
        );
        assert_eq!(packet.packet().identity.worldspace, world);
        assert_eq!(upload.identity, inventory.identity());
        assert_eq!(header[8], 0, "reader must not mint closed-world coverage");
        if header[3] == 0 {
            println!(
                "F1 world={world:08X} has no positive authored pages: published terrain permits missing-VHGT defaults; this world remains unknown rather than proving empty terrain"
            );
        }
        total_positive_pages += u64::from(header[3]);
    }
    assert!(
        total_positive_pages > 0,
        "this dataset provides no positive authored pages and cannot demonstrate positive adapter acceptance"
    );
}

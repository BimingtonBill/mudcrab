//! Final, asset-aware validation performed after all offline conversions.

use crate::{
    asset_path::{AssetKind, canonical_asset_path},
    mesh::MeshConverter,
};
use color_eyre::{Result, eyre::WrapErr};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

const MAX_REPORTED_ISSUES: usize = 100;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrationReport {
    pub schema_version: u32,
    pub statics_total: u64,
    pub statics_with_models: u64,
    pub bounds_updated: u64,
    pub references_total: u64,
    pub exterior_cells: u64,
    pub terrain_cells: u64,
    pub cache_cells: u64,
    pub texture_sets_with_diffuse: u64,
    pub waters_with_flow_normal: u64,
    pub lod_grids: u64,
    pub lod_terrain_blocks: u64,
    pub lod_object_blocks: u64,
    pub lod_tree_types: u64,
    pub lod_tree_instances: u64,
    pub missing_model_count: u64,
    pub invalid_model_count: u64,
    pub unavailable_model_source_count: u64,
    pub unbounded_model_count: u64,
    /// Interior room graph counters (db-schema.md section 15); all zero without room data.
    #[serde(default)]
    pub room_graph: crate::esm::rooms::RoomGraphReport,
    pub missing_texture_count: u64,
    pub unavailable_texture_source_count: u64,
    pub missing_lod_mesh_count: u64,
    pub issues: Vec<String>,
    /// Non-fatal observations. They are reported but do not fail the run.
    pub warnings: Vec<String>,
    pub passed: bool,
}

pub fn finalize_world_database(staging: &Path) -> Result<Option<IntegrationReport>> {
    let database_path = staging.join("skyrim_world.db");
    if !database_path.is_file() {
        return Ok(None);
    }
    let mut connection = Connection::open(&database_path)?;
    let mut report = IntegrationReport {
        schema_version: connection.query_row(
            "SELECT version FROM schema_info LIMIT 1",
            [],
            |row| row.get(0),
        )?,
        statics_total: count(&connection, "SELECT count(*) FROM statics")?,
        statics_with_models: count(
            &connection,
            "SELECT count(*) FROM statics WHERE model_path IS NOT NULL AND model_path <> ''",
        )?,
        references_total: count(&connection, "SELECT count(*) FROM \"references\"")?,
        exterior_cells: count(
            &connection,
            "SELECT count(*) FROM cells WHERE worldspace_id IS NOT NULL AND grid_x IS NOT NULL AND grid_y IS NOT NULL",
        )?,
        terrain_cells: count(&connection, "SELECT count(*) FROM land")?,
        texture_sets_with_diffuse: count(
            &connection,
            "SELECT count(*) FROM texture_sets WHERE diffuse_path IS NOT NULL AND diffuse_path <> ''",
        )?,
        waters_with_flow_normal: count(
            &connection,
            "SELECT count(*) FROM waters WHERE flow_normal_path IS NOT NULL AND flow_normal_path <> ''",
        )?,
        // The distant-LOD inventory is written by the pipeline before the
        // textures are converted; the report counts what reached the database.
        lod_grids: count(&connection, "SELECT count(*) FROM lod_grid")?,
        lod_terrain_blocks: count(
            &connection,
            "SELECT count(*) FROM lod_block WHERE kind = 'terrain'",
        )?,
        lod_object_blocks: count(
            &connection,
            "SELECT count(*) FROM lod_block WHERE kind = 'objects'",
        )?,
        lod_tree_types: count(&connection, "SELECT count(*) FROM lod_tree_type")?,
        lod_tree_instances: count(&connection, "SELECT count(*) FROM lod_tree_instance")?,
        ..Default::default()
    };
    let files = converted_file_index(staging)?;
    let sources = source_file_index(staging)?;
    let static_models = {
        let mut statement = connection.prepare(
            "SELECT id,model_path FROM statics WHERE model_path IS NOT NULL AND model_path <> '' ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let transaction = connection.transaction()?;
    for (form_id, model_path) in static_models {
        let key = converted_key(&model_path, "meshes", "glb")?;
        let Some(path) = files.get(&key) else {
            let source_key = converted_key(&model_path, "meshes", "nif")?;
            if sources.contains_key(&source_key) {
                report.missing_model_count += 1;
                issue(
                    &mut report,
                    format!("missing converted model {model_path} for {form_id:08X}"),
                );
            } else {
                report.unavailable_model_source_count += 1;
            }
            continue;
        };
        match MeshConverter::glb_bounds(path) {
            Ok(bounds) => {
                transaction.execute(
                    "UPDATE statics SET bounds_min_x=?1,bounds_min_y=?2,bounds_min_z=?3,bounds_max_x=?4,bounds_max_y=?5,bounds_max_z=?6,bounds_valid=1 WHERE id=?7",
                    params![bounds.min[0], bounds.min[1], bounds.min[2], bounds.max[0], bounds.max[1], bounds.max[2], form_id],
                )?;
                report.bounds_updated += 1;
            }
            Err(error) => {
                // Bethesda ships statics whose NIF carries no readable bounds
                // (427 of them in the retail tree), so this is a warning: the
                // runtime keeps the conservative default box for them.
                report.unbounded_model_count += 1;
                warning(
                    &mut report,
                    format!("model has no static bounds {model_path}: {error:#}"),
                );
            }
        }
    }
    transaction.commit()?;
    // Room membership by model bounds needs the bounds just written.
    crate::esm::rooms::add_overlap_rows(&mut connection)?;
    report.room_graph = crate::esm::rooms::RoomGraphReport::read(&connection)?;

    let diffuse_paths = {
        let mut statement = connection.prepare(
            "SELECT id,diffuse_path FROM texture_sets WHERE diffuse_path IS NOT NULL AND diffuse_path <> '' ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (form_id, texture_path) in diffuse_paths {
        let key = converted_key(&texture_path, "textures", "ktx2")?;
        if !files.contains_key(&key) {
            let source_key = converted_key(&texture_path, "textures", "dds")?;
            if sources.contains_key(&source_key) {
                report.missing_texture_count += 1;
                issue(
                    &mut report,
                    format!(
                        "missing converted diffuse texture {texture_path} for TXST {form_id:08X}"
                    ),
                );
            } else {
                report.unavailable_texture_source_count += 1;
            }
        }
    }
    let flow_paths = {
        let mut statement = connection.prepare(
            "SELECT id,flow_normal_path FROM waters WHERE flow_normal_path IS NOT NULL AND flow_normal_path <> '' ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (form_id, texture_path) in flow_paths {
        let key = converted_key(&texture_path, "textures", "ktx2")?;
        if !files.contains_key(&key) {
            let source_key = converted_key(&texture_path, "textures", "dds")?;
            if sources.contains_key(&source_key) {
                report.missing_texture_count += 1;
                issue(
                    &mut report,
                    format!(
                        "missing converted flow-normal texture {texture_path} for WATR {form_id:08X}"
                    ),
                );
            } else {
                report.unavailable_texture_source_count += 1;
            }
        }
    }
    // Distant-LOD meshes are published by the pipeline before the textures are
    // converted, so a missing one here means the block would spawn nothing.
    let lod_meshes = {
        let mut statement = connection.prepare(
            "SELECT DISTINCT mesh_path FROM lod_block \
             UNION SELECT mesh_path FROM lod_tree_type",
        )?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for mesh_path in lod_meshes {
        if !files.contains_key(&normalize(Path::new(&mesh_path))) {
            report.missing_lod_mesh_count += 1;
            issue(
                &mut report,
                format!("missing converted distant LOD mesh {mesh_path}"),
            );
        }
    }
    let cache_path = staging.join("cell_cache.rkyv");
    if cache_path.is_file() {
        let mmap = crate::esm::cell_cache::validate_cell_cache(&cache_path)?;
        let cache = rkyv::access::<shared::ArchivedCellCache, rkyv::rancor::Error>(&mmap)
            .wrap_err("invalid integration cell cache")?;
        report.cache_cells = cache.cells.len() as u64;
        if report.cache_cells != report.terrain_cells {
            let database_cells = report.terrain_cells;
            let cache_cells = report.cache_cells;
            issue(
                &mut report,
                format!(
                    "terrain/cache cell count mismatch: database={}, cache={}",
                    database_cells, cache_cells
                ),
            );
        }
    } else {
        issue(&mut report, "missing cell_cache.rkyv".to_owned());
    }
    // Every recorded issue is fatal: the counts above are only one view of
    // them, and a missing cell cache is invisible to the count comparison when
    // the database has no `land` rows (0 == 0).
    report.passed =
        report.schema_version == shared::WORLD_DATABASE_SCHEMA_VERSION && report.issues.is_empty();
    let output = staging.join("integration-report.json");
    // Staged outputs may share an inode with a previous pack via
    // hard link; replace the path instead of writing through it.
    if output.is_file() {
        fs::remove_file(&output)?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&report)?)
        .wrap_err_with(|| format!("failed to write {}", output.display()))?;
    Ok(Some(report))
}

fn source_file_index(staging: &Path) -> Result<HashMap<String, PathBuf>> {
    let root = staging.join("vfs");
    let mut files = HashMap::new();
    if !root.is_dir() {
        return Ok(files);
    }
    for entry in WalkDir::new(&root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let relative = entry.path().strip_prefix(&root)?;
            files.insert(normalize(relative), entry.into_path());
        }
    }
    Ok(files)
}

fn converted_file_index(staging: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut files = HashMap::new();
    for entry in WalkDir::new(staging).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let relative = entry.path().strip_prefix(staging)?;
            files.insert(normalize(relative), entry.into_path());
        }
    }
    Ok(files)
}

fn converted_key(source: &str, kind: &str, extension: &str) -> Result<String> {
    let kind = match kind {
        "meshes" => AssetKind::Mesh,
        "textures" => AssetKind::Texture,
        "scripts" => AssetKind::Script,
        _ => color_eyre::eyre::bail!("unsupported asset kind: {kind}"),
    };
    canonical_asset_path(source, kind, extension)
}

fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn count(connection: &Connection, sql: &str) -> Result<u64> {
    Ok(connection.query_row(sql, [], |row| row.get(0))?)
}

fn issue(report: &mut IntegrationReport, message: String) {
    if report.issues.len() < MAX_REPORTED_ISSUES {
        report.issues.push(message);
    }
}

fn warning(report: &mut IntegrationReport, message: String) {
    if report.warnings.len() < MAX_REPORTED_ISSUES {
        report.warnings.push(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_cache(directory: &Path) {
        let cache = shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: vec![],
        };
        fs::write(
            directory.join("cell_cache.rkyv"),
            rkyv::to_bytes::<rkyv::rancor::Error>(&cache).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn maps_creation_paths_to_converted_assets() {
        assert_eq!(
            converted_key("Meshes\\Architecture\\Wall.NIF", "meshes", "glb").unwrap(),
            "meshes/architecture/wall.glb"
        );
        assert_eq!(
            converted_key("land/grass.dds", "textures", "ktx2").unwrap(),
            "textures/land/grass.ktx2"
        );
    }

    #[test]
    fn enriches_database_with_real_glb_bounds() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("skyrim_world.db");
        let connection = Connection::open(&database).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO statics(id,model_path,flags) VALUES(1,'architecture/wall.nif',0)",
                [],
            )
            .unwrap();
        drop(connection);
        let mesh_path = directory.path().join("meshes/architecture/wall.glb");
        fs::create_dir_all(mesh_path.parent().unwrap()).unwrap();
        let mut json = br#"{
            "asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
            "nodes":[{"mesh":0}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
            "accessors":[{"min":[-2,-3,-4],"max":[5,6,7]}]
        }"#
        .to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let total = 20 + json.len();
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        fs::write(mesh_path, glb).unwrap();
        empty_cache(directory.path());

        let report = finalize_world_database(directory.path()).unwrap().unwrap();
        assert!(report.passed);
        assert_eq!(report.bounds_updated, 1);
        let connection = Connection::open(database).unwrap();
        let bounds: (f32, f32, i32) = connection
            .query_row(
                "SELECT bounds_min_x,bounds_max_z,bounds_valid FROM statics WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(bounds, (-2.0, 7.0, 1));
    }

    #[test]
    fn treats_undistributed_model_reference_as_coverage_metric() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("skyrim_world.db")).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO statics(id,model_path,flags) VALUES(1,'test/missing.nif',0)",
                [],
            )
            .unwrap();
        drop(connection);
        empty_cache(directory.path());

        let report = finalize_world_database(directory.path()).unwrap().unwrap();
        assert!(report.passed);
        assert_eq!(report.unavailable_model_source_count, 1);
        assert_eq!(report.missing_model_count, 0);
    }

    #[test]
    fn fails_when_the_cell_cache_is_missing_even_with_no_terrain() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("skyrim_world.db")).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        drop(connection);

        let report = finalize_world_database(directory.path()).unwrap().unwrap();
        assert!(!report.passed);
        assert_eq!(
            report.cache_cells, report.terrain_cells,
            "0 == 0 is not a pass"
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue == "missing cell_cache.rkyv"),
            "{:?}",
            report.issues
        );
    }

    #[test]
    fn reports_models_without_readable_bounds_as_warnings() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("skyrim_world.db")).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO statics(id,model_path,flags) VALUES(1,'test/unbounded.nif',0)",
                [],
            )
            .unwrap();
        drop(connection);
        let mesh_path = directory.path().join("meshes/test/unbounded.glb");
        fs::create_dir_all(mesh_path.parent().unwrap()).unwrap();
        let mut json = br#"{"asset":{"version":"2.0"}}"#.to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let total = 20 + json.len();
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        fs::write(&mesh_path, glb).unwrap();
        empty_cache(directory.path());

        let report = finalize_world_database(directory.path()).unwrap().unwrap();
        assert!(
            report.passed,
            "models without bounds are a warning, not a failure"
        );
        assert_eq!(report.unbounded_model_count, 1);
        assert!(report.issues.is_empty());
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("no static bounds")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn fails_when_existing_model_source_has_no_converted_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("skyrim_world.db")).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO statics(id,model_path,flags) VALUES(1,'test/lost.nif',0)",
                [],
            )
            .unwrap();
        drop(connection);
        let source = directory.path().join("vfs/meshes/test/lost.nif");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(source, b"source exists").unwrap();
        empty_cache(directory.path());

        let report = finalize_world_database(directory.path()).unwrap().unwrap();
        assert!(!report.passed);
        assert_eq!(report.missing_model_count, 1);
        assert_eq!(report.unavailable_model_source_count, 0);
    }

    fn write_box_glb(path: &Path, min: [f32; 3], max: [f32; 3]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut json = format!(
            r#"{{
            "asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
            "accessors":[{{"min":{min:?},"max":{max:?}}}]
        }}"#
        )
        .into_bytes();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((20 + json.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        fs::write(path, glb).unwrap();
    }

    /// Objects whose pivot is outside a room but whose model reaches into it get a
    /// `contained = 0` row. The GLB bounds are glTF-axis (Y-up) numbers in Creation units:
    /// this model is 100 long along Creation X, 20 along Creation Y and 10 along Creation Z.
    #[test]
    fn adds_overlap_rows_from_model_bounds_in_creation_axes_and_units() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("skyrim_world.db");
        let connection = Connection::open(&database).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO statics(id,model_path,flags) VALUES(0x500,'architecture/wall.nif',0)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO room_bounds(ref_id,cell_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,half_x,half_y,half_z)
                 VALUES(0x1001,0x100,0,0,0,0,0,0,100,100,100)",
                [],
            )
            .unwrap();
        let place = |id: u32, position: [f32; 3], rot_z: f32, scale: f32| {
            connection
                .execute(
                    "INSERT INTO \"references\"(id,cell_id,base_form_id,is_exterior,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,scale)
                     VALUES(?1,0x100,0x500,0,?2,?3,?4,0,0,?5,?6)",
                    rusqlite::params![id, position[0], position[1], position[2], rot_z, scale],
                )
                .unwrap();
        };
        let quarter_turn = std::f32::consts::FRAC_PI_2;
        // Pivot inside: a pivot row, never also an overlap row.
        place(0x2001, [10.0, 0.0, 0.0], 0.0, 1.0);
        connection
            .execute(
                "INSERT INTO reference_rooms(reference_id,room_ref,contained) VALUES(0x2001,0x1001,1)",
                [],
            )
            .unwrap();
        // Pivot outside on X; the 100-long side reaches in (x 90..190).
        place(0x2002, [140.0, 0.0, 0.0], 0.0, 1.0);
        // Far away.
        place(0x2003, [300.0, 0.0, 0.0], 0.0, 1.0);
        // A quarter turn about Z puts the long side along Y: x only spans 130..150.
        place(0x2004, [140.0, 0.0, 0.0], quarter_turn, 1.0);
        // The same turn at y = 140: the long side now reaches y 90..190.
        place(0x2005, [0.0, 140.0, 0.0], quarter_turn, 1.0);
        // Scale 0.1 shrinks the reach to 135..145.
        place(0x2006, [140.0, 0.0, 0.0], 0.0, 0.1);
        // Creation Z extent is +-5 (runtime Y), so z = 108 reaches only 103..113. Reading the
        // glTF Y axis as Creation Y would make it +-5 in y and +-10 in z, and overlap.
        place(0x2007, [0.0, 0.0, 108.0], 0.0, 1.0);
        // Creation Y extent is +-10 (runtime Z): y = 108 reaches 98..118 and overlaps.
        place(0x2008, [0.0, 108.0, 0.0], 0.0, 1.0);
        drop(connection);
        write_box_glb(
            &directory.path().join("meshes/architecture/wall.glb"),
            [-50.0, -5.0, -10.0],
            [50.0, 5.0, 10.0],
        );
        empty_cache(directory.path());

        let report = finalize_world_database(directory.path()).unwrap().unwrap();
        assert!(report.passed);
        let connection = Connection::open(database).unwrap();
        let rows: Vec<(u32, i64)> = connection
            .prepare("SELECT reference_id,contained FROM reference_rooms ORDER BY reference_id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![(0x2001, 1), (0x2002, 0), (0x2005, 0), (0x2008, 0),]
        );
        assert_eq!(report.room_graph.overlap_rows, 3);
        // Running the stage again replaces the overlap rows rather than duplicating them.
        let mut connection = connection;
        assert_eq!(
            crate::esm::rooms::add_overlap_rows(&mut connection).unwrap(),
            3
        );
    }

    /// A room turned 0.6 rad with an asymmetric box, and a model turned with it: an object placed
    /// along the room's long axis overlaps, its mirror image across the X axis does not. The
    /// forward matrix, or the opposite turn, would swap the two.
    #[test]
    fn overlap_rows_follow_a_rotated_room_and_rotated_model() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("skyrim_world.db");
        let connection = Connection::open(&database).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO statics(id,model_path,flags) VALUES(0x500,'architecture/wall.nif',0)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO room_bounds(ref_id,cell_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,half_x,half_y,half_z)
                 VALUES(0x1001,0x100,0,0,0,0,0,0.6,200,20,50)",
                [],
            )
            .unwrap();
        let (sin, cos) = 0.6f32.sin_cos();
        // 240 along the room's long axis (local +X is (cos, -sin) in world), turned with it, so
        // the model's 100-long side lies along the axis and spans 190..290, past the end at 200.
        for (id, y_sign) in [(0x2001u32, -1.0f32), (0x2002, 1.0)] {
            connection
                .execute(
                    "INSERT INTO \"references\"(id,cell_id,base_form_id,is_exterior,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,scale)
                     VALUES(?1,0x100,0x500,0,?2,?3,0,0,0,0.6,1)",
                    rusqlite::params![id, 240.0 * cos, y_sign * 240.0 * sin],
                )
                .unwrap();
        }
        drop(connection);
        write_box_glb(
            &directory.path().join("meshes/architecture/wall.glb"),
            [-50.0, -5.0, -10.0],
            [50.0, 5.0, 10.0],
        );
        empty_cache(directory.path());
        finalize_world_database(directory.path()).unwrap().unwrap();
        let connection = Connection::open(database).unwrap();
        let rows: Vec<(u32, i64)> = connection
            .prepare("SELECT reference_id,contained FROM reference_rooms ORDER BY reference_id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![(0x2001, 0)]);
    }

    /// A reused older database whose `statics` has no bounds columns finalises, and the overlap
    /// pass leaves it alone even when it holds rooms.
    #[test]
    fn finalises_a_legacy_database_without_bounds_columns() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("skyrim_world.db");
        let connection = Connection::open(&database).unwrap();
        crate::esm::exporter::create_tables(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE statics;
                 CREATE TABLE statics(id INTEGER PRIMARY KEY, editor_id TEXT, model_path TEXT, flags INTEGER NOT NULL);",
            )
            .unwrap();
        drop(connection);
        empty_cache(directory.path());
        assert!(finalize_world_database(directory.path()).unwrap().is_some());

        let mut connection = Connection::open(&database).unwrap();
        connection
            .execute(
                "INSERT INTO room_bounds(ref_id,cell_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,half_x,half_y,half_z)
                 VALUES(1,1,0,0,0,0,0,0,1,1,1)",
                [],
            )
            .unwrap();
        assert_eq!(
            crate::esm::rooms::add_overlap_rows(&mut connection).unwrap(),
            0
        );
    }
}

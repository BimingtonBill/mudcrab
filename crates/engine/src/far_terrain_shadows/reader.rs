//! Positive-evidence reader adapter for S5's mathematical canonical BASE.
//!
//! Published canonical outputs are trusted producer outputs under the existing
//! package consistency/builder contracts, not a new external allowlist. Positive
//! authored status means the winning projected LAND field plus exact core-cache
//! correspondence; it makes no original-ESM completeness or global parity claim.
//! No systems, registration, absence attestations or rendered-GPU equivalence.

use super::{
    AuthoredStatus, CanonicalRecipeIdentity, CellCoord, CellRect, CoreCacheIdentity,
    DatabaseIdentity, InventoryBuilder, RawGrid, SnapshotIdentity, SurfaceInput, SurfaceKind,
    SurfaceTriangulation, WorldHeightInventory, encode_heights_le,
    packet::{CellInput, RawVhgtStatus, ShadowPacket, SourceStatus},
};
use crate::{
    cluster::{reader::CertifiedPackage, terrain::CanonicalTerrainHeights},
    world::{cache::CellCache, components::CELL_SIZE},
};
use color_eyre::{
    Result,
    eyre::{WrapErr, ensure, eyre},
};
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::Path,
    sync::Arc,
};

const SIDE: usize = 33;
const SAMPLES: usize = SIDE * SIDE;
const CATALOG_RECIPE: &[u8] = b"mudcrab.s5.positive-canonical-reader.v2\0";
// CELL record HEADER flag, not CELL.DATA. xEdit 4.1.5f definitions,
// Core/wbDefinitionsTES5.pas:6107-6113 identify bit 10 as Persistent.
// Persistent exterior containers may also carry XCLC(0,0); they are not a
// second terrain-grid placement. See the inventory research note.
const CELL_HEADER_PERSISTENT: u32 = 0x0000_0400;

/// Created ONCE per active package by its integration owner. Worlds share this
/// Arc; neither world construction nor tracing rehashes/reopens the cell cache.
/// The loaded canonical-height handle is captured once with this package; each
/// world still compares its selected bytes against the same retained database.
/// No cache/type dependency is added to CertifiedPackage itself.
pub(crate) struct CertifiedTerrainSnapshot {
    package: Arc<CertifiedPackage>,
    database: DatabaseIdentity,
    core_cache: CoreCacheIdentity,
    cache: CellCache,
    heights: CanonicalTerrainHeights,
    canonical_package: [u8; 32],
    recipe: u32,
    source: SourceStatus,
}

impl CertifiedTerrainSnapshot {
    pub(crate) fn open(
        package: Arc<CertifiedPackage>,
        heights: CanonicalTerrainHeights,
    ) -> Result<Arc<Self>> {
        let envelope = package.envelope();
        let entry = envelope.terrain.as_ref().ok_or_else(|| {
            eyre!("canonical shadow inventory requires certified terrain extension")
        })?;
        let root = &envelope.root;
        let database_path = root.join("skyrim_world.db");
        no_sqlite_sidecars(&database_path)?;
        plain_file(&database_path)?;
        let database_hash = digest_file(&database_path)?;
        let connection =
            Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let schema: u32 =
            connection.query_row("SELECT version FROM schema_info", [], |r| r.get(0))?;
        let actual_core = shared::extensions::core_build_identity(&connection)?
            .ok_or_else(|| eyre!("missing core identity"))?;
        ensure!(
            actual_core == envelope.core_identity,
            "core identity changed after certification"
        );
        let cache_path = root.join("cell_cache.rkyv");
        plain_file(&cache_path)?;
        let cache_hash = digest_file(&cache_path)?;
        let cache = CellCache::open(&cache_path)?;
        // The terrain manifest is overlay payload; the cell cache and database above are core.
        let manifest_path = envelope.overlay.join("cluster-terrain-manifest.json");
        plain_file(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        ensure!(
            manifest["cell_cache_sha256"].as_str()
                == Some(shared::cluster::hex(&cache_hash).as_str()),
            "terrain cache identity differs"
        );
        let recipe = u32::try_from(
            manifest["terrain_recipe_version"]
                .as_u64()
                .ok_or_else(|| eyre!("missing terrain recipe"))?,
        )?;
        ensure!(recipe == 1, "unsupported canonical terrain recipe");
        let canonical_package = shared::cluster::parse_hash(&entry.sha256)
            .ok_or_else(|| eyre!("invalid certified terrain extension hash"))?;
        let conversion_path = root.join("conversion-manifest.json");
        plain_file(&conversion_path)?;
        let conversion: serde_json::Value = serde_json::from_slice(&fs::read(&conversion_path)?)?;
        let source = SourceStatus {
            complete: manifest_flag(&conversion, "complete")?,
            record_data_complete: manifest_flag(&conversion, "record_data_complete")?,
            // Only exact published canonical grids enter a world inventory.
            // No default/missing surface is made canonical by this flag.
            post_worldspace_weld: true,
        };
        // Both expensive consistency passes are ONCE per snapshot, never per world.
        no_sqlite_sidecars(&database_path)?;
        ensure!(
            digest_file(&database_path)? == database_hash,
            "database changed while opening snapshot"
        );
        ensure!(
            digest_file(&cache_path)? == cache_hash,
            "cache changed while opening snapshot"
        );
        Ok(Arc::new(Self {
            package,
            database: DatabaseIdentity {
                schema_revision: schema,
                content_hash: database_hash,
            },
            core_cache: CoreCacheIdentity {
                version: shared::CELL_CACHE_VERSION,
                hash: cache_hash,
            },
            cache,
            heights,
            canonical_package,
            recipe,
            source,
        }))
    }

    /// Exact world only. None is an unknown/no-explicit-domain result, not empty
    /// terrain. Source completeness flags never attest missing cells or regions.
    pub(crate) fn world(self: &Arc<Self>, world: u32) -> Result<Option<CertifiedWorldInventory>> {
        // The base's core tables and the overlay's `cluster_terrain_heights`, bound and read-only.
        let connection = self.package.envelope().connection()?;
        ensure!(
            connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM worldspaces WHERE id=?1)",
                [world],
                |r| r.get::<_, bool>(0)
            )?,
            "requested world is not explicitly present"
        );
        let cells = enumerate_cells(&connection, world)?;
        let Some(domain) = domain(&cells)? else {
            return Ok(None);
        };
        let mut catalog = Sha256::new();
        catalog.update(CATALOG_RECIPE);
        catalog.update(world.to_le_bytes());
        let mut content = Sha256::new();
        content.update(CATALOG_RECIPE);
        let mut positive = Vec::new();
        for cell in &cells {
            catalog.update(cell.id.to_le_bytes());
            catalog.update(cell.grid.x.to_le_bytes());
            catalog.update(cell.grid.y.to_le_bytes());
            match &cell.vhgt {
                Some(raw) => {
                    catalog.update([1]);
                    catalog.update((raw.len() as u64).to_le_bytes());
                    catalog.update(Sha256::digest(raw));
                }
                None => catalog.update([0]),
            }
            let stored: Option<Vec<u8>> = connection.query_row(
                "SELECT heights FROM cluster_terrain_heights WHERE worldspace_id=?1 AND grid_x=?2 AND grid_y=?3",
                rusqlite::params![world, cell.grid.x, cell.grid.y], |r| r.get(0),
            ).optional()?;
            match &stored {
                Some(bytes) => {
                    catalog.update([1]);
                    catalog.update((bytes.len() as u64).to_le_bytes());
                    catalog.update(Sha256::digest(bytes));
                }
                None => catalog.update([0]),
            }
            let Some(raw) = cell.vhgt.as_deref().filter(|raw| !raw.is_empty()) else {
                // Projection collapses missing VHGT and present-empty. Finite
                // cache zeros cannot establish either authored status.
                continue;
            };
            let decoded = decode_positive_vhgt(raw)
                .wrap_err_with(|| format!("cell {:08X}: malformed VHGT", cell.id))?;
            let (width, height, cached) = self
                .cache
                .terrain_heights(cell.id)
                .ok_or_else(|| eyre!("cell {:08X}: positive LAND missing from cache", cell.id))?;
            validate_correspondence(width, height, cached, &decoded)?;
            let Some(bytes) = stored else { continue };
            validate_canonical(
                &bytes,
                self.heights
                    .get(world, bevy::math::IVec2::new(cell.grid.x, cell.grid.y)),
            )?;
            content.update(cell.id.to_le_bytes());
            content.update(cell.grid.x.to_le_bytes());
            content.update(cell.grid.y.to_le_bytes());
            content.update(Sha256::digest(&bytes));
            positive.push(cell.grid);
        }
        let identity = SnapshotIdentity {
            worldspace: world,
            cell_size_bits: f64::from(CELL_SIZE).to_bits(),
            database: self.database,
            core_cache: self.core_cache,
            canonical: Some(CanonicalRecipeIdentity {
                package: self.canonical_package,
                catalog: catalog.finalize().into(),
                content_digest: content.finalize().into(),
                recipe_revision: self.recipe,
                triangulation: SurfaceTriangulation::CanonicalBase,
            }),
        };
        let mut builder = InventoryBuilder::new(identity, f64::from(CELL_SIZE), domain)
            .map_err(|error| eyre!("invalid inventory domain: {error:?}"))?;
        for cell in &positive {
            let heights = self
                .heights
                .get(world, bevy::math::IVec2::new(cell.x, cell.y))
                .ok_or_else(|| eyre!("canonical resource disappeared"))?;
            let bytes = encode_heights_le(heights);
            builder
                .add_cell(
                    *cell,
                    AuthoredStatus::VhgtPresent,
                    &[SurfaceInput {
                        kind: SurfaceKind::CanonicalFromAuthored,
                        raw: RawGrid {
                            bytes: &bytes,
                            expected_sha256: Sha256::digest(&bytes).into(),
                        },
                    }],
                )
                .map_err(|error| eyre!("invalid inventory cell: {error:?}"))?;
        }
        let positive: BTreeSet<_> = positive.into_iter().collect();
        let packet_cells = cells
            .iter()
            .map(|cell| CellInput {
                cell: cell.grid,
                vhgt: if positive.contains(&cell.grid) {
                    RawVhgtStatus::PresentValid
                } else {
                    RawVhgtStatus::Unknown
                },
            })
            .collect();
        // No known-empty or closed-world attestations, even if source flags true.
        Ok(Some(CertifiedWorldInventory {
            snapshot: self.clone(),
            inventory: builder.build(),
            packet_cells,
        }))
    }
}

pub(crate) struct CertifiedWorldInventory {
    snapshot: Arc<CertifiedTerrainSnapshot>,
    inventory: WorldHeightInventory,
    packet_cells: Vec<CellInput>,
}
impl CertifiedWorldInventory {
    #[cfg(test)]
    pub(crate) fn identity(&self) -> SnapshotIdentity {
        *self.inventory.identity()
    }
    #[cfg(test)]
    pub(crate) fn trace(&self, origin: [f64; 3], direction: [f64; 3]) -> super::RayResult {
        self.inventory.trace(
            &super::SurfaceQuery::canonical_base(
                *self.inventory.identity(),
                super::SurfaceSource::Canonical,
            ),
            origin,
            direction,
        )
    }
    pub(crate) fn build_packet(&self) -> Result<CertifiedShadowPacket> {
        let packet = ShadowPacket::build(
            &self.inventory,
            self.packet_cells.iter().copied(),
            self.snapshot.source,
        )
        .map_err(|error| eyre!("cannot build canonical shadow packet: {error:?}"))?;
        Ok(CertifiedShadowPacket {
            _snapshot: self.snapshot.clone(),
            packet,
        })
    }
}

/// Temporary verbose output of ShadowPacket::build for CPU inspection.
/// Runtime world jobs consume it through into_gpu before publishing; standalone
/// inspection may borrow packet() while this owner retains its snapshot guard.
pub(crate) struct CertifiedShadowPacket {
    _snapshot: Arc<CertifiedTerrainSnapshot>,
    packet: ShadowPacket,
}
impl CertifiedShadowPacket {
    #[cfg(test)]
    pub(crate) fn packet(&self) -> &ShadowPacket {
        &self.packet
    }

    /// Compact on the existing background world job. The verbose packet is
    /// consumed and dropped; the exact snapshot/guard moves into the upload owner.
    pub(crate) fn into_gpu(
        self,
        limits: super::gpu::UploadLimits,
    ) -> std::result::Result<CertifiedGpuUpload, super::gpu::UploadRefusal> {
        let Self { _snapshot, packet } = self;
        let upload = super::gpu::GpuUpload::build(&packet, limits)?;
        #[cfg(target_endian = "big")]
        let storage_le = upload
            .words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        Ok(CertifiedGpuUpload {
            _snapshot,
            upload,
            #[cfg(target_endian = "big")]
            storage_le,
        })
    }
}

/// Private construction binds the compact upload to its certified source guard.
/// A renderer retains this owner through GPU binding retirement, not just upload().
pub(crate) struct CertifiedGpuUpload {
    _snapshot: Arc<CertifiedTerrainSnapshot>,
    upload: super::gpu::GpuUpload,
    #[cfg(target_endian = "big")]
    storage_le: Vec<u8>,
}

impl CertifiedGpuUpload {
    pub(crate) fn upload(&self) -> &super::gpu::GpuUpload {
        &self.upload
    }

    /// Exact little-endian protocol bytes, prepared by the background job.
    /// Borrow native words directly on little-endian hosts; no world-sized
    /// allocation or serialization is performed by the render consumer.
    pub(crate) fn storage_bytes(&self) -> &[u8] {
        #[cfg(target_endian = "little")]
        {
            bytemuck::cast_slice(&self.upload.words)
        }
        #[cfg(target_endian = "big")]
        {
            &self.storage_le
        }
    }
}

fn manifest_flag(manifest: &serde_json::Value, key: &str) -> Result<bool> {
    match manifest.get(key) {
        None | Some(serde_json::Value::Null) => Ok(false),
        Some(serde_json::Value::Bool(value)) => Ok(*value),
        Some(_) => Err(eyre!("invalid source completeness flag {key}")),
    }
}

use rusqlite::OptionalExtension;

struct DatabaseCell {
    id: u32,
    grid: CellCoord,
    vhgt: Option<Vec<u8>>,
}

fn enumerate_cells(connection: &Connection, world: u32) -> Result<Vec<DatabaseCell>> {
    let mut statement = connection.prepare(
        "SELECT c.id,c.grid_x,c.grid_y,l.heightmap,c.flags FROM cells c LEFT JOIN land l ON l.cell_id=c.id \
         WHERE c.worldspace_id=?1 \
         ORDER BY c.grid_y,c.grid_x,c.id",
    )?;
    let projected = statement
        .query_map([world], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, Option<i32>>(1)?,
                r.get::<_, Option<i32>>(2)?,
                r.get::<_, Option<Vec<u8>>>(3)?,
                r.get::<_, u32>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut cells = Vec::with_capacity(projected.len());
    let mut grids = BTreeSet::new();
    for (id, x, y, vhgt, flags) in projected {
        if flags & CELL_HEADER_PERSISTENT != 0 {
            ensure!(
                vhgt.is_none(),
                "persistent CELL {id:08X} has LAND; terrain-grid role is unresolved"
            );
            continue;
        }
        let (Some(x), Some(y)) = (x, y) else {
            continue; // No explicit grid coordinate; never an empty-cell proof.
        };
        let cell = DatabaseCell {
            id,
            grid: CellCoord { x, y },
            vhgt,
        };
        ensure!(
            grids.insert(cell.grid),
            "duplicate cell grid in world {world} at ({x},{y}), CELL {id:08X}"
        );
        cells.push(cell);
    }
    Ok(cells)
}

fn domain(cells: &[DatabaseCell]) -> Result<Option<CellRect>> {
    let Some(first) = cells.first() else {
        return Ok(None);
    };
    let mut min = first.grid;
    let mut max = first.grid;
    for cell in cells {
        min.x = min.x.min(cell.grid.x);
        min.y = min.y.min(cell.grid.y);
        max.x = max.x.max(cell.grid.x);
        max.y = max.y.max(cell.grid.y);
    }
    CellRect::new(min, max)
        .map(Some)
        .map_err(|error| eyre!("invalid domain: {error:?}"))
}

/// Exact converter decode order (esm/cell_cache.rs::decode_vhgt), deliberately
/// refusing the empty-input renderer-default branch. Opaque trailing bytes are
/// retained in the catalog hash and are not interpreted as extra samples.
fn decode_positive_vhgt(raw: &[u8]) -> Result<Vec<f32>> {
    ensure!(raw.len() >= 4 + SAMPLES, "truncated or empty VHGT");
    let offset = f32::from_le_bytes(raw[..4].try_into()?);
    let mut row_origin = offset * 8.0;
    ensure!(
        offset.is_finite() && row_origin.is_finite(),
        "invalid VHGT offset"
    );
    let mut heights = Vec::with_capacity(SAMPLES);
    for row in raw[4..4 + SAMPLES].as_chunks::<SIDE>().0 {
        row_origin += f32::from(row[0] as i8) * 8.0;
        let mut height = row_origin;
        heights.push(height);
        for delta in &row[1..] {
            height += f32::from(*delta as i8) * 8.0;
            heights.push(height);
        }
    }
    ensure!(
        heights.iter().all(|height| height.is_finite()),
        "nonfinite VHGT heights"
    );
    Ok(heights)
}

fn validate_correspondence(
    width: u16,
    height: u16,
    cached: impl ExactSizeIterator<Item = f32>,
    decoded: &[f32],
) -> Result<()> {
    ensure!(
        usize::from(width) == SIDE && usize::from(height) == SIDE,
        "cache is not 33x33"
    );
    ensure!(
        cached.len() == SAMPLES && decoded.len() == SAMPLES,
        "cache sample count differs"
    );
    ensure!(
        cached
            .zip(decoded)
            .all(|(a, b)| a.is_finite() && a.to_bits() == b.to_bits()),
        "cache is not the exact authored VHGT decode"
    );
    Ok(())
}

fn validate_canonical(bytes: &[u8], resource: Option<&[f32]>) -> Result<()> {
    ensure!(bytes.len() == SAMPLES * 4, "canonical height count differs");
    let resource =
        resource.ok_or_else(|| eyre!("canonical DB row missing from terrain resource"))?;
    ensure!(
        resource.len() == SAMPLES && resource.iter().all(|h| h.is_finite()),
        "invalid canonical resource"
    );
    ensure!(
        encode_heights_le(resource) == bytes,
        "terrain resource differs from certified DB"
    );
    Ok(())
}

// The retained lock protects this root, not a target outside it reached by a
// symlink/junction file. Fixed input paths need no generic asset-path resolver.
fn plain_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "inventory input is not a plain regular file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "inventory input is a reparse point"
        );
    }
    Ok(())
}

fn no_sqlite_sidecars(database: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut path = database.as_os_str().to_owned();
        path.push(suffix);
        match fs::symlink_metadata(Path::new(&path)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => {
                return Err(eyre!(
                    "SQLite sidecar present: cannot hash main DB as full snapshot"
                ));
            }
        }
    }
    Ok(())
}
fn digest_file(path: &Path) -> Result<[u8; 32]> {
    let mut input = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vhgt() -> Vec<u8> {
        let mut bytes = 2f32.to_le_bytes().to_vec();
        bytes.extend(vec![0; SAMPLES]);
        bytes[4] = 1;
        bytes[5] = 255;
        bytes[4 + SIDE] = 2;
        bytes.extend([7, 8, 9]);
        bytes
    }
    /// Independently executes the actual converter's cache writer. This is not
    /// a second invocation of the adapter decoder nor hand-authored expected
    /// cache floats. Large offsets exercise f32 rounding during accumulation.
    #[test]
    fn positive_decoder_matches_real_converter_cache_writer_bit_for_bit() {
        use converter::esm::{cell_cache::write_cell_cache, records::RawRecord};
        use std::collections::HashMap;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cell_cache.rkyv");
        let mut records = HashMap::new();
        let mut raw_by_cell = Vec::new();
        for (index, offset) in [2.0_f32, 16_777_216.0, -1000.125].into_iter().enumerate() {
            let cell = 20 + index as u32;
            let mut raw = offset.to_le_bytes().to_vec();
            raw.extend((0..SAMPLES).map(|sample| ((sample * 37 + index * 17) % 256) as u8));
            raw.extend([7, 8, 9]);
            records.insert(
                100 + index as u32,
                RawRecord {
                    form_id: 100 + index as u32,
                    record_type: *b"LAND",
                    flags: 0,
                    subrecords: vec![(b"VHGT".to_vec(), raw.clone())],
                    cell_form_id: Some(cell),
                    worldspace_form_id: Some(60),
                    load_order: 0,
                },
            );
            raw_by_cell.push((cell, raw));
        }
        assert_eq!(write_cell_cache(&records, &path).unwrap(), 3);
        let cache = CellCache::open(&path).unwrap();
        for (cell, raw) in raw_by_cell {
            let decoded = decode_positive_vhgt(&raw).unwrap();
            let (width, height, actual) = cache.terrain_heights(cell).unwrap();
            assert_eq!((width, height), (33, 33));
            assert_eq!(
                actual.map(f32::to_bits).collect::<Vec<_>>(),
                decoded.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                "cell {cell}"
            );
        }
    }

    #[test]
    fn positive_decode_preserves_source_delta_order_and_refuses_default_or_invalid_input() {
        let decoded = decode_positive_vhgt(&vhgt()).unwrap();
        assert_eq!(&decoded[..3], &[24.0, 16.0, 16.0]);
        assert_eq!(decoded[SIDE], 40.0);
        assert_eq!(decoded[SIDE + 1], 40.0);
        assert!(decode_positive_vhgt(&[]).is_err());
        assert!(decode_positive_vhgt(&vhgt()[..4 + SAMPLES - 1]).is_err());
        let mut bad = vhgt();
        bad[..4].copy_from_slice(&f32::INFINITY.to_le_bytes());
        assert!(decode_positive_vhgt(&bad).is_err());
    }
    #[test]
    fn cache_and_canonical_evidence_are_bound_to_actual_bytes_not_just_counts() {
        let decoded = decode_positive_vhgt(&vhgt()).unwrap();
        validate_correspondence(33, 33, decoded.iter().copied(), &decoded).unwrap();
        let mut changed = decoded.clone();
        changed[101] += 8.0;
        assert!(validate_correspondence(33, 33, changed.iter().copied(), &decoded).is_err());
        assert!(validate_correspondence(17, 33, decoded.iter().copied(), &decoded).is_err());
        let bytes = encode_heights_le(&changed); // Canonical weld CAN differ from raw cache.
        validate_canonical(&bytes, Some(&changed)).unwrap();
        assert!(validate_canonical(&bytes, Some(&decoded)).is_err());
        assert!(validate_canonical(&bytes, None).is_err());
        let mut nonfinite = changed;
        nonfinite[99] = f32::NAN;
        assert!(validate_canonical(&encode_heights_le(&nonfinite), Some(&nonfinite)).is_err());
    }
    #[test]
    fn actual_db_enumeration_is_same_world_and_missing_land_stays_unresolved() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,flags INTEGER NOT NULL DEFAULT 0); CREATE TABLE land(cell_id INTEGER PRIMARY KEY,heightmap BLOB NOT NULL); INSERT INTO cells(id,worldspace_id,grid_x,grid_y) VALUES(1,60,-2,3),(2,60,-1,3),(3,61,-2,3),(4,60,NULL,NULL); INSERT INTO land VALUES(1,x''),(3,x'01');").unwrap();
        let cells = enumerate_cells(&db, 60).unwrap();
        assert_eq!(cells.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(cells[0].vhgt.as_deref(), Some([].as_slice()));
        assert!(cells[1].vhgt.is_none());
        let bounds = domain(&cells).unwrap().unwrap();
        assert_eq!(bounds.min(), CellCoord { x: -2, y: 3 });
        assert_eq!(bounds.max(), CellCoord { x: -1, y: 3 });
        assert!(
            domain(&enumerate_cells(&db, 99).unwrap())
                .unwrap()
                .is_none()
        );
        db.execute_batch("INSERT INTO cells(id,worldspace_id,grid_x,grid_y) VALUES(5,60,-2,3)")
            .unwrap();
        assert!(enumerate_cells(&db, 60).is_err());
    }
    #[test]
    fn persistent_containers_do_not_alias_grid_cells_or_turn_missing_land_into_empty() {
        for persistent_id in [1, 9] {
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch("CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,flags INTEGER NOT NULL); CREATE TABLE land(cell_id INTEGER PRIMARY KEY,heightmap BLOB NOT NULL); INSERT INTO cells VALUES(5,70,0,0,262144); INSERT INTO land VALUES(5,x'01');").unwrap();
            db.execute(
                "INSERT INTO cells VALUES(?1,70,0,0,263168)",
                [persistent_id],
            )
            .unwrap();
            let cells = enumerate_cells(&db, 70).unwrap();
            assert_eq!(cells.len(), 1);
            assert_eq!(cells[0].id, 5, "role must not follow FormID priority");
            assert_eq!(cells[0].vhgt.as_deref(), Some([1].as_slice()));
            db.execute("DELETE FROM land", []).unwrap();
            let cells = enumerate_cells(&db, 70).unwrap();
            assert_eq!(cells.len(), 1);
            assert_eq!(cells[0].id, 5);
            assert!(
                cells[0].vhgt.is_none(),
                "ordinary missing LAND remains Unknown"
            );
            db.execute("INSERT INTO land VALUES(?1,x'')", [persistent_id])
                .unwrap();
            assert!(
                format!("{:#}", enumerate_cells(&db, 70).err().unwrap())
                    .contains("persistent CELL")
            );
        }
    }
    #[test]
    fn genuine_grid_ambiguity_and_invalid_header_flags_are_not_resolved_by_land_priority() {
        for land_ids in [&[][..], &[1][..], &[1, 2][..]] {
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch("CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,flags INTEGER NOT NULL); CREATE TABLE land(cell_id INTEGER PRIMARY KEY,heightmap BLOB NOT NULL); INSERT INTO cells VALUES(1,70,0,0,0),(2,70,0,0,0);").unwrap();
            for id in land_ids {
                db.execute("INSERT INTO land VALUES(?1,x'01')", [id])
                    .unwrap();
            }
            assert!(
                format!("{:#}", enumerate_cells(&db, 70).err().unwrap())
                    .contains("duplicate cell grid")
            );
        }
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,flags INTEGER NOT NULL); CREATE TABLE land(cell_id INTEGER PRIMARY KEY,heightmap BLOB NOT NULL); INSERT INTO cells VALUES(1,70,NULL,NULL,1024); INSERT INTO land VALUES(1,x'01');").unwrap();
        assert!(
            enumerate_cells(&db, 70).is_err(),
            "unlocated persistent LAND is also contradictory"
        );
        db.execute("DELETE FROM land", []).unwrap();
        db.execute("UPDATE cells SET flags=-1", []).unwrap();
        assert!(
            enumerate_cells(&db, 70).is_err(),
            "header flags must be valid u32"
        );
    }
    #[test]
    fn main_database_hash_never_ignores_a_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("world.db");
        fs::write(&database, b"invented snapshot").unwrap();
        no_sqlite_sidecars(&database).unwrap();
        let before = digest_file(&database).unwrap();
        fs::write(dir.path().join("world.db-wal"), b"uncheckpointed").unwrap();
        assert!(no_sqlite_sidecars(&database).is_err());
        fs::remove_file(dir.path().join("world.db-wal")).unwrap();
        fs::write(&database, b"different snapshot").unwrap();
        assert_ne!(digest_file(&database).unwrap(), before);
    }
}

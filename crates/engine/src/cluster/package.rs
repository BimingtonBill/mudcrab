//! Loading a converted cluster package (ADR-0013) into the cluster draw path.
//!
//! A background task certifies the package while source scenes decode speculatively. Only after
//! certification passes does the prototype read each instance block and mesh payload
//! the blocks name, then one batch per distinct material. Streaming blocks and meshes in and out
//! as the camera moves replaces this module's loading; reference mapping is validated separately.
//!
//! A batch is the instances that draw with one stock material: the same model, glTF material
//! slot and winding. The material is the one the stock path draws that primitive
//! with, loaded through the same glTF label, so the engine's material handler has treated it
//! identically.

use super::{
    assets::{ClusterBlockAsset, ClusterMeshAsset},
    instance_identity::ClusterInstanceIdentity,
    mesh_payload::{MeshExpectation, checked_mesh},
    reader::{CertifiedPackage, RecordKey},
    render::{ClusterBatchHandles, ClusterInstance, ClusterScene, SceneBlock, world_from_local},
};
use crate::{
    config::EngineConfig,
    render::WaterReflectionCamera,
    streaming::{ActiveSpace, RenderOrigin},
    world::components::StreamingCamera,
};
use bevy::{
    asset::AssetId,
    ecs::schedule::common_conditions::resource_exists,
    gltf::GltfAssetLabel,
    prelude::*,
    tasks::{IoTaskPool, Task, futures_lite::future},
};
use rusqlite::Connection;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(test)]
mod derived_overlay_tests;

/// Registers the converted cluster package draw path.
pub struct ClusterPackagePlugin {
    /// The converted dataset the stock path loads from (`--assets`). Its models are
    /// what the clusters replace.
    pub base: PathBuf,
    /// The cluster overlay folder resolved for that dataset (`--clusters`): `clusters.db` with the
    /// cluster tables, `clusters/` payloads and `overlay.json`.
    pub overlay: PathBuf,
    /// Register terrain materials and load optional terrain metadata.
    pub terrain: bool,
    /// Where the camera starts, in Creation coordinates (the space of a block's bounds, z up): the
    /// blocks reach the GPU nearest first around it. `None` keeps the package's own order.
    pub priority_point: Option<Vec3>,
}

#[derive(Resource)]
struct PackageSource {
    base: PathBuf,
    overlay: PathBuf,
    terrain: bool,
    priority_point: Option<Vec3>,
}

/// Retains authoritative validation and its dataset lock. This resource does
/// not establish render readiness, which is decided separately for each view.
#[derive(Resource)]
pub(crate) struct ValidatedPackage(pub Arc<CertifiedPackage>);

/// Package certification status only; `Ready` never means GPU draw-ready.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PackageValidationStatus {
    Pending,
    Ready,
    Failed(String),
}

#[derive(Resource)]
struct PendingPackageValidation(Task<Result<CertifiedPackage, String>>);

// Speculative assets remain private until certification passes. Only explicit all-batch
// preload may request them: ordinary lazy loading keeps its view-based priority and residency.
#[derive(Resource)]
struct PendingModelPrefetch {
    incoming: crossbeam_channel::Receiver<ModelPrefetch>,
    remaining: std::collections::VecDeque<String>,
    scenes: HashMap<String, Handle<WorldAsset>>,
    in_flight: Vec<Handle<WorldAsset>>,
}

impl PendingModelPrefetch {
    fn new(incoming: crossbeam_channel::Receiver<ModelPrefetch>) -> Self {
        Self {
            incoming,
            remaining: default(),
            scenes: default(),
            in_flight: default(),
        }
    }

    fn receive(&mut self) {
        if let Ok(prefetch) = self.incoming.try_recv() {
            self.remaining.extend(prefetch.remaining);
            self.scenes.extend(prefetch.scenes);
            self.in_flight.extend(prefetch.in_flight);
        }
    }
}

struct ModelPrefetch {
    remaining: std::collections::VecDeque<String>,
    scenes: HashMap<String, Handle<WorldAsset>>,
    in_flight: Vec<Handle<WorldAsset>>,
}

fn start_model_prefetch(
    mut paths: Vec<String>,
    preload_all: bool,
    assets: &AssetServer,
) -> ModelPrefetch {
    if !preload_all {
        paths.clear();
    }
    let mut prefetch = ModelPrefetch {
        remaining: paths.into(),
        scenes: default(),
        in_flight: default(),
    };
    // Dispatch on the certification worker: Startup can block the first Update until after
    // certification finishes. AssetServer starts background reads/decoding without that Update.
    while prefetch.in_flight.len() < MAX_LOADING {
        let Some(path) = prefetch.remaining.pop_front() else {
            break;
        };
        let handle = assets.load(GltfAssetLabel::Scene(0).from_asset(path.clone()));
        prefetch.in_flight.push(handle.clone());
        prefetch.scenes.insert(path, handle);
    }
    info!(
        models = prefetch.scenes.len(),
        queued = prefetch.remaining.len(),
        "cluster batch model prefetch started; certification remains pending"
    );
    prefetch
}

#[derive(Resource, Default)]
struct PrefetchedBatchModels(HashMap<String, Handle<WorldAsset>>, Vec<Handle<WorldAsset>>);

fn prefetch_batch_paths(
    connection: &Connection,
    published: &[(String, String)],
) -> rusqlite::Result<Vec<String>> {
    let published: std::collections::HashSet<_> =
        published.iter().map(|(path, _)| path.as_str()).collect();
    let mut statement =
        connection.prepare("SELECT DISTINCT model_path FROM cluster_meshes ORDER BY model_path")?;
    let paths = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(paths
        .into_iter()
        .filter(|path| {
            // Published paths already passed reader::published_files' relative-path checks. A
            // malformed material claim cannot turn speculative decoding into an arbitrary read.
            let lower = path.to_ascii_lowercase();
            published.contains(path.as_str())
                && lower.starts_with("meshes/")
                && lower.ends_with(".glb")
        })
        .collect())
}

fn prefetch_pending_models(
    pending: Option<ResMut<PendingModelPrefetch>>,
    assets: Res<AssetServer>,
) {
    let Some(mut pending) = pending else {
        return;
    };
    pending.receive();
    pending
        .scenes
        .retain(|_, handle| !asset_failed(&assets, handle.id()));
    pending.in_flight.retain(|handle| {
        !assets.is_loaded_with_dependencies(handle.id()) && !asset_failed(&assets, handle.id())
    });
    while pending.in_flight.len() < MAX_LOADING {
        let Some(path) = pending.remaining.pop_front() else {
            break;
        };
        let handle = assets.load(GltfAssetLabel::Scene(0).from_asset(path.clone()));
        pending.in_flight.push(handle.clone());
        pending.scenes.insert(path, handle);
    }
}

/// The overlay-to-base binding: the overlay must have been built for exactly this conversion of
/// the dataset the stock path loads from. Refuses, naming the rebuild command, an overlay of
/// another base or another conversion of it, and an old full-copy dataset.
#[cfg(test)]
fn checked_package_database(base: &Path, overlay: &Path) -> Result<(), String> {
    shared::cluster_overlay::checked_overlay(base, overlay)
        .map(|_| ())
        .map_err(|error| format!("{error:#}"))
}

fn begin_package_validation(
    mut commands: Commands,
    source: Res<PackageSource>,
    preload: Option<Res<ClusterBatchPreload>>,
    assets: Res<AssetServer>,
) {
    let base = source.base.clone();
    let overlay = source.overlay.clone();
    let terrain = source.terrain;
    let preload_all = preload.is_some();
    let assets = assets.clone();
    let (send, receive) = crossbeam_channel::bounded(1);
    commands.insert_resource(PendingModelPrefetch::new(receive));
    commands.insert_resource(PendingPackageValidation(IoTaskPool::get().spawn(
        async move {
            CertifiedPackage::open_with_prefetch(&base, &overlay, terrain, |connection, published| {
                if !preload_all {
                    return;
                }
                match prefetch_batch_paths(connection, published) {
                    Ok(paths) => {
                        let _ = send.send(start_model_prefetch(paths, preload_all, &assets));
                    }
                    // This is only speculative work. The original validation and loader queries
                    // still run and retain their existing failure policy and refusal verdict.
                    Err(error) => {
                        warn!(%error, "cluster batch prefetch unavailable; certification continues")
                    }
                }
            })
            .map_err(|error| format!("{error:#}"))
        },
    )));
}

fn poll_package_validation(
    mut commands: Commands,
    pending: Option<ResMut<PendingPackageValidation>>,
    prefetched: Option<ResMut<PendingModelPrefetch>>,
    assets: Option<Res<AssetServer>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut pending) = pending else {
        return;
    };
    let Some(result) = future::block_on(future::poll_once(&mut pending.0)) else {
        return;
    };
    commands.remove_resource::<PendingPackageValidation>();
    commands.remove_resource::<PendingModelPrefetch>();
    match result {
        Ok(package) => {
            if let Some(mut prefetched) = prefetched {
                // The worker can send after this frame's prefetch system ran, then finish
                // before this poll. Task completion orders the send before this final drain.
                prefetched.receive();
                // A late worker send can contain a failure that the pending system has not
                // seen. Failed speculation must leave the normal loader free to retry.
                if let Some(assets) = &assets {
                    prefetched
                        .scenes
                        .retain(|_, handle| !asset_failed(assets, handle.id()));
                    prefetched
                        .in_flight
                        .retain(|handle| !asset_failed(assets, handle.id()));
                }
                commands.insert_resource(PrefetchedBatchModels(
                    std::mem::take(&mut prefetched.scenes),
                    std::mem::take(&mut prefetched.in_flight),
                ));
            }
            commands.insert_resource(ValidatedPackage(Arc::new(package)));
            commands.insert_resource(PackageValidationStatus::Ready);
        }
        Err(reason) => {
            // An explicitly requested package that fails authoritative validation
            // is not a successful stock-only cluster run. Match the dataset policy
            // for a declared extension that its reader cannot safely consume.
            error!(%reason, "requested cluster package failed authoritative validation");
            commands.insert_resource(PackageValidationStatus::Failed(reason));
            exit.write(AppExit::error());
        }
    }
}

impl Plugin for ClusterPackagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(super::source_identity::SourceIdentityPlugin);
        if self.terrain {
            app.add_plugins(super::terrain::ClusterTerrainPlugin);
        }
        app.add_plugins(super::render::ClusterRenderPlugin)
            .insert_resource(PackageSource {
                base: self.base.clone(),
                overlay: self.overlay.clone(),
                terrain: self.terrain,
                priority_point: self.priority_point,
            })
            .insert_resource(PackageValidationStatus::Pending)
            .add_systems(Startup, begin_package_validation)
            .add_systems(
                Update,
                (
                    prefetch_pending_models,
                    poll_package_validation.after(prefetch_pending_models),
                    load_cluster_package.after(poll_package_validation),
                    load_batches_in_view
                        .after(load_cluster_package)
                        .run_if(resource_exists::<LazyBatches>),
                    // After the loader, which sets the pending count this adds to.
                    count_blocks_not_resident.after(load_batches_in_view),
                ),
            );
    }
}

/// One `cluster_meshes` row.
struct MeshRow {
    expected: MeshExpectation,
    payload_path: String,
    model_path: String,
    material_index: Option<u32>,
}

/// What distinguishes the materials batches draw with.
#[derive(Clone, PartialEq, Eq, Hash)]
struct BatchKey {
    model_path: String,
    material_index: Option<u32>,
    /// Mirrored instances use the glTF loader's scale-inverted variant of the material.
    scale_inverted: bool,
}

fn hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_column(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<[u8; 32]> {
    let value: String = row.get(column)?;
    shared::cluster::parse_hash(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "cluster mesh row has an invalid SHA-256",
            )),
        )
    })
}

fn read_rows(connection: &Connection) -> Result<HashMap<(String, u32), MeshRow>, String> {
    let mut statement = connection
        .prepare(
            "SELECT mesh_hash, primitive_index, payload_path, model_path, material_index,
                    content_hash, payload_bytes, vertex_count, cluster_count FROM cluster_meshes",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, u32>(1)?),
                MeshRow {
                    expected: MeshExpectation {
                        mesh_hash: hash_column(row, 0)?,
                        primitive_index: row.get(1)?,
                        content_hash: hash_column(row, 5)?,
                        payload_bytes: row.get(6)?,
                        vertex_count: row.get(7)?,
                        cluster_count: row.get(8)?,
                    },
                    payload_path: row.get(2)?,
                    model_path: row.get(3)?,
                    material_index: row.get(4)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Domain comes from the owning SQL table, never a missing reference mapping.
#[derive(Clone, Copy)]
enum ChunkDomain {
    Object,
    Terrain(u32),
}

/// Material identity is a property of this record's explicit domain, not its
/// shared geometry hash. Object source rows remain authoritative and immutable.
fn record_batch_key(
    row: &MeshRow,
    domain: ChunkDomain,
    blocks: &[super::terrain::TerrainBlock],
    scale_inverted: bool,
) -> Result<BatchKey, String> {
    let (model_path, material_index) = match domain {
        ChunkDomain::Object => (row.model_path.clone(), row.material_index),
        ChunkDomain::Terrain(index) => {
            let block = blocks
                .get(index as usize)
                .ok_or("terrain block index out of range")?;
            if block.mesh_hash != hex(&row.expected.mesh_hash)
                || block.primitive_index != row.expected.primitive_index
            {
                return Err("terrain record geometry differs from its explicit block".into());
            }
            // Package-local block identity keeps equal MCL bytes with distinct
            // atlases/worldspaces separate, even if their meshes remain pooled.
            (format!("cluster-terrain:{index}"), None)
        }
    };
    Ok(BatchKey {
        model_path,
        material_index,
        scale_inverted,
    })
}

fn bind_terrain_batch(
    blocks: &mut [super::terrain::TerrainBlock],
    index: u32,
    batch: u32,
    handles: &mut ClusterBatchHandles,
) -> Result<(), String> {
    let block = blocks
        .get_mut(index as usize)
        .ok_or("terrain block index out of range")?;
    if block.batch.is_some_and(|previous| previous != batch) {
        return Err("terrain block resolved to inconsistent material batches".into());
    }
    handles.set(batch, block.material.clone());
    block.batch = Some(batch);
    Ok(())
}

struct ChunkSource {
    path: String,
    domain: ChunkDomain,
}

fn read_chunk_paths(connection: &Connection) -> Result<Vec<ChunkSource>, String> {
    let mut statement = connection
        .prepare(
            "SELECT payload_path FROM cluster_chunks ORDER BY worldspace_id, anchor_y, anchor_x",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| {
            Ok(ChunkSource {
                path: row.get(0)?,
                domain: ChunkDomain::Object,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// The asset path of the `StandardMaterial` the stock path draws a glTF material slot with: the
/// stock PBR handler publishes it as `<material label>/std`, and the engine's native handler
/// publishes `<material label>/nif` for the materials that need Skyrim's own conventions.
fn standard_material_path(model: &str, slot: u32, scale_inverted: bool, native: bool) -> String {
    let label = GltfAssetLabel::Material {
        index: slot as usize,
        is_scale_inverted: scale_inverted,
    };
    format!("{model}#{label}{}", if native { "/nif" } else { "/std" })
}

/// For each material slot of a model, whether the engine builds its `StandardMaterial` natively
/// (`crate::nif_material`). Only the glTF's JSON chunk is read: the binary chunk of a model can be
/// megabytes and is never needed here. A model that cannot be read has no native materials.
fn native_materials(path: &Path) -> Vec<bool> {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    // 12-byte GLB header (magic, version, length), then the first chunk's length and type.
    let mut header = [0u8; 20];
    if file.read_exact(&mut header).is_err() || &header[0..4] != b"glTF" {
        return Vec::new();
    }
    let json_length = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    // A GLB of the header and the JSON chunk alone is a valid document with no binary data.
    let mut document = Vec::with_capacity(20 + json_length);
    document.extend_from_slice(&header);
    document.resize(20 + json_length, 0);
    if file.read_exact(&mut document[20..]).is_err() {
        return Vec::new();
    }
    let total = (document.len() as u32).to_le_bytes();
    document[8..12].copy_from_slice(&total);
    let Ok(gltf) = bevy::gltf::gltf::Gltf::from_slice_without_validation(&document) else {
        return Vec::new();
    };
    gltf.materials()
        .map(|material| crate::nif_material::needs_native_material(&material))
        .collect()
}

/// A payload reused by any object retains the ordinary tangent-generation path.
pub(crate) fn terrain_only_meshes(instances: &[ClusterInstance]) -> std::collections::HashSet<u32> {
    let mut terrain = std::collections::HashSet::new();
    let mut objects = std::collections::HashSet::new();
    for instance in instances {
        match instance.identity {
            ClusterInstanceIdentity::Terrain { .. } => {
                terrain.insert(instance.mesh);
            }
            ClusterInstanceIdentity::Object(_) | ClusterInstanceIdentity::Fixture => {
                objects.insert(instance.mesh);
            }
        }
    }
    terrain.retain(|mesh| !objects.contains(mesh));
    terrain
}

type PackageLoadGate<'w, 's> = (
    Res<'w, PackageSource>,
    Option<Res<'w, ValidatedPackage>>,
    Local<'s, bool>,
    Option<ResMut<'w, PrefetchedBatchModels>>,
);

fn stop_unusable_overlay(
    exit: &mut MessageWriter<AppExit>,
    package: &CertifiedPackage,
    reason: impl std::fmt::Display,
) {
    let envelope = package.envelope();
    error!(
        %reason,
        rebuild = %envelope.metadata.rebuild_hint(&envelope.root),
        "requested cluster overlay payload is unusable; stopping before publishing the scene"
    );
    exit.write(AppExit::error());
}

fn load_cluster_package(
    mut commands: Commands,
    gate: PackageLoadGate<'_, '_>,
    mut handles: ResMut<ClusterBatchHandles>,
    mut images: ResMut<Assets<Image>>,
    mut terrain_materials: Option<ResMut<Assets<super::terrain::ClusterTerrainMaterial>>>,
    mut exit: MessageWriter<AppExit>,
) {
    let (source, validated, mut loaded, mut prefetched) = gate;
    if *loaded {
        return;
    }
    let Some(validated) = validated else {
        return;
    };
    *loaded = true;
    let (prefetched_scenes, prefetch_in_flight) = prefetched
        .as_deref_mut()
        .map(|models| (std::mem::take(&mut models.0), std::mem::take(&mut models.1)))
        .unwrap_or_default();
    commands.remove_resource::<PrefetchedBatchModels>();
    // Core files (models, textures, the cell cache) live in the base; payloads, manifests and the
    // cluster tables live in the overlay.
    let base_root = &validated.0.envelope().root;
    let package_root = &validated.0.envelope().overlay;
    let started = std::time::Instant::now();
    let connection = match validated.0.envelope().connection() {
        Ok(connection) => connection,
        // The clusters were asked for and certified: an overlay that no longer opens bound to its
        // base (changed after certification) stops the run, it never draws without them.
        Err(error) => {
            error!("requested cluster overlay unreadable: {error:#}");
            exit.write(AppExit::error());
            return;
        }
    };
    let rows = match read_rows(&connection) {
        Ok(rows) => rows,
        Err(error) => {
            error!("requested cluster overlay unreadable: {error}");
            exit.write(AppExit::error());
            return;
        }
    };
    let mut chunk_paths = match read_chunk_paths(&connection) {
        Ok(paths) => paths,
        Err(error) => {
            error!("requested cluster overlay chunks unreadable: {error}");
            exit.write(AppExit::error());
            return;
        }
    };
    info!(
        phase = "mesh_rows_and_chunk_paths",
        wall_ms = started.elapsed().as_secs_f64() * 1000.0,
        rows = rows.len(),
        chunks = chunk_paths.len(),
        "cluster main-thread loader phase"
    );
    let phase_started = std::time::Instant::now();
    let mut terrain = if source.terrain {
        let Some(materials) = terrain_materials.as_deref_mut() else {
            error!("cluster terrain material plugin is missing");
            exit.write(AppExit::error());
            return;
        };
        match super::terrain::ClusterTerrain::load_certified(
            validated.0.envelope(),
            &mut images,
            materials,
        ) {
            Ok(terrain) => {
                if let Err(error) = terrain.validate_dataset(base_root) {
                    error!("cluster terrain source dataset mismatch: {error}");
                    exit.write(AppExit::error());
                    return;
                }
                Some(terrain)
            }
            Err(error) => {
                error!("cluster terrain package refused: {error}");
                exit.write(AppExit::error());
                return;
            }
        }
    } else {
        None
    };
    if let Some(terrain) = &mut terrain {
        if let Some(materials) = terrain_materials.as_deref_mut() {
            terrain.set_near_coverage(
                None,
                &std::collections::HashSet::new(),
                IVec2::ZERO,
                &super::terrain::TerrainDrawReadiness::default(),
                materials,
            );
        }
        chunk_paths.extend(
            terrain
                .blocks
                .iter()
                .enumerate()
                .map(|(index, block)| ChunkSource {
                    path: block.instance_path.clone(),
                    domain: ChunkDomain::Terrain(index as u32),
                }),
        );
    }
    info!(
        phase = "terrain_load_and_validate",
        wall_ms = phase_started.elapsed().as_secs_f64() * 1000.0,
        "cluster main-thread loader phase"
    );
    let phase_started = std::time::Instant::now();
    let mut payload_bytes = 0u64;
    let mut block_read_parse_us = 0u128;
    let mut mesh_read_check_us = 0u128;
    let identity = validated.0.envelope().builder_identity.clone();

    let mut mesh_index: HashMap<(String, u32), u32> = HashMap::new();
    let mut scene_meshes: Vec<ClusterMeshAsset> = Vec::new();
    let mut batch_index: HashMap<BatchKey, u32> = HashMap::new();
    let mut batch_keys: Vec<BatchKey> = Vec::new();
    // Where each instance is, for loading its batch's material when it comes into view.
    let mut instance_spheres: Vec<(Vec3, f32)> = Vec::new();
    let mut instances: Vec<ClusterInstance> = Vec::new();
    // Which block each instance came from, and every block's box.
    let mut instance_block: Vec<u32> = Vec::new();
    let mut block_bounds: Vec<([f32; 3], [f32; 3])> = Vec::new();
    for chunk_source in &chunk_paths {
        let chunk = &chunk_source.path;
        let block_started = std::time::Instant::now();
        let bytes = match std::fs::read(package_root.join(chunk)) {
            Ok(bytes) => bytes,
            Err(error) => {
                stop_unusable_overlay(
                    &mut exit,
                    &validated.0,
                    format_args!("cluster block {chunk} unreadable: {error}"),
                );
                return;
            }
        };
        payload_bytes += bytes.len() as u64;
        let block = match ClusterBlockAsset::from_bytes(&bytes) {
            Ok(block) => block,
            Err(error) => {
                stop_unusable_overlay(
                    &mut exit,
                    &validated.0,
                    format_args!("cluster block {chunk} invalid: {error}"),
                );
                return;
            }
        };
        block_read_parse_us += block_started.elapsed().as_micros();
        if hex(&block.builder_identity) != identity {
            stop_unusable_overlay(
                &mut exit,
                &validated.0,
                format_args!(
                    "cluster block {chunk} has another builder identity; expected {identity}"
                ),
            );
            return;
        }
        let block_number = block_bounds.len() as u32;
        block_bounds.push((block.bounds_min, block.bounds_max));
        // The block's mesh table, resolved once per block.
        let mut block_meshes: Vec<(u32, &MeshRow)> = Vec::new();
        for reference in &block.meshes {
            let key = (hex(&reference.mesh_hash), reference.primitive_index);
            let Some(row) = rows.get(&key) else {
                stop_unusable_overlay(
                    &mut exit,
                    &validated.0,
                    format_args!(
                        "cluster block {chunk} references missing mesh {} primitive {}",
                        key.0, key.1
                    ),
                );
                return;
            };
            let index = match mesh_index.get(&key) {
                Some(index) => *index,
                None => {
                    let mesh_started = std::time::Instant::now();
                    let loaded = std::fs::read(package_root.join(&row.payload_path))
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| {
                            payload_bytes += bytes.len() as u64;
                            let builder = shared::cluster::parse_hash(&identity)
                                .ok_or("invalid package builder identity")?;
                            checked_mesh(bytes, builder, &row.expected)
                        });
                    mesh_read_check_us += mesh_started.elapsed().as_micros();
                    match loaded {
                        Ok(mesh) => {
                            let index = scene_meshes.len() as u32;
                            scene_meshes.push(mesh);
                            mesh_index.insert(key.clone(), index);
                            index
                        }
                        Err(error) => {
                            stop_unusable_overlay(
                                &mut exit,
                                &validated.0,
                                format_args!("cluster mesh {} unusable: {error}", row.payload_path),
                            );
                            return;
                        }
                    }
                }
            };
            block_meshes.push((index, row));
        }
        // Preserve the final writer order as the stable record identity.
        // Neither scene reorder nor later batch sorting may renumber this key.
        for (original_ordinal, record) in block.instances.iter().enumerate() {
            let instance_identity = match chunk_source.domain {
                ChunkDomain::Object => ClusterInstanceIdentity::Object(RecordKey {
                    worldspace: block.worldspace_id,
                    tier: block.tier,
                    anchor: [block.anchor_x, block.anchor_y],
                    // read_mci represents the record count as u32.
                    ordinal: original_ordinal as u32,
                }),
                ChunkDomain::Terrain(index) => ClusterInstanceIdentity::Terrain {
                    block: index,
                    worldspace: block.worldspace_id,
                    tier: block.tier,
                    anchor: [block.anchor_x, block.anchor_y],
                },
            };
            let Some((mesh, row)) = block_meshes.get(record.mesh as usize) else {
                stop_unusable_overlay(
                    &mut exit,
                    &validated.0,
                    format_args!(
                        "cluster block {chunk} record {original_ordinal} references missing mesh index {}",
                        record.mesh
                    ),
                );
                return;
            };
            let matrix = world_from_local(record);
            let terrain_blocks = terrain
                .as_ref()
                .map_or(&[][..], |terrain| terrain.blocks.as_slice());
            let key = match record_batch_key(
                row,
                chunk_source.domain,
                terrain_blocks,
                matrix.determinant() < 0.0,
            ) {
                Ok(key) => key,
                Err(reason) => {
                    error!(%reason, "cluster terrain material identity refused");
                    exit.write(AppExit::error());
                    return;
                }
            };
            let batch = *batch_index.entry(key.clone()).or_insert_with(|| {
                batch_keys.push(key);
                (batch_keys.len() - 1) as u32
            });
            if let ChunkDomain::Terrain(index) = chunk_source.domain {
                let Some(terrain) = terrain.as_mut() else {
                    error!("terrain instance has no terrain metadata");
                    exit.write(AppExit::error());
                    return;
                };
                if let Err(reason) =
                    bind_terrain_batch(&mut terrain.blocks, index, batch, &mut handles)
                {
                    error!(%reason, "cluster terrain material binding refused");
                    exit.write(AppExit::error());
                    return;
                }
            }
            let bounds = scene_meshes[*mesh as usize].bounds;
            let scale = matrix
                .x_axis
                .truncate()
                .length()
                .max(matrix.y_axis.truncate().length())
                .max(matrix.z_axis.truncate().length());
            instance_spheres.push((
                matrix.transform_point3(Vec3::new(bounds[0], bounds[1], bounds[2])),
                bounds[3] * scale,
            ));
            instances.push(ClusterInstance {
                identity: instance_identity,
                mesh: *mesh,
                batch,
                world_from_local: matrix,
                source_worldspace: record.source_worldspace_id,
            });
            instance_block.push(block_number);
        }
    }
    info!(
        phase = "payloads_and_instances",
        wall_ms = phase_started.elapsed().as_secs_f64() * 1000.0,
        bytes = payload_bytes,
        block_read_parse_us = block_read_parse_us as u64,
        mesh_read_check_us = mesh_read_check_us as u64,
        meshes = scene_meshes.len(),
        instances = instances.len(),
        "cluster main-thread loader phase"
    );
    let phase_started = std::time::Instant::now();
    let mut batch_spheres: Vec<Vec<LazySphere>> = vec![Vec::new(); batch_keys.len()];
    for (instance, sphere) in instances.iter().zip(&instance_spheres) {
        batch_spheres[instance.batch as usize].push(LazySphere {
            center: sphere.0,
            radius: sphere.1,
            world: lazy_instance_world(&instance.identity, Some(validated.0.as_ref())),
        });
    }
    if instances.is_empty() {
        // A certified package may have no eligible instances. Still publish its
        // explicit empty scene so initial automation can distinguish it from loading.
        warn!("cluster package holds no drawable instances, drawing none");
    }

    let assets_dir = base_root.clone();
    // A batch entity is cheap and exists from the start; its material is requested only once an
    // instance of the batch comes into view (`load_batches_in_view`), so loading follows what the
    // camera sees and not the size of the package.
    commands.insert_resource(LazyBatches {
        batches: batch_keys
            .iter()
            .zip(batch_spheres)
            .map(|(key, spheres)| LazyBatch {
                key: key.clone(),
                bounds: enclosing_sphere(
                    &spheres
                        .iter()
                        .map(|sphere| (sphere.center, sphere.radius))
                        .collect::<Vec<_>>(),
                ),
                spheres,
                scene: None,
                source_reload_from: None,
                loading: None,
                requested: key.model_path.starts_with("cluster-terrain:"),
                requested_at: 0.0,
                reported: false,
            })
            .collect(),
        assets_dir,
        native_by_model: HashMap::new(),
        scenes: prefetched_scenes,
        retried_sources: default(),
        prefetch_in_flight,
        unrequested: (0..batch_keys.len())
            .filter(|&index| !batch_keys[index].model_path.starts_with("cluster-terrain:"))
            .collect(),
        in_flight: Vec::new(),
        failed: Vec::new(),
    });
    info!(
        meshes = scene_meshes.len(),
        instances = instances.len(),
        batches = batch_keys.len(),
        blocks = chunk_paths.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "cluster package loaded"
    );
    let terrain_view_blocks = terrain.as_ref().map_or_else(Vec::new, |terrain| {
        terrain
            .blocks
            .iter()
            .map(super::terrain::TerrainViewBlock::from)
            .collect()
    });
    if let Some(terrain) = terrain {
        if terrain.blocks.iter().any(|block| block.batch.is_none()) {
            error!("cluster terrain has a block with no valid drawable payload");
            exit.write(AppExit::error());
            return;
        }
        commands.insert_resource(TerrainBatchIds(
            terrain
                .blocks
                .iter()
                .filter_map(|block| block.batch)
                .collect(),
        ));
        commands.insert_resource(terrain);
    }
    // The scene reaches the GPU a block at a time, nearest the camera's start first. Each block is a
    // run of the scene's instances, so the instances are laid out block by block in that order.
    let mut per_block: Vec<Vec<usize>> = vec![Vec::new(); block_bounds.len()];
    for (index, block) in instance_block.iter().enumerate() {
        per_block[*block as usize].push(index);
    }
    let mut order: Vec<usize> = (0..block_bounds.len())
        .filter(|&block| !per_block[block].is_empty())
        .collect();
    if let Some(point) = source.priority_point {
        let distance = |block: usize| {
            let (low, high) = block_bounds[block];
            super::residency::planner::distance_to_box(point.to_array(), low, high)
        };
        order.sort_by(|a, b| distance(*a).total_cmp(&distance(*b)));
    }
    let mut ordered: Vec<ClusterInstance> = Vec::with_capacity(instances.len());
    let mut scene_blocks: Vec<SceneBlock> = Vec::with_capacity(order.len());
    for block in order {
        let first = ordered.len();
        let mut mesh_ids: Vec<u32> = Vec::new();
        for &index in &per_block[block] {
            ordered.push(instances[index]);
            mesh_ids.push(instances[index].mesh);
        }
        mesh_ids.sort_unstable();
        mesh_ids.dedup();
        scene_blocks.push(SceneBlock {
            first_instance: first as u32,
            instance_count: (ordered.len() - first) as u32,
            meshes: mesh_ids,
            ready: Arc::default(),
            resident: Arc::default(),
        });
    }
    let instances = ordered;
    if let Some(point) = source.priority_point {
        // The first blocks, and how far from the start they are: the order is the point.
        let nearest: Vec<String> = scene_blocks
            .iter()
            .take(3)
            .map(|block| {
                let instance = &instances[block.first_instance as usize];
                format!(
                    "{} instances, first at {:?}",
                    block.instance_count,
                    instance.world_from_local.w_axis.truncate()
                )
            })
            .collect();
        info!(start = ?point, ?nearest, "cluster blocks are ordered nearest the start first");
    }
    info!(
        phase = "batching_and_scene_order",
        wall_ms = phase_started.elapsed().as_secs_f64() * 1000.0,
        blocks = scene_blocks.len(),
        "cluster main-thread loader phase"
    );
    // Terrain's colour-only atlas never reads tangents. Fill its unused GPU slots without
    // generating geometry derivatives; shared object payloads keep ordinary tangent generation.
    let terrain_only = Arc::new(terrain_only_meshes(&instances));
    info!(
        meshes = terrain_only.len(),
        "cluster terrain tangent generation skipped (unused slots)"
    );
    // Tangents are generated for object meshes whose payload has none (mikktspace, as the stock path
    // runs it): the dominant cost of building a mesh's GPU data, 43 s on the render thread for the
    // Tamriel package. Plain threads take the blocks in order from one counter and prepare the
    // block's meshes, then mark it ready; the render world appends it from there. (Bevy's compute
    // pool ran this about 1.15 times parallel on 8 threads, measured 2026-10-16.) Half the cores:
    // the rest keep the frame going while the world fills in.
    let threads = (std::thread::available_parallelism().map_or(2, usize::from) / 2).max(1);
    let terrain_blocks: Vec<_> = scene_blocks
        .iter()
        .map(|block| {
            matches!(
                instances[block.first_instance as usize].identity,
                ClusterInstanceIdentity::Terrain { .. }
            )
        })
        .collect();
    let domain_busy = Arc::new([
        std::sync::atomic::AtomicU64::new(0),
        std::sync::atomic::AtomicU64::new(0),
    ]);
    let prepared = Arc::new((
        scene_meshes.clone(),
        scene_blocks.clone(),
        std::sync::atomic::AtomicUsize::new(0),
        std::sync::atomic::AtomicU64::new(0),
        std::time::Instant::now(),
        std::sync::atomic::AtomicUsize::new(0),
    ));
    for _ in 0..threads {
        let prepared = prepared.clone();
        let terrain_only = terrain_only.clone();
        let domain_busy = domain_busy.clone();
        let terrain_blocks = terrain_blocks.clone();
        std::thread::spawn(move || {
            let (meshes, blocks, next, busy_nanos, started, done) = &*prepared;
            loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(block) = blocks.get(index) else {
                    break;
                };
                let began = std::time::Instant::now();
                for &mesh in &block.meshes {
                    if terrain_only.contains(&mesh) {
                        meshes[mesh as usize].prepare_unused_tangent_slots();
                    } else {
                        meshes[mesh as usize].prepare_tangents();
                    }
                }
                let elapsed_nanos = began.elapsed().as_nanos() as u64;
                busy_nanos.fetch_add(elapsed_nanos, std::sync::atomic::Ordering::Relaxed);
                domain_busy[usize::from(terrain_blocks[index])]
                    .fetch_add(elapsed_nanos, std::sync::atomic::Ordering::Relaxed);
                block
                    .ready
                    .store(true, std::sync::atomic::Ordering::Release);
                if done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1 == blocks.len() {
                    info!(
                        blocks = blocks.len(),
                        threads,
                        object_busy_ms = domain_busy[0].load(std::sync::atomic::Ordering::Relaxed)
                            as f64
                            / 1.0e6,
                        terrain_busy_ms = domain_busy[1].load(std::sync::atomic::Ordering::Relaxed)
                            as f64
                            / 1.0e6,
                        busy_ms =
                            busy_nanos.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1.0e6,
                        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                        "cluster block tangents prepared (every block)"
                    );
                }
            }
        });
    }
    commands.insert_resource(ClusterScene {
        certificate: Some(validated.0.clone()),
        terrain_blocks: terrain_view_blocks,
        meshes: scene_meshes,
        instances,
        origin_offset: Vec3::ZERO,
        blocks: scene_blocks,
    });
    info!(
        wall_ms = started.elapsed().as_secs_f64() * 1000.0,
        "cluster main-thread loader total"
    );
}

/// Holds the world back from being ready while blocks of the scene have not reached the GPU: a
/// capture or a benchmark waits for all of them, so its pictures do not depend on the timing of the
/// stream-in.
fn count_blocks_not_resident(
    scene: Option<Res<ClusterScene>>,
    validation: Option<Res<PackageValidationStatus>>,
    mut metrics: Option<ResMut<crate::streaming::StreamingMetrics>>,
) {
    let Some(metrics) = metrics.as_deref_mut() else {
        return;
    };
    if matches!(
        validation.as_deref(),
        Some(PackageValidationStatus::Pending)
    ) {
        // The loader does not exist yet to reset its counts. Assign rather than
        // accumulate, so every pending frame holds settle with the same marker.
        metrics.pending_cluster_batches = 1;
        return;
    }
    let Some(scene) = scene else {
        return;
    };
    // A scene without blocks is one block that is appended on the first frame.
    let waiting = scene
        .blocks
        .iter()
        .filter(|block| !block.resident.load(std::sync::atomic::Ordering::Acquire))
        .count();
    metrics.pending_cluster_batches += waiting;
}

/// A sphere that holds every sphere of `spheres` (the middle of their box, reaching the farthest).
fn enclosing_sphere(spheres: &[(Vec3, f32)]) -> (Vec3, f32) {
    let (low, high) = spheres.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), (center, _)| (low.min(*center), high.max(*center)),
    );
    if spheres.is_empty() {
        return (Vec3::ZERO, 0.0);
    }
    let middle = (low + high) * 0.5;
    let radius = spheres
        .iter()
        .map(|(center, radius)| center.distance(middle) + radius)
        .fold(0.0, f32::max);
    (middle, radius)
}

/// Fixture is a deliberate invented-scene domain, never a fallback for missing authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LazyWorld {
    Placed(u32),
    Fixture,
    Unknown,
}

#[derive(Clone, Copy)]
struct LazySphere {
    center: Vec3,
    radius: f32,
    world: LazyWorld,
}

fn lazy_instance_world(
    identity: &ClusterInstanceIdentity,
    certificate: Option<&CertifiedPackage>,
) -> LazyWorld {
    if matches!(identity, ClusterInstanceIdentity::Fixture) && certificate.is_none() {
        LazyWorld::Fixture
    } else {
        identity
            .placed_worldspace(certificate)
            .map_or(LazyWorld::Unknown, LazyWorld::Placed)
    }
}

/// The streaming camera and its water reflection share the active exterior world.
fn lazy_camera_world(streaming: bool, reflection: bool, active_world: Option<u32>) -> Option<u32> {
    if streaming || reflection {
        active_world
    } else {
        None
    }
}

fn lazy_world_matches(instance: LazyWorld, camera: Option<u32>) -> bool {
    match instance {
        LazyWorld::Placed(world) => camera == Some(world),
        LazyWorld::Fixture => true,
        LazyWorld::Unknown => false,
    }
}

struct LazyView {
    frustum: bevy::camera::primitives::Frustum,
    transform: GlobalTransform,
    world: Option<u32>,
}

impl LazyView {
    fn snapshot(&self) -> (GlobalTransform, [Vec4; 6], Option<u32>) {
        (
            self.transform,
            self.frustum.half_spaces.map(|plane| plane.normal_d()),
            self.world,
        )
    }
}

/// A cross-world sphere is never a candidate, even at identical coordinates.
/// The enclosing sphere is only a conservative fast rejection; each matching instance
/// still receives the existing margin and exact frustum/distance test.
fn lazy_view_distance(
    bounds: (Vec3, f32),
    spheres: &[LazySphere],
    view: &LazyView,
    offset: Vec3,
    margin: f32,
) -> f32 {
    if !view.frustum.intersects_sphere(
        &bevy::camera::primitives::Sphere {
            center: Vec3A::from(bounds.0 + offset),
            radius: bounds.1 + margin,
        },
        false,
    ) {
        return f32::INFINITY;
    }
    spheres
        .iter()
        .filter(|sphere| lazy_world_matches(sphere.world, view.world))
        .filter_map(|sphere| {
            let center = sphere.center + offset;
            view.frustum
                .intersects_sphere(
                    &bevy::camera::primitives::Sphere {
                        center: Vec3A::from(center),
                        radius: sphere.radius + margin,
                    },
                    false,
                )
                .then(|| view.transform.translation().distance(center))
        })
        .fold(f32::INFINITY, f32::min)
}

/// One batch whose material is not requested until it is seen.
struct LazyBatch {
    key: BatchKey,
    /// Absolute spheres retain each instance's certified PLACED world, not its storage block.
    spheres: Vec<LazySphere>,
    /// One sphere around all of `spheres`: a batch whose own is out of the view has no instance in
    /// it, and its instances are not tested one by one.
    bounds: (Vec3, f32),
    /// The model's scene, requested first, by the same path and label the cell loader uses, so
    /// the asset server loads the model's file once for both. Asking for one of the file's
    /// other labels while the cell loader's scene load is in flight is a second load of the
    /// file, and a second load re-instances every drawn copy of the model, losing
    /// the stock-visibility annotations on those copies.
    /// Held only until the file is in; the material is requested then.
    scene: Option<Handle<WorldAsset>>,
    /// One forced retry must publish a new result before old dependency failure is terminal.
    source_reload_from: Option<bevy::ecs::world::WorldId>,
    /// The base material while it is still loading; cleared once it is ready (or has failed).
    loading: Option<Handle<StandardMaterial>>,
    requested: bool,
    /// When the material was requested (seconds since startup), and whether a request that has
    /// taken too long was reported.
    requested_at: f32,
    reported: bool,
}

/// Present when every batch is to be requested during the startup load
/// (`--cluster-preload-batches`), not only those in or near a view.
#[derive(Resource)]
pub(crate) struct ClusterBatchPreload;

#[derive(Resource)]
pub(super) struct LazyBatches {
    batches: Vec<LazyBatch>,
    assets_dir: PathBuf,
    native_by_model: HashMap<String, Vec<bool>>,
    /// Every model scene the loader has asked for, kept for the run. The cell loader asks for the
    /// same scenes by the same path; if this side let go of one while the other's load or spawn
    /// was in flight, the asset would be freed and loaded again, which re-instances the stock copy
    /// and loses its cluster-owned primitive visibility annotations.
    scenes: HashMap<String, Handle<WorldAsset>>,
    /// Failed cached dependencies get at most one forced restart per source path.
    retried_sources: std::collections::HashSet<String>,
    // Source I/O begun before certification. Capacity only: never ordinary view demand.
    prefetch_in_flight: Vec<Handle<WorldAsset>>,
    /// The batches whose material has not been requested yet, ascending. Everything the loader
    /// looks for each frame (a batch coming into view, a hole in the picture) is among these or
    /// among `in_flight`, so a frame never walks the batches that are loaded and done.
    unrequested: Vec<usize>,
    /// The batches whose scene or material is being loaded.
    in_flight: Vec<usize>,
    /// Terminal source/material failures remain explicit, separate from pending work.
    failed: Vec<usize>,
}

impl LazyBatches {
    /// One explicitly mapped source for real-loader extraction tests, without the streaming loader.
    #[cfg(test)]
    pub(crate) fn for_source_test(model_path: &str, scene: Handle<WorldAsset>) -> Self {
        Self {
            batches: vec![LazyBatch {
                key: BatchKey {
                    model_path: model_path.to_owned(),
                    material_index: Some(0),
                    scale_inverted: false,
                },
                spheres: Vec::new(),
                bounds: (Vec3::ZERO, 0.0),
                scene: Some(scene.clone()),
                source_reload_from: None,
                loading: None,
                requested: true,
                requested_at: 0.0,
                reported: false,
            }],
            assets_dir: PathBuf::new(),
            native_by_model: HashMap::new(),
            scenes: HashMap::from([(model_path.to_owned(), scene)]),
            retried_sources: default(),
            prefetch_in_flight: Vec::new(),
            unrequested: Vec::new(),
            in_flight: vec![0],
            failed: Vec::new(),
        }
    }

    fn prefetch_only_in_flight(&self) -> usize {
        let ordinary: std::collections::HashSet<_> = self
            .in_flight
            .iter()
            .filter_map(|&index| self.batches[index].scene.as_ref().map(Handle::id))
            .collect();
        self.prefetch_in_flight
            .iter()
            .filter(|handle| !ordinary.contains(&handle.id()))
            .count()
    }

    pub(super) fn source_progress(&self, batch: usize) -> Option<(bool, bool, bool)> {
        let batch = self.batches.get(batch)?;
        Some((
            batch.requested,
            batch.scene.is_some(),
            batch.loading.is_some(),
        ))
    }

    pub(super) fn source_scene(&self, batch: u32) -> Option<&Handle<WorldAsset>> {
        self.scenes
            .get(&self.batches.get(batch as usize)?.key.model_path)
    }
}

/// How far outside the view an instance may be and still have its material requested, in world
/// units: the model needs time to load before the camera turns to it.
const LOAD_MARGIN: f32 = 2048.0;

/// How long a material request may take before it is reported as stalled.
const STALLED_SECONDS: f32 = 20.0;

/// Materials that may be loading at once. Every request is a whole glTF with its textures; asking
/// for all of a landscape's at once starves the engine's own streaming of the same task pool.
const MAX_LOADING: usize = 16;

/// What the last full look at the batches saw, so a frame in which nothing it depends on moved can
/// reuse it: the cameras (their frustum and transform change flags), the render origin offset, and
/// how many batches are unrequested, in flight and loading.
#[derive(Default)]
struct ScanCache {
    valid: bool,
    offset: Vec3,
    counts: (usize, usize, usize, usize),
    // Wanted batches still blocked by speculative I/O capacity.
    waiting: usize,
    holes: u64,
    /// Each camera's transform and frustum planes as they were: compared by value, because a
    /// harness that re-applies the same pose every frame marks the transform changed without
    /// moving it.
    cameras: Vec<(GlobalTransform, [Vec4; 6], Option<u32>)>,
}

/// Requests the material of the batches that have an instance in or near a camera's view, the
/// nearest first, keeping at most [`MAX_LOADING`] requests in flight.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn load_batches_in_view(
    mut handles: ResMut<ClusterBatchHandles>,
    mut lazy: ResMut<LazyBatches>,
    scene: Option<Res<ClusterScene>>,
    cameras: Query<
        (
            &Camera,
            &bevy::camera::primitives::Frustum,
            &GlobalTransform,
            Has<StreamingCamera>,
            Has<WaterReflectionCamera>,
        ),
        With<Camera3d>,
    >,
    (asset_server, world_assets): (Res<AssetServer>, Res<Assets<WorldAsset>>),
    mut scan: Local<ScanCache>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut metrics: Option<ResMut<crate::streaming::StreamingMetrics>>,
    mut profiler: Option<ResMut<crate::profiling::ProfilingState>>,
    time: Res<Time>,
    mut settled: Local<bool>,
    origin: Option<Res<RenderOrigin>>,
    config: Res<EngineConfig>,
    (active, preload): (Option<Res<ActiveSpace>>, Option<Res<ClusterBatchPreload>>),
) {
    let _timer = super::render::queue_stats::Timed::start(&super::render::queue_stats::LOAD_NANOS);
    let Some(scene) = scene else {
        return;
    };
    // The same offset the render world places the instances with this frame.
    let offset = scene.origin_offset
        + origin.map_or(Vec3::ZERO, |origin| {
            super::render::render_origin_offset(origin.0)
        });
    let active_world = if active
        .as_ref()
        .is_some_and(|active| active.interior.is_some())
    {
        None
    } else {
        Some(
            active
                .as_ref()
                .and_then(|active| active.worldspace_id)
                .unwrap_or(config.worldspace_id),
        )
    };
    let views: Vec<LazyView> = cameras
        .iter()
        .filter(|(camera, ..)| camera.is_active)
        .map(|(_, frustum, transform, streaming, reflection)| LazyView {
            frustum: *frustum,
            transform: *transform,
            world: lazy_camera_world(streaming, reflection, active_world),
        })
        .collect();
    let lazy = &mut *lazy;
    let mut entered = 0u64;
    let mut failed = 0u64;
    // Requests that have finished (or failed) stop counting against the limit.
    let mut loading = 0usize;
    let in_flight = std::mem::take(&mut lazy.in_flight);
    let mut still_in_flight: Vec<usize> = Vec::with_capacity(in_flight.len());
    for index in in_flight {
        let batch = &mut lazy.batches[index];
        // Phase one: the model's file is being loaded under the scene label the cell loader
        // uses. Once it is in (its sub-assets exist), the material label is a lookup, not a
        // second load of the file.
        if let Some(scene) = &batch.scene {
            if let Some(previous) = batch.source_reload_from {
                // glTF reloads construct a new template World. In-place annotations keep its
                // identity, so they cannot acknowledge a retry before its actual publication.
                let published = world_assets
                    .get(scene.id())
                    .is_some_and(|asset| asset.world.id() != previous);
                let failed = matches!(
                    asset_server.load_state(scene.id()),
                    bevy::asset::LoadState::Failed(_)
                );
                if !published && !failed {
                    loading += 1;
                    still_in_flight.push(index);
                    continue;
                }
                batch.source_reload_from = None;
            }
            if asset_failed(&asset_server, scene.id()) {
                warn!(model = %batch.key.model_path, "cluster source scene failed; stock replacement remains disabled");
                batch.scene = None;
                lazy.failed.push(index);
                failed += 1;
                continue;
            }
            if source_asset_ready(
                world_assets.contains(scene.id()),
                asset_server.is_loaded_with_dependencies(scene.id()),
            ) {
                batch.scene = None;
                request_material(
                    &mut handles,
                    batch,
                    index,
                    &lazy.assets_dir,
                    &mut lazy.native_by_model,
                    &asset_server,
                    &mut standard_materials,
                    time.elapsed_secs(),
                );
                loading += 1;
            } else {
                loading += 1;
            }
            still_in_flight.push(index);
            continue;
        }
        if let Some(handle) = &batch.loading {
            let id = handle.id();
            let load_failed = asset_failed(&asset_server, id);
            let material_ready = source_asset_ready(
                standard_materials.contains(id),
                batch.key.material_index.is_none() || asset_server.is_loaded_with_dependencies(id),
            );
            if material_ready || load_failed {
                batch.loading = None;
                // The batch's objects start drawing from the cluster path (the stock primitives
                // replacement must be coordinated for each view).
                if load_failed {
                    lazy.failed.push(index);
                    failed += 1;
                    warn!(
                        model = %batch.key.model_path,
                        material_index = ?batch.key.material_index,
                        state = ?asset_server.load_state(id),
                        "cluster batch material failed to load; stock replacement remains disabled"
                    );
                } else {
                    entered += 1;
                }
            } else {
                loading += 1;
                still_in_flight.push(index);
                if !batch.reported && time.elapsed_secs() - batch.requested_at > STALLED_SECONDS {
                    batch.reported = true;
                    warn!(
                        model = %batch.key.model_path,
                        material_index = ?batch.key.material_index,
                        state = ?asset_server.load_state(handle.id()),
                        recursive = ?asset_server.recursive_dependency_load_state(handle.id()),
                        "cluster batch material still loading after {STALLED_SECONDS} s"
                    );
                }
            }
        }
    }
    lazy.in_flight = still_in_flight;
    lazy.prefetch_in_flight.retain(|handle| {
        !asset_server.is_loaded_with_dependencies(handle.id())
            && !asset_failed(&asset_server, handle.id())
    });
    let prefetch_only = lazy.prefetch_only_in_flight();
    if let Some(metrics) = metrics.as_deref_mut() {
        metrics.failed_cluster_batches = lazy
            .failed
            .iter()
            .filter(|&&index| {
                let batch = &lazy.batches[index];
                views.iter().any(|view| {
                    lazy_view_distance(batch.bounds, &batch.spheres, view, offset, LOAD_MARGIN)
                        .is_finite()
                })
            })
            .count();
    }
    // When no camera moved, the origin did not shift and no batch changed state, the full look
    // below would see what it saw last frame: nothing to request, the same holes.
    let counts = (
        lazy.unrequested.len(),
        lazy.in_flight.len(),
        loading,
        prefetch_only,
    );
    let current: Vec<_> = views.iter().map(LazyView::snapshot).collect();
    // With `ClusterBatchPreload` every batch is wanted, nearest first: the startup cover waits for
    // all of them, and nothing the views or a crossing's destination proof depend on changes
    // under them during play. The scan cache must not stop the requests while any are left.
    let preload = preload.is_some();
    let unchanged = !(preload && !lazy.unrequested.is_empty())
        && scan.valid
        && scan.offset == offset
        && scan.counts == counts
        && scan.cameras == current;
    // Pop-in counters, the cluster side of the baked object LOD's `lod/object_blocks_*`: batches
    // that start drawing, and the hole count: batches with an instance inside the view (no load
    // margin) whose material is not ready, so the objects they own are drawn by nothing. A hole is
    // counted once per frame it lasts, from the first frame that nothing was pending (the startup
    // load is not pop-in). Clusters never leave (nothing is unloaded) and a level
    // change is under the screen-error threshold, so there is no `left` or `coarsened` count.
    if let Some(profiler) = profiler.as_deref_mut() {
        if !*settled
            && metrics
                .as_deref()
                .is_some_and(|metrics| metrics.pending_cluster_batches == 0)
            && lazy.batches.iter().any(|batch| batch.requested)
        {
            *settled = true;
        }
        let mut holes = 0u64;
        if unchanged {
            holes = scan.holes;
        }
        for batch in lazy
            .unrequested
            .iter()
            .chain(&lazy.in_flight)
            .map(|&index| &lazy.batches[index])
            .filter(|_| !unchanged)
        {
            if *settled && (!batch.requested || batch.loading.is_some() || batch.scene.is_some()) {
                let in_view = views.iter().any(|view| {
                    lazy_view_distance(batch.bounds, &batch.spheres, view, offset, 0.0).is_finite()
                });
                if in_view {
                    holes += 1;
                }
            }
        }
        scan.holes = holes;
        if entered > 0 {
            profiler.increment("cluster/batches_entered", entered);
        }
        if failed > 0 {
            profiler.increment("cluster/batches_failed", failed);
        }
        if holes > 0 {
            profiler.increment("cluster/batch_hole_frames", holes);
        }
    }
    if loading >= MAX_LOADING {
        if let Some(metrics) = metrics.as_deref_mut() {
            metrics.pending_cluster_batches = loading;
        }
        return;
    }
    if unchanged {
        if let Some(metrics) = metrics.as_deref_mut() {
            metrics.pending_cluster_batches = loading + scan.waiting;
        }
        return;
    }
    scan.valid = true;
    scan.offset = offset;
    scan.counts = counts;
    scan.cameras = current;
    // Unrequested batches with an instance in view, by the distance of the nearest one.
    let mut candidates: Vec<(f32, usize)> = Vec::new();
    for &index in &lazy.unrequested {
        let batch = &lazy.batches[index];
        let mut nearest = f32::INFINITY;
        for view in &views {
            nearest = nearest.min(lazy_view_distance(
                batch.bounds,
                &batch.spheres,
                view,
                offset,
                LOAD_MARGIN,
            ));
        }
        if nearest.is_finite() {
            candidates.push((nearest, index));
        } else if preload {
            candidates.push((f32::MAX, index));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Loading and wanted-but-not-yet-requested batches both hold a view back from settling.
    if let Some(metrics) = metrics.as_deref_mut() {
        metrics.pending_cluster_batches = loading + candidates.len();
    }
    let wanted = candidates.len();
    let mut taken: Vec<usize> = Vec::new();
    let mut source_slots = MAX_LOADING.saturating_sub(loading + prefetch_only);
    let mut restarting_sources: HashMap<_, _> = lazy
        .in_flight
        .iter()
        .filter_map(|&index| {
            let batch = &lazy.batches[index];
            batch
                .source_reload_from
                .zip(batch.scene.as_ref().map(Handle::id))
                .map(|(world, id)| (id, world))
        })
        .collect();
    for (_, index) in candidates {
        if taken.len() == MAX_LOADING - loading {
            break;
        }
        // A failure after certification handoff must not bypass a fresh load. Check only
        // this candidate: scanning all retained scenes would add world-sized work each frame.
        let path = &lazy.batches[index].key.model_path;
        if lazy
            .scenes
            .get(path)
            .is_some_and(|handle| asset_failed(&asset_server, handle.id()))
        {
            lazy.scenes.remove(path);
        }
        // Reusing a speculative source starts no additional I/O and must not be blocked
        // by the capacity that very source already occupies.
        if !lazy
            .scenes
            .contains_key(&lazy.batches[index].key.model_path)
        {
            if source_slots == 0 {
                continue;
            }
            source_slots -= 1;
        }
        let path = lazy.batches[index].key.model_path.clone();
        let scene = lazy
            .scenes
            .entry(path.clone())
            .or_insert_with(|| asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.clone())))
            .clone();
        // Request mode retries root failures, but Bevy 0.19 reuses a Loaded scene with a
        // failed image dependency. Restart that cached source once, coalescing its batches.
        if matches!(
            asset_server.load_state(scene.id()),
            bevy::asset::LoadState::Loaded
        ) && asset_failed(&asset_server, scene.id())
            && !lazy.retried_sources.contains(&path)
        {
            // Wait for the old template publication before recording its identity. A mere
            // Loaded flag can precede Assets publication and cannot identify a new attempt.
            let Some(previous) = world_assets.get(scene.id()).map(|asset| asset.world.id()) else {
                // Publication can change without a camera or loader-count change.
                scan.valid = false;
                continue;
            };
            lazy.retried_sources.insert(path.clone());
            restarting_sources.insert(scene.id(), previous);
            asset_server.reload(GltfAssetLabel::Scene(0).from_asset(path.clone()));
            warn!(model = %path, "retrying cached source dependency failure once");
        }
        taken.push(index);
        let batch = &mut lazy.batches[index];
        batch.requested = true;
        batch.requested_at = time.elapsed_secs();
        batch.source_reload_from = restarting_sources.get(&scene.id()).copied();
        // Default-material primitives also need the actual shared source template.
        batch.scene = Some(scene);
    }
    scan.waiting = wanted - taken.len();
    if !taken.is_empty() {
        lazy.unrequested.retain(|index| !taken.contains(index));
        lazy.in_flight.extend(taken);
    }
}

/// AssetServer completion can precede the asset becoming visible in Assets.
/// Both facts are needed before extraction can qualify the source geometry/material.
fn source_asset_ready(present: bool, dependencies_loaded: bool) -> bool {
    present && dependencies_loaded
}

fn asset_failed<A: Asset>(server: &AssetServer, id: AssetId<A>) -> bool {
    matches!(server.load_state(id), bevy::asset::LoadState::Failed(_))
        || matches!(
            server.recursive_dependency_load_state(id),
            bevy::asset::RecursiveDependencyLoadState::Failed(_)
        )
}

/// Requests a batch's base material and spawns the entity that holds it. A batch with no glTF
/// material slot (the default material) is made here directly.
#[allow(clippy::too_many_arguments)]
fn request_material(
    handles: &mut ResMut<ClusterBatchHandles>,
    batch: &mut LazyBatch,
    index: usize,
    assets_dir: &Path,
    native_by_model: &mut HashMap<String, Vec<bool>>,
    asset_server: &AssetServer,
    standard_materials: &mut Assets<StandardMaterial>,
    now: f32,
) {
    batch.requested_at = now;
    let key = &batch.key;
    let base: Handle<StandardMaterial> = match key.material_index {
        Some(slot) => {
            let native = native_by_model
                .entry(key.model_path.clone())
                .or_insert_with(|| native_materials(&assets_dir.join(&key.model_path)));
            let native = native.get(slot as usize).copied().unwrap_or(false);
            asset_server.load(standard_material_path(
                &key.model_path,
                slot,
                key.scale_inverted,
                native,
            ))
        }
        None => standard_materials.add(StandardMaterial::default()),
    };
    batch.loading = Some(base.clone());
    handles.set(index as u32, base);
}

#[cfg(test)]
#[path = "package/readiness_tests.rs"]
mod readiness_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exclusively_terrain_meshes_skip_generation_including_shared_payloads() {
        let terrain = |mesh| ClusterInstance {
            identity: ClusterInstanceIdentity::Terrain {
                block: mesh,
                worldspace: 42,
                tier: 1,
                anchor: [0, 0],
            },
            mesh,
            batch: 0,
            world_from_local: Mat4::IDENTITY,
            source_worldspace: 42,
        };
        let object = |mesh| ClusterInstance {
            identity: ClusterInstanceIdentity::Object(RecordKey {
                worldspace: 73,
                tier: 1,
                anchor: [2, 3],
                ordinal: mesh,
            }),
            mesh,
            batch: 0,
            world_from_local: Mat4::IDENTITY,
            source_worldspace: 73,
        };
        let instances = [terrain(0), terrain(1), object(1), object(2), terrain(0)];
        assert_eq!(
            terrain_only_meshes(&instances),
            std::collections::HashSet::from([0])
        );
        assert!(terrain_only_meshes(&[object(0)]).is_empty());
        let fixture = ClusterInstance {
            identity: ClusterInstanceIdentity::Fixture,
            ..terrain(0)
        };
        assert!(terrain_only_meshes(&[terrain(0), fixture]).is_empty());
        assert!(terrain_only_meshes(&[]).is_empty());
    }

    #[test]
    fn equal_geometry_keeps_each_terrain_atlas_and_preserves_object_material() {
        use super::super::terrain::{ClusterTerrainMaterial, TerrainBlock, TerrainViewBlock};
        let mut images = Assets::<Image>::default();
        let atlas_a = images.add(Image::default());
        let atlas_b = images.add(Image::default());
        let mut materials = Assets::<ClusterTerrainMaterial>::default();
        let material_a = materials.add(ClusterTerrainMaterial {
            base: StandardMaterial {
                base_color_texture: Some(atlas_a.clone()),
                ..default()
            },
            extension: default(),
        });
        let material_b = materials.add(ClusterTerrainMaterial {
            base: StandardMaterial {
                base_color_texture: Some(atlas_b.clone()),
                ..default()
            },
            extension: default(),
        });
        let row = MeshRow {
            expected: MeshExpectation {
                content_hash: [2; 32],
                payload_bytes: 1,
                mesh_hash: [1; 32],
                primitive_index: 0,
                vertex_count: 3,
                cluster_count: 1,
            },
            payload_path: "clusters/meshes/shared.mcl".into(),
            model_path: "meshes/native-object.glb".into(),
            material_index: Some(4),
        };
        let mut blocks: Vec<_> = [material_a.clone(), material_b.clone()]
            .into_iter()
            .enumerate()
            .map(|(index, material)| TerrainBlock {
                mesh_hash: hex(&row.expected.mesh_hash),
                primitive_index: 0,
                worldspace_id: 70 + index as u32,
                anchor: IVec2::new(index as i32 * 16, 0),
                side_cells: 16,
                instance_path: format!("clusters/terrain/instances/{index}.mci"),
                cells: vec![IVec2::new(index as i32 * 16, 0)],
                material,
                batch: None,
            })
            .collect();
        let object = record_batch_key(&row, ChunkDomain::Object, &blocks, true).unwrap();
        assert_eq!(object.model_path, "meshes/native-object.glb");
        assert_eq!(object.material_index, Some(4));
        assert!(object.scale_inverted);
        let keys: Vec<_> = (0..2)
            .map(|index| {
                record_batch_key(&row, ChunkDomain::Terrain(index), &blocks, false).unwrap()
            })
            .collect();
        assert!(keys[0] != keys[1]);
        assert!(keys.iter().all(|key| key.material_index.is_none()));
        let mut handles = ClusterBatchHandles::default();
        let mut batches = HashMap::new();
        for (index, key) in keys.into_iter().enumerate() {
            let next = batches.len() as u32;
            let batch = *batches.entry(key).or_insert(next);
            bind_terrain_batch(&mut blocks, index as u32, batch, &mut handles).unwrap();
            // Reusing the same resolved batch must still assign that exact block.
            bind_terrain_batch(&mut blocks, index as u32, batch, &mut handles).unwrap();
        }
        assert_eq!(batches.len(), 2);
        assert_eq!(blocks[0].batch, Some(0));
        assert_eq!(blocks[1].batch, Some(1));
        assert_eq!(blocks[0].material, material_a);
        assert_eq!(blocks[1].material, material_b);
        assert_eq!(
            materials.get(&material_a).unwrap().base.base_color_texture,
            Some(atlas_a)
        );
        assert_eq!(
            materials.get(&material_b).unwrap().base.base_color_texture,
            Some(atlas_b)
        );
        assert_eq!(TerrainViewBlock::from(&blocks[0]).batch, Some(0));
        assert_eq!(TerrainViewBlock::from(&blocks[1]).batch, Some(1));
        // Object resolution after terrain binding still uses the untouched GLB slot.
        let after = record_batch_key(&row, ChunkDomain::Object, &blocks, true).unwrap();
        assert!(object == after);
        assert_eq!(row.model_path, "meshes/native-object.glb");
        assert_eq!(row.material_index, Some(4));
        assert!(record_batch_key(&row, ChunkDomain::Terrain(2), &blocks, false).is_err());
        blocks[1].primitive_index = 1;
        assert!(record_batch_key(&row, ChunkDomain::Terrain(1), &blocks, false).is_err());
    }

    #[test]
    fn batch_prefetch_decodes_while_pending_then_reuses_or_drops_exact_handles() {
        use crate::cluster::source_identity::actual_loader_tests::loader_app;
        use bevy::ecs::system::RunSystemOnce;
        for accept in [false, true] {
            let directory = crate::cluster::reader::producer_fixture_tests::published_for_loader();
            let connection = crate::cluster::overlay_fixture::read_db(directory.path());
            let rows = read_rows(&connection).unwrap();
            let published: Vec<_> = rows
                .values()
                .map(|row| (row.model_path.clone(), String::new()))
                .collect();
            let mut app = loader_app(directory.path());
            app.init_asset::<StandardMaterial>();
            let paths = prefetch_batch_paths(&connection, &published).unwrap();
            assert!(!paths.is_empty());
            let (send, receive) = crossbeam_channel::bounded(1);
            let assets = app.world().resource::<AssetServer>().clone();
            // Dispatch from the same kind of worker as certification, before any App::update.
            // Startup work delaying Update must not delay the first speculative source reads.
            std::thread::scope(|scope| {
                scope
                    .spawn(|| send.send(start_model_prefetch(paths, true, &assets)))
                    .join()
                    .unwrap()
                    .unwrap();
            });
            let initial = receive.try_recv().unwrap();
            assert!(!initial.scenes.is_empty());
            for handle in initial.scenes.values() {
                assert!(assets.get_load_state(handle.id()).is_some());
            }
            send.send(initial).unwrap();
            drop(send);
            app.add_message::<AppExit>()
                .init_resource::<crate::streaming::StreamingMetrics>()
                .insert_resource(PackageValidationStatus::Pending)
                .insert_resource(PendingModelPrefetch::new(receive))
                .insert_resource(PendingPackageValidation(
                    IoTaskPool::get().spawn(future::pending()),
                ))
                .add_systems(
                    Update,
                    (
                        prefetch_pending_models,
                        poll_package_validation,
                        count_blocks_not_resident,
                    )
                        .chain(),
                );
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                app.update();
                assert_eq!(
                    app.world().resource::<PackageValidationStatus>(),
                    &PackageValidationStatus::Pending
                );
                assert_eq!(
                    app.world()
                        .resource::<crate::streaming::StreamingMetrics>()
                        .pending_cluster_batches,
                    1
                );
                assert!(!app.world().contains_resource::<ValidatedPackage>());
                assert!(!app.world().contains_resource::<ClusterScene>());
                assert!(!app.world().contains_resource::<LazyBatches>());
                let world = app.world_mut();
                assert_eq!(world.query::<&WorldAssetRoot>().iter(world).count(), 0);
                let prefetched = world.resource::<PendingModelPrefetch>();
                if !prefetched.scenes.is_empty()
                    && prefetched.scenes.values().all(|handle| {
                        world
                            .resource::<AssetServer>()
                            .is_loaded_with_dependencies(handle.id())
                    })
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "speculative source scene failed to decode"
                );
                std::thread::yield_now();
            }
            for handle in app
                .world()
                .resource::<PendingModelPrefetch>()
                .scenes
                .values()
            {
                assert!(
                    app.world()
                        .resource::<Assets<WorldAsset>>()
                        .contains(handle.id())
                );
            }
            if accept {
                // Accept before this additional source has any asset publication. Its I/O
                // reservation must survive both certificate and scene-loader handoffs.
                let pending = app
                    .world()
                    .resource::<Assets<WorldAsset>>()
                    .reserve_handle();
                let mut prefetch = app.world_mut().resource_mut::<PendingModelPrefetch>();
                prefetch
                    .scenes
                    .insert("meshes/pending-handoff.glb".into(), pending.clone());
                prefetch.in_flight.push(pending);
            }
            let outstanding: Vec<_> = app
                .world()
                .resource::<PendingModelPrefetch>()
                .in_flight
                .iter()
                .map(Handle::id)
                .collect();
            let expected: HashMap<_, _> = app
                .world()
                .resource::<PendingModelPrefetch>()
                .scenes
                .iter()
                .map(|(path, handle)| (path.clone(), handle.id()))
                .collect();
            let package = directory.path().to_owned();
            app.insert_resource(PendingPackageValidation(IoTaskPool::get().spawn(
                async move {
                    if accept {
                        crate::cluster::overlay_fixture::open_fixture(&package, false)
                            .map_err(|error| format!("{error:#}"))
                    } else {
                        Err("invented certification failure after prefetch".into())
                    }
                },
            )));
            while app.world().contains_resource::<PendingPackageValidation>() {
                app.update();
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert!(!app.world().contains_resource::<PendingModelPrefetch>());
            if accept {
                assert!(app.world().contains_resource::<ValidatedPackage>());
                assert!(!app.world().contains_resource::<ClusterScene>());
                assert_eq!(
                    app.world()
                        .resource::<PrefetchedBatchModels>()
                        .1
                        .iter()
                        .map(Handle::id)
                        .collect::<Vec<_>>(),
                    outstanding
                );

                assert_eq!(
                    app.world()
                        .resource::<PrefetchedBatchModels>()
                        .0
                        .iter()
                        .map(|(path, handle)| (path.clone(), handle.id()))
                        .collect::<HashMap<_, _>>(),
                    expected
                );
                app.insert_resource(PackageSource {
                    base: directory.path().to_owned(),
                    overlay: crate::cluster::overlay_fixture::overlay_path(directory.path()),
                    terrain: false,
                    priority_point: None,
                })
                .init_resource::<ClusterBatchHandles>();
                app.world_mut()
                    .run_system_once(load_cluster_package)
                    .unwrap();
                assert!(app.world().contains_resource::<ClusterScene>());
                assert_eq!(
                    app.world()
                        .resource::<LazyBatches>()
                        .prefetch_in_flight
                        .iter()
                        .map(Handle::id)
                        .collect::<Vec<_>>(),
                    outstanding
                );
                assert_eq!(
                    app.world()
                        .resource::<LazyBatches>()
                        .scenes
                        .iter()
                        .map(|(path, handle)| (path.clone(), handle.id()))
                        .collect::<HashMap<_, _>>(),
                    expected
                );
            } else {
                assert!(!app.world().contains_resource::<PrefetchedBatchModels>());
                assert!(!app.world().contains_resource::<ValidatedPackage>());
                assert!(!app.world().contains_resource::<ClusterScene>());
                assert!(!app.world().contains_resource::<LazyBatches>());
                // No test-owned strong handles mask a retained prefetch allocation.
                while expected
                    .values()
                    .any(|id| app.world().resource::<Assets<WorldAsset>>().contains(*id))
                {
                    app.update();
                    assert!(
                        std::time::Instant::now() < deadline,
                        "refused prefetch kept source scene assets alive"
                    );
                    std::thread::yield_now();
                }
            }
        }
    }

    #[test]
    fn certification_handoff_drains_prefetch_sent_after_the_frames_receiver_pass() {
        use bevy::ecs::system::RunSystemOnce;
        let directory = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let package =
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap();
        let mut app =
            crate::cluster::source_identity::actual_loader_tests::loader_app(directory.path());
        app.add_message::<AppExit>();
        let (send, receive) = crossbeam_channel::bounded(1);
        app.insert_resource(PendingModelPrefetch::new(receive));
        app.world_mut()
            .run_system_once(prefetch_pending_models)
            .unwrap();
        let handle = app
            .world()
            .resource::<Assets<WorldAsset>>()
            .reserve_handle();
        let id = handle.id();
        let assets = app.world().resource::<AssetServer>().clone();
        let failed: Handle<WorldAsset> =
            assets.load(GltfAssetLabel::Scene(0).from_asset("meshes/absent-late-prefetch.glb"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !asset_failed(&assets, failed.id()) {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        send.send(ModelPrefetch {
            remaining: default(),
            scenes: HashMap::from([
                ("meshes/handoff.glb".into(), handle.clone()),
                ("meshes/absent-late-prefetch.glb".into(), failed.clone()),
            ]),
            in_flight: vec![handle, failed],
        })
        .unwrap();
        let task = IoTaskPool::get().spawn(async move { Ok(package) });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !task.is_finished() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.insert_resource(PendingPackageValidation(task));
        // No prefetch system or App::update between the late send and certificate acceptance.
        app.world_mut()
            .run_system_once(poll_package_validation)
            .unwrap();
        let prefetched = app.world().resource::<PrefetchedBatchModels>();
        assert_eq!(prefetched.0["meshes/handoff.glb"].id(), id);
        assert_eq!(
            prefetched.0.len(),
            1,
            "late failed speculation was retained"
        );
        assert_eq!(
            prefetched.1.iter().map(Handle::id).collect::<Vec<_>>(),
            [id]
        );
        assert!(!app.world().contains_resource::<PendingModelPrefetch>());
        assert!(app.world().contains_resource::<ValidatedPackage>());
        assert!(!app.world().contains_resource::<ClusterScene>());
    }

    #[test]
    fn speculative_requests_obey_the_loader_limit_and_lazy_launches_bound_total_residency() {
        use bevy::ecs::system::RunSystemOnce;
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("OWNER.txt"),
            "owner: bounded prefetch test\nclass: A, regenerable\n",
        )
        .unwrap();
        for preload_all in [false, true] {
            let mut app =
                crate::cluster::source_identity::actual_loader_tests::loader_app(directory.path());
            let (send, receive) = crossbeam_channel::bounded(1);
            let initial = start_model_prefetch(
                (0..MAX_LOADING * 3)
                    .map(|i| format!("meshes/absent-{i}.glb"))
                    .collect(),
                preload_all,
                app.world().resource::<AssetServer>(),
            );
            assert_eq!(
                initial.in_flight.len(),
                if preload_all { MAX_LOADING } else { 0 }
            );
            assert_eq!(
                initial.scenes.len(),
                if preload_all { MAX_LOADING } else { 0 }
            );
            assert_eq!(
                initial.remaining.len(),
                if preload_all { MAX_LOADING * 2 } else { 0 }
            );
            send.send(initial).unwrap();
            app.insert_resource(PendingModelPrefetch::new(receive));
            app.world_mut()
                .run_system_once(prefetch_pending_models)
                .unwrap();
            let pending = app.world().resource::<PendingModelPrefetch>();
            assert!(pending.in_flight.len() <= MAX_LOADING);
            assert_eq!(
                pending.scenes.len() + pending.remaining.len(),
                if preload_all { MAX_LOADING * 3 } else { 0 }
            );
        }
    }

    #[test]
    fn failed_speculative_scene_is_dropped_and_a_fresh_load_recovers() {
        use bevy::ecs::system::RunSystemOnce;
        let directory = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let rows = read_rows(&crate::cluster::overlay_fixture::read_db(directory.path())).unwrap();
        let good_path = rows.values().next().unwrap().model_path.clone();
        let path = "meshes/transient-prefetch.glb";
        let mut app =
            crate::cluster::source_identity::actual_loader_tests::loader_app(directory.path());
        app.init_asset::<StandardMaterial>();
        let assets = app.world().resource::<AssetServer>().clone();
        let initial = start_model_prefetch(vec![path.into()], true, &assets);
        let failed_id = initial.scenes[path].id();
        let (send, receive) = crossbeam_channel::bounded(1);
        send.send(initial).unwrap();
        app.insert_resource(PendingModelPrefetch::new(receive));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !asset_failed(&assets, failed_id) {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.world_mut()
            .run_system_once(prefetch_pending_models)
            .unwrap();
        assert!(
            !app.world()
                .resource::<PendingModelPrefetch>()
                .scenes
                .contains_key(path),
            "a failed speculative handle must not bypass the lazy loader's fresh load"
        );
        assert!(
            app.world()
                .resource::<PendingModelPrefetch>()
                .in_flight
                .is_empty()
        );
        std::fs::copy(
            directory.path().join(good_path),
            directory.path().join(path),
        )
        .unwrap();
        let retried: Handle<WorldAsset> = assets.load(GltfAssetLabel::Scene(0).from_asset(path));
        while !assets.is_loaded_with_dependencies(retried.id()) {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "fresh request did not recover"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn speculative_model_paths_are_limited_to_published_glbs() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE cluster_meshes(model_path TEXT); INSERT INTO cluster_meshes VALUES ('../outside.glb'), ('meshes/absent.glb'), ('textures/a.ktx2')").unwrap();
        let models =
            prefetch_batch_paths(&connection, &[("textures/a.ktx2".into(), String::new())])
                .unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn failed_requested_package_exits_nonzero_once_and_never_publishes_a_certificate() {
        let mut app = App::new();
        let task = IoTaskPool::get_or_init(Default::default).spawn(async {
            Err::<CertifiedPackage, _>("invented certification failure".to_owned())
        });
        app.add_message::<AppExit>()
            .insert_resource(PackageValidationStatus::Pending)
            .insert_resource(PendingPackageValidation(task))
            .add_systems(Update, poll_package_validation);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            app.update();
            if matches!(
                app.world().resource::<PackageValidationStatus>(),
                PackageValidationStatus::Failed(_)
            ) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "failure task did not complete"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            app.world().resource::<PackageValidationStatus>(),
            &PackageValidationStatus::Failed("invented certification failure".into())
        );
        assert!(!app.world().contains_resource::<ValidatedPackage>());
        assert!(!app.world().contains_resource::<PendingPackageValidation>());
        let exits = app
            .world()
            .resource::<Messages<AppExit>>()
            .iter_current_update_messages()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(exits, vec![AppExit::error()]);
        app.world_mut().resource_mut::<Messages<AppExit>>().clear();
        for _ in 0..3 {
            app.update();
            assert!(
                app.world()
                    .resource::<Messages<AppExit>>()
                    .iter_current_update_messages()
                    .next()
                    .is_none()
            );
        }
    }

    #[test]
    fn pending_validation_holds_settle_without_a_scene_or_accumulating_counts() {
        let mut app = App::new();
        app.insert_resource(PackageValidationStatus::Pending)
            .init_resource::<crate::streaming::StreamingMetrics>()
            .add_systems(Update, count_blocks_not_resident);
        for _ in 0..3 {
            app.update();
            assert_eq!(
                app.world()
                    .resource::<crate::streaming::StreamingMetrics>()
                    .pending_cluster_batches,
                1
            );
        }
    }

    #[test]
    fn overlay_and_stock_may_spell_the_same_dataset_differently() {
        let directory = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let base = directory.path();
        let overlay = crate::cluster::overlay_fixture::overlay_path(base);
        checked_package_database(base, &overlay).unwrap();
        checked_package_database(&base.join("."), &overlay).unwrap();
        checked_package_database(
            &base.join("derived").join("..").join("."),
            &base.join("derived/.").join(overlay.file_name().unwrap()),
        )
        .unwrap();
    }

    #[test]
    fn an_overlay_of_another_conversion_of_the_dataset_is_refused() {
        let first = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let second = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let overlay = crate::cluster::overlay_fixture::overlay_path(first.path());
        // Byte-identical conversions are the same conversion: the binding is content identity.
        checked_package_database(second.path(), &overlay).unwrap();
        let manifest = second.path().join("conversion-manifest.json");
        let mut bytes = std::fs::read(&manifest).unwrap();
        bytes.push(b'\n');
        std::fs::write(&manifest, bytes).unwrap();
        let error = checked_package_database(second.path(), &overlay).unwrap_err();
        assert!(
            error.contains("another conversion of this base")
                && error.contains("converter derive-clusters"),
            "{error}"
        );
        assert!(checked_package_database(first.path(), &first.path().join("missing")).is_err());
    }

    #[test]
    fn a_sphere_around_spheres_holds_every_one() {
        let spheres = [
            (Vec3::new(0.0, 0.0, 0.0), 1.0),
            (Vec3::new(10.0, 0.0, 0.0), 2.0),
            (Vec3::new(-4.0, 6.0, 0.0), 0.5),
        ];
        let (center, radius) = enclosing_sphere(&spheres);
        for (sphere_center, sphere_radius) in spheres {
            assert!(center.distance(sphere_center) + sphere_radius <= radius + 1.0e-4);
        }
        assert_eq!(enclosing_sphere(&[]), (Vec3::ZERO, 0.0));
    }

    #[test]
    fn a_hex_identity_is_lowercase_and_64_characters() {
        let text = hex(&[0xab; 32]);
        assert_eq!(text.len(), 64);
        assert!(text.starts_with("abab"));
    }
}

/// Terrain batches for readiness publication, copied without the main-world material assets.
#[derive(Resource, Clone, Default, bevy::render::extract_resource::ExtractResource)]
pub(crate) struct TerrainBatchIds(pub Vec<u32>);

#[cfg(test)]
mod lazy_world_tests {
    use super::*;

    fn view(world: Option<u32>) -> LazyView {
        // The actual default frustum has zero planes and admits every finite positive-radius
        // sphere; geometry-specific margin behavior is tested with an explicit half-space below.
        LazyView {
            frustum: default(),
            transform: GlobalTransform::IDENTITY,
            world,
        }
    }
    fn sphere(world: LazyWorld) -> LazySphere {
        LazySphere {
            center: Vec3::new(0.0, 0.0, -10.0),
            radius: 1.0,
            world,
        }
    }
    fn distance(spheres: &[LazySphere], view: &LazyView) -> f32 {
        lazy_view_distance((Vec3::ZERO, 100.0), spheres, view, Vec3::ZERO, LOAD_MARGIN)
    }

    #[test]
    fn identical_coordinates_only_request_the_placed_world_and_shared_batches_keep_matching_members()
     {
        let other = sphere(LazyWorld::Placed(20));
        assert!(!distance(&[other], &view(Some(10))).is_finite());
        assert!(distance(&[other], &view(Some(20))).is_finite());
        let matching = sphere(LazyWorld::Placed(10));
        assert!(distance(&[other, matching], &view(Some(10))).is_finite());
        assert!(!distance(&[other, matching], &view(None)).is_finite());
        // Hole counters call this exact same helper with zero margin.
        assert!(
            !lazy_view_distance(
                (Vec3::ZERO, 100.0),
                &[other],
                &view(Some(10)),
                Vec3::ZERO,
                0.0
            )
            .is_finite()
        );
    }

    #[test]
    fn matching_world_keeps_load_margin_without_creating_cross_world_demand() {
        let mut camera = view(Some(10));
        camera.frustum.half_spaces[0] = bevy::math::primitives::HalfSpace::new(Vec4::X);
        let item = LazySphere {
            center: Vec3::new(-100.0, 0.0, 0.0),
            radius: 1.0,
            world: LazyWorld::Placed(10),
        };
        let bounds = (item.center, item.radius);
        assert!(!lazy_view_distance(bounds, &[item], &camera, Vec3::ZERO, 0.0).is_finite());
        assert!(lazy_view_distance(bounds, &[item], &camera, Vec3::ZERO, LOAD_MARGIN).is_finite());
        camera.world = Some(20);
        assert!(!lazy_view_distance(bounds, &[item], &camera, Vec3::ZERO, LOAD_MARGIN).is_finite());
    }

    #[test]
    fn public_camera_roles_and_world_changes_keep_main_and_reflection_demand() {
        let main = lazy_camera_world(true, false, Some(10));
        let reflection = lazy_camera_world(false, true, Some(10));
        assert_eq!((main, reflection), (Some(10), Some(10)));
        assert_eq!(lazy_camera_world(false, false, Some(10)), None);
        assert_eq!(lazy_camera_world(true, false, None), None);
        assert_eq!(lazy_camera_world(false, true, None), None);
        let camera = view(main);
        let changed_world = view(Some(20));
        assert_ne!(
            camera.snapshot(),
            changed_world.snapshot(),
            "same-pose world switch must invalidate ScanCache"
        );
        assert_eq!(camera.snapshot(), view(main).snapshot());
    }

    #[test]
    fn fixture_is_explicit_and_missing_certified_object_authority_is_never_a_wildcard() {
        assert_eq!(
            lazy_instance_world(&ClusterInstanceIdentity::Fixture, None),
            LazyWorld::Fixture
        );
        let key = RecordKey {
            worldspace: 10,
            tier: 16,
            anchor: [0, 0],
            ordinal: 0,
        };
        let unknown = lazy_instance_world(&ClusterInstanceIdentity::Object(key), None);
        assert_eq!(unknown, LazyWorld::Unknown);
        assert!(!distance(&[sphere(unknown)], &view(Some(10))).is_finite());
        assert!(distance(&[sphere(LazyWorld::Fixture)], &view(None)).is_finite());
    }

    #[test]
    fn real_certificate_child_follows_placed_world_instead_of_parent_storage_block() {
        let directory = super::super::reader::producer_fixture_tests::published();
        let package =
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap();
        let (key, record) = package
            .records()
            .find(|(key, record)| key.worldspace != record.worldspace)
            .expect("real producer fixture has child placement in parent block");
        let ownership = lazy_instance_world(&ClusterInstanceIdentity::Object(*key), Some(&package));
        assert_eq!(ownership, LazyWorld::Placed(record.worldspace));
        assert!(!distance(&[sphere(ownership)], &view(Some(key.worldspace))).is_finite());
        assert!(distance(&[sphere(ownership)], &view(Some(record.worldspace))).is_finite());
        assert_eq!(
            lazy_instance_world(&ClusterInstanceIdentity::Fixture, Some(&package)),
            LazyWorld::Unknown
        );
    }
}

#[cfg(test)]
mod empty_certified_loader_tests {
    use super::*;

    #[test]
    fn real_empty_certified_store_publishes_empty_scene_without_gpu_or_placeholder_block() {
        use converter::{
            cache::ConversionManifest,
            cluster::{
                LodMode,
                authority::CoreAuthority,
                probe::ProbeResult,
                publish::{PublishContext, finalize_base},
                stage::{StageSettings, run_stage},
            },
            esm::exporter::create_tables,
        };
        use rusqlite::OpenFlags;
        use shared::asset_lock::AssetLock;
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let lock = AssetLock::acquire_exclusive(root).unwrap();
        let db = Connection::open(root.join("skyrim_world.db")).unwrap();
        create_tables(&db).unwrap();
        db.execute("INSERT INTO lod_build VALUES(1,?1)", ["cd".repeat(32)])
            .unwrap();
        db.execute(
            "INSERT INTO worldspaces VALUES(112,'Invented',NULL,0,0,0)",
            [],
        )
        .unwrap();
        drop(db);
        let settings = StageSettings {
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
        };
        let mut manifest = ConversionManifest::default();
        let probe = ProbeResult {
            passed: true,
            missing: Vec::new(),
            backend: "test".into(),
            adapter_name: "invented empty loader fixture, no GPU".into(),
            json: serde_json::json!({"test":true}),
        };
        let (report, _) = run_stage(
            &settings,
            Some((probe, 0.0)),
            root,
            &root.join("no-previous"),
            &manifest,
        )
        .unwrap();
        let authority = CoreAuthority::open(
            Connection::open_with_flags(
                root.join("skyrim_world.db"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap(),
            root,
            &manifest,
            &settings,
            report.builder_identity,
        )
        .unwrap();
        let context = PublishContext::new(&lock, root, root).unwrap();
        let base = finalize_base(&context, Some(&authority)).unwrap();
        converter::extensions::refresh_manifest_with_finalized(
            &mut manifest,
            root,
            &[base.extension],
        )
        .unwrap();
        // A real empty store still contains its declared empty folder roots.
        // With no assets the stage does not create these as a side effect.
        for entry in &manifest.extensions {
            for owned in &entry.owns.files {
                if let Some(directory) = owned.strip_suffix('/') {
                    std::fs::create_dir_all(root.join(directory)).unwrap();
                }
            }
        }
        manifest.complete = true;
        manifest
            .save(&root.join("conversion-manifest.json"))
            .unwrap();
        drop(authority);
        drop(lock);
        crate::cluster::overlay_fixture::split_full_copy(root);
        let certificate =
            Arc::new(crate::cluster::overlay_fixture::open_fixture(root, false).unwrap());
        assert_eq!(certificate.records().count(), 0);
        let mut app = App::new();
        app.insert_resource(PackageSource {
            base: root.to_owned(),
            overlay: crate::cluster::overlay_fixture::overlay_path(root),
            terrain: false,
            priority_point: None,
        })
        .insert_resource(ValidatedPackage(certificate.clone()))
        .insert_resource(PackageValidationStatus::Ready)
        .init_resource::<ClusterBatchHandles>()
        .init_resource::<Assets<Image>>()
        .init_resource::<crate::streaming::StreamingMetrics>()
        .add_message::<AppExit>()
        .add_systems(
            Update,
            (
                load_cluster_package,
                count_blocks_not_resident.after(load_cluster_package),
            ),
        );
        app.update();
        let scene = app.world().resource::<ClusterScene>();
        assert!(scene.instances.is_empty() && scene.meshes.is_empty() && scene.blocks.is_empty());
        assert!(Arc::ptr_eq(
            scene.certificate.as_ref().unwrap(),
            &certificate
        ));
        assert!(app.world().resource::<LazyBatches>().batches.is_empty());
        assert_eq!(
            app.world()
                .resource::<crate::streaming::StreamingMetrics>()
                .pending_cluster_batches,
            0
        );
        app.update();
        assert!(app.world().resource::<ClusterScene>().blocks.is_empty());
        // Release actual retained reader guards before tempfile cleanup.
        drop(app);
        drop(certificate);
    }

    #[test]
    fn a_binding_change_after_certification_stops_the_run_instead_of_drawing_none() {
        use bevy::ecs::system::RunSystemOnce;
        let directory = crate::cluster::reader::producer_fixture_tests::published();
        let root = directory.path();
        let certificate =
            Arc::new(crate::cluster::overlay_fixture::open_fixture(root, false).unwrap());
        // Simulate an uncooperative base replacement despite the retained reader lock. The launch
        // receipt retains the checked manifest fingerprint, but each connection must still refuse
        // a changed core identity before it attaches or reads the overlay.
        let database = Connection::open(root.join("skyrim_world.db")).unwrap();
        database
            .execute(
                "UPDATE lod_build SET build_identity = ?1",
                ["ef".repeat(32)],
            )
            .unwrap();
        drop(database);
        let mut app = App::new();
        app.insert_resource(PackageSource {
            base: root.to_owned(),
            overlay: crate::cluster::overlay_fixture::overlay_path(root),
            terrain: false,
            priority_point: None,
        })
        .insert_resource(ValidatedPackage(certificate.clone()))
        .insert_resource(PackageValidationStatus::Ready)
        .init_resource::<ClusterBatchHandles>()
        .init_resource::<Assets<Image>>()
        .add_message::<AppExit>();
        app.world_mut()
            .run_system_once(load_cluster_package)
            .unwrap();
        let exits = app.world().resource::<Messages<AppExit>>();
        assert!(
            exits.iter_current_update_messages().any(AppExit::is_error),
            "requested clusters that no longer open must end the run with an error"
        );
        // Nothing was published before the error: no scene, no batch handles to draw.
        assert!(!app.world().contains_resource::<ClusterScene>());
        assert!(!app.world().contains_resource::<LazyBatches>());
        drop(app);
        drop(certificate);
    }
    #[test]
    fn lost_or_corrupt_payload_after_certification_stops_requested_clusters() {
        use bevy::ecs::system::RunSystemOnce;
        for corruption in [
            "missing_block",
            "invalid_block",
            "invalid_mesh",
            "wrong_builder",
            "missing_mesh_row",
        ] {
            let directory = crate::cluster::reader::producer_fixture_tests::published();
            let root = directory.path();
            let certificate =
                Arc::new(crate::cluster::overlay_fixture::open_fixture(root, false).unwrap());
            let db = crate::cluster::overlay_fixture::overlay_db(root);
            let sql = if corruption == "invalid_mesh" {
                "SELECT payload_path FROM cluster_meshes LIMIT 1"
            } else {
                "SELECT payload_path FROM cluster_chunks LIMIT 1"
            };
            let relative: String = db.query_row(sql, [], |row| row.get(0)).unwrap();
            if corruption == "missing_mesh_row" {
                db.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM cluster_meshes;")
                    .unwrap();
            }
            drop(db);
            let overlay = crate::cluster::overlay_fixture::overlay_path(root);
            if corruption == "missing_block" {
                std::fs::remove_file(overlay.join(relative)).unwrap();
            } else if corruption == "wrong_builder" {
                let path = overlay.join(relative);
                let mut bytes = std::fs::read(&path).unwrap();
                bytes[8..40].fill(77);
                std::fs::write(path, bytes).unwrap();
            } else if corruption != "missing_mesh_row" {
                std::fs::write(
                    overlay.join(relative),
                    b"corrupt payload after certification",
                )
                .unwrap();
            }
            let mut app = App::new();
            app.insert_resource(PackageSource {
                base: root.to_owned(),
                overlay,
                terrain: false,
                priority_point: None,
            })
            .insert_resource(ValidatedPackage(certificate.clone()))
            .insert_resource(PackageValidationStatus::Ready)
            .init_resource::<ClusterBatchHandles>()
            .init_resource::<Assets<Image>>()
            .add_message::<AppExit>();
            app.world_mut()
                .run_system_once(load_cluster_package)
                .unwrap();
            assert!(
                app.world()
                    .resource::<Messages<AppExit>>()
                    .iter_current_update_messages()
                    .any(AppExit::is_error),
                "{corruption}: an unusable requested overlay must stop, rather than publish a partial scene"
            );
            assert!(!app.world().contains_resource::<ClusterScene>());
            assert!(!app.world().contains_resource::<LazyBatches>());
            drop(app);
            drop(certificate);
        }
    }
}

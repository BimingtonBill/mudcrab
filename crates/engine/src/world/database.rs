use super::enable_state::{EnableStateError, EnableStateResolver, InitialEnableInputs};
use crate::doors::DoorwayPlacement;
use crate::world::is_safe_relative_asset_path;
use bevy::prelude::{Reflect, Resource, warn};
use color_eyre::{Result, eyre::WrapErr};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use shared::cluster_recipe::classify::CELL_SIZE;
use shared::lod::{ChunkAnchor, ChunkKey, LodOrigin, LodTier, chunk_payload_path};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    f32::consts::{PI, TAU},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};

pub(crate) const EXTERIOR_CELL_ID_SQL: &str = "SELECT c.id FROM cells c
     LEFT JOIN land l ON l.cell_id=c.id
     WHERE c.worldspace_id=?1 AND c.grid_x=?2 AND c.grid_y=?3
     ORDER BY (l.cell_id IS NOT NULL) DESC, c.id DESC
     LIMIT 1";

#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellKey {
    Exterior {
        worldspace_id: u32,
        grid_x: i32,
        grid_y: i32,
    },
    Interior(u32),
}

/// How much of a cell a request asks for. The full-detail grid is loaded as [`CellDetail::Full`];
/// the terrain-only ring beyond it is loaded as [`CellDetail::Terrain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellDetail {
    /// The whole cell: the landscape, its water plane, and every reference with its light and its
    /// load door.
    Full,
    /// The landscape and its water plane only. A terrain-only cell is drawn and never entered, so
    /// nothing in it can be seen, lit, walked through or opened - and its references are never
    /// queried, which is most of the cost of a request.
    ///
    /// Exteriors only: an interior has no terrain to fall back on, so [`load_cell`] loads a
    /// terrain request for one in full.
    Terrain,
}

#[derive(Debug, Clone)]
pub struct ReferenceRow {
    pub form_id: u32,
    /// New-game enable-state snapshot, including parent chains. All rows remain available;
    /// this bootstrap value does not represent subsequent script/quest/save state.
    pub initially_enabled: bool,
    pub cell_id: u32,
    pub base_form_id: u32,
    /// Authoritative base record type; `statics` also contains movable clutter.
    pub base_record_type: Option<String>,
    pub model_path: Option<String>,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: f32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub bounds_valid: bool,
    /// The reference's `door_links` row, when the database has one. `None` for every ordinary
    /// reference, and for every reference in a database converted before doors were exported.
    pub door: Option<DoorLinkRow>,
    /// The reference's own doorway, when it is a load door and the database can place one: the
    /// model's bounds box under the reference's placement, and the model's own axis convention
    /// (`crate::doors::DoorwayPlacement`). `None` for an ordinary reference, and for a door of a
    /// database converted before the `door_links` table existed.
    pub doorway: Option<DoorwayPlacement>,
    /// The `lights` row of the reference's base record, when the database has one and the record is
    /// a `LIGH`. `None` for every other reference, and for every reference in a database converted
    /// before lights were exported.
    pub light: Option<LightRow>,
    /// The reference's own light radius (`XRDS`), which wins over [`LightRow::radius`]. `None` when
    /// the reference carries no override, and in a database converted before the column existed.
    pub light_radius_override: Option<f32>,
    /// The 8-bit sRGB colour the reference's `XEMI` light or region lends its model's
    /// `External_Emittance` surfaces by day, the engine's one time of day (see `sky.rs`). `None`
    /// when the reference names no source the converter could resolve, and in a database
    /// converted before the `emittance` table existed.
    pub emittance: Option<[u8; 3]>,
    /// Whether the reference's base object is one of Skyrim's auto-load door markers - an invisible
    /// `AutoLoadDoor*` that crosses on contact instead of on the `E` key. Read only for references
    /// that carry a [`door`](Self::door); see [`AUTO_LOAD_COLUMN`] for how the base identifies one.
    pub auto_load: bool,
}

/// A `lights` row as the converted database stores it (`docs/design/lights-and-auto-doors.md`, the
/// converter's schema 5): one row per `LIGH` record, with or without a model.
#[derive(Debug, Clone, PartialEq)]
pub struct LightRow {
    /// Radius in Creation units, from `DATA`'s radius.
    pub radius: f32,
    /// `DATA`'s colour bytes, red first.
    pub color: [u8; 3],
    /// `DATA`'s flags, uninterpreted; see the flag bits in [`crate::lights`].
    pub flags: u32,
    /// `DATA`'s falloff exponent.
    pub falloff: f32,
    /// `FNAM`'s fade, when the record has one.
    pub fade: Option<f32>,
}

/// A `door_links` row as the converted database stores it, plus the destination's label.
///
/// The destination is unresolved when [`Self::destination_cell_id`] is `None`: the converter
/// found the link but not the cell it points at. The engine turns a resolved row into a
/// [`DoorDestination`](crate::doors::DoorDestination); this type stays the table's shape.
/// The arrival frame of a link that leads back into a door: where the player lands coming through
/// it and which way they face, `(position, rotation)` in Creation-engine units and radians, exactly
/// as the `XTEL` of the door that leads there stores it. See [`DoorLinkRow::return_arrival`].
pub type ReturnArrival = ([f32; 3], [f32; 3]);

#[derive(Debug, Clone, PartialEq)]
pub struct DoorLinkRow {
    /// The destination door reference (`XTEL` bytes 0..4).
    pub destination_ref_id: u32,
    /// The destination reference's cell, `None` when the converter could not resolve it.
    pub destination_cell_id: Option<u32>,
    /// The destination reference's worldspace; `None` means the destination is an interior.
    pub destination_worldspace_id: Option<u32>,
    /// Arrival position in Creation-engine units (`XTEL` bytes 4..16).
    pub arrival_position: [f32; 3],
    /// Arrival rotation in Creation-engine radians (`XTEL` bytes 16..28).
    pub arrival_rotation: [f32; 3],
    /// The destination interior's `interior_name`, else the destination worldspace's
    /// `editor_id`, else empty.
    pub label: String,
    /// The arrival frame of the link that leads **back** to this door, when the database has one:
    /// the `XTEL` of a door that opens into this one. The game puts that arrival point in front of
    /// this door, facing away from it, which is what gives the door its own outward direction
    /// without trusting the door model's axes ([`crate::doors::LoadDoor::outward`]). `None` for a
    /// door nothing leads back to - a one-way link, or a database whose door links the converter
    /// could not resolve.
    pub return_arrival: Option<ReturnArrival>,
    /// The **destination reference's** own doorway, when the database can place one: its position,
    /// rotation, scale, bounds box and model convention ([`DoorwayPlacement`]). This is what the
    /// destination door's own geometry is read from under a doorway anchor - the `XTEL` arrival
    /// point is not the destination door, and `destination_ref_id` names the reference that is.
    /// `None` for a link the database cannot resolve to a reference it has placed.
    pub destination_doorway: Option<DoorwayPlacement>,
}

#[derive(Debug, Clone)]
pub struct CellPayload {
    pub generation: u64,
    pub key: CellKey,
    pub cell_id: u32,
    pub references: Vec<ReferenceRow>,
}

#[derive(Debug)]
pub enum DatabaseRequest {
    Load {
        generation: u64,
        key: CellKey,
        detail: CellDetail,
        queued_at: Instant,
    },
    LoadLodChunks {
        generation: u64,
        query: LodChunkQuery,
        queued_at: Instant,
    },
    LoadReferences {
        generation: u64,
        key: CellKey,
        reference_ids: BTreeSet<u32>,
        responses: Sender<ReferenceResponse>,
        queued_at: Instant,
    },
    Shutdown,
}

#[derive(Debug)]
pub struct DatabaseResponse {
    pub generation: u64,
    pub key: CellKey,
    /// The detail the request asked for, echoed back so the commit knows what it is holding even
    /// when the request that asked for it is no longer the one the plan wants.
    pub detail: CellDetail,
    pub result: std::result::Result<CellPayload, String>,
    pub query_micros: u64,
    pub queue_wait_micros: u64,
    pub total_request_micros: u64,
    pub row_count: usize,
}

/// One sparse request's exact selected rows; its bounded response channel is caller-owned.
#[derive(Debug)]
pub struct ReferenceResponse {
    pub generation: u64,
    pub key: CellKey,
    pub result: std::result::Result<Vec<ReferenceRow>, String>,
    pub query_micros: u64,
    pub queue_wait_micros: u64,
    pub total_request_micros: u64,
}

/// World-space AABB used to discover LOD chunks. Bounds are XY Creation units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodChunkQuery {
    pub worldspace_id: u32,
    pub tier: LodTier,
    pub bounds_min: [f64; 2],
    pub bounds_max: [f64; 2],
}

/// Validated world-space bounds stored for an LOD chunk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodChunkBounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// Database metadata needed to validate and place one file-backed LOD chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct LodChunkMetadata {
    pub key: ChunkKey,
    /// Canonical path relative to the converted assets root.
    pub payload_path: String,
    /// Lowercase SHA-256 of the published GLB payload.
    pub content_hash: String,
    pub bounds: LodChunkBounds,
    /// Source exterior cells as absolute `(grid_x, grid_y)` coordinates.
    pub source_cells: Vec<[i32; 2]>,
    pub build_identity: String,
    pub origin: LodOrigin,
}

/// Generation-tagged result of an asynchronous LOD range query.
#[derive(Debug)]
pub struct LodChunkResponse {
    pub generation: u64,
    pub query: LodChunkQuery,
    pub result: std::result::Result<Vec<LodChunkMetadata>, LodQueryFailure>,
    pub query_micros: u64,
    pub queue_wait_micros: u64,
    pub total_request_micros: u64,
    pub chunk_count: usize,
}

/// Whether an asynchronous query failed temporarily or rejected invalid metadata.
#[derive(Debug)]
pub struct LodQueryFailure {
    pub transient: bool,
    pub reason: String,
}

impl LodQueryFailure {
    fn from_error(error: color_eyre::Report) -> Self {
        let transient = error.downcast_ref::<rusqlite::Error>().is_some_and(|error| {
            matches!(error, rusqlite::Error::SqliteFailure(code, _) if matches!(code.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked | rusqlite::ErrorCode::SystemIoFailure))
        });
        Self {
            transient,
            reason: format!("{error:#}"),
        }
    }
}

impl std::fmt::Display for LodQueryFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

/// Which door-link contract the worker may read. Core packs discover the optional table;
/// legacy extension packs require a valid declaration and may explicitly disable the reader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DoorLinksAccess {
    /// Upstream core output; older core packs may have no door table.
    #[default]
    Core,
    /// A validated legacy extension declaration requires a complete door table.
    LegacyExtension,
    /// A stale or unsupported legacy declaration must not be read as core data.
    Disabled,
}

/// Dataset capabilities used by the world database worker (`crate::extensions`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldDatabaseFeatures {
    /// Ownership and validation policy for the promoted `door_links` table.
    pub door_links: DoorLinksAccess,
}

#[derive(Resource)]
pub struct WorldDatabase {
    requests: Sender<DatabaseRequest>,
    responses: Receiver<DatabaseResponse>,
    lod_responses: Receiver<LodChunkResponse>,
    worker: Option<thread::JoinHandle<()>>,
    worker_stopped: Arc<AtomicBool>,
    /// The worldspaces whose `lod_chunks` hold at least one chunk, read once in
    /// [`WorldDatabase::open`]. Empty for a database without the table, or with no chunks compiled.
    ///
    /// [`WorldDatabase::has_lod_for`] answers from this, so the terrain ring's resolution
    /// ([`EngineConfig::effective_terrain_radius`](crate::config::EngineConfig::effective_terrain_radius))
    /// costs a set lookup rather than a query on every frame. The database is opened read-only and
    /// the conversion never writes it while a run holds it (`--reuse-assets` builds elsewhere), so
    /// the answer cannot go stale within a run.
    lod_worldspaces: HashSet<u32>,
    /// Immutable published LAND requirements. None is reserved for channel-only test doubles.
    terrain_cells: Option<HashMap<u32, HashSet<[i32; 2]>>>,
    /// The receiving end [`WorldDatabase::saturated_queue`] keeps open so its one request slot
    /// fills. `None` in every run: a real database's worker owns the receiving end. See
    /// [`Drop`], which releases it before the shutdown request.
    #[cfg(test)]
    held_receiver: Option<Receiver<DatabaseRequest>>,
}

/// A water's Skyrim colours and reflectivity factors, decoded by the converter from its WATR
/// record's `DNAM` (`crates/converter/src/esm/exporter.rs`). `fresnel`/`reflectivity` are
/// additive `waters` columns: a database converted before they existed lacks them, so
/// [`AssetCatalog::water_colors`] returns `None` for every water rather than erroring, and
/// callers fall back to Skyrim's DefaultWater values (see `render::DEFAULT_WATER_FRESNEL` /
/// `render::DEFAULT_WATER_REFLECTIVITY`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterColors {
    pub shallow: [u8; 3],
    pub deep: [u8; 3],
    pub reflection: [u8; 3],
    pub fresnel: f32,
    pub reflectivity: f32,
}

fn unpack_color(value: u32) -> [u8; 3] {
    let bytes = value.to_le_bytes();
    [bytes[0], bytes[1], bytes[2]]
}

#[derive(Resource, Default)]
pub struct AssetCatalog {
    landscape_diffuse: std::collections::HashMap<u32, String>,
    landscape_normal: std::collections::HashMap<u32, String>,
    water_flow: std::collections::HashMap<u32, String>,
    water_colors: std::collections::HashMap<u32, WaterColors>,
}

impl AssetCatalog {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut statement = connection.prepare(
            "SELECT l.id,t.diffuse_path FROM landscape_textures l JOIN texture_sets t ON t.id=l.texture_set_id WHERE t.diffuse_path IS NOT NULL",
        )?;
        let landscape_diffuse = statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(std::result::Result::ok)
            .filter_map(|(id, path)| converted_texture_path(path).map(|path| (id, path)))
            .collect();
        drop(statement);
        // Each landscape texture's normal map, where its texture set has one. A database written
        // before `texture_sets` carried `normal_path` (the in-memory fixtures) has none, and the
        // terrain then draws with the geometric normal alone.
        let landscape_normal = connection
            .prepare(
                "SELECT l.id,t.normal_path FROM landscape_textures l JOIN texture_sets t ON t.id=l.texture_set_id WHERE t.normal_path IS NOT NULL AND t.normal_path <> ''",
            )
            .map(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?)))
                    .map(|rows| {
                        rows.filter_map(std::result::Result::ok)
                            .filter_map(|(id, path)| converted_texture_path(path).map(|path| (id, path)))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let mut statement = connection.prepare(
            "SELECT id,flow_normal_path FROM waters WHERE flow_normal_path IS NOT NULL AND flow_normal_path <> ''",
        )?;
        let water_flow = statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(std::result::Result::ok)
            .filter_map(|(id, path)| converted_texture_path(path).map(|path| (id, path)))
            .collect();
        drop(statement);
        // A database converted before `fresnel`/`reflectivity` existed has no such columns, and
        // `prepare` fails with a "no such column" error rather than an empty result. Treat that
        // the same as no water colours at all instead of refusing to open the catalog.
        let water_colors = connection
            .prepare(
                "SELECT id,shallow_color,deep_color,reflection_color,fresnel,reflectivity FROM waters \
                 WHERE shallow_color IS NOT NULL AND deep_color IS NOT NULL \
                 AND reflection_color IS NOT NULL AND fresnel IS NOT NULL AND reflectivity IS NOT NULL",
            )
            .ok()
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, u32>(0)?,
                            WaterColors {
                                shallow: unpack_color(row.get::<_, u32>(1)?),
                                deep: unpack_color(row.get::<_, u32>(2)?),
                                reflection: unpack_color(row.get::<_, u32>(3)?),
                                fresnel: row.get::<_, f32>(4)?,
                                reflectivity: row.get::<_, f32>(5)?,
                            },
                        ))
                    })
                    .ok()
                    .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            })
            .unwrap_or_default();
        Ok(Self {
            landscape_diffuse,
            landscape_normal,
            water_flow,
            water_colors,
        })
    }

    pub fn landscape_diffuse(&self, form_id: u32) -> Option<&str> {
        self.landscape_diffuse.get(&form_id).map(String::as_str)
    }

    /// The converted normal map of landscape texture `form_id`, if its texture set has one.
    pub fn landscape_normal(&self, form_id: u32) -> Option<&str> {
        self.landscape_normal.get(&form_id).map(String::as_str)
    }

    pub fn water_flow(&self, form_id: u32) -> Option<&str> {
        self.water_flow.get(&form_id).map(String::as_str)
    }

    pub fn water_colors(&self, form_id: u32) -> Option<WaterColors> {
        self.water_colors.get(&form_id).copied()
    }
}

/// Maps a texture path stored in the world database to the converted `.ktx2` under `textures/`.
///
/// The database is input the engine did not write, so a path that could leave `textures/` - a
/// `..` or `.` segment, an absolute or drive-prefixed path, an empty segment, or a `:` (a Windows
/// drive or stream, or an asset source such as `embedded://`) - is refused rather than loaded.
fn converted_texture_path(path: String) -> Option<String> {
    // Converted assets are published with lowercase canonical paths, so the
    // lookup must lowercase too (matching world-inspect's resolver).
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let without_prefix = normalized.strip_prefix("textures/").unwrap_or(&normalized);
    if without_prefix.is_empty()
        || !is_safe_relative_asset_path(without_prefix)
        || without_prefix.contains(':')
        || without_prefix
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
    {
        return None;
    }
    let mut converted = std::path::PathBuf::from("textures").join(without_prefix);
    converted.set_extension("ktx2");
    Some(converted.to_string_lossy().replace('\\', "/"))
}

impl WorldDatabase {
    /// A database with no worker whose request queue holds `capacity` requests, with the far ends of
    /// both channels handed to the test: it reads what the engine asked for and answers as the
    /// worker would. Tests only.
    #[cfg(test)]
    pub(crate) fn channel_fixture(
        capacity: usize,
    ) -> (Self, Receiver<DatabaseRequest>, Sender<DatabaseResponse>) {
        let (requests, request_rx) = bounded(capacity);
        let (response_tx, responses) = unbounded();
        let (_lod_tx, lod_responses) = unbounded();
        (
            Self {
                requests,
                responses,
                lod_responses,
                worker: None,
                worker_stopped: Arc::new(AtomicBool::new(true)),
                // No database behind this one, so it holds no LOD data either.
                lod_worldspaces: HashSet::new(),
                terrain_cells: None,
                held_receiver: None,
            },
            request_rx,
            response_tx,
        )
    }

    /// Opens core data, including available core door links (`WorldDatabaseFeatures::default`).
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, WorldDatabaseFeatures::default())
    }

    /// Opens a world database and reads the extensions in `features`. A listed extension whose table
    /// is missing is an error here, at open, not a cell that fails later.
    pub fn open_with(path: &Path, features: WorldDatabaseFeatures) -> Result<Self> {
        Self::open_with_lod(path, features, true)
    }

    /// Cluster runs retain core LAND and reference sources without inspecting baked LOD.
    pub(crate) fn open_with_lod(
        path: &Path,
        features: WorldDatabaseFeatures,
        baked_lod: bool,
    ) -> Result<Self> {
        validate(path)?;
        if features.door_links == DoorLinksAccess::LegacyExtension {
            require_table(path, "door_links", crate::extensions::Feature::DoorLinks)?;
        }
        let path = path.to_owned();
        // Before the worker takes the file: which worldspaces draw their own terrain LOD. Read here
        // rather than in the worker because a run resolves the ring on the frame the space changes
        // and cannot wait for a query to come back (`EngineConfig::effective_terrain_radius`).
        let lod_worldspaces = if baked_lod {
            lod_worldspaces(&path)
        } else {
            HashSet::new()
        };
        let terrain_cells = Some(read_terrain_cells(&path)?);
        let (request_tx, request_rx) = bounded(128);
        // Responses must not block shutdown if the main world stops polling.
        let (response_tx, response_rx) = unbounded();
        let (lod_response_tx, lod_response_rx) = unbounded();
        let worker_stopped = Arc::new(AtomicBool::new(false));
        let stopped = worker_stopped.clone();
        let worker = thread::Builder::new()
            .name("mudcrab-world-db".into())
            .spawn(move || {
                worker(path, features, request_rx, response_tx, lod_response_tx);
                stopped.store(true, Ordering::Release);
            })
            .wrap_err("failed to start world database worker")?;
        Ok(Self {
            requests: request_tx,
            responses: response_rx,
            lod_responses: lod_response_rx,
            worker: Some(worker),
            worker_stopped,
            lod_worldspaces,
            terrain_cells,
            #[cfg(test)]
            held_receiver: None,
        })
    }

    /// Whether the database holds at least one terrain LOD chunk for `worldspace_id`, which is what
    /// decides that the worldspace draws its own distant terrain (see
    /// [`EngineConfig::effective_terrain_radius`](crate::config::EngineConfig::effective_terrain_radius)):
    /// the terrain ring is left off where the LOD covers that distance and kept where it does not.
    ///
    /// A database without a `lod_chunks` table - one converted before LOD existed - answers false
    /// for every worldspace, and so does a worldspace the conversion compiled no chunks for. Both
    /// keep the ring, which is what they drew before LOD was a thing. The answer is read once in
    /// [`Self::open`] and cached for the run.
    ///
    /// The answer is worldspace-granular and says nothing about coverage: *one* chunk turns the ring
    /// off for the whole worldspace, so a mod worldspace or a partial conversion whose LOD covers
    /// only part of its grid draws LOD where it has chunks and neither LOD nor ring past them, where
    /// the ring would have drawn ground. Per-area coverage would need a query per frame and is not
    /// what this asks; a conversion that compiles LOD for a worldspace is expected to compile it for
    /// its whole reach (the converter walks the grid), so the assumption is the conversion's.
    pub fn has_lod_for(&self, worldspace_id: u32) -> bool {
        self.lod_worldspaces.contains(&worldspace_id)
    }

    /// A terrain-only request is meaningful only for an actual LAND cell in this world.
    /// Missing LAND does not require a replacement and must not create a failed phantom request.
    pub fn requires_terrain(&self, world: u32, cell: [i32; 2]) -> bool {
        self.terrain_cells.as_ref().is_none_or(|worlds| {
            worlds
                .get(&world)
                .is_some_and(|cells| cells.contains(&cell))
        })
    }

    pub fn request(&self, request: DatabaseRequest) -> Result<()> {
        self.requests
            .send(request)
            .wrap_err("world database worker stopped")
    }

    /// [`Self::request`] without blocking. Ordinary streaming intake must never wait for the worker
    /// on the main thread.
    ///
    /// The planner asks for a whole ring of cells at once, hundreds of them, and the worker drains
    /// its bounded queue at one query per answer: blocking on a full queue would stall the frame
    /// for as long as that takes. A request that does not fit is refused as a full queue and left
    /// for the next frame - the plan's wanted list is ordered nearest first, so the cells closest
    /// to the camera are always the ones that do. A stopped worker is refused with its own error.
    pub(crate) fn try_request(&self, request: DatabaseRequest) -> Result<()> {
        enqueue_request(&self.requests, request)
    }

    /// A database with no worker running and a request queue of one slot that nothing drains: the
    /// first request fills it and every later one is refused with `Full`, which is what the planner
    /// sees on a frame where the worker is behind. Tests only - a run would never get a cell out of
    /// this.
    #[cfg(test)]
    pub(crate) fn saturated_queue() -> Self {
        let (requests, held_receiver) = bounded(1);
        let (_responses, responses) = unbounded();
        let (_lod_responses, lod_responses) = unbounded();
        Self {
            requests,
            responses,
            lod_responses,
            worker: None,
            worker_stopped: Arc::new(AtomicBool::new(true)),
            // No database behind this one, so it holds no LOD data either; every worldspace gets
            // the ring (`EngineConfig::effective_terrain_radius`).
            lod_worldspaces: HashSet::new(),
            terrain_cells: None,
            // Held open and never read, so the one slot fills: dropping the receiver instead would
            // refuse requests as `Disconnected`, which is the stopped-worker case rather than a
            // full queue.
            held_receiver: Some(held_receiver),
        }
    }

    pub fn try_response(&self) -> Option<DatabaseResponse> {
        self.responses.try_recv().ok()
    }

    /// Includes returned payloads that the main world has not polled yet.
    pub(crate) fn pending_cell_responses(&self) -> usize {
        self.responses.len()
    }

    /// Queue a world-space LOD chunk range query on the existing database worker.
    pub fn request_lod_chunks(&self, generation: u64, query: LodChunkQuery) -> Result<()> {
        enqueue_lod_query(&self.requests, generation, query)
    }

    /// Read the next LOD response without consuming ordinary cell responses.
    pub fn try_lod_response(&self) -> Option<LodChunkResponse> {
        self.lod_responses.try_recv().ok()
    }

    /// Request exact exterior references on the existing worker, independently of cell/LOD
    /// response polling. A canceled receiver does not cancel or stop that shared worker.
    pub fn request_references(
        &self,
        generation: u64,
        key: CellKey,
        reference_ids: BTreeSet<u32>,
    ) -> Result<Receiver<ReferenceResponse>> {
        enqueue_reference_query(&self.requests, generation, key, reference_ids)
    }
}

fn enqueue_reference_query(
    requests: &Sender<DatabaseRequest>,
    generation: u64,
    key: CellKey,
    reference_ids: BTreeSet<u32>,
) -> Result<Receiver<ReferenceResponse>> {
    color_eyre::eyre::ensure!(
        matches!(key, CellKey::Exterior { .. }),
        "sparse reference requests require an exterior position grid"
    );
    let (responses, receiver) = bounded(1);
    let request = DatabaseRequest::LoadReferences {
        generation,
        key,
        reference_ids,
        responses,
        queued_at: Instant::now(),
    };
    match requests.try_send(request) {
        Ok(()) => Ok(receiver),
        Err(TrySendError::Full(_)) => Err(color_eyre::eyre::eyre!(
            "world database request queue is full"
        )),
        Err(TrySendError::Disconnected(_)) => {
            Err(color_eyre::eyre::eyre!("world database worker stopped"))
        }
    }
}

fn enqueue_lod_query(
    requests: &Sender<DatabaseRequest>,
    generation: u64,
    query: LodChunkQuery,
) -> Result<()> {
    let request = DatabaseRequest::LoadLodChunks {
        generation,
        query,
        queued_at: Instant::now(),
    };
    enqueue_request(requests, request)
}

fn enqueue_request(requests: &Sender<DatabaseRequest>, request: DatabaseRequest) -> Result<()> {
    match requests.try_send(request) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(color_eyre::eyre::eyre!(
            "world database request queue is full"
        )),
        Err(TrySendError::Disconnected(_)) => {
            Err(color_eyre::eyre::eyre!("world database worker stopped"))
        }
    }
}

impl Drop for WorldDatabase {
    fn drop(&mut self) {
        // A test-only receiver goes first. Only a running worker is sent the shutdown, a blocking
        // send that it drains; a database with no worker (a test fixture) has nothing to stop.
        #[cfg(test)]
        drop(self.held_receiver.take());
        if let Some(worker) = self.worker.take() {
            let _ = self.requests.send(DatabaseRequest::Shutdown);
            let _ = worker.join();
        }
        debug_assert!(self.worker_stopped.load(Ordering::Acquire));
    }
}

/// A listed extension's table must exist: a data set that names the extension and lacks the table is
/// corrupt, and says so when the database opens.
fn require_table(path: &Path, table: &str, feature: crate::extensions::Feature) -> Result<()> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .wrap_err_with(|| format!("failed to open {}", path.display()))?;
    let found: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |row| row.get(0),
    )?;
    color_eyre::eyre::ensure!(
        found > 0,
        "extension {} is listed but {} has no `{table}` table",
        feature.extension(),
        path.display()
    );
    Ok(())
}

fn validate(path: &Path) -> Result<()> {
    shared::world_assets::validate_world_database(path).map(|_| ())
}

pub(crate) use shared::world_assets::validate_lod_build_contract;

fn worker(
    path: std::path::PathBuf,
    features: WorldDatabaseFeatures,
    requests: Receiver<DatabaseRequest>,
    responses: Sender<DatabaseResponse>,
    lod_responses: Sender<LodChunkResponse>,
) {
    // Which optional tables and columns this database has does not change while it is open, so
    // the reference query is built once for the connection. If the database cannot be opened or
    // probed, every request is still answered, with that error, so the cells fail visibly instead
    // of waiting forever.
    let mut setup = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(color_eyre::eyre::Report::from)
    .and_then(|connection| {
        let query = ReferenceQuery::for_connection(&connection, features)?;
        Ok((connection, query))
    })
    .map_err(|error| format!("world database {} is unusable: {error:#}", path.display()));
    // The links that lead back into a door, and every door's own doorway, are the same rows for
    // every cell, so they are read once for the life of the connection instead of once per cell.
    let mut tables = DoorTables::default();
    while let Ok(request) = requests.recv() {
        match request {
            DatabaseRequest::Shutdown => break,
            DatabaseRequest::Load {
                generation,
                key,
                detail,
                queued_at,
            } => {
                let queue_wait_micros = elapsed_micros(queued_at);
                let started = Instant::now();
                let result = match &mut setup {
                    Ok((connection, query)) => {
                        load_cell(connection, query, generation, key, detail, &mut tables)
                            .map_err(|error| format!("{error:#}"))
                    }
                    Err(error) => Err(error.clone()),
                };
                let query_micros = elapsed_micros(started);
                let row_count = result
                    .as_ref()
                    .map_or(0, |payload| payload.references.len());
                if responses
                    .send(DatabaseResponse {
                        generation,
                        key,
                        detail,
                        result,
                        query_micros,
                        queue_wait_micros,
                        total_request_micros: elapsed_micros(queued_at),
                        row_count,
                    })
                    .is_err()
                {
                    break;
                }
            }
            DatabaseRequest::LoadLodChunks {
                generation,
                query,
                queued_at,
            } => {
                let queue_wait_micros = elapsed_micros(queued_at);
                let started = Instant::now();
                let result = match &setup {
                    Ok((connection, _)) => {
                        load_lod_chunks(connection, query).map_err(LodQueryFailure::from_error)
                    }
                    Err(error) => Err(LodQueryFailure {
                        transient: false,
                        reason: error.clone(),
                    }),
                };
                let query_micros = elapsed_micros(started);
                let chunk_count = result.as_ref().map_or(0, Vec::len);
                if lod_responses
                    .send(LodChunkResponse {
                        generation,
                        query,
                        result,
                        query_micros,
                        queue_wait_micros,
                        total_request_micros: elapsed_micros(queued_at),
                        chunk_count,
                    })
                    .is_err()
                {
                    break;
                }
            }
            DatabaseRequest::LoadReferences {
                generation,
                key,
                reference_ids,
                responses: reference_responses,
                queued_at,
            } => {
                let queue_wait_micros = elapsed_micros(queued_at);
                let started = Instant::now();
                let result = match &mut setup {
                    Ok((connection, query)) => {
                        load_references(connection, query, key, &reference_ids, &mut tables)
                            .map_err(|error| format!("{error:#}"))
                    }
                    Err(error) => Err(error.clone()),
                };
                let query_micros = elapsed_micros(started);
                // Exactly one reply fits the fresh one-slot channel. A canceled sparse
                // consumer must not stop unrelated cell/LOD requests on this worker.
                let response = ReferenceResponse {
                    generation,
                    key,
                    result,
                    query_micros,
                    queue_wait_micros,
                    total_request_micros: elapsed_micros(queued_at),
                };
                if let Err(TrySendError::Full(_)) = reference_responses.try_send(response) {
                    // DatabaseRequest is public: a caller can supply a full or rendezvous
                    // channel instead of request_references' fresh one-slot receiver.
                    warn!(
                        generation,
                        ?key,
                        "sparse reference reply channel has no capacity"
                    );
                }
            }
        }
    }
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

fn load_lod_chunks(connection: &Connection, query: LodChunkQuery) -> Result<Vec<LodChunkMetadata>> {
    validate_lod_query(query)?;

    if !has_lod_table(connection)? {
        let version: u32 =
            connection.query_row("SELECT version FROM schema_info", [], |row| row.get(0))?;
        color_eyre::eyre::ensure!(
            version < shared::WORLD_DATABASE_LOD_SCHEMA_VERSION,
            "world database has no LOD chunk table"
        );
        return Ok(Vec::new());
    }
    let has_chunks: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM lod_chunks WHERE worldspace_id=?1)",
        [query.worldspace_id],
        |row| row.get(0),
    )?;
    if !has_chunks {
        return Ok(Vec::new());
    }

    let (origin_x, origin_y): (Option<i32>, Option<i32>) = connection
        .query_row(
            "SELECT lod_origin_x,lod_origin_y FROM worldspaces WHERE id=?1",
            [query.worldspace_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .wrap_err_with(|| {
            format!(
                "LOD worldspace {} is missing from worldspaces or has invalid origin columns",
                query.worldspace_id
            )
        })?;
    let origin = LodOrigin::new(
        origin_x.ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "LOD worldspace {} has no valid X origin",
                query.worldspace_id
            )
        })?,
        origin_y.ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "LOD worldspace {} has no valid Y origin",
                query.worldspace_id
            )
        })?,
    );

    let build_identity: String = connection
        .query_row(
            "SELECT build_identity FROM lod_build WHERE id=1",
            [],
            |row| row.get(0),
        )
        .wrap_err("LOD build identity is missing from lod_build")?;
    color_eyre::eyre::ensure!(
        is_canonical_sha256(&build_identity),
        "LOD build identity is not a lowercase SHA-256 digest"
    );

    let mut statement = connection.prepare_cached(
        "SELECT DISTINCT c.worldspace_id,c.tier,c.anchor_x,c.anchor_y,c.payload_path,c.content_hash,\
         c.bounds_min_x,c.bounds_min_y,c.bounds_min_z,c.bounds_max_x,c.bounds_max_y,c.bounds_max_z,\
         c.source_cells FROM lod_chunks_spatial r \
         JOIN lod_chunks c ON c.worldspace_id=r.worldspace_id AND c.tier=r.tier \
         AND c.anchor_x=r.anchor_x AND c.anchor_y=r.anchor_y \
         WHERE r.worldspace_id=?1 AND r.tier=?2 \
         AND r.minX<=?4 AND r.maxX>=?3 AND r.minY<=?6 AND r.maxY>=?5",
    )?;
    let rows = statement.query_map(
        params![
            query.worldspace_id,
            query.tier.side_cells(),
            query.bounds_min[0],
            query.bounds_max[0],
            query.bounds_min[1],
            query.bounds_max[1],
        ],
        RawLodChunk::from_row,
    )?;

    rows.map(|row| {
        let raw = row?;
        validate_lod_chunk(raw, query.worldspace_id, origin, &build_identity)
    })
    .collect()
}

fn has_lod_table(connection: &Connection) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='lod_chunks')",
        [],
        |row| row.get(0),
    )
}

/// LAND, not a rectangular chunk bound or an empty CELL record, defines required terrain.
fn read_terrain_cells(path: &Path) -> Result<HashMap<u32, HashSet<[i32; 2]>>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut worlds: HashMap<u32, HashSet<[i32; 2]>> = HashMap::new();
    if !has_table(&connection, "land")? {
        return Ok(worlds);
    }
    let mut query = connection.prepare(
        "SELECT c.worldspace_id,c.grid_x,c.grid_y FROM cells c JOIN land l ON l.cell_id=c.id \
         WHERE c.worldspace_id IS NOT NULL AND c.grid_x IS NOT NULL AND c.grid_y IS NOT NULL",
    )?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        worlds
            .entry(row.get(0)?)
            .or_default()
            .insert([row.get(1)?, row.get(2)?]);
    }
    Ok(worlds)
}

/// The worldspaces [`WorldDatabase::has_lod_for`] answers true for: the ones with at least one
/// `lod_chunks` row, from the database at `path`.
///
/// The question is per worldspace and the answer is a property of the converted database, which a
/// run cannot write, so it is read once - one `DISTINCT` over the whole table rather than an
/// `EXISTS` per worldspace - and kept. A database without the table (converted before LOD existed)
/// has none, silently: that is a database this engine renders, and every worldspace of it keeps the
/// ring. A `lod_chunks` table that is there and cannot be read is a fault of its own and is
/// reported, and every worldspace keeps the ring rather than the run refusing to open over a table
/// it may never query; the LOD streamer fails loudly on the same table when it does query it
/// (`load_lod_chunks`).
fn lod_worldspaces(path: &Path) -> HashSet<u32> {
    match read_lod_worldspaces(path) {
        Ok(worldspaces) => worldspaces,
        Err(error) => {
            warn!(
                %error,
                "world database's LOD worldspace list is unreadable: every worldspace keeps its \
                 terrain ring"
            );
            HashSet::new()
        }
    }
}

fn read_lod_worldspaces(path: &Path) -> rusqlite::Result<HashSet<u32>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if !has_lod_table(&connection)? {
        return Ok(HashSet::new());
    }
    let mut statement = connection.prepare("SELECT DISTINCT worldspace_id FROM lod_chunks")?;
    let rows = statement.query_map([], |row| row.get::<_, u32>(0))?;
    rows.collect()
}

fn validate_lod_query(query: LodChunkQuery) -> Result<()> {
    for (axis, min, max) in [
        ("X", query.bounds_min[0], query.bounds_max[0]),
        ("Y", query.bounds_min[1], query.bounds_max[1]),
    ] {
        color_eyre::eyre::ensure!(
            min.is_finite() && max.is_finite() && min <= max,
            "LOD query has non-finite or unordered {axis} bounds: {min}..{max}"
        );
    }
    Ok(())
}

struct RawLodChunk {
    worldspace_id: i64,
    tier: i64,
    anchor_x: i64,
    anchor_y: i64,
    payload_path: String,
    content_hash: String,
    bounds_min: [f64; 3],
    bounds_max: [f64; 3],
    source_cells: String,
}

impl RawLodChunk {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            worldspace_id: row.get(0)?,
            tier: row.get(1)?,
            anchor_x: row.get(2)?,
            anchor_y: row.get(3)?,
            payload_path: row.get(4)?,
            content_hash: row.get(5)?,
            bounds_min: [row.get(6)?, row.get(7)?, row.get(8)?],
            bounds_max: [row.get(9)?, row.get(10)?, row.get(11)?],
            source_cells: row.get(12)?,
        })
    }
}

fn validate_lod_chunk(
    raw: RawLodChunk,
    requested_worldspace: u32,
    origin: LodOrigin,
    build_identity: &str,
) -> Result<LodChunkMetadata> {
    let worldspace_id = u32::try_from(raw.worldspace_id)
        .wrap_err_with(|| format!("LOD chunk has invalid worldspace id {}", raw.worldspace_id))?;
    color_eyre::eyre::ensure!(
        worldspace_id == requested_worldspace,
        "LOD chunk worldspace {worldspace_id} does not match requested worldspace {requested_worldspace}"
    );
    let tier = lod_tier_from_database(raw.tier)?;
    let anchor_x = i32::try_from(raw.anchor_x)
        .wrap_err_with(|| format!("LOD chunk has invalid anchor X {}", raw.anchor_x))?;
    let anchor_y = i32::try_from(raw.anchor_y)
        .wrap_err_with(|| format!("LOD chunk has invalid anchor Y {}", raw.anchor_y))?;
    let key = ChunkKey::new(worldspace_id, tier, ChunkAnchor::new(anchor_x, anchor_y));

    let canonical_path = chunk_payload_path(key);
    color_eyre::eyre::ensure!(
        is_safe_relative_asset_path(&raw.payload_path) && raw.payload_path == canonical_path,
        "LOD chunk {key:?} has unsafe or non-canonical payload path {:?}; expected {canonical_path:?}",
        raw.payload_path
    );
    color_eyre::eyre::ensure!(
        is_canonical_sha256(&raw.content_hash),
        "LOD chunk {key:?} has an invalid content hash"
    );
    for axis in 0..3 {
        color_eyre::eyre::ensure!(
            raw.bounds_min[axis].is_finite()
                && raw.bounds_max[axis].is_finite()
                && raw.bounds_min[axis] <= raw.bounds_max[axis],
            "LOD chunk {key:?} has non-finite or unordered world-space bounds on axis {axis}"
        );
    }

    Ok(LodChunkMetadata {
        key,
        payload_path: raw.payload_path,
        content_hash: raw.content_hash,
        bounds: LodChunkBounds {
            min: raw.bounds_min,
            max: raw.bounds_max,
        },
        source_cells: parse_source_cells(&raw.source_cells)
            .wrap_err_with(|| format!("LOD chunk {key:?} has invalid source_cells metadata"))?,
        build_identity: build_identity.to_owned(),
        origin,
    })
}

fn lod_tier_from_database(value: i64) -> Result<LodTier> {
    let side_cells =
        i32::try_from(value).wrap_err_with(|| format!("LOD chunk has invalid tier {value}"))?;
    LodTier::from_side_cells(side_cells)
        .ok_or_else(|| color_eyre::eyre::eyre!("LOD chunk has unsupported tier {side_cells}"))
}

fn parse_source_cells(value: &str) -> Result<Vec<[i32; 2]>> {
    color_eyre::eyre::ensure!(!value.is_empty(), "source_cells is empty");
    let mut seen = BTreeSet::new();
    let mut cells = Vec::new();
    for pair in value.split(';') {
        let (x, y) = pair
            .split_once(',')
            .ok_or_else(|| color_eyre::eyre::eyre!("expected x,y cell coordinate, got {pair:?}"))?;
        let cell = [
            x.parse::<i32>()
                .wrap_err_with(|| format!("invalid source cell X coordinate {x:?}"))?,
            y.parse::<i32>()
                .wrap_err_with(|| format!("invalid source cell Y coordinate {y:?}"))?,
        ];
        color_eyre::eyre::ensure!(seen.insert(cell), "duplicate source cell {cell:?}");
        cells.push(cell);
    }
    Ok(cells)
}

fn is_canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The reference columns every cell query selects, in the order [`map_reference`] reads them.
const REFERENCE_COLUMNS: &str = concat!(
    "r.id,r.cell_id,r.base_form_id,s.model_path,",
    "r.pos_x,r.pos_y,r.pos_z,r.rot_x,r.rot_y,r.rot_z,r.scale,",
    "COALESCE(s.bounds_min_x,-64),COALESCE(s.bounds_min_y,-64),COALESCE(s.bounds_min_z,-64),",
    "COALESCE(s.bounds_max_x,64),COALESCE(s.bounds_max_y,64),COALESCE(s.bounds_max_z,64),",
    "COALESCE(s.bounds_valid,0)",
);

// The reference query's column layout, which [`map_reference`] reads by index:
//
// - 0..=17: the reference and its base's bounds ([`REFERENCE_COLUMNS`]); 18: the base record type;
// - 19..=23: the base's `lights` row ([`LIGHT_COLUMNS`]); 24: the `XRDS` radius override;
// - 25..=33: the `door_links` row ([`DOOR_COLUMNS`]); 34..=36: the `XEMI` day colour;
// - 37..=39: the reference's enable-state inputs ([`REFERENCE_ENABLE_START`]);
// - 40: the door's destination label; 41..=42: the light's falloff and fade; 43: the auto-load rule.
//
// Every optional table or column reads as `NULL` (or the model-only auto-load rule) when the
// database lacks it, so no index moves between database versions and one feature's projection can
// never be read as another's.
const REFERENCE_ENABLE_START: usize = 37;
const DOOR_LABEL_INDEX: usize = 40;
const LIGHT_DETAIL_START: usize = 41;
const AUTO_LOAD_INDEX: usize = 43;

/// The `door_links` row of the reference, in the order [`map_reference`] reads them.
const DOOR_COLUMNS: &str = "d.destination_ref_id,d.destination_cell_id,d.destination_worldspace_id,\
     d.pos_x,d.pos_y,d.pos_z,d.rot_x,d.rot_y,d.rot_z";

/// Stand-in for [`DOOR_COLUMNS`] in a database whose doors are not read: every reference reads as a
/// non-door, with the column order unchanged.
const ABSENT_DOOR_COLUMNS: &str = "NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL";

/// The destination's label: the destination interior's `interior_name`, else the destination
/// worldspace's `editor_id`, else empty. [`DOOR_LABEL_WITHOUT_WORLDSPACES_COLUMN`] is the same rule
/// for a database without the `worldspaces` table, which can only name interiors.
const DOOR_LABEL_COLUMN: &str = "COALESCE(NULLIF(dc.interior_name,''),NULLIF(ws.editor_id,''),'')";
const DOOR_LABEL_WITHOUT_WORLDSPACES_COLUMN: &str = "COALESCE(NULLIF(dc.interior_name,''),'')";
const ABSENT_DOOR_LABEL_COLUMN: &str = "NULL";

/// The `lights` row of the reference's base record, in the order [`map_reference`] reads them.
const LIGHT_COLUMNS: &str = "l.radius,l.color_r,l.color_g,l.color_b,l.flags";

/// Stand-in for [`LIGHT_COLUMNS`] in a database that predates the `lights` table: every reference
/// reads as unlit, with the column order unchanged.
const ABSENT_LIGHT_COLUMNS: &str = "NULL,NULL,NULL,NULL,NULL";

/// The rest of the `lights` row: `DATA`'s falloff exponent and `FNAM`'s fade.
const LIGHT_DETAIL_COLUMNS: &str = "l.falloff,l.fade";
const ABSENT_LIGHT_DETAIL_COLUMNS: &str = "NULL,NULL";

/// The reference's own `XRDS` light radius, which wins over the record's. The column arrived with
/// the `lights` table in conversion schema 5; a database from before it has no such column.
const RADIUS_OVERRIDE_COLUMN: &str = "r.radius_override";
const ABSENT_RADIUS_OVERRIDE_COLUMN: &str = "NULL";

/// Whether the reference's base object is an auto-load door marker: the `statics` row of the base
/// record, whose editor id starts `AutoLoadDoor` (`AutoLoadDoor01`, `AutoLoadDoorMinUse01`,
/// `AutoLoadDoorHiddenMinUse01` in Skyrim.esm) or whose model is an `AutoLoadMarker*.nif`. In the
/// converted database the two rules agree exactly - 315 of the 2204 placed load doors, and no door
/// matched one without the other (`tools/research/auto_load_doors.py`) - so an editor id is only
/// needed for a base the model rule cannot see. SQLite's `LIKE` is case-insensitive for ASCII, and
/// the model test is lowercased anyway so the rule does not rest on that.
const AUTO_LOAD_COLUMN: &str = "CASE WHEN s.editor_id LIKE 'AutoLoadDoor%' \
OR LOWER(COALESCE(s.model_path,'')) LIKE '%autoloadmarker%' THEN 1 ELSE 0 END";

/// Stand-in for [`AUTO_LOAD_COLUMN`] for a `statics` table with no `editor_id`: the model rule
/// alone, which is all such a database can answer. Every exported database has both columns.
const ABSENT_EDITOR_ID_AUTO_LOAD_COLUMN: &str =
    "CASE WHEN LOWER(COALESCE(s.model_path,'')) LIKE '%autoloadmarker%' THEN 1 ELSE 0 END";

/// The destination label a prompt draws. An interior's `interior_name` is the `FULL` subrecord's
/// bytes as the converter read them (`crates/converter/src/esm/extractors.rs`), so it ends in the
/// NUL the game's format terminates it with - an invisible character the prompt renders as a box
/// after `Alftand01`. Trailing NULs and whitespace are not part of anyone's cell name.
fn door_label(label: String) -> String {
    label
        .trim_end_matches(|character: char| character == '\0' || character.is_whitespace())
        .to_owned()
}

const REFERENCE_JOIN: &str = " LEFT JOIN statics s ON s.id=r.base_form_id";

/// The `door_links` row of the reference and the destination cell its label is read from.
const DOOR_JOIN: &str = concat!(
    " LEFT JOIN door_links d ON d.ref_id=r.id",
    " LEFT JOIN cells dc ON dc.id=d.destination_cell_id",
);

/// The destination worldspace an exterior door's label is read from, when the database has the
/// `worldspaces` table.
const DOOR_WORLDSPACE_JOIN: &str = " LEFT JOIN worldspaces ws ON ws.id=d.destination_worldspace_id";

/// The `lights` row of the reference's base record: one `LIGH` record can be placed many times,
/// each reference lighting the space at its own radius.
const LIGHT_JOIN: &str = " LEFT JOIN lights l ON l.id=r.base_form_id";

/// The day colour of the reference's `XEMI` source (`docs/specs/converters/db-schema.md`). The
/// `emittance` table and `references.emittance_id` are optional: an older database has neither.
const EMITTANCE_COLUMNS: &str = "e.day_r,e.day_g,e.day_b";
const ABSENT_EMITTANCE_COLUMNS: &str = "NULL,NULL,NULL";
const EMITTANCE_JOIN: &str = " LEFT JOIN emittance e ON e.id=r.emittance_id";

fn has_table(connection: &Connection, table: &str) -> Result<bool> {
    let count: i64 = connection
        .prepare_cached("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1")?
        .query_row([table], |row| row.get(0))?;
    Ok(count > 0)
}

fn has_records(connection: &Connection) -> Result<bool> {
    has_table(connection, "records")
}

/// Whether the database carries the `lights` table. A database converted before lights were
/// exported still loads; every reference then reads as unlit.
fn has_lights(connection: &Connection) -> Result<bool> {
    has_table(connection, "lights")
}

/// Every `door_links` column [`DOOR_COLUMNS`] and [`DOOR_JOIN`] read.
const DOOR_LINK_COLUMN_NAMES: [&str; 10] = [
    "ref_id",
    "destination_ref_id",
    "destination_cell_id",
    "destination_worldspace_id",
    "pos_x",
    "pos_y",
    "pos_z",
    "rot_x",
    "rot_y",
    "rot_z",
];

/// Whether the database carries a `door_links` table with every column the reference query reads.
/// It is optional: a database converted before doors were exported still loads, and no reference
/// in it is a load door. A table of another shape is treated the same way (and logged) rather than
/// failing every cell's query, like [`has_radius_override`] checks its column.
pub(crate) fn has_door_links(connection: &Connection) -> Result<bool> {
    let mut statement =
        connection.prepare_cached("SELECT name FROM pragma_table_info('door_links')")?;
    let columns: BTreeSet<String> = statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if columns.is_empty() {
        return Ok(false);
    }
    let missing: Vec<&str> = DOOR_LINK_COLUMN_NAMES
        .into_iter()
        .filter(|name| !columns.contains(*name))
        .collect();
    if !missing.is_empty() {
        bevy::log::warn!(
            ?missing,
            "world database door_links table lacks columns; no reference is a load door"
        );
        return Ok(false);
    }
    Ok(true)
}

/// Whether `"references"` carries the `XRDS` light radius override. It arrived with the `lights`
/// table, but the two are detected separately: the override is a reference column, and a database
/// with the table and without the column must still load.
fn has_radius_override(connection: &Connection) -> Result<bool> {
    let count: i64 = connection
        .prepare_cached(
            "SELECT COUNT(*) FROM pragma_table_info('references') WHERE name='radius_override'",
        )?
        .query_row([], |row| row.get(0))?;
    Ok(count > 0)
}

/// Whether the database carries both `references.emittance_id` and the `emittance` table it names.
fn has_emittance(connection: &Connection) -> Result<bool> {
    let count: i64 = connection
        .prepare_cached(
            "SELECT (SELECT COUNT(*) FROM pragma_table_info('references') WHERE name='emittance_id')
                  * (SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='emittance')",
        )?
        .query_row([], |row| row.get(0))?;
    Ok(count > 0)
}

/// Whether `statics` carries `editor_id`. Exported databases have had it since the table was
/// written, and only the engine's own fixtures predate it; without the column the auto-load rule
/// falls back to the base's model, which is all such a table can answer.
fn has_statics_editor_id(connection: &Connection) -> Result<bool> {
    let count: i64 = connection
        .prepare_cached("SELECT COUNT(*) FROM pragma_table_info('statics') WHERE name='editor_id'")?
        .query_row([], |row| row.get(0))?;
    Ok(count > 0)
}

/// The links that lead back into a door, read from `door_links` the first time a cell needs them
/// and kept from then on.
///
/// The table is the same for every cell of a database, so answering "which way does this door
/// face" out of it costs one scan for the whole run instead of one scan per cell load: a cell with
/// doors used to pay for the whole table on every load, which is the shape of cost that is
/// invisible on an interior with four doors and ruinous on a full exterior grid. The connection
/// the engine opens is read-only (`SQLITE_OPEN_READ_ONLY` in [`validate`] and in [`worker`]), so
/// what was read once cannot go stale.
#[derive(Default)]
struct ReturnLinks(Option<HashMap<u32, ReturnArrival>>);

impl ReturnLinks {
    /// The links, read on the first call and the same map on every one after it.
    fn get_or_build(&mut self, connection: &Connection) -> Result<&HashMap<u32, ReturnArrival>> {
        if self.0.is_none() {
            self.0 = Some(return_links(connection)?);
        }
        Ok(self.0.as_ref().expect("just read"))
    }
}

/// Every link that leads **to** a reference, keyed by that reference's FormID: what
/// [`DoorLinkRow::return_arrival`] is filled from.
///
/// This is the data a `LEFT JOIN door_links back ON back.destination_ref_id = r.id` would bring in,
/// read in one pass instead of one join per reference. The join is the shape a reader expects, but
/// `door_links` has no index on `destination_ref_id` (the converter creates the table with `ref_id`
/// as its only key, `crates/converter/src/esm/exporter.rs`), so it would scan the whole table once
/// for every reference of every cell - and it fans out: `destination_ref_id` is not unique, and two
/// doors that lead into the same door would return that door's reference twice, spawning two doors
/// where the game has one. One pass over the table cannot duplicate a reference.
///
/// The one pass is [`ReturnLinks`]'s, once for the whole run.
///
/// When several links lead to the same door the lowest `ref_id` wins, so the answer does not depend
/// on the order the table happens to be in. A link whose arrival point is NULL (one the converter
/// could not finish) is no link here either, as in [`map_reference`].
fn return_links(connection: &Connection) -> Result<HashMap<u32, ReturnArrival>> {
    let mut statement = connection.prepare_cached(
        "SELECT ref_id,destination_ref_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z FROM door_links \
         WHERE pos_x IS NOT NULL AND pos_y IS NOT NULL AND pos_z IS NOT NULL \
         AND rot_x IS NOT NULL AND rot_y IS NOT NULL AND rot_z IS NOT NULL",
    )?;
    let mut rows = statement.query([])?;
    let mut links: HashMap<u32, (u32, [f32; 3], [f32; 3])> = HashMap::new();
    while let Some(row) = rows.next()? {
        let ref_id: u32 = row.get(0)?;
        let destination: u32 = row.get(1)?;
        if links
            .get(&destination)
            .is_some_and(|existing| existing.0 <= ref_id)
        {
            continue;
        }
        links.insert(
            destination,
            (
                ref_id,
                [row.get(2)?, row.get(3)?, row.get(4)?],
                [row.get(5)?, row.get(6)?, row.get(7)?],
            ),
        );
    }
    Ok(links
        .into_iter()
        .map(|(destination, (_, position, rotation))| (destination, (position, rotation)))
        .collect())
}

/// The fewest placements a model needs before its own axis convention is read from them, the share
/// of those placements that has to agree with the mean, and how far one may stand from it and still
/// count as agreeing, in degrees.
///
/// This is `reliable_models` of `tools/research/portal_door_alignment.py`, the rule the doorway
/// alignment report's section 3 measures with: 41 of the instal's 104 door models pass it, and a
/// model that does not (the invisible `AutoLoadMarker01`, ship trapdoors, ladder doors, the few
/// real doors placed in two orientations) has a circular mean of disagreement that predicts
/// nothing.
const CONVENTION_MIN_PLACEMENTS: usize = 4;
const CONVENTION_MIN_SHARE: f32 = 0.8;
const CONVENTION_WITHIN_DEGREES: f32 = 15.0;

/// Every reference with a `door_links` row, as its own doorway: [`DoorwayPlacement`] by FormID.
///
/// Read once for the life of the connection, like [`ReturnLinks`]: the table is the same for every
/// cell of a database, and it is what gives a spawned door the geometry its doorway anchor is built
/// from. The convention of each model is fitted here and carried on every one of its placements, so
/// a caller never needs a second lookup for it.
#[derive(Default)]
struct Doorways(Option<HashMap<u32, DoorwayPlacement>>);

impl Doorways {
    /// The placements, read on the first call and the same map on every one after it.
    fn get_or_build(&mut self, connection: &Connection) -> Result<&HashMap<u32, DoorwayPlacement>> {
        if self.0.is_none() {
            self.0 = Some(doorway_placements(connection)?);
        }
        Ok(self.0.as_ref().expect("just read"))
    }
}

/// The tables a cell load needs that are not rows of the cell itself, read once for the life of the
/// connection: the links that lead back into a door ([`ReturnLinks`]) and every door's own doorway
/// ([`Doorways`]).
#[derive(Default)]
struct DoorTables {
    links: ReturnLinks,
    doorways: Doorways,
}

/// Every door reference's own doorway, and its model's own axis convention.
///
/// The convention is a fact about the *model*, fitted from every placement of it in the instal: the
/// circular mean of (link-derived facing - reference yaw) over those placements, kept only for a
/// model whose placements agree with it - at least [`CONVENTION_MIN_PLACEMENTS`] of them, at least
/// [`CONVENTION_MIN_SHARE`] of them within [`CONVENTION_WITHIN_DEGREES`] degrees of the mean. The
/// link-derived facing is [`crate::doors::outward_from_return_link`], the same evidence a spawned
/// door's `outward` is read from, so this pass and the engine agree on what a door's link data says
/// about it.
///
/// The query is `tools/research/portal_door_alignment.py`'s `DOOR_SELECT` exactly (2,204 rows in
/// the installed data), minus what only the report needs: one row per placed load door, joined to
/// the `statics` row that says which model it is and to the cell that places it on a grid.
fn doorway_placements(connection: &Connection) -> Result<HashMap<u32, DoorwayPlacement>> {
    let links = return_links(connection)?;
    let mut statement = connection.prepare_cached(
        "SELECT r.id,r.pos_x,r.pos_y,r.pos_z,r.rot_x,r.rot_y,r.rot_z,r.scale,\
         COALESCE(s.model_path,''),\
         s.bounds_min_x,s.bounds_min_y,s.bounds_min_z,s.bounds_max_x,s.bounds_max_y,s.bounds_max_z,\
         COALESCE(s.bounds_valid,0),c.grid_x,c.grid_y \
         FROM \"references\" r JOIN door_links d ON d.ref_id=r.id \
         LEFT JOIN statics s ON s.id=r.base_form_id LEFT JOIN cells c ON c.id=r.cell_id",
    )?;
    let mut rows = statement.query([])?;
    let mut placements: HashMap<u32, DoorwayPlacement> = HashMap::new();
    // The evidence each model's convention is fitted from: (link-derived facing - reference yaw),
    // one per placement whose return link carries a usable direction.
    let mut offsets: HashMap<String, Vec<f32>> = HashMap::new();
    while let Some(row) = rows.next()? {
        let form_id: u32 = row.get(0)?;
        let position: [f32; 3] = [row.get(1)?, row.get(2)?, row.get(3)?];
        let rotation = [row.get(4)?, row.get(5)?, row.get(6)?];
        let scale: f32 = row.get(7)?;
        let model: String = row.get::<_, Option<String>>(8)?.unwrap_or_default();
        let bounds: [Option<f32>; 6] = [
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
            row.get(12)?,
            row.get(13)?,
            row.get(14)?,
        ];
        let bounds_valid = row.get::<_, Option<i64>>(15)?.is_some_and(|flag| flag != 0);
        let (box_centre, box_bottom) = match (bounds_valid, bounds) {
            (
                true,
                [
                    Some(min_x),
                    Some(min_y),
                    Some(min_z),
                    Some(max_x),
                    Some(max_y),
                    Some(max_z),
                ],
            ) => (
                Some([
                    (min_x + max_x) * 0.5,
                    (min_y + max_y) * 0.5,
                    (min_z + max_z) * 0.5,
                ]),
                Some(min_y),
            ),
            _ => (None, None),
        };
        let grid = match (
            row.get::<_, Option<i32>>(16)?,
            row.get::<_, Option<i32>>(17)?,
        ) {
            // The cell's own grid says only that the reference is in an exterior: the position
            // says where. Load doors are often in a worldspace's persistent cell, whose grid is
            // (0, 0) wherever its references stand (Riverwood's are), so the grid is the
            // reference's own position's.
            (Some(_), Some(_)) => Some([
                (position[0] / 4096.0).floor() as i32,
                (position[1] / 4096.0).floor() as i32,
            ]),
            _ => None,
        };
        let return_arrival_z = links.get(&form_id).map(|(arrival, _)| arrival[2]);
        if let Some((arrival, arrival_rotation)) = links.get(&form_id)
            && let Some(outward) =
                crate::doors::outward_from_return_link(position, *arrival, *arrival_rotation)
        {
            offsets
                .entry(model.clone())
                .or_default()
                .push(outward[0].atan2(outward[1]) - rotation[2]);
        }
        placements.insert(
            form_id,
            DoorwayPlacement {
                position,
                rotation,
                scale,
                box_centre,
                box_bottom,
                model,
                // Filled in below, once every model's placements have been seen.
                convention: None,
                grid,
                return_arrival_z,
            },
        );
    }
    let conventions = model_conventions(&offsets);
    for placement in placements.values_mut() {
        placement.convention = conventions.get(&placement.model).copied();
    }
    Ok(placements)
}

/// The models whose own axis convention can be read from their placements, and what it is: the
/// circular mean of each model's offsets, in radians, for the models [`CONVENTION_MIN_PLACEMENTS`]
/// of whose placements agree with it to within [`CONVENTION_WITHIN_DEGREES`] degrees at least
/// [`CONVENTION_MIN_SHARE`] of the time. A model that fails the test is left out - its placements
/// disagree by more than the mean can absorb, and a door of it keeps today's frame (tier 3 or 4).
fn model_conventions(offsets: &HashMap<String, Vec<f32>>) -> HashMap<String, f32> {
    let mut conventions = HashMap::new();
    for (model, offsets) in offsets {
        if offsets.len() < CONVENTION_MIN_PLACEMENTS {
            continue;
        }
        // A circular mean, so angles that wrap around north average the way they should.
        let (sine, cosine) = offsets.iter().fold((0.0, 0.0), |(sine, cosine), offset| {
            (sine + offset.sin(), cosine + offset.cos())
        });
        let mean = sine.atan2(cosine);
        let within = offsets
            .iter()
            .filter(|offset| {
                ((**offset - mean + PI).rem_euclid(TAU) - PI).abs()
                    <= CONVENTION_WITHIN_DEGREES.to_radians()
            })
            .count() as f32
            / offsets.len() as f32;
        if within >= CONVENTION_MIN_SHARE {
            conventions.insert(model.clone(), mean);
        }
    }
    conventions
}

/// The reference query's column list and joins for one database: every optional table (`records`,
/// `lights`, `door_links`, `emittance`, `worldspaces`) and column (`radius_override`,
/// `statics.editor_id`, the enable-state columns) is joined when the database has it and read as
/// `NULL` (or the model-only auto-load rule, or "enabled") when it does not, so [`map_reference`]'s
/// column indices are the same either way.
///
/// Load doors are read when the manifest lists the `mudcrab.door_links` extension (`features`, whose
/// table [`WorldDatabase::open_with`] has already required) or, for a data set that carries the
/// table as part of its core schema, when [`has_door_links`] finds a complete table. A listed
/// extension whose table lacks a column is an error rather than a data set with no doors.
///
/// Enable-state metadata is optional for legacy packages; the resolver survives cell unloads on
/// this worker.
struct ReferenceQuery {
    columns: String,
    joins: String,
    has_doors: bool,
    parent_sql: String,
    enable_states: EnableStateResolver,
    enable_errors: HashSet<EnableStateError>,
}

impl ReferenceQuery {
    fn for_connection(connection: &Connection, features: WorldDatabaseFeatures) -> Result<Self> {
        let has_records = has_records(connection)?;
        let record_column = if has_records { "b.record_type" } else { "NULL" };
        let has_lights = has_lights(connection)?;
        let (light_columns, light_detail_columns) = if has_lights {
            (LIGHT_COLUMNS, LIGHT_DETAIL_COLUMNS)
        } else {
            (ABSENT_LIGHT_COLUMNS, ABSENT_LIGHT_DETAIL_COLUMNS)
        };
        let override_column = if has_radius_override(connection)? {
            RADIUS_OVERRIDE_COLUMN
        } else {
            ABSENT_RADIUS_OVERRIDE_COLUMN
        };
        let auto_load_column = if has_statics_editor_id(connection)? {
            AUTO_LOAD_COLUMN
        } else {
            ABSENT_EDITOR_ID_AUTO_LOAD_COLUMN
        };
        let complete_door_links =
            features.door_links != DoorLinksAccess::Disabled && has_door_links(connection)?;
        color_eyre::eyre::ensure!(
            complete_door_links || features.door_links != DoorLinksAccess::LegacyExtension,
            "extension {} is listed but its door_links table lacks columns",
            crate::extensions::Feature::DoorLinks.extension()
        );
        let has_doors = complete_door_links;
        let has_worldspaces = has_doors && has_table(connection, "worldspaces")?;
        let (door_columns, door_label_column) = match (has_doors, has_worldspaces) {
            (true, true) => (DOOR_COLUMNS, DOOR_LABEL_COLUMN),
            (true, false) => (DOOR_COLUMNS, DOOR_LABEL_WITHOUT_WORLDSPACES_COLUMN),
            (false, _) => {
                bevy::log::info!(
                    "world database has no door_links table; no reference is a load door"
                );
                (ABSENT_DOOR_COLUMNS, ABSENT_DOOR_LABEL_COLUMN)
            }
        };
        let has_emittance = has_emittance(connection)?;
        let emittance_columns = if has_emittance {
            EMITTANCE_COLUMNS
        } else {
            ABSENT_EMITTANCE_COLUMNS
        };
        let mut joins = String::from(REFERENCE_JOIN);
        if has_records {
            joins.push_str(" LEFT JOIN records b ON b.form_id=r.base_form_id");
        }
        if has_lights {
            joins.push_str(LIGHT_JOIN);
        }
        if has_doors {
            joins.push_str(DOOR_JOIN);
            if has_worldspaces {
                joins.push_str(DOOR_WORLDSPACE_JOIN);
            }
        }
        if has_emittance {
            joins.push_str(EMITTANCE_JOIN);
        }
        let enable_column_count: usize = connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('references') \
             WHERE name IN ('header_flags','enable_parent_id','enable_parent_flags')",
            [],
            |row| row.get(0),
        )?;
        let enable_columns = match enable_column_count {
            0 => {
                bevy::log::warn!(
                    "world database has no reference enable-state metadata; references default to enabled; reconvert assets to filter initially disabled references"
                );
                "0,NULL,0"
            }
            3 => "header_flags,enable_parent_id,enable_parent_flags",
            _ => color_eyre::eyre::bail!(
                "incomplete reference enable-state columns: expected header_flags, enable_parent_id and enable_parent_flags"
            ),
        };
        let reference_enable_columns = match enable_column_count {
            0 => "0,NULL,0",
            _ => "r.header_flags,r.enable_parent_id,r.enable_parent_flags",
        };
        Ok(Self {
            columns: format!(
                "{REFERENCE_COLUMNS},{record_column},{light_columns},{override_column},\
                 {door_columns},{emittance_columns},{reference_enable_columns},\
                 {door_label_column},{light_detail_columns},{auto_load_column}"
            ),
            joins,
            has_doors,
            parent_sql: format!("SELECT {enable_columns} FROM \"references\" WHERE id=?1"),
            enable_states: EnableStateResolver::default(),
            enable_errors: HashSet::new(),
        })
    }

    /// Resolves each row's new-game enable state from its own inputs and its parent chain, which
    /// may lead outside the loaded cell. A reference whose chain cannot be resolved (a missing or
    /// cyclic parent) stays in the payload but reads as initially disabled, and each kind of
    /// failure is logged once for the connection.
    fn resolve_enable_states(
        &mut self,
        connection: &Connection,
        rows: Vec<(ReferenceRow, InitialEnableInputs)>,
    ) -> Result<Vec<ReferenceRow>> {
        let mut references = Vec::with_capacity(rows.len());
        for (mut reference, inputs) in rows {
            let state = self
                .enable_states
                .resolve(reference.form_id, inputs, |parent| {
                    connection
                        .prepare_cached(&self.parent_sql)?
                        .query_row([parent], |row| map_enable_inputs(row, 0))
                        .optional()
                })
                .wrap_err_with(|| format!("enable state of reference {:08X}", reference.form_id))?;
            reference.initially_enabled = match state {
                Ok(enabled) => enabled,
                Err(error) => {
                    if self.enable_errors.insert(error) {
                        bevy::log::warn!(
                            "reference {:08X}: {error}; unresolved references will not spawn",
                            reference.form_id
                        );
                    }
                    false
                }
            };
            references.push(reference);
        }
        Ok(references)
    }
}

fn load_cell(
    connection: &Connection,
    query: &mut ReferenceQuery,
    generation: u64,
    key: CellKey,
    detail: CellDetail,
    tables: &mut DoorTables,
) -> Result<CellPayload> {
    let cell_id: u32 = match key {
        CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        } => connection.query_row(
            EXTERIOR_CELL_ID_SQL,
            params![worldspace_id, grid_x, grid_y],
            |row| row.get(0),
        )?,
        CellKey::Interior(cell_id) => cell_id,
    };
    // A terrain-only cell is drawn and never entered, so the landscape the cell cache holds for
    // `cell_id` is everything the request needs. Answering it here also skips the reference query
    // below, which is the whole cost of a request at this distance.
    if detail == CellDetail::Terrain && matches!(key, CellKey::Exterior { .. }) {
        return Ok(CellPayload {
            generation,
            key,
            cell_id,
            references: Vec::new(),
        });
    }
    let columns = &query.columns;
    let joins = &query.joins;
    let rows = match key {
        CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        } => {
            let sql = format!(
                "SELECT {columns} FROM exterior_spatial x JOIN \"references\" r ON r.id=x.id{joins} \
                 WHERE x.worldspace_id=?1 AND x.minX>=?2 AND x.minX<?3 AND x.minY>=?4 AND x.minY<?5"
            );
            let min_x = grid_x as f32 * 4096.0;
            let min_y = grid_y as f32 * 4096.0;
            connection
                .prepare_cached(&sql)?
                .query_map(
                    params![worldspace_id, min_x, min_x + 4096.0, min_y, min_y + 4096.0],
                    map_reference,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        }
        CellKey::Interior(_) => {
            let sql = format!("SELECT {columns} FROM \"references\" r{joins} WHERE r.cell_id=?1");
            connection
                .prepare_cached(&sql)?
                .query_map([cell_id], map_reference)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    let mut references = query.resolve_enable_states(connection, rows)?;
    finish_references(connection, query.has_doors, &mut references, tables)?;
    Ok(CellPayload {
        generation,
        key,
        cell_id,
        references,
    })
}

/// Selected-ID lookups use the same projection as near streaming, but membership is exact
/// placed-world plus the shared cluster recipe's position grid, never an owning CELL or Rtree.
/// The rows carry the same enable state and door enrichment a cell load gives them.
fn load_references(
    connection: &Connection,
    query: &mut ReferenceQuery,
    key: CellKey,
    reference_ids: &BTreeSet<u32>,
    tables: &mut DoorTables,
) -> Result<Vec<ReferenceRow>> {
    let CellKey::Exterior {
        worldspace_id,
        grid_x,
        grid_y,
    } = key
    else {
        color_eyre::eyre::bail!("sparse reference requests require an exterior position grid");
    };
    if reference_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {} FROM \"references\" r{} \
         WHERE r.id=?1 AND r.is_exterior=1 AND r.worldspace_id=?2",
        query.columns, query.joins
    );
    let mut statement = connection.prepare_cached(&sql)?;
    let mut rows = Vec::with_capacity(reference_ids.len());
    for &reference_id in reference_ids {
        let (reference, inputs) = statement
            .query_row(params![reference_id, worldspace_id], map_reference)
            .optional()?
            .ok_or_else(|| color_eyre::eyre::eyre!(
                "reference {reference_id:08X} is missing or not exterior in placed world {worldspace_id:08X}"
            ))?;
        color_eyre::eyre::ensure!(
            reference.position[..2]
                .iter()
                .all(|value| value.is_finite()),
            "reference {reference_id:08X} has non-finite XY position"
        );
        let position_grid = (
            (reference.position[0] / CELL_SIZE).floor() as i32,
            (reference.position[1] / CELL_SIZE).floor() as i32,
        );
        color_eyre::eyre::ensure!(
            position_grid == (grid_x, grid_y),
            "reference {reference_id:08X} position grid {position_grid:?} differs from requested ({grid_x}, {grid_y})"
        );
        rows.push((reference, inputs));
    }
    let mut references = query.resolve_enable_states(connection, rows)?;
    finish_references(connection, query.has_doors, &mut references, tables)?;
    Ok(references)
}

fn finish_references(
    connection: &Connection,
    has_doors: bool,
    references: &mut [ReferenceRow],
    tables: &mut DoorTables,
) -> Result<()> {
    // What a door of this cell needs beyond its own row: which way it faces, from the links that
    // lead back to it (the doors of this cell are the rows a `door_links` row's
    // `destination_ref_id` can name), and its own doorway and its destination's, from the doorway
    // table. Both are read once for the whole run.
    if has_doors && references.iter().any(|reference| reference.door.is_some()) {
        let links = tables.links.get_or_build(connection)?;
        let doorways = tables.doorways.get_or_build(connection)?;
        for reference in references {
            if let Some(door) = reference.door.as_mut() {
                door.return_arrival = links.get(&reference.form_id).copied();
                door.destination_doorway = doorways.get(&door.destination_ref_id).cloned();
            }
            if reference.door.is_some() {
                reference.doorway = doorways.get(&reference.form_id).cloned();
            }
        }
    }
    Ok(())
}

fn map_enable_inputs(
    row: &rusqlite::Row<'_>,
    start: usize,
) -> rusqlite::Result<InitialEnableInputs> {
    let parent_id: Option<u32> = row.get(start + 1)?;
    let parent_flags: Option<u32> = row.get(start + 2)?;
    if parent_id.is_some_and(|id| id != 0) && parent_flags.is_none() {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            start + 2,
            rusqlite::types::Type::Null,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "enable parent has no XESP flags",
            )),
        ));
    }
    Ok(InitialEnableInputs {
        header_flags: row.get(start)?,
        parent_id,
        parent_flags: parent_flags.unwrap_or(0),
    })
}

fn map_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<(ReferenceRow, InitialEnableInputs)> {
    let base_record_type: Option<String> = row.get(18)?;
    // The `lights` row is present exactly when the join found one and it has the radius the design
    // makes `NOT NULL`; anything else is a reference this database cannot light.
    let radius: Option<f32> = row.get(19)?;
    let light = match radius {
        Some(radius) => Some(LightRow {
            radius,
            color: [row.get(20)?, row.get(21)?, row.get(22)?],
            flags: row.get(23)?,
            falloff: row.get(LIGHT_DETAIL_START)?,
            fade: row.get(LIGHT_DETAIL_START + 1)?,
        }),
        None => None,
    };
    let destination_ref_id: Option<u32> = row.get(25)?;
    let door = match destination_ref_id {
        Some(destination_ref_id) => {
            // A row whose arrival point is NULL (a link the converter could not finish) is no
            // link; it must not fail the cell.
            let mut arrival = [None::<f32>; 6];
            for (slot, column) in arrival.iter_mut().zip(28..34) {
                *slot = row.get(column)?;
            }
            if arrival.iter().all(Option::is_some) {
                let value = |index: usize| arrival[index].unwrap_or_default();
                Some(DoorLinkRow {
                    destination_ref_id,
                    destination_cell_id: row.get(26)?,
                    destination_worldspace_id: row.get(27)?,
                    arrival_position: [value(0), value(1), value(2)],
                    arrival_rotation: [value(3), value(4), value(5)],
                    label: door_label(
                        row.get::<_, Option<String>>(DOOR_LABEL_INDEX)?
                            .unwrap_or_default(),
                    ),
                    // Both filled in after the query: the return link from the links that lead
                    // back into this cell, the destination doorway from the doorway table.
                    return_arrival: None,
                    destination_doorway: None,
                })
            } else {
                bevy::log::debug!(
                    reference = format_args!("{:08X}", row.get::<_, u32>(0)?),
                    "door_links row has a NULL arrival position or rotation; the reference is no load door"
                );
                None
            }
        }
        None => None,
    };
    let emittance = match row.get::<_, Option<u8>>(34)? {
        Some(red) => Some([red, row.get(35)?, row.get(36)?]),
        None => None,
    };
    Ok((
        ReferenceRow {
            form_id: row.get(0)?,
            initially_enabled: true,
            cell_id: row.get(1)?,
            base_form_id: row.get(2)?,
            base_record_type,
            model_path: row.get(3)?,
            position: [row.get(4)?, row.get(5)?, row.get(6)?],
            rotation: [row.get(7)?, row.get(8)?, row.get(9)?],
            scale: row.get(10)?,
            bounds_min: [row.get(11)?, row.get(12)?, row.get(13)?],
            bounds_max: [row.get(14)?, row.get(15)?, row.get(16)?],
            bounds_valid: row.get(17)?,
            door,
            doorway: None,
            light,
            light_radius_override: row.get(24)?,
            emittance,
            auto_load: row.get(AUTO_LOAD_INDEX)?,
        },
        map_enable_inputs(row, REFERENCE_ENABLE_START)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_mode_database_ignores_populated_baked_inventory() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE schema_info(version INTEGER); INSERT INTO schema_info VALUES({});
             CREATE TABLE lod_chunks(worldspace_id INTEGER); INSERT INTO lod_chunks VALUES(7);",
                shared::WORLD_DATABASE_SCHEMA_VERSION,
            ))
            .unwrap();
        let cluster =
            WorldDatabase::open_with_lod(&path, WorldDatabaseFeatures::default(), false).unwrap();
        assert!(!cluster.has_lod_for(7));
        let baked = WorldDatabase::open(&path).unwrap();
        assert!(baked.has_lod_for(7));
    }

    #[test]
    fn terrain_requirements_ignore_empty_cells_and_preserve_world_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER);
            CREATE TABLE land(cell_id INTEGER PRIMARY KEY);
            INSERT INTO cells VALUES(1,7,-3,-2),(2,7,-2,-2),(3,8,-3,-2);
            INSERT INTO land VALUES(1),(3);").unwrap();
        drop(connection);
        let required = read_terrain_cells(&path).unwrap();
        assert_eq!(required[&7], HashSet::from([[-3, -2]]));
        assert_eq!(required[&8], HashSet::from([[-3, -2]]));
        assert!(!required[&7].contains(&[-2, -2]));
        // A source hole at [-2,-2] must not be mistaken for missing LAND.
        assert_eq!(required[&7].len(), 1);
        assert!(!required.contains_key(&9));
    }

    #[test]
    fn lod_query_classification_preserves_transient_sqlite_causes_through_context() {
        for code in [
            rusqlite::ffi::SQLITE_BUSY,
            rusqlite::ffi::SQLITE_LOCKED,
            rusqlite::ffi::SQLITE_IOERR,
        ] {
            let error = rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None);
            let wrapped = color_eyre::Report::from(error).wrap_err("query context");
            assert!(LodQueryFailure::from_error(wrapped).transient);
        }
        assert!(
            !LodQueryFailure::from_error(color_eyre::eyre::eyre!("invalid LOD path")).transient
        );
        assert!(!LodQueryFailure::from_error(rusqlite::Error::InvalidQuery.into()).transient);
    }

    /// The three-argument shape the tests below were written against: every one of them asks for a
    /// whole cell. A test about the terrain ring calls [`super::load_cell`] with its own detail.
    ///
    /// The return-link cache is fresh on every call, so a test that changes `door_links` between
    /// two loads sees the table as it is: one cache for a whole run is what [`ReturnLinks`] is for,
    /// which `the_return_links_are_read_once_and_kept` checks on purpose.
    fn load_cell(connection: &Connection, generation: u64, key: CellKey) -> Result<CellPayload> {
        super::load_cell(
            connection,
            &mut ReferenceQuery::for_connection(connection, WorldDatabaseFeatures::default())?,
            generation,
            key,
            CellDetail::Full,
            &mut DoorTables::default(),
        )
    }

    /// [`super::load_cell`] for a whole cell with a query the test keeps across loads (the enable
    /// state resolver lives on it) and a fresh door-table cache.
    fn load_cell_with(
        connection: &Connection,
        query: &mut ReferenceQuery,
        generation: u64,
        key: CellKey,
    ) -> Result<CellPayload> {
        super::load_cell(
            connection,
            query,
            generation,
            key,
            CellDetail::Full,
            &mut DoorTables::default(),
        )
    }

    /// The features a fixture database stands for: it lists `mudcrab.door_links` exactly when it
    /// holds the table, which is what a converted data set's manifest says about the real one.
    fn fixture_features(connection: &Connection) -> WorldDatabaseFeatures {
        let found: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='door_links'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        WorldDatabaseFeatures {
            door_links: if found > 0 {
                DoorLinksAccess::LegacyExtension
            } else {
                DoorLinksAccess::Core
            },
        }
    }

    fn fixture(connection: &Connection) {
        connection
            .execute_batch(&format!(
                r#"CREATE TABLE schema_info(version INTEGER NOT NULL);
                INSERT INTO schema_info VALUES({version});
                CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,interior_name TEXT);
                CREATE TABLE land(cell_id INTEGER PRIMARY KEY);
                CREATE TABLE statics(id INTEGER PRIMARY KEY,editor_id TEXT,model_path TEXT,bounds_min_x REAL,bounds_min_y REAL,bounds_min_z REAL,bounds_max_x REAL,bounds_max_y REAL,bounds_max_z REAL,bounds_valid INTEGER NOT NULL);
                CREATE TABLE "references"(id INTEGER PRIMARY KEY,cell_id INTEGER,base_form_id INTEGER,pos_x REAL,pos_y REAL,pos_z REAL,rot_x REAL,rot_y REAL,rot_z REAL,scale REAL);
                CREATE VIRTUAL TABLE exterior_spatial USING rtree(id,minX,maxX,minY,maxY,minZ,maxZ,+cell_id,+worldspace_id);
                INSERT INTO cells VALUES(10,60,2,-3,NULL);
                INSERT INTO statics(id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid) VALUES(20,'architecture/wall.nif',-1,-2,-3,1,2,3,1);
                INSERT INTO "references" VALUES(30,10,20,8200,-12200,50,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(30,8200,8200,-12200,-12200,50,50,10,60);
                INSERT INTO "references" VALUES(31,99,20,8250,-12150,55,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(31,8250,8250,-12150,-12150,55,55,99,60);"#,
                version = shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
    }

    /// The schema-4 door tables, the interior cell a door leads into, and its `door_links` row.
    /// A database without any of this is the pre-door shape [`load_cell`] must still handle.
    fn door_fixture(connection: &Connection) {
        connection
            .execute_batch(
                r#"CREATE TABLE worldspaces(id INTEGER PRIMARY KEY,editor_id TEXT NOT NULL,parent_world INTEGER,flags INTEGER NOT NULL);
                CREATE TABLE door_links(ref_id INTEGER PRIMARY KEY,destination_ref_id INTEGER NOT NULL,
                    pos_x REAL NOT NULL,pos_y REAL NOT NULL,pos_z REAL NOT NULL,
                    rot_x REAL NOT NULL,rot_y REAL NOT NULL,rot_z REAL NOT NULL,
                    destination_cell_id INTEGER,destination_worldspace_id INTEGER);
                INSERT INTO cells VALUES(99,NULL,NULL,NULL,'Alftand01');
                INSERT INTO worldspaces VALUES(614,'Blackreach',60,0);
                INSERT INTO door_links VALUES(30,77,-947.038,3958.835,591.917,0,0,2.96989,99,NULL);"#,
            )
            .unwrap();
    }

    fn sparse_reference_fixture(connection: &Connection) {
        fixture(connection);
        door_fixture(connection);
        light_fixture(connection);
        connection
            .execute_batch(
                r#"CREATE TABLE records(form_id INTEGER PRIMARY KEY,record_type TEXT);
                INSERT INTO records VALUES(20,'DOOR'),(21,'LIGH');
                UPDATE statics SET editor_id='AutoLoadDoorTest' WHERE id=20;
                INSERT INTO cells VALUES(11,60,0,0,NULL),(12,614,0,0,NULL);
                UPDATE "references" SET cell_id=11 WHERE id=31;
                UPDATE door_links SET destination_ref_id=31,destination_cell_id=11,destination_worldspace_id=60 WHERE ref_id=30;
                INSERT INTO door_links VALUES(31,30,8201,-12201,51,0,0,1,10,60);
                ALTER TABLE "references" ADD COLUMN is_exterior INTEGER;
                ALTER TABLE "references" ADD COLUMN worldspace_id INTEGER;
                UPDATE "references" SET is_exterior=1,worldspace_id=60;
                INSERT INTO "references"(id,cell_id,base_form_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,scale,is_exterior,worldspace_id) VALUES
                    (55,12,20,8200,-12200,50,0,0,0,1,1,614),
                    (56,99,20,8200,-12200,50,0,0,0,1,0,NULL),
                    (57,11,20,12288,-12200,50,0,0,0,1,1,60),
                    (60,11,20,-0.25,-4096,50,0,0,0,1,1,60),
                    (61,11,20,-4096,-0.25,50,0,0,0,1,1,60),
                    (62,11,20,-4096.25,-4096,50,0,0,0,1,1,60),
                    (63,11,20,0,0,50,0,0,0,1,1,60),
                    (64,11,20,4096,-4096,50,0,0,0,1,1,60);"#,
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO \"references\"(id,cell_id,base_form_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,scale,is_exterior,worldspace_id) \
                 VALUES(58,11,20,?1,-12200,50,0,0,0,1,1,60)",
                [f64::INFINITY],
            )
            .unwrap();
    }

    fn enable_columns(connection: &Connection) {
        connection
            .execute_batch(
                "ALTER TABLE \"references\" ADD COLUMN header_flags INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE \"references\" ADD COLUMN enable_parent_id INTEGER;
             ALTER TABLE \"references\" ADD COLUMN enable_parent_flags INTEGER;",
            )
            .unwrap();
    }

    fn sparse_key(worldspace_id: u32, grid_x: i32, grid_y: i32) -> CellKey {
        CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        }
    }

    fn sparse_rows(
        connection: &Connection,
        key: CellKey,
        ids: impl IntoIterator<Item = u32>,
    ) -> Result<Vec<ReferenceRow>> {
        load_references(
            connection,
            &mut ReferenceQuery::for_connection(connection, fixture_features(connection))?,
            key,
            &ids.into_iter().collect(),
            &mut DoorTables::default(),
        )
    }

    #[test]
    fn sparse_references_select_only_sorted_ids_and_use_placed_world_and_position() {
        let connection = Connection::open_in_memory().unwrap();
        sparse_reference_fixture(&connection);
        // The exact-ID path does not need a cell-wide spatial query, even for a persistent
        // reference whose owning CELL disagrees with its position grid.
        connection
            .execute("DROP TABLE exterior_spatial", [])
            .unwrap();
        let rows = sparse_rows(&connection, sparse_key(60, 2, -3), [40, 30]).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.form_id).collect::<Vec<_>>(),
            [30, 40]
        );
        let persistent = sparse_rows(&connection, sparse_key(60, 2, -3), [31]).unwrap();
        assert_eq!(persistent[0].cell_id, 11, "authored CELL stays intact");
        assert!(sparse_rows(&connection, sparse_key(60, 0, 0), [31]).is_err());
        let child = sparse_rows(&connection, sparse_key(614, 2, -3), [55]).unwrap();
        assert_eq!(child[0].cell_id, 12);
        assert!(sparse_rows(&connection, sparse_key(60, 2, -3), [55]).is_err());
    }

    #[test]
    fn sparse_references_preserve_all_near_projection_and_door_enrichment() {
        let connection = Connection::open_in_memory().unwrap();
        sparse_reference_fixture(&connection);
        let key = sparse_key(60, 2, -3);
        let near = load_cell(&connection, 1, key).unwrap();
        let sparse = sparse_rows(&connection, key, [30, 40]).unwrap();
        assert_eq!(near.references.len(), 3);
        for actual in &sparse {
            let expected = near
                .references
                .iter()
                .find(|row| row.form_id == actual.form_id)
                .unwrap();
            assert_eq!(actual.form_id, expected.form_id);
            assert_eq!(actual.cell_id, expected.cell_id);
            assert_eq!(actual.base_form_id, expected.base_form_id);
            assert_eq!(actual.base_record_type, expected.base_record_type);
            assert_eq!(actual.model_path, expected.model_path);
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.rotation, expected.rotation);
            assert_eq!(actual.scale, expected.scale);
            assert_eq!(actual.bounds_min, expected.bounds_min);
            assert_eq!(actual.bounds_max, expected.bounds_max);
            assert_eq!(actual.bounds_valid, expected.bounds_valid);
            assert_eq!(actual.door, expected.door);
            assert_eq!(actual.doorway, expected.doorway);
            assert_eq!(actual.light, expected.light);
            assert_eq!(actual.light_radius_override, expected.light_radius_override);
            assert_eq!(actual.auto_load, expected.auto_load);
        }
        let door = sparse[0].door.as_ref().unwrap();
        assert!(door.return_arrival.is_some());
        assert!(door.destination_doorway.is_some());
        assert!(sparse[0].doorway.is_some());
        assert!(sparse[0].auto_load);
        assert!(sparse[1].light.is_some());
        assert_eq!(sparse[1].light_radius_override, Some(850.8));
    }

    #[test]
    fn sparse_references_reject_the_whole_batch_for_any_invalid_selected_id() {
        let connection = Connection::open_in_memory().unwrap();
        sparse_reference_fixture(&connection);
        for (id, reason) in [
            (999, "missing or not exterior"),
            (55, "missing or not exterior"),
            (56, "missing or not exterior"),
            (57, "position grid"),
            (58, "non-finite XY"),
        ] {
            let error = sparse_rows(&connection, sparse_key(60, 2, -3), [30, id])
                .unwrap_err()
                .to_string();
            assert!(error.contains(&format!("{id:08X}")), "{error}");
            assert!(error.contains(reason), "{error}");
        }
    }

    #[test]
    fn sparse_references_use_floor_at_negative_and_exact_cell_boundaries() {
        let connection = Connection::open_in_memory().unwrap();
        sparse_reference_fixture(&connection);
        for (id, x, y) in [
            (60, -1, -1),
            (61, -1, -1),
            (62, -2, -1),
            (63, 0, 0),
            (64, 1, -1),
        ] {
            let rows = sparse_rows(&connection, sparse_key(60, x, y), [id]).unwrap();
            assert_eq!(rows[0].form_id, id);
            assert!(sparse_rows(&connection, sparse_key(60, x + 1, y), [id]).is_err());
        }
    }

    #[test]
    fn empty_sparse_requests_do_not_require_new_columns_or_read_a_cell() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        assert_eq!(
            load_cell(&connection, 1, sparse_key(60, 2, -3))
                .unwrap()
                .references
                .len(),
            2
        );
        // This exterior key has no owning cell, and the old fixture lacks the new placement
        // columns. Empty selection still succeeds without preparing any reference SQL.
        assert!(
            sparse_rows(&connection, sparse_key(614, 900, 900), [])
                .unwrap()
                .is_empty()
        );
        assert!(sparse_rows(&connection, sparse_key(60, 2, -3), [30]).is_err());
        assert!(sparse_rows(&connection, CellKey::Interior(10), []).is_err());
    }

    #[test]
    fn sparse_worker_responses_are_isolated_and_canceled_receivers_do_not_stop_cells() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("OWNER.txt"),
            "database sparse worker test\n",
        )
        .unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        sparse_reference_fixture(&connection);
        drop(connection);
        let key = sparse_key(60, 2, -3);
        let child_key = sparse_key(614, 2, -3);
        let (requests, request_rx) = bounded(8);
        let first = enqueue_reference_query(&requests, 7, key, BTreeSet::from([30])).unwrap();
        let second =
            enqueue_reference_query(&requests, 8, child_key, BTreeSet::from([55])).unwrap();
        let canceled = enqueue_reference_query(&requests, 9, key, BTreeSet::from([30])).unwrap();
        let failed = enqueue_reference_query(&requests, 10, key, BTreeSet::from([999])).unwrap();
        let empty = enqueue_reference_query(&requests, 11, key, BTreeSet::new()).unwrap();
        let (zero_capacity, stalled_receiver) = bounded(0);
        requests
            .send(DatabaseRequest::LoadReferences {
                generation: 99,
                key,
                reference_ids: BTreeSet::from([30]),
                responses: zero_capacity,
                queued_at: Instant::now(),
            })
            .unwrap();
        assert_eq!(first.capacity(), Some(1));
        assert_eq!(second.capacity(), Some(1));
        drop(canceled);
        requests
            .send(DatabaseRequest::Load {
                generation: 12,
                key,
                detail: CellDetail::Full,
                queued_at: Instant::now(),
            })
            .unwrap();
        requests.send(DatabaseRequest::Shutdown).unwrap();
        let (responses, response_rx) = unbounded();
        let (lod_responses, lod_rx) = unbounded();
        worker(
            path,
            WorldDatabaseFeatures {
                door_links: crate::world::database::DoorLinksAccess::LegacyExtension,
            },
            request_rx,
            responses,
            lod_responses,
        );
        for (receiver, generation, expected_key, id) in
            [(first, 7, key, 30), (second, 8, child_key, 55)]
        {
            let response = receiver.recv().unwrap();
            assert_eq!(response.generation, generation);
            assert_eq!(response.key, expected_key);
            let rows = response.result.unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].form_id, id);
            assert!(response.total_request_micros >= response.queue_wait_micros);
            assert!(response.total_request_micros >= response.query_micros);
            assert!(receiver.try_recv().is_err());
        }
        let failed = failed.recv().unwrap();
        assert_eq!(failed.generation, 10);
        assert!(failed.result.is_err());
        let empty = empty.recv().unwrap();
        assert_eq!(empty.generation, 11);
        assert!(empty.result.unwrap().is_empty());
        let ordinary = response_rx.recv().unwrap();
        assert_eq!(ordinary.generation, 12);
        assert_eq!(ordinary.result.unwrap().references.len(), 3);
        assert!(response_rx.try_recv().is_err());
        assert!(lod_rx.try_recv().is_err());
        assert!(stalled_receiver.try_recv().is_err());
    }

    #[test]
    fn sparse_requests_report_backpressure_disconnection_and_interior_keys() {
        let database = WorldDatabase::saturated_queue();
        let key = sparse_key(60, 2, -3);
        let response = database
            .request_references(1, key, BTreeSet::from([30]))
            .unwrap();
        assert_eq!(response.capacity(), Some(1));
        assert!(
            database
                .request_references(2, key, BTreeSet::from([31]))
                .unwrap_err()
                .to_string()
                .contains("request queue is full")
        );
        assert!(
            database
                .request_references(3, CellKey::Interior(10), BTreeSet::new())
                .unwrap_err()
                .to_string()
                .contains("exterior position grid")
        );
        let (requests, request_rx) = bounded(1);
        drop(request_rx);
        assert!(
            enqueue_reference_query(&requests, 4, key, BTreeSet::from([30]))
                .unwrap_err()
                .to_string()
                .contains("worker stopped")
        );
    }

    #[test]
    fn disabled_legacy_doors_are_not_read_through_core_discovery() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        let core =
            ReferenceQuery::for_connection(&connection, WorldDatabaseFeatures::default()).unwrap();
        assert!(core.has_doors);
        let denied = ReferenceQuery::for_connection(
            &connection,
            WorldDatabaseFeatures {
                door_links: DoorLinksAccess::Disabled,
            },
        )
        .unwrap();
        assert!(!denied.has_doors);
        assert!(!denied.joins.contains("door_links"));
        assert!(denied.columns.contains(ABSENT_DOOR_COLUMNS));
    }

    #[test]
    fn enable_metadata_does_not_decode_reserved_door_slots() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        enable_columns(&connection);
        connection
            .execute_batch(
                "UPDATE \"references\" SET header_flags=2048,enable_parent_id=20,enable_parent_flags=1 WHERE id=30;",
            )
            .unwrap();
        let mut query =
            ReferenceQuery::for_connection(&connection, WorldDatabaseFeatures::default()).unwrap();
        let sql = format!(
            "SELECT {} FROM \"references\" r{} WHERE r.id=30",
            query.columns, query.joins
        );
        connection
            .query_row(&sql, [], |row| {
                for index in 25..34 {
                    assert_eq!(row.get::<_, Option<i64>>(index)?, None);
                }
                for index in 34..37 {
                    assert_eq!(row.get::<_, Option<i64>>(index)?, None);
                }
                Ok(())
            })
            .unwrap();

        // Model #128's nine-column projection with distinct destination and
        // arrival values, without implementing or depending on its door tables.
        // Bounds COALESCE expressions contain commas, so replace the exact
        // suffix instead of treating the SQL text as a CSV list.
        query.columns = query.columns.replace(
            &format!(",{ABSENT_DOOR_COLUMNS},{ABSENT_EMITTANCE_COLUMNS},r.header_flags"),
            ",123,456,NULL,1.25,2.5,3.75,0.1,0.2,0.3,NULL,NULL,NULL,r.header_flags",
        );
        let sql = format!(
            "SELECT {} FROM \"references\" r{} WHERE r.id=30",
            query.columns, query.joins
        );
        connection
            .query_row(&sql, [], |row| {
                assert_eq!(row.get::<_, u32>(25)?, 123);
                assert_eq!(row.get::<_, u32>(26)?, 456);
                assert_eq!(row.get::<_, Option<u32>>(27)?, None);
                assert_eq!(row.get::<_, f32>(28)?, 1.25);
                let (_, inputs) = map_reference(row)?;
                assert_eq!(inputs.header_flags, 2048);
                assert_eq!(inputs.parent_id, Some(20));
                assert_eq!(inputs.parent_flags, 1);
                Ok(())
            })
            .unwrap();
        for key in [
            CellKey::Interior(10),
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        ] {
            let payload = load_cell_with(&connection, &mut query, 1, key).unwrap();
            let child = payload
                .references
                .iter()
                .find(|row| row.form_id == 30)
                .unwrap();
            assert!(!child.initially_enabled);
        }
    }

    #[test]
    fn initial_enable_state_uses_parents_outside_the_loaded_cell_without_models() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        enable_columns(&connection);
        connection.execute_batch(
            "INSERT INTO \"references\"(id,cell_id,base_form_id,header_flags) VALUES(100,500,9999,2048);
             INSERT INTO \"references\"(id,cell_id,base_form_id,header_flags,enable_parent_id,enable_parent_flags)
                 VALUES(200,600,9998,0,100,1);
             UPDATE \"references\" SET header_flags=2048,enable_parent_id=200,enable_parent_flags=952044802 WHERE id=30;
             UPDATE \"references\" SET enable_parent_id=30,enable_parent_flags=1 WHERE id=31;"
        ).unwrap();
        let mut query =
            ReferenceQuery::for_connection(&connection, WorldDatabaseFeatures::default()).unwrap();
        for _ in 0..2 {
            let interior =
                load_cell_with(&connection, &mut query, 1, CellKey::Interior(99)).unwrap();
            assert_eq!(interior.references.len(), 1);
            assert!(!interior.references[0].initially_enabled);
            let exterior = load_cell_with(
                &connection,
                &mut query,
                2,
                CellKey::Exterior {
                    worldspace_id: 60,
                    grid_x: 2,
                    grid_y: -3,
                },
            )
            .unwrap();
            assert_eq!(
                exterior.references.len(),
                2,
                "hidden rows remain in the payload"
            );
            assert!(
                exterior
                    .references
                    .iter()
                    .find(|row| row.form_id == 30)
                    .unwrap()
                    .initially_enabled
            );
            assert!(
                !exterior
                    .references
                    .iter()
                    .find(|row| row.form_id == 31)
                    .unwrap()
                    .initially_enabled
            );
        }
        assert!(query.enable_errors.is_empty());
    }

    #[test]
    fn missing_and_cyclic_enable_parents_remain_hidden_and_are_diagnosed_once() {
        for parent in [99, 31] {
            let connection = Connection::open_in_memory().unwrap();
            fixture(&connection);
            enable_columns(&connection);
            connection.execute(
                "UPDATE \"references\" SET enable_parent_id=?1,enable_parent_flags=1 WHERE id=30",
                [parent],
            ).unwrap();
            connection.execute_batch(
                "UPDATE \"references\" SET enable_parent_id=30,enable_parent_flags=1 WHERE id=31;"
            ).unwrap();
            let mut query =
                ReferenceQuery::for_connection(&connection, WorldDatabaseFeatures::default())
                    .unwrap();
            for _ in 0..2 {
                let payload = load_cell_with(
                    &connection,
                    &mut query,
                    1,
                    CellKey::Exterior {
                        worldspace_id: 60,
                        grid_x: 2,
                        grid_y: -3,
                    },
                )
                .unwrap();
                assert_eq!(payload.references.len(), 2);
                assert!(payload.references.iter().all(|row| !row.initially_enabled));
                assert_eq!(query.enable_errors.len(), 1);
            }
        }
    }

    #[test]
    fn legacy_reference_columns_default_enabled_but_partial_columns_fail() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        let payload = load_cell(&connection, 1, CellKey::Interior(99)).unwrap();
        assert!(payload.references.iter().all(|row| row.initially_enabled));
        connection
            .execute_batch(
                "ALTER TABLE \"references\" ADD COLUMN header_flags INTEGER NOT NULL DEFAULT 0;",
            )
            .unwrap();
        let error = load_cell(&connection, 1, CellKey::Interior(99)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("incomplete reference enable-state columns")
        );
    }

    #[test]
    fn corrupt_parent_flags_fail_the_cell_instead_of_inventing_enabled_state() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        enable_columns(&connection);
        connection.execute_batch(
            "INSERT INTO \"references\"(id,cell_id,base_form_id,header_flags) VALUES(100,500,9999,-1);
             UPDATE \"references\" SET enable_parent_id=100,enable_parent_flags=1 WHERE id=31;"
        ).unwrap();
        let error = load_cell(&connection, 1, CellKey::Interior(99)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("enable state of reference 0000001F")
        );
    }

    #[test]
    fn nonzero_enable_parent_requires_flags_in_the_database() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        enable_columns(&connection);
        connection
            .execute_batch("UPDATE \"references\" SET enable_parent_id=30 WHERE id=31;")
            .unwrap();
        let error = load_cell(&connection, 1, CellKey::Interior(99)).unwrap_err();
        assert!(format!("{error:#}").contains("enable parent has no XESP flags"));
    }

    fn lod_fixture(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE worldspaces(id INTEGER PRIMARY KEY,lod_origin_x INTEGER,lod_origin_y INTEGER);
                 CREATE TABLE lod_build(id INTEGER PRIMARY KEY,build_identity TEXT NOT NULL);
                 CREATE TABLE lod_chunks(
                    worldspace_id INTEGER NOT NULL,tier INTEGER NOT NULL,anchor_x INTEGER NOT NULL,anchor_y INTEGER NOT NULL,
                    payload_path TEXT NOT NULL,content_hash TEXT NOT NULL,
                    bounds_min_x REAL NOT NULL,bounds_min_y REAL NOT NULL,bounds_min_z REAL NOT NULL,
                    bounds_max_x REAL NOT NULL,bounds_max_y REAL NOT NULL,bounds_max_z REAL NOT NULL,
                    source_cells TEXT NOT NULL,PRIMARY KEY(worldspace_id,tier,anchor_x,anchor_y));
                 CREATE VIRTUAL TABLE lod_chunks_spatial USING rtree(
                    id,minX,maxX,minY,maxY,+worldspace_id,+tier,+anchor_x,+anchor_y);
                 INSERT INTO worldspaces VALUES(60,-8,12);",
            )
            .unwrap();
        connection
            .execute("INSERT INTO lod_build VALUES(1,?1)", ["a".repeat(64)])
            .unwrap();
        connection
            .execute(
                "INSERT INTO lod_chunks VALUES(60,4,-1,2,'lod/0000003c/4/cell_-1_2.glb',?1,\
                 -50000.0,50000.0,0.0,-40000.0,60000.0,100.0,'-12,20;-11,20')",
                ["b".repeat(64)],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lod_chunks_spatial VALUES(1,-50000.0,-40000.0,50000.0,60000.0,60,4,-1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lod_chunks VALUES(60,8,-1,2,'lod/0000003c/8/cell_-1_2.glb',?1,\
                 -50000.0,50000.0,0.0,-40000.0,60000.0,100.0,'-12,20')",
                ["c".repeat(64)],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lod_chunks_spatial VALUES(2,-50000.0,-40000.0,50000.0,60000.0,60,8,-1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lod_chunks VALUES(61,4,-1,2,'lod/0000003d/4/cell_-1_2.glb',?1,\
                 -50000.0,50000.0,0.0,-40000.0,60000.0,100.0,'-12,20')",
                ["d".repeat(64)],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lod_chunks_spatial VALUES(3,-50000.0,-40000.0,50000.0,60000.0,61,4,-1,2)",
                [],
            )
            .unwrap();
    }

    fn lod_query(min: [f64; 2], max: [f64; 2]) -> LodChunkQuery {
        LodChunkQuery {
            worldspace_id: 60,
            tier: LodTier::Tier4,
            bounds_min: min,
            bounds_max: max,
        }
    }

    /// Builds a database and conversion manifest with the current combined LOD identities.
    fn lod_build_contract_fixture(path: &Path, chunks: u32, identity: &str) {
        std::fs::write(
            path.parent().unwrap().join("conversion-manifest.json"),
            serde_json::to_vec(
                &serde_json::json!({"schema_version": shared::LOD_CONVERTER_SCHEMA_VERSION}),
            )
            .unwrap(),
        )
        .unwrap();
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_info(version INTEGER NOT NULL);
                 CREATE TABLE lod_chunks(id INTEGER PRIMARY KEY);
                 CREATE TABLE lod_build(id INTEGER PRIMARY KEY,build_identity TEXT NOT NULL);",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO schema_info VALUES(?1)",
                [shared::WORLD_DATABASE_SCHEMA_VERSION],
            )
            .unwrap();
        for id in 0..chunks {
            connection
                .execute("INSERT INTO lod_chunks(id) VALUES(?1)", [id])
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO lod_build(id,build_identity) VALUES(1,?1)",
                [identity],
            )
            .unwrap();
    }

    #[test]
    #[ignore = "portal: the runtime loads only the portal's own world database schema"]
    fn legacy_databases_render_without_lod_but_cannot_advertise_lod() {
        for version in [3, 4, 5] {
            let directory = tempfile::tempdir().unwrap();
            let connection = Connection::open(directory.path().join("skyrim_world.db")).unwrap();
            connection.execute_batch(&format!(
                "CREATE TABLE schema_info(version INTEGER NOT NULL); INSERT INTO schema_info VALUES({version});"
            )).unwrap();
            if version == 5 {
                connection.execute_batch("CREATE TABLE grass_types(id INTEGER PRIMARY KEY); CREATE TABLE landscape_texture_grasses(ltex_id INTEGER,gras_id INTEGER); INSERT INTO grass_types VALUES(1); INSERT INTO landscape_texture_grasses VALUES(2,1);").unwrap();
            }
            assert!(
                load_lod_chunks(&connection, lod_query([0.0, 0.0], [1.0, 1.0]))
                    .unwrap()
                    .is_empty()
            );
            validate_lod_build_contract(directory.path(), 20).unwrap();
            std::fs::write(directory.path().join("lod-manifest.json"), b"{}").unwrap();
            assert!(validate_lod_build_contract(directory.path(), 20).is_err());
        }
    }

    /// Current database schemas require LOD tables even when no LOD manifest is present.
    #[test]
    fn current_database_requires_lod_tables_even_without_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("skyrim_world.db")).unwrap();
        connection
            .execute_batch("CREATE TABLE schema_info(version INTEGER NOT NULL);")
            .unwrap();
        connection
            .execute(
                "INSERT INTO schema_info VALUES(?1)",
                [shared::WORLD_DATABASE_SCHEMA_VERSION],
            )
            .unwrap();
        assert!(load_lod_chunks(&connection, lod_query([0.0, 0.0], [1.0, 1.0])).is_err());
        assert!(validate_lod_build_contract(directory.path(), 20).is_err());
    }

    #[test]
    fn combined_lod_requires_exact_latest_producer_and_database() {
        let directory = tempfile::tempdir().unwrap();
        let identity = "a".repeat(64);
        let database = directory.path().join("skyrim_world.db");
        lod_build_contract_fixture(&database, 1, &identity);
        for (producer, world, accepted) in [
            (
                shared::LOD_CONVERTER_SCHEMA_VERSION,
                shared::WORLD_DATABASE_SCHEMA_VERSION,
                true,
            ),
            (25, shared::WORLD_DATABASE_SCHEMA_VERSION, false),
            (shared::LOD_CONVERTER_SCHEMA_VERSION, 8, false),
            (25, 8, false),
            (24, shared::WORLD_DATABASE_SCHEMA_VERSION, false),
            (shared::LOD_CONVERTER_SCHEMA_VERSION, 7, false),
            (24, 7, false),
            (24, 6, false),
            (23, 7, false),
            (22, 7, false),
            (21, 7, false),
            (20, 6, false),
            (19, 7, false),
            (16, 7, false),
        ] {
            std::fs::write(
                directory.path().join("conversion-manifest.json"),
                serde_json::to_vec(&serde_json::json!({"schema_version": producer})).unwrap(),
            )
            .unwrap();
            Connection::open(&database)
                .unwrap()
                .execute("UPDATE schema_info SET version=?1", [world])
                .unwrap();
            std::fs::write(
                directory.path().join("lod-manifest.json"),
                serde_json::to_vec(&serde_json::json!({
                    "build_identity": identity, "converter_schema": producer,
                    "world_database_schema": world, "chunks": 1,
                    "land_texture_repeats_per_cell": shared::LAND_TEXTURE_REPEATS_PER_CELL,
                }))
                .unwrap(),
            )
            .unwrap();
            assert_eq!(
                validate_lod_build_contract(directory.path(), producer).is_ok(),
                accepted,
                "producer={producer} world={world}"
            );
        }
        Connection::open(&database)
            .unwrap()
            .execute(
                "UPDATE schema_info SET version=?1",
                [shared::WORLD_DATABASE_SCHEMA_VERSION],
            )
            .unwrap();
        std::fs::write(
            directory.path().join("lod-manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "build_identity": identity,
                "converter_schema": shared::LOD_CONVERTER_SCHEMA_VERSION,
                "world_database_schema": shared::WORLD_DATABASE_SCHEMA_VERSION,
                "chunks": 1, "land_texture_repeats_per_cell": shared::LAND_TEXTURE_REPEATS_PER_CELL,
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("conversion-manifest.json"),
            br#"{"schema_version":20}"#,
        )
        .unwrap();
        assert!(validate_lod_build_contract(directory.path(), 21).is_err());
    }

    #[test]
    fn worlds_without_compiled_chunks_do_not_require_lod_origins() {
        let connection = Connection::open_in_memory().unwrap();
        lod_fixture(&connection);
        let mut query = lod_query([0.0, 0.0], [1.0, 1.0]);
        query.worldspace_id = 99;
        assert!(load_lod_chunks(&connection, query).unwrap().is_empty());
    }

    #[test]
    fn full_database_queue_rejects_lod_queries_without_blocking() {
        let (requests, _receiver) = bounded(1);
        requests.send(DatabaseRequest::Shutdown).unwrap();
        let error = enqueue_lod_query(&requests, 1, lod_query([0.0, 0.0], [1.0, 1.0]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("queue is full"), "{error}");
    }

    #[test]
    fn full_database_queue_rejects_cell_intake_without_blocking() {
        let (database, requests, _responses) = WorldDatabase::channel_fixture(1);
        let request = || DatabaseRequest::Load {
            generation: 1,
            key: CellKey::Interior(1),
            detail: CellDetail::Full,
            queued_at: Instant::now(),
        };
        database.try_request(request()).unwrap();
        let error = database.try_request(request()).unwrap_err().to_string();
        assert!(error.contains("queue is full"), "{error}");
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn runtime_lod_contract_rejects_missing_mixed_and_incomplete_manifests() {
        let directory = tempfile::tempdir().unwrap();
        let identity = "a".repeat(64);
        let database = directory.path().join("skyrim_world.db");
        lod_build_contract_fixture(&database, 1, &identity);
        let manifest_path = directory.path().join("lod-manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&serde_json::json!({
                "build_identity": identity,
                "converter_schema": shared::LOD_CONVERTER_SCHEMA_VERSION,
                "land_texture_repeats_per_cell": shared::LAND_TEXTURE_REPEATS_PER_CELL,
                "world_database_schema": shared::WORLD_DATABASE_SCHEMA_VERSION,
                "chunks": 1,
            }))
            .unwrap(),
        )
        .unwrap();
        validate_lod_build_contract(directory.path(), shared::LOD_CONVERTER_SCHEMA_VERSION)
            .unwrap();
        let current: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        let mut stale = current.clone();
        stale["land_texture_repeats_per_cell"] = serde_json::json!(8.0);
        std::fs::write(&manifest_path, serde_json::to_vec(&stale).unwrap()).unwrap();
        assert!(
            validate_lod_build_contract(directory.path(), shared::LOD_CONVERTER_SCHEMA_VERSION)
                .unwrap_err()
                .to_string()
                .contains("texture scale is stale")
        );
        std::fs::write(&manifest_path, serde_json::to_vec(&current).unwrap()).unwrap();

        let connection = Connection::open(&database).unwrap();
        connection
            .execute(
                "UPDATE lod_build SET build_identity=?1 WHERE id=1",
                ["b".repeat(64)],
            )
            .unwrap();
        drop(connection);
        let error =
            validate_lod_build_contract(directory.path(), shared::LOD_CONVERTER_SCHEMA_VERSION)
                .unwrap_err()
                .to_string();
        assert!(error.contains("identities do not match"), "{error}");

        std::fs::remove_file(&manifest_path).unwrap();
        let error =
            validate_lod_build_contract(directory.path(), shared::LOD_CONVERTER_SCHEMA_VERSION)
                .unwrap_err()
                .to_string();
        assert!(error.contains("lod-manifest.json is missing"), "{error}");
    }

    #[test]
    fn runtime_lod_contract_allows_a_world_with_no_compiled_chunks() {
        let directory = tempfile::tempdir().unwrap();
        lod_build_contract_fixture(
            &directory.path().join("skyrim_world.db"),
            0,
            &"a".repeat(64),
        );
        validate_lod_build_contract(directory.path(), 16).unwrap();
    }

    #[test]
    fn loads_exterior_cell_through_spatial_index() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        let payload = load_cell(
            &connection,
            9,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        assert_eq!(payload.generation, 9);
        assert_eq!(payload.cell_id, 10);
        assert_eq!(payload.references.len(), 2);
        assert!(
            payload
                .references
                .iter()
                .any(|reference| reference.cell_id == 99)
        );
        assert_eq!(
            payload.references[0].model_path.as_deref(),
            Some("architecture/wall.nif")
        );
        assert_eq!(payload.references[0].bounds_max, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn legacy_world6_lod_metadata_remains_queryable() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE schema_info(version INTEGER NOT NULL); INSERT INTO schema_info VALUES(6);").unwrap();
        lod_fixture(&connection);
        let chunks = load_lod_chunks(
            &connection,
            lod_query([-46000.0, 54000.0], [-45000.0, 55000.0]),
        )
        .unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].origin, LodOrigin::new(-8, 12));
        assert_eq!(chunks[0].source_cells, vec![[-12, 20], [-11, 20]]);
    }

    #[test]
    fn lod_query_uses_world_space_overlap_and_returns_validated_metadata() {
        let connection = Connection::open_in_memory().unwrap();
        lod_fixture(&connection);

        let chunks = load_lod_chunks(
            &connection,
            lod_query([-46000.0, 54000.0], [-45000.0, 55000.0]),
        )
        .unwrap();
        assert_eq!(chunks.len(), 1);
        let chunk = &chunks[0];
        assert_eq!(
            chunk.key,
            ChunkKey::new(60, LodTier::Tier4, ChunkAnchor::new(-1, 2))
        );
        assert_eq!(chunk.payload_path, "lod/0000003c/4/cell_-1_2.glb");
        assert_eq!(chunk.content_hash, "b".repeat(64));
        assert_eq!(chunk.build_identity, "a".repeat(64));
        assert_eq!(chunk.origin, LodOrigin::new(-8, 12));
        assert_eq!(chunk.bounds.min, [-50000.0, 50000.0, 0.0]);
        assert_eq!(chunk.bounds.max, [-40000.0, 60000.0, 100.0]);
        assert_eq!(chunk.source_cells, vec![[-12, 20], [-11, 20]]);

        let misses = load_lod_chunks(&connection, lod_query([0.0, 0.0], [10.0, 10.0])).unwrap();
        assert!(misses.is_empty());
    }

    #[test]
    fn lod_query_rejects_malformed_tier_path_bounds_and_query_aabb() {
        assert!(lod_tier_from_database(3).is_err());

        let connection = Connection::open_in_memory().unwrap();
        lod_fixture(&connection);
        connection
            .execute(
                "UPDATE lod_chunks SET payload_path='../outside.glb' WHERE worldspace_id=60",
                [],
            )
            .unwrap();
        let error = load_lod_chunks(
            &connection,
            lod_query([-46000.0, 54000.0], [-45000.0, 55000.0]),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("unsafe or non-canonical payload path"),
            "{error}"
        );

        connection
            .execute(
                "UPDATE lod_chunks SET payload_path='lod/0000003c/4/cell_-1_2.glb',bounds_max_x=-60000.0 \
                 WHERE worldspace_id=60",
                [],
            )
            .unwrap();
        let error = load_lod_chunks(
            &connection,
            lod_query([-46000.0, 54000.0], [-45000.0, 55000.0]),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("unordered world-space bounds"), "{error}");

        let error = load_lod_chunks(&connection, lod_query([f64::NAN, 0.0], [1.0, 1.0]))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("non-finite or unordered X bounds"),
            "{error}"
        );
    }

    #[test]
    fn lod_query_rejects_missing_origin_and_invalid_source_cells() {
        let connection = Connection::open_in_memory().unwrap();
        lod_fixture(&connection);
        connection
            .execute("UPDATE worldspaces SET lod_origin_x=NULL WHERE id=60", [])
            .unwrap();
        let error = load_lod_chunks(
            &connection,
            lod_query([-46000.0, 54000.0], [-45000.0, 55000.0]),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("no valid X origin"), "{error}");

        connection
            .execute("UPDATE worldspaces SET lod_origin_x=-8 WHERE id=60", [])
            .unwrap();
        connection
            .execute(
                "UPDATE lod_chunks SET source_cells='not-a-cell' WHERE worldspace_id=60",
                [],
            )
            .unwrap();
        let error = load_lod_chunks(
            &connection,
            lod_query([-46000.0, 54000.0], [-45000.0, 55000.0]),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("invalid source_cells metadata"), "{error}");
    }

    #[test]
    fn rejects_traversal_and_rooted_texture_paths() {
        assert_eq!(
            converted_texture_path("textures/../../secrets.dds".to_owned()),
            None
        );
        assert_eq!(
            converted_texture_path("textures//etc/passwd".to_owned()),
            None
        );
        assert_eq!(
            converted_texture_path(r"textures\..\..\secrets.dds".to_owned()),
            None
        );
        // Windows treats a drive-prefixed path as absolute (and `PathBuf::join`
        // would let it replace the base path entirely); Rust's path parsing is
        // OS-native, so this case only bites on the Windows target this engine
        // ships for.
        #[cfg(windows)]
        assert_eq!(
            converted_texture_path("textures/C:/Windows/evil.dds".to_owned()),
            None
        );
        // A plain relative path is unaffected.
        assert_eq!(
            converted_texture_path("textures/land/grass.dds".to_owned()),
            Some("textures/land/grass.ktx2".to_owned())
        );
    }

    #[test]
    fn base_record_type_distinguishes_fixed_static_from_movable_model() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        connection
            .execute_batch(
                "CREATE TABLE records(form_id INTEGER PRIMARY KEY,record_type TEXT NOT NULL);
                 INSERT INTO records VALUES(20,'STAT');
                 INSERT INTO statics(id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid) VALUES(22,'clutter/barrel.nif',-1,-1,-1,1,1,1,1);
                 INSERT INTO records VALUES(22,'MISC');
                 INSERT INTO \"references\" VALUES(40,10,22,8250,-12150,55,0,0,0,1);
                 INSERT INTO exterior_spatial VALUES(40,8250,8250,-12150,-12150,55,55,10,60);",
            )
            .unwrap();
        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        let fixed = payload.references.iter().find(|r| r.form_id == 30).unwrap();
        let movable = payload.references.iter().find(|r| r.form_id == 40).unwrap();
        assert_eq!(fixed.base_record_type.as_deref(), Some("STAT"));
        assert_eq!(movable.base_record_type.as_deref(), Some("MISC"));
    }

    #[test]
    fn catalog_rewrites_landscape_texture_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT); CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER); CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT); INSERT INTO texture_sets VALUES(2,'Textures\\Landscape\\Tundra02.DDS'); INSERT INTO landscape_textures VALUES(1,2); INSERT INTO waters VALUES(9,'textures/water/flow.dds');",
            )
            .unwrap();
        drop(connection);
        let catalog = AssetCatalog::open(&path).unwrap();
        assert_eq!(
            catalog.landscape_diffuse(1),
            Some("textures/landscape/tundra02.ktx2")
        );
        assert_eq!(catalog.water_flow(9), Some("textures/water/flow.ktx2"));
        // This fixture's `waters` table predates the fresnel/reflectivity columns entirely, the
        // same shape a database converted before this change has; the catalog must tolerate it
        // rather than fail to open.
        assert_eq!(catalog.water_colors(9), None);
    }

    #[test]
    fn catalog_returns_water_colors_and_factors() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT); \
                 CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER); \
                 CREATE TABLE waters(id INTEGER PRIMARY KEY,shallow_color INTEGER,deep_color INTEGER,reflection_color INTEGER,fresnel REAL,reflectivity REAL,flow_normal_path TEXT);",
            )
            .unwrap();
        let shallow = u32::from_le_bytes([38, 39, 24, 0]);
        let deep = u32::from_le_bytes([5, 14, 18, 0]);
        let reflection = u32::from_le_bytes([119, 140, 157, 0]);
        connection
            .execute(
                "INSERT INTO waters(id,shallow_color,deep_color,reflection_color,fresnel,reflectivity,flow_normal_path) VALUES (?1,?2,?3,?4,?5,?6,NULL)",
                params![18u32, shallow, deep, reflection, 0.10f32, 0.8f32],
            )
            .unwrap();
        drop(connection);
        let catalog = AssetCatalog::open(&path).unwrap();
        assert_eq!(
            catalog.water_colors(18),
            Some(WaterColors {
                shallow: [38, 39, 24],
                deep: [5, 14, 18],
                reflection: [119, 140, 157],
                fresnel: 0.10,
                reflectivity: 0.8,
            })
        );
    }

    #[test]
    fn texture_paths_that_could_leave_the_textures_folder_are_refused() {
        for path in [
            "textures/../../secret.dds",
            "..\\..\\secret.dds",
            "textures/land/../../../secret.dds",
            "/etc/secret.dds",
            "C:/Windows/secret.dds",
            "C:secret.dds",
            "textures//grass.dds",
            "textures/./grass.dds",
            "embedded://grass.dds",
            "textures/grass.dds:stream",
            "",
            "textures/",
        ] {
            assert_eq!(converted_texture_path(path.to_owned()), None, "{path}");
        }
        assert_eq!(
            converted_texture_path("Textures\\Land\\Grass01.dds".to_owned()).as_deref(),
            Some("textures/land/grass01.ktx2")
        );
        assert_eq!(
            converted_texture_path("land/grass..old.dds".to_owned()).as_deref(),
            Some("textures/land/grass..old.ktx2")
        );
    }

    #[test]
    fn catalog_finds_landscape_normal_maps_and_tolerates_their_absence() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT,normal_path TEXT); CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER); CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT); INSERT INTO texture_sets VALUES(2,'textures/land/snow.dds','textures/land/snow_n.dds'); INSERT INTO texture_sets VALUES(3,'textures/land/dirt.dds',''); INSERT INTO landscape_textures VALUES(1,2); INSERT INTO landscape_textures VALUES(4,3);",
            )
            .unwrap();
        drop(connection);
        let catalog = AssetCatalog::open(&path).unwrap();
        assert_eq!(
            catalog.landscape_normal(1),
            Some("textures/land/snow_n.ktx2")
        );
        assert_eq!(
            catalog.landscape_normal(4),
            None,
            "an empty normal path is no normal map"
        );

        // A database whose texture sets have no normal_path column at all still opens; the
        // terrain then draws with its geometric normal.
        let older = directory.path().join("older.db");
        let connection = Connection::open(&older).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT); CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER); CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT); INSERT INTO texture_sets VALUES(2,'textures/land/grass.dds'); INSERT INTO landscape_textures VALUES(1,2);",
            )
            .unwrap();
        drop(connection);
        let catalog = AssetCatalog::open(&older).unwrap();
        assert_eq!(
            catalog.landscape_diffuse(1),
            Some("textures/land/grass.ktx2")
        );
        assert_eq!(catalog.landscape_normal(1), None);
    }

    /// The terrain ring asks for the cell's landscape and nothing else: the cell id is what the
    /// cell cache is keyed by, and the references - which are most of a cell's cost - are not
    /// queried at all.
    #[test]
    fn loads_a_terrain_only_exterior_cell_without_its_references() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        auto_load_fixture(&connection);
        light_fixture(&connection);
        let key = CellKey::Exterior {
            worldspace_id: 60,
            grid_x: 2,
            grid_y: -3,
        };

        let terrain_only = super::load_cell(
            &connection,
            &mut ReferenceQuery::for_connection(&connection, fixture_features(&connection))
                .unwrap(),
            7,
            key,
            CellDetail::Terrain,
            &mut DoorTables::default(),
        )
        .unwrap();
        assert_eq!(terrain_only.generation, 7);
        assert_eq!(terrain_only.cell_id, 10);
        assert_eq!(terrain_only.key, key);
        assert!(
            terrain_only.references.is_empty(),
            "a terrain-only cell carries no references, lights or doors to spawn"
        );

        // The same cell at full detail is the one the ring is a cheaper version of.
        let full = load_cell(&connection, 7, key).unwrap();
        assert_eq!(full.cell_id, terrain_only.cell_id);
        assert!(
            !full.references.is_empty(),
            "the fixture's cell has references, so an empty list above means they were skipped"
        );
    }

    /// An interior has no landscape to fall back on, so a terrain request for one loads it whole:
    /// the ring never asks for an interior, and a request that does must not return a cell with
    /// nothing in it.
    #[test]
    fn loads_a_terrain_request_for_an_interior_in_full() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);

        let payload = super::load_cell(
            &connection,
            &mut ReferenceQuery::for_connection(&connection, fixture_features(&connection))
                .unwrap(),
            1,
            CellKey::Interior(99),
            CellDetail::Terrain,
            &mut DoorTables::default(),
        )
        .unwrap();
        assert_eq!(payload.cell_id, 99);
        assert_eq!(payload.references.len(), 1);
    }

    /// The ring covers a square of grid cells, and a worldspace does not fill it: a grid with no
    /// cell at all is an error at both details, so the plan marks it failed instead of spawning an
    /// empty root for it.
    #[test]
    fn fails_a_terrain_request_for_a_grid_the_worldspace_does_not_cover() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        let absent = CellKey::Exterior {
            worldspace_id: 60,
            grid_x: 900,
            grid_y: 900,
        };
        assert!(
            super::load_cell(
                &connection,
                &mut ReferenceQuery::for_connection(&connection, fixture_features(&connection))
                    .unwrap(),
                1,
                absent,
                CellDetail::Terrain,
                &mut DoorTables::default()
            )
            .is_err()
        );
        assert!(
            super::load_cell(
                &connection,
                &mut ReferenceQuery::for_connection(&connection, fixture_features(&connection))
                    .unwrap(),
                1,
                absent,
                CellDetail::Full,
                &mut DoorTables::default()
            )
            .is_err()
        );
    }

    /// The ring's question, asked once per run: which worldspaces draw their own terrain LOD.
    /// Read from `lod_chunks` when the database opens; false - so the ring stays - where the
    /// table or the rows are missing.
    #[test]
    fn has_lod_for_answers_from_the_lod_chunks_table() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(&format!(
                r#"CREATE TABLE schema_info(version INTEGER NOT NULL);
                INSERT INTO schema_info VALUES({version});
                CREATE TABLE lod_chunks(
                    worldspace_id INTEGER NOT NULL,tier INTEGER NOT NULL,anchor_x INTEGER NOT NULL,anchor_y INTEGER NOT NULL,
                    payload_path TEXT NOT NULL,content_hash TEXT NOT NULL,
                    bounds_min_x REAL NOT NULL,bounds_min_y REAL NOT NULL,bounds_min_z REAL NOT NULL,
                    bounds_max_x REAL NOT NULL,bounds_max_y REAL NOT NULL,bounds_max_z REAL NOT NULL,
                    source_cells TEXT NOT NULL,PRIMARY KEY(worldspace_id,tier,anchor_x,anchor_y));
                INSERT INTO lod_chunks VALUES(60,4,0,0,'lod/0000003c/4/cell_0_0.glb','hash',
                 0.0,0.0,0.0,1.0,1.0,1.0,'');"#,
                version = shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
        let database = WorldDatabase::open(&path).unwrap();
        assert!(
            database.has_lod_for(60),
            "the worldspace the conversion compiled chunks for"
        );
        assert!(
            !database.has_lod_for(614),
            "a worldspace with no chunk of its own keeps the ring"
        );

        // A database converted before LOD existed has no table at all, and every worldspace of it
        // keeps the ring rather than the run refusing to open.
        let legacy_path = directory.path().join("legacy.db");
        Connection::open(&legacy_path)
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE schema_info(version INTEGER NOT NULL); INSERT INTO schema_info VALUES({});",
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
        assert!(!WorldDatabase::open(&legacy_path).unwrap().has_lod_for(60));
    }

    #[test]
    fn rejects_previous_database_schema() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("old.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_info(version INTEGER); INSERT INTO schema_info VALUES(2);",
            )
            .unwrap();
        drop(connection);
        assert!(validate(&path).is_err());
    }

    #[test]
    fn prefers_exterior_cell_with_land_over_persistent_cell_at_same_grid() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        connection
            .execute_batch("INSERT INTO cells VALUES(9,60,2,-3,NULL); INSERT INTO land VALUES(10);")
            .unwrap();

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();

        assert_eq!(payload.cell_id, 10);
        assert_eq!(payload.references.len(), 2);
    }

    #[test]
    fn rejects_truncated_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("truncated.db");
        std::fs::write(&path, b"SQLite format 3\0truncated").unwrap();
        assert!(WorldDatabase::open(&path).is_err());
    }

    #[test]
    fn an_unusable_database_answers_every_request_with_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.db");
        let (request_tx, request_rx) = bounded(4);
        let (response_tx, response_rx) = unbounded();
        let (lod_response_tx, _lod_response_rx) = unbounded();
        let key = CellKey::Interior(99);
        for generation in 0..2 {
            request_tx
                .send(DatabaseRequest::Load {
                    generation,
                    key,
                    detail: CellDetail::Full,
                    queued_at: Instant::now(),
                })
                .unwrap();
        }
        request_tx.send(DatabaseRequest::Shutdown).unwrap();

        worker(
            path,
            WorldDatabaseFeatures::default(),
            request_rx,
            response_tx,
            lod_response_tx,
        );

        let responses: Vec<DatabaseResponse> = response_rx.try_iter().collect();
        assert_eq!(responses.len(), 2, "every request is answered");
        for response in responses {
            let error = response
                .result
                .expect_err("the cell fails instead of loading");
            assert!(error.contains("is unusable"), "{error}");
        }
    }

    #[test]
    fn lod_worker_preserves_generation_on_its_distinct_response_channel() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        lod_fixture(&connection);
        drop(connection);

        let (request_tx, request_rx) = bounded(2);
        let (cell_response_tx, cell_response_rx) = unbounded();
        let (lod_response_tx, lod_response_rx) = unbounded();
        request_tx
            .send(DatabaseRequest::LoadLodChunks {
                generation: 7,
                query: lod_query([-46000.0, 54000.0], [-45000.0, 55000.0]),
                queued_at: Instant::now(),
            })
            .unwrap();
        request_tx.send(DatabaseRequest::Shutdown).unwrap();

        worker(
            path,
            WorldDatabaseFeatures::default(),
            request_rx,
            cell_response_tx,
            lod_response_tx,
        );

        let response = lod_response_rx.try_recv().unwrap();
        assert_eq!(response.generation, 7);
        assert_eq!(response.chunk_count, 1);
        assert_eq!(response.result.unwrap().len(), 1);
        assert!(cell_response_rx.try_recv().is_err());
    }

    #[test]
    fn drop_drains_a_full_request_queue_and_joins_worker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        fixture(&connection);
        drop(connection);

        let database = WorldDatabase::open(&path).unwrap();
        let stopped = database.worker_stopped.clone();
        for generation in 0..256 {
            database
                .request(DatabaseRequest::Load {
                    generation,
                    key: CellKey::Exterior {
                        worldspace_id: 60,
                        grid_x: 2,
                        grid_y: -3,
                    },
                    detail: CellDetail::Full,
                    queued_at: Instant::now(),
                })
                .unwrap();
        }
        drop(database);
        assert!(stopped.load(Ordering::Acquire));
    }

    #[test]
    fn loads_interior_cell_without_spatial_lookup() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        let payload = load_cell(&connection, 4, CellKey::Interior(99)).unwrap();
        assert_eq!(payload.cell_id, 99);
        assert_eq!(payload.references.len(), 1);
        assert_eq!(payload.references[0].form_id, 31);
    }

    #[test]
    fn returns_the_door_link_of_a_reference_and_the_interior_label() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();

        let door = payload
            .references
            .iter()
            .find(|reference| reference.form_id == 30)
            .expect("reference 30 is in the cell")
            .door
            .clone()
            .expect("reference 30 has a door_links row");
        assert_eq!(door.destination_ref_id, 77);
        assert_eq!(door.destination_cell_id, Some(99));
        assert_eq!(
            door.destination_worldspace_id, None,
            "an interior destination has no worldspace"
        );
        assert_eq!(door.arrival_position, [-947.038, 3958.835, 591.917]);
        assert_eq!(door.arrival_rotation, [0.0, 0.0, 2.96989]);
        assert_eq!(door.label, "Alftand01");

        assert!(
            payload
                .references
                .iter()
                .find(|reference| reference.form_id == 31)
                .expect("reference 31 is in the cell")
                .door
                .is_none(),
            "a reference without a door_links row is not a door"
        );
    }

    /// A door model's own axis convention is a fact about the model, fitted from every placement of
    /// it in the install: the circular mean of (link-derived facing - reference yaw) over those
    /// placements, kept only for a model whose placements agree with it. This is what tier 2 of the
    /// doorway anchor faces a doorway with (`docs/research/portal-door-alignment.md` section 3).
    #[test]
    fn a_models_axis_convention_is_fitted_from_its_placements() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        connection
            .execute_batch(
                r#"
                INSERT INTO statics VALUES(40,'dweDoor','doors/dwe.nif',-50,-60,-5,50,60,5,1);
                INSERT INTO statics VALUES(41,'twoWays','doors/twoways.nif',-50,-60,-5,50,60,5,1);
                INSERT INTO statics VALUES(42,'rare','doors/rare.nif',-50,-60,-5,50,60,5,1);
                -- Four placements of model 40, each at reference yaw 0 and each with a link 32 units
                -- east of it: the model's own axis is a quarter turn from its reference's.
                INSERT INTO "references" VALUES(100,10,40,0,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(101,10,40,10,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(102,10,40,20,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(103,10,40,30,0,0,0,0,0,1);
                INSERT INTO door_links VALUES(100,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(101,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(102,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(103,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(200,100,32,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(201,101,42,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(202,102,52,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(203,103,62,0,0,0,0,0.1,99,NULL);
                -- Model 41 is placed both ways: four like the first, four a quarter turn round, so
                -- its mean predicts nothing about a placement of it.
                INSERT INTO "references" VALUES(110,10,41,0,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(111,10,41,10,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(112,10,41,20,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(113,10,41,30,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(114,10,41,40,0,0,0,0,1.5707963,1);
                INSERT INTO "references" VALUES(115,10,41,50,0,0,0,0,1.5707963,1);
                INSERT INTO "references" VALUES(116,10,41,60,0,0,0,0,1.5707963,1);
                INSERT INTO "references" VALUES(117,10,41,70,0,0,0,0,1.5707963,1);
                INSERT INTO door_links VALUES(110,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(111,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(112,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(113,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(114,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(115,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(116,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(117,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(210,110,32,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(211,111,42,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(212,112,52,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(213,113,62,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(214,114,72,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(215,115,82,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(216,116,92,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(217,117,102,0,0,0,0,0.1,99,NULL);
                -- Model 42 has three placements: too few to fit a convention from.
                INSERT INTO "references" VALUES(120,10,42,0,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(121,10,42,10,0,0,0,0,0,1);
                INSERT INTO "references" VALUES(122,10,42,20,0,0,0,0,0,1);
                INSERT INTO door_links VALUES(120,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(121,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(122,30,1,-1,0,0,0,0,10,60);
                INSERT INTO door_links VALUES(220,120,32,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(221,121,42,0,0,0,0,0.1,99,NULL);
                INSERT INTO door_links VALUES(222,122,52,0,0,0,0,0.1,99,NULL);"#,
            )
            .unwrap();

        let placements = super::doorway_placements(&connection).unwrap();
        let convention = |form_id: u32| {
            placements
                .get(&form_id)
                .unwrap_or_else(|| panic!("reference {form_id} is in the doorway table"))
                .convention
        };
        assert!(
            (convention(100).expect("four placements agree") - PI / 2.0).abs() < 1.0e-3,
            "the model faces a quarter turn from its reference's own yaw: {:?}",
            convention(100)
        );
        assert_eq!(
            convention(110),
            None,
            "a model placed in two orientations has no convention"
        );
        assert_eq!(
            convention(120),
            None,
            "three placements are too few to fit one from"
        );

        // The placement itself: the model's bounds box centre (in the model's frame) and the grid
        // the reference stands in, which is what the anchor's destination grid comes from. That is
        // the grid of the reference's own position, not its cell's: cell 10 says (2, -3), and a
        // worldspace's persistent cell says (0, 0) for every reference in it wherever it stands -
        // Riverwood's load doors are there, and streaming the cell's grid put the portal in the
        // wrong part of Skyrim.
        let placement = placements.get(&100).unwrap();
        assert_eq!(placement.box_centre, Some([0.0, 0.0, 0.0]));
        assert_eq!(placement.grid, Some([0, 0]));
        assert_eq!(placement.scale, 1.0);
        assert_eq!(
            placement.return_arrival_z,
            Some(0.0),
            "the link that leads back into it arrives at the door's own height"
        );
    }

    /// A door's own doorway and its destination's are read for it, and the anchor built from them
    /// rides on the spawned reference: the two rows are what `crate::streaming::door_anchor`
    /// decides tier 1 from, with no convention needed at all.
    #[test]
    fn a_door_reads_its_own_doorway_and_its_destinations() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        // The destination door of reference 30's link (77), standing on the arrival point itself:
        // the game lands the player in its doorway, so the gate passes.
        connection
            .execute_batch(
                r#"INSERT INTO "references" VALUES(77,99,20,-947.038,3958.835,591.917,0,0,1.2,1);
                INSERT INTO door_links VALUES(77,30,8232.0,-12200.0,50.0,0,0,1.60570,10,60);"#,
            )
            .unwrap();

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        let reference = payload
            .references
            .iter()
            .find(|reference| reference.form_id == 30)
            .expect("the door is in the cell");
        let doorway = reference
            .doorway
            .as_ref()
            .expect("a door has a doorway row");
        assert_eq!(doorway.model, "architecture/wall.nif");
        assert_eq!(
            doorway.box_centre,
            Some([0.0, 0.0, 0.0]),
            "the fixture's bounds are symmetric about the model's origin"
        );
        let destination = reference
            .door
            .as_ref()
            .unwrap()
            .destination_doorway
            .as_ref()
            .expect("the destination reference is a door too");
        assert_eq!(destination.position, [-947.038, 3958.835, 591.917]);
        assert_eq!(destination.grid, None, "an interior cell has no grid");

        let anchor = crate::streaming::door_anchor(reference).expect("the pair anchors");
        assert_eq!(anchor.tier, crate::doors::DoorAnchorTier::SameModel);
        assert_eq!(anchor.source_box_centre, [0.0, 0.0, 0.0]);
        assert_eq!(anchor.destination.box_centre, [0.0, 0.0, 0.0]);
        assert_eq!(anchor.destination_grid, None);

        // And a reference whose link points at a door the database cannot place has no anchor.
        let mut orphan = reference.clone();
        orphan.door.as_mut().unwrap().destination_doorway = None;
        assert_eq!(crate::streaming::door_anchor(&orphan), None);
    }

    /// A door whose front is known from the link that leads back into it, shaped like the Alftand
    /// ruined tower's door: the door itself at 8200, -12200 and a link from another door arriving
    /// 32 units east of it, heading 92 degrees.
    #[test]
    fn returns_the_link_that_leads_back_into_a_door() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        connection
            .execute_batch(
                r#"INSERT INTO door_links VALUES(77,30,8232.0,-12200.0,50.0,0,0,1.60570,10,60);
                INSERT INTO door_links VALUES(31,78,21088.559,18512.045,2434.0,0,0,-1.87080,120,614);"#,
            )
            .unwrap();

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        let door_of = |form_id| {
            payload
                .references
                .iter()
                .find(|reference| reference.form_id == form_id)
                .expect("the reference is in the cell")
                .door
                .clone()
                .expect("the reference is a door")
        };

        let tower = door_of(30);
        assert_eq!(
            tower.return_arrival,
            Some(([8232.0, -12200.0, 50.0], [0.0, 0.0, 1.60570])),
            "the arrival of the door that leads here is the door's front"
        );
        assert_eq!(
            tower.destination_ref_id, 77,
            "the door's own link is untouched: it still leads to 77"
        );
        assert_eq!(tower.arrival_position, [-947.038, 3958.835, 591.917]);

        assert_eq!(
            door_of(31).return_arrival,
            None,
            "nothing leads back to a door that only leads away"
        );

        // The link that leads back is read once for the whole cell, not joined per reference, so a
        // second door leading into the same door does not return that door's reference twice.
        connection
            .execute_batch(
                "INSERT INTO door_links VALUES(78,30,8232.0,-12200.0,50.0,0,0,0.7,10,60);",
            )
            .unwrap();
        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        assert_eq!(
            payload
                .references
                .iter()
                .filter(|reference| reference.form_id == 30)
                .count(),
            1,
            "a second link into a door must not spawn the door twice"
        );
        let tower = payload
            .references
            .iter()
            .find(|reference| reference.form_id == 30)
            .unwrap();
        assert_eq!(
            tower.door.as_ref().unwrap().return_arrival,
            Some(([8232.0, -12200.0, 50.0], [0.0, 0.0, 1.60570])),
            "two links that lead to the same door are the same doorway: the lower ref_id decides"
        );
    }

    /// `door_links` is the same table for every cell, so a [`ReturnLinks`] that lives for a whole
    /// run reads it once: the second cell load of a run pays nothing for the links, which is what
    /// makes a door-bearing exterior cell cost one query rather than a whole-table scan.
    ///
    /// A connection the engine opens is read-only, so what was read once cannot go stale under it.
    /// This one can change: the cache is the reason a link deleted after the first load is still in
    /// the answer, and a fresh cache is what notices.
    #[test]
    fn the_return_links_are_read_once_and_kept() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        let key = CellKey::Exterior {
            worldspace_id: 60,
            grid_x: 2,
            grid_y: -3,
        };
        let link = "INSERT INTO door_links VALUES(77,30,8232.0,-12200.0,50.0,0,0,1.60570,10,60)";
        connection.execute_batch(link).unwrap();
        let arrival = |payload: &CellPayload| {
            payload
                .references
                .iter()
                .find(|reference| reference.form_id == 30)
                .expect("reference 30 is in the cell")
                .door
                .clone()
                .expect("reference 30 is a door")
                .return_arrival
        };

        let mut cache = DoorTables::default();
        let loaded = |connection: &Connection, cache: &mut DoorTables| {
            super::load_cell(
                connection,
                &mut ReferenceQuery::for_connection(connection, fixture_features(connection))
                    .unwrap(),
                1,
                key,
                CellDetail::Full,
                cache,
            )
            .unwrap()
        };
        assert!(
            arrival(&loaded(&connection, &mut cache)).is_some(),
            "the link that leads back into the door is read"
        );

        connection
            .execute_batch("DELETE FROM door_links WHERE ref_id=77")
            .unwrap();
        assert!(
            arrival(&loaded(&connection, &mut cache)).is_some(),
            "the links were read once: the table is not asked again"
        );
        assert!(
            arrival(&loaded(&connection, &mut DoorTables::default())).is_none(),
            "and a cache that has not read them yet sees the table as it is"
        );
    }

    /// The destination label is the destination interior's `FULL` subrecord bytes, which the game
    /// terminates with a NUL - the prompt drew `Alftand01` and then an empty box.
    #[test]
    fn trims_the_terminator_off_a_door_label() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        connection
            .execute(
                "UPDATE cells SET interior_name=?1 WHERE id=99",
                params!["Alftand01\0"],
            )
            .unwrap();

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        let door = payload
            .references
            .iter()
            .find(|reference| reference.form_id == 30)
            .expect("reference 30 is in the cell")
            .door
            .clone()
            .expect("reference 30 has a door_links row");
        assert_eq!(door.label, "Alftand01");

        assert_eq!(door_label("Alftand01".to_owned()), "Alftand01");
        assert_eq!(door_label("Blackreach \0\0 ".to_owned()), "Blackreach");
        assert_eq!(
            door_label("\0".to_owned()),
            "",
            "a label that is only the terminator is empty, not invisible"
        );
    }

    /// Four load doors on the fixture's cell, one per way a base can look: Skyrim's
    /// `AutoLoadDoor01` with its `AutoLoadMarker01.nif` (reference 41), a `MinUse` variant with
    /// another model (42), a mod marker whose editor id says nothing but whose model says
    /// "autoloadmarker" (43), and a plain `DweDoorLarge01Load` (44). See [`AUTO_LOAD_COLUMN`].
    fn auto_load_fixture(connection: &Connection) {
        connection
            .execute_batch(
                r#"INSERT INTO statics(id,editor_id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid)
                    VALUES(41,'AutoLoadDoor01','architecture/doors/AutoLoadMarker01.nif',0,0,0,0,0,0,0);
                INSERT INTO statics(id,editor_id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid)
                    VALUES(42,'AutoLoadDoorMinUse01','architecture/doors/Marker.nif',0,0,0,0,0,0,0);
                INSERT INTO statics(id,editor_id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid)
                    VALUES(43,'SomeModDoor','architecture/doors/autoloadmarker01.nif',0,0,0,0,0,0,0);
                INSERT INTO statics(id,editor_id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid)
                    VALUES(44,'DweDoorLarge01Load','dungeons/dwemer/door/dwemerlargedoorload01.nif',0,0,0,0,0,0,0);
                INSERT INTO "references" VALUES(41,10,41,8200,-12200,50,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(41,8200,8200,-12200,-12200,50,50,10,60);
                INSERT INTO "references" VALUES(42,10,42,8250,-12150,55,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(42,8250,8250,-12150,-12150,55,55,10,60);
                INSERT INTO "references" VALUES(43,10,43,8300,-12100,55,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(43,8300,8300,-12100,-12100,55,55,10,60);
                INSERT INTO "references" VALUES(44,10,44,8300,-12200,55,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(44,8300,8300,-12200,-12200,55,55,10,60);
                INSERT INTO door_links VALUES(41,42,-947.038,3958.835,591.917,0,0,2.96989,99,NULL);
                INSERT INTO door_links VALUES(42,41,-947.038,3958.835,591.917,0,0,2.96989,99,NULL);
                INSERT INTO door_links VALUES(43,41,-947.038,3958.835,591.917,0,0,2.96989,99,NULL);
                INSERT INTO door_links VALUES(44,41,-947.038,3958.835,591.917,0,0,2.96989,99,NULL);"#,
            )
            .unwrap();
    }

    fn auto_load_of(connection: &Connection, form_id: u32) -> bool {
        load_cell(
            connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap()
        .references
        .iter()
        .find(|reference| reference.form_id == form_id)
        .expect("the reference is in the cell")
        .auto_load
    }

    #[test]
    fn marks_auto_load_doors_by_their_base_editor_id_or_model() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        auto_load_fixture(&connection);

        assert!(
            auto_load_of(&connection, 41),
            "AutoLoadDoor01 with the AutoLoadMarker01 model is an auto-load door"
        );
        assert!(
            auto_load_of(&connection, 42),
            "AutoLoadDoorMinUse01 is one too, whatever its model is called"
        );
        assert!(
            auto_load_of(&connection, 43),
            "a mod's marker is found by its model, case-insensitively"
        );
        assert!(
            !auto_load_of(&connection, 44),
            "a plain DweDoorLarge01Load keeps its E key"
        );
        assert!(
            !auto_load_of(&connection, 30),
            "the fixture's plain wall is not an auto-load door"
        );

        // The same thing as the spawned door sees it: the flag the database put on the reference is
        // the one `LoadDoor` carries, which is what crosses the door on contact.
        let spawned = |form_id| {
            crate::streaming::load_door(
                load_cell(
                    &connection,
                    1,
                    CellKey::Exterior {
                        worldspace_id: 60,
                        grid_x: 2,
                        grid_y: -3,
                    },
                )
                .unwrap()
                .references
                .iter()
                .find(|reference| reference.form_id == form_id)
                .expect("the reference is in the cell"),
            )
            .expect("the reference has a resolved door link")
            .auto_load
        };
        assert!(spawned(41), "an AutoLoadDoor01 base crosses on contact");
        assert!(!spawned(44), "a DweDoorLarge01Load base keeps the E key");
    }

    /// A `statics` table without `editor_id` is not a shape the converter ever wrote, but the
    /// engine's own fixtures had it, and the rule falls back to the base's model rather than
    /// failing the cell load.
    #[test]
    fn falls_back_to_the_model_when_statics_has_no_editor_id() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        connection
            .execute_batch(
                r#"CREATE TABLE plain_statics(id INTEGER PRIMARY KEY,model_path TEXT,
                    bounds_min_x REAL,bounds_min_y REAL,bounds_min_z REAL,
                    bounds_max_x REAL,bounds_max_y REAL,bounds_max_z REAL,bounds_valid INTEGER NOT NULL);
                INSERT INTO plain_statics(id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid) VALUES(20,'architecture/wall.nif',-1,-2,-3,1,2,3,1);
                DROP TABLE statics;
                ALTER TABLE plain_statics RENAME TO statics;"#,
            )
            .unwrap();

        assert!(!auto_load_of(&connection, 30));
        assert!(!has_statics_editor_id(&connection).unwrap());
    }

    /// The schema-5 light tables: a `LIGH` record placed as reference 40 with an `XRDS` radius of
    /// its own, and the plain statics reference 30, which is not a light at all.
    fn light_fixture(connection: &Connection) {
        connection
            .execute_batch(
                r#"CREATE TABLE lights(id INTEGER PRIMARY KEY,editor_id TEXT,
                    radius REAL NOT NULL,color_r INTEGER NOT NULL,color_g INTEGER NOT NULL,
                    color_b INTEGER NOT NULL,flags INTEGER NOT NULL,falloff REAL NOT NULL,fade REAL);
                ALTER TABLE "references" ADD COLUMN radius_override REAL;
                INSERT INTO lights VALUES(21,'DefaultCandleLight01',256.0,255,150,80,8,1.25,0.5);
                INSERT INTO statics(id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid) VALUES(21,'clutter/candle.nif',-2,-3,-4,2,3,4,1);
                INSERT INTO "references" VALUES(40,10,21,8250,-12150,55,0,0,0,1,850.8);
                INSERT INTO exterior_spatial VALUES(40,8250,8250,-12150,-12150,55,55,10,60);"#,
            )
            .unwrap();
    }

    #[test]
    fn returns_the_light_row_of_a_lit_reference_and_its_radius_override() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        light_fixture(&connection);

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();

        let lit = payload
            .references
            .iter()
            .find(|reference| reference.form_id == 40)
            .expect("reference 40 is in the cell");
        assert_eq!(
            lit.light,
            Some(LightRow {
                radius: 256.0,
                color: [255, 150, 80],
                flags: 8,
                falloff: 1.25,
                fade: Some(0.5),
            })
        );
        assert_eq!(
            lit.light_radius_override,
            Some(850.8),
            "the reference's own XRDS radius comes back with it"
        );

        let plain = payload
            .references
            .iter()
            .find(|reference| reference.form_id == 30)
            .expect("reference 30 is in the cell");
        assert_eq!(plain.light, None, "a statics reference is not a light");
        assert_eq!(plain.light_radius_override, None);
    }

    /// A reference's light and its override are separate columns of separate tables, so a database
    /// converted between the two still loads.
    #[test]
    fn loads_lights_from_a_database_without_the_radius_override_column() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        connection
            .execute_batch(
                r#"CREATE TABLE lights(id INTEGER PRIMARY KEY,editor_id TEXT,
                    radius REAL NOT NULL,color_r INTEGER NOT NULL,color_g INTEGER NOT NULL,
                    color_b INTEGER NOT NULL,flags INTEGER NOT NULL,falloff REAL NOT NULL,fade REAL);
                INSERT INTO lights VALUES(20,'Torch01',512.0,255,200,120,0,1.0,NULL);"#,
            )
            .unwrap();
        assert!(!has_radius_override(&connection).unwrap());

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();

        let lit = payload
            .references
            .iter()
            .find(|reference| reference.form_id == 30)
            .expect("reference 30 is in the cell");
        assert_eq!(
            lit.light.as_ref().map(|light| light.radius),
            Some(512.0),
            "the light still loads without the override column"
        );
        assert_eq!(lit.light_radius_override, None);
    }

    /// Reference 30 names a region the converter resolved; reference 40 names one it could not.
    fn emittance_fixture(connection: &Connection) {
        connection
            .execute_batch(
                r#"INSERT INTO statics(id,model_path,bounds_min_x,bounds_min_y,bounds_min_z,bounds_max_x,bounds_max_y,bounds_max_z,bounds_valid) VALUES(22,'clutter/barrel.nif',-2,-3,-4,2,3,4,1);
                INSERT INTO "references" VALUES(40,10,22,8250,-12150,55,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(40,8250,8250,-12150,-12150,55,55,10,60);
                ALTER TABLE "references" ADD COLUMN emittance_id INTEGER;
                UPDATE "references" SET emittance_id=535419 WHERE id=30;
                UPDATE "references" SET emittance_id=535420 WHERE id=40;
                CREATE TABLE emittance(id INTEGER PRIMARY KEY,source_type TEXT NOT NULL,weather_id INTEGER,
                    sunrise_r INTEGER NOT NULL,sunrise_g INTEGER NOT NULL,sunrise_b INTEGER NOT NULL,
                    day_r INTEGER NOT NULL,day_g INTEGER NOT NULL,day_b INTEGER NOT NULL,
                    sunset_r INTEGER NOT NULL,sunset_g INTEGER NOT NULL,sunset_b INTEGER NOT NULL,
                    night_r INTEGER NOT NULL,night_g INTEGER NOT NULL,night_b INTEGER NOT NULL);
                INSERT INTO emittance VALUES(535419,'REGN',535418,124,70,16,12,34,56,243,118,54,242,176,55);"#,
            )
            .unwrap();
    }

    #[test]
    fn returns_the_day_colour_of_a_reference_emittance_source() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        emittance_fixture(&connection);
        assert!(has_emittance(&connection).unwrap());

        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        let emittance = |form_id: u32| {
            payload
                .references
                .iter()
                .find(|reference| reference.form_id == form_id)
                .expect("the reference is in the cell")
                .emittance
        };
        assert_eq!(emittance(30), Some([12, 34, 56]), "the day column only");
        assert_eq!(
            emittance(40),
            None,
            "a source without an emittance row lends nothing"
        );
    }

    /// The column alone, without the table it names, does not make the query join it.
    #[test]
    fn needs_both_the_emittance_column_and_table() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        connection
            .execute_batch(r#"ALTER TABLE "references" ADD COLUMN emittance_id INTEGER;"#)
            .unwrap();
        assert!(!has_emittance(&connection).unwrap());
        let payload = load_cell(&connection, 1, CellKey::Interior(99)).unwrap();
        assert_eq!(payload.references[0].emittance, None);
    }

    #[test]
    fn loads_a_database_whose_references_are_all_unlit() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);

        assert!(!has_lights(&connection).unwrap());
        assert!(!has_radius_override(&connection).unwrap());
        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        assert_eq!(payload.references.len(), 2);
        assert!(payload.references.iter().all(
            |reference| reference.light.is_none() && reference.light_radius_override.is_none()
        ));
        assert!(!has_emittance(&connection).unwrap());
        assert!(
            payload
                .references
                .iter()
                .all(|reference| reference.emittance.is_none()),
            "a database without the emittance table lends no emittance"
        );
        assert_eq!(
            payload.references[0].model_path.as_deref(),
            Some("architecture/wall.nif"),
            "the plain query still joins the base object"
        );
    }

    #[test]
    fn labels_an_exterior_destination_with_the_worldspace_editor_id() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        door_fixture(&connection);
        connection
            .execute_batch("INSERT INTO door_links VALUES(31,78,21088.559,18512.045,2434.0,0,0,-1.8708,120,614);")
            .unwrap();

        let payload = load_cell(&connection, 1, CellKey::Interior(99)).unwrap();
        let door = payload.references[0]
            .door
            .clone()
            .expect("reference 31 has a door_links row");
        assert_eq!(door.destination_worldspace_id, Some(614));
        assert_eq!(
            door.destination_cell_id,
            Some(120),
            "the destination ref's own cell is kept"
        );
        assert_eq!(door.label, "Blackreach");
        assert_eq!(
            door.arrival_position,
            [21088.559, 18512.045, 2434.0],
            "the arrival point is the XTEL position, not the destination door's"
        );
    }

    #[test]
    fn loads_a_database_whose_references_are_all_non_doors() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);

        assert_eq!(
            fixture_features(&connection).door_links,
            DoorLinksAccess::Core
        );
        let payload = load_cell(
            &connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap();
        assert_eq!(payload.references.len(), 2);
        assert!(
            payload
                .references
                .iter()
                .all(|reference| reference.door.is_none())
        );
        assert_eq!(
            payload.references[0].model_path.as_deref(),
            Some("architecture/wall.nif"),
            "the plain query still joins the base object"
        );
    }

    /// A `door_links` table and its rows: reference 30 (exterior cell 10) leads into interior cell
    /// 500, reference 31 leads out to worldspace 60 and reference 32 is a link whose destination
    /// the converter could not resolve.
    fn add_door_links(connection: &Connection) {
        connection
            .execute_batch(
                r#"CREATE TABLE door_links(ref_id INTEGER PRIMARY KEY,destination_ref_id INTEGER NOT NULL,
                    pos_x REAL NOT NULL,pos_y REAL NOT NULL,pos_z REAL NOT NULL,
                    rot_x REAL NOT NULL,rot_y REAL NOT NULL,rot_z REAL NOT NULL,
                    destination_cell_id INTEGER,destination_worldspace_id INTEGER);
                INSERT INTO "references" VALUES(32,10,20,8260,-12100,50,0,0,0,1);
                INSERT INTO exterior_spatial VALUES(32,8260,8260,-12100,-12100,50,50,10,60);
                INSERT INTO door_links VALUES(30,700,10.5,-20.25,30,0,0,1.5,500,NULL);
                INSERT INTO door_links VALUES(31,701,8200,-12200,50,0,0,3.0,10,60);
                INSERT INTO door_links VALUES(32,702,1,2,3,0,0,0,NULL,NULL);"#,
            )
            .unwrap();
    }

    fn exterior_payload(connection: &Connection) -> CellPayload {
        load_cell(
            connection,
            1,
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 2,
                grid_y: -3,
            },
        )
        .unwrap()
    }

    #[test]
    fn door_links_and_emittance_load_independently_and_together() {
        for doors in [false, true] {
            for emittance in [false, true] {
                let connection = Connection::open_in_memory().unwrap();
                fixture(&connection);
                if doors {
                    add_door_links(&connection);
                }
                if emittance {
                    emittance_fixture(&connection);
                }
                let payload = exterior_payload(&connection);
                let reference = payload
                    .references
                    .iter()
                    .find(|reference| reference.form_id == 30)
                    .unwrap();
                assert_eq!(
                    reference.door,
                    doors.then_some(DoorLinkRow {
                        destination_ref_id: 700,
                        destination_cell_id: Some(500),
                        destination_worldspace_id: None,
                        arrival_position: [10.5, -20.25, 30.0],
                        arrival_rotation: [0.0, 0.0, 1.5],
                        // No destination cell row or worldspace names the label, nothing leads back
                        // into door 30 and its destination has no doorway.
                        label: String::new(),
                        return_arrival: None,
                        destination_doorway: None,
                    }),
                    "doors={doors}, emittance={emittance}"
                );
                assert_eq!(
                    reference.emittance,
                    emittance.then_some([12, 34, 56]),
                    "doors={doors}, emittance={emittance}"
                );
            }
        }
    }

    #[test]
    fn a_door_links_row_rides_on_the_reference_row() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        add_door_links(&connection);

        assert!(has_door_links(&connection).unwrap());
        let payload = exterior_payload(&connection);
        let door = |form_id: u32| {
            payload
                .references
                .iter()
                .find(|reference| reference.form_id == form_id)
                .unwrap()
                .door
                .clone()
        };
        assert_eq!(
            door(30),
            Some(DoorLinkRow {
                destination_ref_id: 700,
                destination_cell_id: Some(500),
                destination_worldspace_id: None,
                arrival_position: [10.5, -20.25, 30.0],
                arrival_rotation: [0.0, 0.0, 1.5],
                // No destination cell row or worldspace names the label, nothing leads back
                // into door 30 and its destination has no doorway.
                label: String::new(),
                return_arrival: None,
                destination_doorway: None,
            })
        );
        assert_eq!(door(31).unwrap().destination_worldspace_id, Some(60));
        assert_eq!(door(32).unwrap().destination_cell_id, None);
        assert_eq!(payload.references.len(), 3);
    }

    #[test]
    fn a_door_links_row_with_null_arrival_columns_is_no_link_and_does_not_fail_the_cell() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        connection
            .execute_batch(
                r#"CREATE TABLE door_links(ref_id INTEGER PRIMARY KEY,destination_ref_id INTEGER NOT NULL,
                    pos_x REAL,pos_y REAL,pos_z REAL,rot_x REAL,rot_y REAL,rot_z REAL,
                    destination_cell_id INTEGER,destination_worldspace_id INTEGER);
                INSERT INTO door_links VALUES(30,700,NULL,NULL,NULL,NULL,NULL,NULL,500,NULL);
                INSERT INTO door_links VALUES(31,701,8200,-12200,50,0,0,NULL,10,60);"#,
            )
            .unwrap();

        let payload = exterior_payload(&connection);
        assert_eq!(payload.references.len(), 2);
        assert!(
            payload
                .references
                .iter()
                .all(|reference| reference.door.is_none())
        );
    }

    #[test]
    fn a_door_links_table_of_another_shape_is_ignored_and_does_not_fail_the_cell() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);
        connection
            .execute_batch(
                r#"CREATE TABLE door_links(ref_id INTEGER PRIMARY KEY,destination_ref_id INTEGER NOT NULL,
                    pos_x REAL,pos_y REAL,pos_z REAL,rot_x REAL,rot_y REAL,rot_z REAL);
                INSERT INTO door_links VALUES(30,700,1,2,3,0,0,0);"#,
            )
            .unwrap();

        assert!(!has_door_links(&connection).unwrap());
        let payload = exterior_payload(&connection);
        assert_eq!(payload.references.len(), 2);
        assert!(
            payload
                .references
                .iter()
                .all(|reference| reference.door.is_none())
        );
    }

    #[test]
    fn a_database_without_door_links_yields_no_links() {
        let connection = Connection::open_in_memory().unwrap();
        fixture(&connection);

        assert!(!has_door_links(&connection).unwrap());
        let payload = exterior_payload(&connection);
        assert_eq!(payload.references.len(), 2);
        assert!(
            payload
                .references
                .iter()
                .all(|reference| reference.door.is_none())
        );
    }
    #[test]
    fn accepts_schema_four_and_five_databases_with_legacy_query_columns() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = Connection::open(&path).unwrap();
        fixture(&connection);
        drop(connection);
        for version in [4, 5] {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute("UPDATE schema_info SET version=?1", [version])
                .unwrap();
            drop(connection);
            validate(&path).unwrap();
            let connection = Connection::open(&path).unwrap();
            let payload = load_cell(
                &connection,
                1,
                CellKey::Exterior {
                    worldspace_id: 60,
                    grid_x: 2,
                    grid_y: -3,
                },
            )
            .unwrap();
            assert_eq!(payload.references.len(), 2);
        }
        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE schema_info SET version=?1",
                [shared::WORLD_DATABASE_SCHEMA_VERSION + 1],
            )
            .unwrap();
        drop(connection);
        assert!(validate(&path).is_err());
    }
}

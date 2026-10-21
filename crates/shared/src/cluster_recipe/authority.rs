//! The production authority: what the dataset's own winning inputs say, recomputed when a
//! package is finalized, apart from anything the cluster stage wrote.
//!
//! [`CoreAuthority`] reads the core database (references, base records, statics, worldspaces and
//! the build identity) and the published models' GLB headers, and answers
//! [`AuthoritativeInputs`] through the same recipe the stage uses ([`resolve`]). It reads
//! reference by reference by default. The startup reader may preload the examined rows with one
//! query and retain them for mapping validation and runtime indexes.

use super::{
    classify::{
        ScopedReference, Sources, read_sources, scoped_reference_by_id, scoped_reference_ids,
        scoped_references_by_ids,
    },
    resolve::{ModelCatalog, snapshot},
    scope::{Scope, ScopeRequest},
};
use crate::{
    cluster_mapping::{AuthoritativeInputs, ReferenceSnapshot},
    extensions::core_build_identity,
};
use color_eyre::Result;
use rusqlite::{Connection, OpenFlags, limits::Limit};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

/// Connection-local page reuse for dedicated read-only certification connections. mmap shares OS
/// pages between workers; the 64 MiB page cache is an upper budget per connection, not a trust
/// cache. SQLite still executes every query, checks its own file state and may clamp mmap_size.
pub fn configure_read_only_cache(connection: &Connection) -> rusqlite::Result<()> {
    configure_schema_cache(connection, None)
}

/// [`configure_read_only_cache`] for the attached database `schema` (a cluster overlay attached to
/// the base): cache and mmap sizes are per database, so each attached file is tuned on its own.
pub fn configure_attached_read_only_cache(
    connection: &Connection,
    schema: &str,
) -> rusqlite::Result<()> {
    configure_schema_cache(connection, Some(rusqlite::DatabaseName::Attached(schema)))
}

fn configure_schema_cache(
    connection: &Connection,
    schema: Option<rusqlite::DatabaseName<'_>>,
) -> rusqlite::Result<()> {
    connection.pragma_update(schema, "cache_size", -(64 * 1024))?;
    let _: i64 = connection.pragma_update_and_check(
        schema,
        "mmap_size",
        2_i64 * 1024 * 1024 * 1024,
        |row| row.get(0),
    )?;
    Ok(())
}

pub struct CoreAuthority {
    timings: Vec<(&'static str, Duration)>,
    connection: Connection,
    core_identity: Option<String>,
    builder_identity: String,
    scope: Scope,
    sources: Sources,
    catalog: ModelCatalog,
    examined: BTreeSet<u32>,
    preloaded: Option<BTreeMap<u32, Option<ScopedReference>>>,
    preloaded_snapshots: Option<BTreeMap<u32, Option<ReferenceSnapshot>>>,
    origins: HashMap<u32, (i32, i32)>,
}

impl CoreAuthority {
    /// Launch-local source hashing attribution; no read or verdict is skipped by these counters.
    pub fn model_read_metrics(&self) -> super::resolve::ModelReadMetrics {
        self.catalog.read_metrics()
    }

    /// Opens the authority over `connection` (the dataset's world database), the published files
    /// under `assets_root` (`published` are their (path, content hash) pairs), for the scope that
    /// `request` and this database declare (the snow source is read from the database itself).
    /// `builder_identity` is the hex identity of the converter that is publishing.
    pub fn open(
        connection: Connection,
        assets_root: &Path,
        published: impl IntoIterator<Item = (String, String)>,
        request: &ScopeRequest,
        builder_identity: String,
    ) -> Result<Self> {
        let sources_started = Instant::now();
        let sources = read_sources(&connection, &request.child_worldspaces_sorted())?;
        let scope = Scope::from_request(request, sources.snow_source);
        Self::finish(
            connection,
            assets_root,
            published,
            scope,
            sources,
            builder_identity,
            sources_started.elapsed(),
        )
    }

    /// Opens the authority for a scope a package declares (`classifier_scope`, read with
    /// [`Scope::from_declared_json`]). The declared snow source must be the one this database has:
    /// the reader's authority is this database, not the declaration.
    pub fn open_declared(
        connection: Connection,
        assets_root: &Path,
        published: impl IntoIterator<Item = (String, String)>,
        scope: Scope,
        builder_identity: String,
    ) -> Result<Self> {
        let sources_started = Instant::now();
        let sources = read_sources(&connection, &scope.child_worldspaces)?;
        color_eyre::eyre::ensure!(
            sources.snow_source == scope.snow_source,
            "the declared snow source {} is not this database's {}",
            scope.snow_source.as_str(),
            sources.snow_source.as_str()
        );
        Self::finish(
            connection,
            assets_root,
            published,
            scope,
            sources,
            builder_identity,
            sources_started.elapsed(),
        )
    }

    fn finish(
        connection: Connection,
        assets_root: &Path,
        published: impl IntoIterator<Item = (String, String)>,
        scope: Scope,
        sources: Sources,
        builder_identity: String,
        sources_elapsed: Duration,
    ) -> Result<Self> {
        let mut timings = vec![("read_sources", sources_elapsed)];
        let started = Instant::now();
        let core_identity = core_build_identity(&connection)?;
        timings.push(("authority_core_identity", started.elapsed()));
        let started = Instant::now();
        let examined = scoped_reference_ids(&connection, &sources, scope.include_trees)?
            .into_iter()
            .filter(|(_, cell, world)| scope.contains(*world, *cell))
            .map(|(id, _, _)| id)
            .collect();
        timings.push(("scoped_reference_ids", started.elapsed()));
        let origins = sources
            .worldspaces
            .iter()
            .map(|world| {
                (
                    world.worldspace_id,
                    (world.origin.grid_x, world.origin.grid_y),
                )
            })
            .collect();
        let started = Instant::now();
        let catalog = ModelCatalog::new(assets_root, published)
            .map_err(|problem| color_eyre::eyre::eyre!("{problem}"))?;
        timings.push(("model_catalog_new", started.elapsed()));
        Ok(Self {
            timings,
            connection,
            core_identity,
            builder_identity,
            scope,
            sources,
            catalog,
            examined,
            preloaded: None,
            preloaded_snapshots: None,
            origins,
        })
    }

    /// Drains timings from this open only; no content identities or trust are cached.
    pub fn take_timings(&mut self) -> Vec<(&'static str, Duration)> {
        std::mem::take(&mut self.timings)
    }

    /// Preloads exactly the examined set with one query. Individual decode failures remain
    /// unavailable, as before. On a query-wide storage/SQL error, rerun the original per-ID
    /// reads so an unrelated later failure cannot change the first validation refusal.
    /// Returns that diagnostic for the caller to report; all certification checks still run.
    pub fn preload_references(&mut self) -> Option<color_eyre::Report> {
        self.preloaded_snapshots = None;
        match scoped_references_by_ids(
            &self.connection,
            &self.sources,
            &self.examined,
            self.scope.include_trees,
        ) {
            Ok(references) => {
                self.preloaded = Some(references);
                None
            }
            Err(error) => {
                self.preloaded = Some(
                    self.examined
                        .iter()
                        .map(|&id| {
                            let reference = scoped_reference_by_id(
                                &self.connection,
                                &self.sources,
                                id,
                                self.scope.include_trees,
                            )
                            .ok()
                            .flatten();
                            (id, reference)
                        })
                        .collect(),
                );
                Some(error)
            }
        }
    }

    /// Preloads file-backed source rows on independent read-only connections. The caller must
    /// retain its dataset source lock through certification, as for the sequential preload.
    /// Each worker applies the same row recipe to an exact, disjoint reference-ID partition.
    /// Use only with plain connections using SQLite's standard functions/collations, as the
    /// engine's dedicated source connection does. Callers installing custom SQL functions or
    /// connection hooks must use `preload_references` instead. Transactions, temporary/attached
    /// schemas, views and worker/query failures replay the original preload and return a
    /// diagnostic for the caller to report; no source input or refusal is skipped.
    pub fn preload_references_parallel(&mut self) -> Option<color_eyre::Report> {
        self.preloaded_snapshots = None;
        match self.read_reference_partitions() {
            Ok(references) => {
                self.preloaded = Some(references);
                None
            }
            Err(error) => {
                let recovery = self.preload_references();
                Some(match recovery {
                    Some(recovery) => color_eyre::eyre::eyre!(
                        "parallel source preload used original reads: {error:#}; original bulk query also required per-ID reads: {recovery:#}"
                    ),
                    None => color_eyre::eyre::eyre!(
                        "parallel source preload used original reads: {error:#}"
                    ),
                })
            }
        }
    }

    fn read_reference_partitions(&self) -> Result<BTreeMap<u32, Option<ScopedReference>>> {
        let path = self
            .connection
            .path()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "in-memory/temporary source database has no reopenable path"
                )
            })?;
        color_eyre::eyre::ensure!(
            self.connection.is_autocommit(),
            "source connection has a private transaction"
        );
        let has_temp: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_temp_master)",
            [],
            |row| row.get(0),
        )?;
        color_eyre::eyre::ensure!(!has_temp, "source connection has private temporary objects");
        let has_views: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM main.sqlite_master WHERE type = 'view')",
            [],
            |row| row.get(0),
        )?;
        color_eyre::eyre::ensure!(!has_views, "source database has connection-sensitive views");
        let mut statement = self.connection.prepare("PRAGMA database_list")?;
        let databases = statement.query_map([], |row| row.get::<_, String>(1))?;
        for database in databases {
            let database = database?;
            color_eyre::eyre::ensure!(
                database == "main" || database == "temp",
                "source connection has an attached database"
            );
        }
        let limits: Vec<_> = [
            Limit::SQLITE_LIMIT_LENGTH,
            Limit::SQLITE_LIMIT_SQL_LENGTH,
            Limit::SQLITE_LIMIT_COLUMN,
            Limit::SQLITE_LIMIT_EXPR_DEPTH,
            Limit::SQLITE_LIMIT_COMPOUND_SELECT,
            Limit::SQLITE_LIMIT_VDBE_OP,
            Limit::SQLITE_LIMIT_FUNCTION_ARG,
            Limit::SQLITE_LIMIT_ATTACHED,
            Limit::SQLITE_LIMIT_LIKE_PATTERN_LENGTH,
            Limit::SQLITE_LIMIT_VARIABLE_NUMBER,
            Limit::SQLITE_LIMIT_TRIGGER_DEPTH,
            Limit::SQLITE_LIMIT_WORKER_THREADS,
        ]
        .into_iter()
        .map(|limit| (limit, self.connection.limit(limit)))
        .collect();
        if self.examined.is_empty() {
            return Ok(BTreeMap::new());
        }
        let ids: Vec<_> = self.examined.iter().copied().collect();
        let workers = thread::available_parallelism()?
            .get()
            .saturating_sub(2)
            .max(1)
            .min(ids.len());
        let partitions: Vec<BTreeSet<u32>> = ids
            .chunks(ids.len().div_ceil(workers))
            .map(|partition| partition.iter().copied().collect())
            .collect();
        let sources = &self.sources;
        let include_trees = self.scope.include_trees;
        let batches = parallel_collect(
            &partitions,
            "cluster-source-rows",
            |partition| -> Result<_> {
                let connection =
                    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                configure_read_only_cache(&connection)?;
                for &(limit, value) in &limits {
                    connection.set_limit(limit, value);
                }
                scoped_references_by_ids(&connection, sources, partition, include_trees)
            },
        )?;
        let mut references = BTreeMap::new();
        for batch in batches {
            references.extend(batch?);
        }
        Ok(references)
    }

    /// Borrows the fully preloaded authority without retaining the SQLite connection. The
    /// returned view can be shared with certification workers; no source read is skipped.
    pub fn preloaded_authority(&self) -> Result<PreloadedAuthority<'_>> {
        let snapshots = self.preloaded_snapshots.as_ref().ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "authority snapshots must be preloaded before parallel validation"
            )
        })?;
        Ok(PreloadedAuthority {
            core_identity: &self.core_identity,
            builder_identity: &self.builder_identity,
            scope: &self.scope,
            catalog: &self.catalog,
            examined: &self.examined,
            snapshots,
            origins: &self.origins,
        })
    }

    /// A retained examined row, when explicitly preloaded and successfully decoded. The reader
    /// uses it only after mapping validation has accepted the authority's snapshots.
    pub fn preloaded_reference(&self, reference_id: u32) -> Option<&ScopedReference> {
        self.preloaded.as_ref()?.get(&reference_id)?.as_ref()
    }

    /// Computes the original snapshot recipe exactly once for every preloaded reference. Each
    /// unique candidate model is resolved first, retaining the catalog's full byte, texture,
    /// alias and material checks. Dynamic work assignment avoids concentrating expensive models
    /// on one worker. Only the catalog and decoded rows cross threads, never the connection.
    pub fn preload_snapshots(&mut self) -> Result<()> {
        self.preload_snapshots_with_headroom(0)
    }

    /// Retains the normal two-CPU headroom plus a caller's concurrently busy threads. The
    /// caller must join that work before consuming snapshots or reporting a certification.
    pub fn preload_snapshots_with_headroom(
        &mut self,
        additional_busy_threads: usize,
    ) -> Result<()> {
        if self.preloaded_snapshots.is_some() {
            return Ok(());
        }
        let references = self
            .preloaded
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("reference rows must be preloaded first"))?;
        let catalog = &self.catalog;
        let model_paths: Vec<_> = references
            .values()
            .flatten()
            .filter_map(|reference| reference.candidate.as_ref())
            .map(|candidate| candidate.glb_path.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let started = Instant::now();
        parallel_collect_with_headroom(
            &model_paths,
            "cluster-model-authority",
            additional_busy_threads,
            |path| {
                catalog.info(path);
            },
        )?;
        self.timings
            .push(("authority_model_resolution", started.elapsed()));
        let started = Instant::now();
        let rows: Vec<_> = references
            .iter()
            .map(|(&id, row)| (id, row.as_ref()))
            .collect();
        let snapshots = parallel_collect_with_headroom(
            &rows,
            "cluster-reference-authority",
            additional_busy_threads,
            |&(id, row)| (id, row.map(|reference| snapshot(reference, catalog))),
        )?;
        self.preloaded_snapshots = Some(snapshots.into_iter().collect());
        self.timings
            .push(("authority_reference_snapshots", started.elapsed()));
        Ok(())
    }
}

/// Immutable, connection-free view of source facts freshly read by [`CoreAuthority`].
pub struct PreloadedAuthority<'a> {
    core_identity: &'a Option<String>,
    builder_identity: &'a str,
    scope: &'a Scope,
    catalog: &'a ModelCatalog,
    examined: &'a BTreeSet<u32>,
    snapshots: &'a BTreeMap<u32, Option<ReferenceSnapshot>>,
    origins: &'a HashMap<u32, (i32, i32)>,
}

impl AuthoritativeInputs for PreloadedAuthority<'_> {
    fn core_build_identity(&self) -> Option<String> {
        self.core_identity.clone()
    }

    fn builder_identity(&self) -> String {
        self.builder_identity.to_owned()
    }

    fn reference(&self, reference_id: u32) -> Option<ReferenceSnapshot> {
        self.snapshots.get(&reference_id).cloned().flatten()
    }

    fn reference_view(&self, reference_id: u32) -> Option<&ReferenceSnapshot> {
        self.snapshots.get(&reference_id).and_then(Option::as_ref)
    }

    fn classifier_scope(&self) -> String {
        self.scope.json()
    }

    fn lod_origin(&self, worldspace_id: u32) -> Option<(i32, i32)> {
        self.origins.get(&worldspace_id).copied()
    }

    fn model_primitives(&self, model_hash: &str) -> Option<BTreeSet<u32>> {
        self.catalog.facts_by_hash(model_hash)
    }

    fn model_materials(
        &self,
        model_hash: &str,
    ) -> Option<BTreeMap<u32, crate::cluster_mapping::MaterialSource>> {
        self.catalog.materials_by_hash(model_hash)
    }

    fn examined_references(&self) -> BTreeSet<u32> {
        self.examined.clone()
    }
}

/// Runs bounded scoped workers without involving Bevy's compute pool. Every worker is joined
/// before returning; spawn failures and panics refuse the preload rather than skipping work.
fn parallel_collect<T: Sync, R: Send>(
    inputs: &[T],
    name: &str,
    operation: impl Fn(&T) -> R + Sync,
) -> Result<Vec<R>> {
    parallel_collect_with_headroom(inputs, name, 0, operation)
}

fn parallel_collect_with_headroom<T: Sync, R: Send>(
    inputs: &[T],
    name: &str,
    additional_busy_threads: usize,
    operation: impl Fn(&T) -> R + Sync,
) -> Result<Vec<R>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let workers = thread::available_parallelism()?
        .get()
        .saturating_sub(2)
        .saturating_sub(additional_busy_threads)
        .max(1)
        .min(inputs.len());
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let next = &next;
            let operation = &operation;
            handles.push(
                thread::Builder::new()
                    .name(format!("{name}-{worker}"))
                    .spawn_scoped(scope, move || {
                        let mut output = Vec::new();
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some(input) = inputs.get(index) else {
                                break;
                            };
                            output.push(operation(input));
                        }
                        output
                    })?,
            );
        }
        let mut output = Vec::with_capacity(inputs.len());
        let mut panicked = false;
        for handle in handles {
            match handle.join() {
                Ok(values) => output.extend(values),
                Err(_) => panicked = true,
            }
        }
        color_eyre::eyre::ensure!(!panicked, "{name} certification worker panicked");
        Ok(output)
    })
}

impl AuthoritativeInputs for CoreAuthority {
    fn core_build_identity(&self) -> Option<String> {
        self.core_identity.clone()
    }

    fn builder_identity(&self) -> String {
        self.builder_identity.clone()
    }

    fn reference(&self, reference_id: u32) -> Option<ReferenceSnapshot> {
        if !self.examined.contains(&reference_id) {
            return None;
        }
        if let Some(snapshots) = &self.preloaded_snapshots {
            return snapshots.get(&reference_id).cloned().flatten();
        }
        if self.preloaded.is_some() {
            return self
                .preloaded_reference(reference_id)
                .map(|scoped| snapshot(scoped, &self.catalog));
        }
        let scoped = scoped_reference_by_id(
            &self.connection,
            &self.sources,
            reference_id,
            self.scope.include_trees,
        )
        .ok()??;
        Some(snapshot(&scoped, &self.catalog))
    }

    fn reference_view(&self, reference_id: u32) -> Option<&ReferenceSnapshot> {
        self.preloaded_snapshots
            .as_ref()?
            .get(&reference_id)?
            .as_ref()
    }

    fn classifier_scope(&self) -> String {
        self.scope.json()
    }

    fn lod_origin(&self, worldspace_id: u32) -> Option<(i32, i32)> {
        self.origins.get(&worldspace_id).copied()
    }

    fn model_primitives(&self, model_hash: &str) -> Option<BTreeSet<u32>> {
        self.catalog.facts_by_hash(model_hash)
    }

    fn model_materials(
        &self,
        model_hash: &str,
    ) -> Option<std::collections::BTreeMap<u32, crate::cluster_mapping::MaterialSource>> {
        self.catalog.materials_by_hash(model_hash)
    }

    fn examined_references(&self) -> BTreeSet<u32> {
        self.examined.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_recipe::{classify::SnowSource, scope::CellArea};

    /// A world database with one worldspace that has a LOD origin and one static reference, plus
    /// the snow tables `snow` asks for.
    fn database(snow: SnowSource) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE worldspaces (id INTEGER PRIMARY KEY, lod_origin_x INTEGER,
                     lod_origin_y INTEGER, parent_world INTEGER);
                 INSERT INTO worldspaces VALUES (60, -32, -32, NULL);
                 CREATE TABLE records (form_id INTEGER, record_type TEXT);
                 INSERT INTO records VALUES (17, 'STAT');
                 CREATE TABLE statics (id INTEGER PRIMARY KEY);
                 CREATE TABLE \"references\" (id INTEGER PRIMARY KEY, base_form_id INTEGER,
                     worldspace_id INTEGER, is_exterior INTEGER, pos_x REAL, pos_y REAL);
                 INSERT INTO \"references\" VALUES (256, 17, 60, 1, 100.0, 100.0);
                 CREATE TABLE lod_build (id INTEGER PRIMARY KEY, build_identity TEXT);
                 INSERT INTO lod_build VALUES (1, 'core-1');",
            )
            .unwrap();
        match snow {
            SnowSource::StaticMaterials => connection
                .execute_batch(
                    "CREATE TABLE static_materials (id INTEGER PRIMARY KEY,
                         material_object INTEGER, material_max_angle REAL);",
                )
                .unwrap(),
            SnowSource::StaticsColumn => connection
                .execute_batch("ALTER TABLE statics ADD COLUMN material_object INTEGER;")
                .unwrap(),
            SnowSource::None => {}
        }
        connection
    }

    fn declared(snow: SnowSource) -> Scope {
        Scope::from_declared_json(&Scope::from_request(&ScopeRequest::default(), snow).json())
            .unwrap()
    }

    fn open_declared(connection: Connection, scope: Scope) -> Result<CoreAuthority> {
        CoreAuthority::open_declared(connection, Path::new("."), [], scope, "builder".into())
    }

    /// Complete reference columns, with accepted/classifier-refused/malformed rows and rows
    /// outside each kind of scope. Unlike the open-only fixture, this exercises the row recipe.
    fn reference_database(snow: SnowSource) -> Connection {
        let connection = database(snow);
        connection
            .execute_batch(
                "ALTER TABLE records ADD COLUMN data BLOB;
                 UPDATE records SET data = X'';
                 INSERT INTO records VALUES (18, 'STAT', X'');
                 INSERT INTO records VALUES (19, 'ACTI', X'');
                 INSERT INTO records VALUES (20, 'TREE', X'');
                 ALTER TABLE statics ADD COLUMN model_path TEXT;
                 INSERT INTO statics(id, model_path) VALUES (17, 'architecture/test.nif');
                 INSERT INTO statics(id, model_path) VALUES (18, NULL);
                 INSERT INTO statics(id, model_path) VALUES (20, 'trees/test.nif');
                 CREATE TABLE scripts (form_id INTEGER);
                 ALTER TABLE \"references\" ADD COLUMN pos_z REAL DEFAULT 0;
                 ALTER TABLE \"references\" ADD COLUMN rot_x REAL DEFAULT 0;
                 ALTER TABLE \"references\" ADD COLUMN rot_y REAL DEFAULT 0;
                 ALTER TABLE \"references\" ADD COLUMN rot_z REAL DEFAULT 0;
                 ALTER TABLE \"references\" ADD COLUMN scale REAL DEFAULT 1;
                 ALTER TABLE \"references\" ADD COLUMN header_flags INTEGER DEFAULT 0;
                 ALTER TABLE \"references\" ADD COLUMN enable_parent_id INTEGER;
                 ALTER TABLE \"references\" ADD COLUMN cell_id INTEGER DEFAULT 7;
                 ALTER TABLE \"references\" ADD COLUMN data BLOB DEFAULT X'';
                 INSERT INTO worldspaces VALUES (61, NULL, NULL, 60);
                 INSERT INTO worldspaces VALUES (62, NULL, NULL, NULL);
                 INSERT INTO \"references\"(id, base_form_id, worldspace_id, is_exterior,
                     pos_x, pos_y, header_flags) VALUES (257, 17, 60, 1, 100, 100, 2048);
                 INSERT INTO \"references\"(id, base_form_id, worldspace_id, is_exterior,
                     pos_x, pos_y) VALUES
                     (258, 18, 60, 1, 100, 100),
                     (259, 19, 60, 1, 100, 100),
                     (260, 17, 61, 1, 100, 100),
                     (261, 17, 60, 1, 100, 100),
                     (262, 17, 60, 1, 8192, 100),
                     (263, 17, 60, 0, 100, 100),
                     (264, 20, 60, 1, 100, 100),
                     (265, 17, 62, 1, 100, 100);
                 UPDATE \"references\" SET scale = 'malformed' WHERE id = 261;",
            )
            .unwrap();
        connection
    }

    struct DiskSourceFixture(std::path::PathBuf);

    impl DiskSourceFixture {
        fn new(snow: SnowSource) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "mudcrab-parallel-source-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&root).unwrap();
            std::fs::write(
                root.join("OWNER.txt"),
                "owner: shared parallel source preload test\nclass: A, regenerable\n",
            )
            .unwrap();
            let fixture = Self(root);
            reference_database(snow)
                .execute("VACUUM INTO ?1", [fixture.database().to_str().unwrap()])
                .unwrap();
            fixture
        }

        fn database(&self) -> std::path::PathBuf {
            self.0.join("source.db")
        }

        fn authority(&self, request: &ScopeRequest) -> CoreAuthority {
            CoreAuthority::open(
                Connection::open_with_flags(self.database(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .unwrap(),
                &self.0,
                [],
                request,
                "builder".into(),
            )
            .unwrap()
        }
    }

    impl Drop for DiskSourceFixture {
        fn drop(&mut self) {
            assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
            assert!(
                self.0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("mudcrab-parallel-source-")
            );
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn disk_parallel_preload_matches_sequential_rows_snapshots_and_refusals() {
        for snow in [
            SnowSource::None,
            SnowSource::StaticMaterials,
            SnowSource::StaticsColumn,
        ] {
            let disk = DiskSourceFixture::new(snow);
            for request in [
                ScopeRequest::default(),
                ScopeRequest {
                    include_trees: true,
                    child_worldspaces: vec![61],
                    area: None,
                },
                ScopeRequest {
                    include_trees: false,
                    child_worldspaces: vec![61],
                    area: Some(CellArea {
                        worldspace_id: 60,
                        min: (0, 0),
                        max: (0, 0),
                    }),
                },
            ] {
                let mut sequential = disk.authority(&request);
                let mut parallel = disk.authority(&request);
                // Include absent, interior, wrong-world and malformed fixture rows, alongside
                // classifier refusals and inherited child-world references.
                sequential.examined.extend(256..=266);
                parallel.examined = sequential.examined.clone();
                assert!(sequential.preload_references().is_none());
                let diagnostic = parallel.preload_references_parallel();
                assert!(diagnostic.is_none(), "{diagnostic:?}");
                assert_eq!(
                    parallel
                        .preloaded
                        .as_ref()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>(),
                    sequential
                        .preloaded
                        .as_ref()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>()
                );
                for id in 256..=266 {
                    assert_eq!(
                        format!("{:?}", parallel.preloaded_reference(id)),
                        format!("{:?}", sequential.preloaded_reference(id)),
                        "raw row {id}, snow={snow:?}, request={request:?}"
                    );
                }
                sequential.preload_snapshots().unwrap();
                parallel.preload_snapshots_with_headroom(1).unwrap();
                for id in 256..=266 {
                    assert_eq!(
                        parallel.reference(id),
                        sequential.reference(id),
                        "snapshot {id}, snow={snow:?}, request={request:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn parallel_preload_reports_recovery_for_connection_private_state() {
        let disk = DiskSourceFixture::new(SnowSource::None);
        for private_state in [
            "BEGIN",
            "CREATE TEMP TABLE private_source_state (id INTEGER)",
            "ATTACH DATABASE ':memory:' AS private_source_state",
        ] {
            let mut authority = disk.authority(&ScopeRequest::default());
            authority.connection.execute_batch(private_state).unwrap();
            let expected = authority.reference(256);
            assert!(authority.preload_references_parallel().is_some());
            assert_eq!(authority.reference(256), expected);
            authority.preload_snapshots().unwrap();
            assert_eq!(authority.reference(256), expected);
        }
        Connection::open(disk.database())
            .unwrap()
            .execute_batch("CREATE VIEW source_view AS SELECT id FROM statics")
            .unwrap();
        let mut with_view = disk.authority(&ScopeRequest::default());
        let expected = with_view.reference(256);
        let diagnostic = with_view.preload_references_parallel().unwrap();
        assert!(format!("{diagnostic:#}").contains("connection-sensitive views"));
        assert_eq!(with_view.reference(256), expected);
        drop(with_view);
        let mut memory = CoreAuthority::open(
            reference_database(SnowSource::None),
            Path::new("."),
            [],
            &ScopeRequest::default(),
            "builder".into(),
        )
        .unwrap();
        let expected = memory.reference(256);
        assert!(memory.preload_references_parallel().is_some());
        assert_eq!(memory.reference(256), expected);
    }

    #[test]
    fn parallel_query_failure_replays_original_per_id_reads_and_invalidates_snapshots() {
        let disk = DiskSourceFixture::new(SnowSource::None);
        let mut authority = disk.authority(&ScopeRequest::default());
        let diagnostic = authority.preload_references_parallel();
        assert!(diagnostic.is_none(), "{diagnostic:?}");
        authority.preload_snapshots().unwrap();
        // Worker SQL must honor this connection's limits. Both the partition query and
        // original bulk query now fail; per-ID recovery retains the original missing result.
        authority
            .connection
            .set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 256);
        assert!(authority.preload_references_parallel().is_some());
        assert!(authority.preloaded_snapshots.is_none());
        assert!(authority.preloaded_reference(256).is_none());
        assert!(authority.reference(256).is_none());
    }

    #[test]
    fn bulk_and_per_row_authorities_have_identical_snapshots_and_refusals() {
        for snow in [
            SnowSource::StaticMaterials,
            SnowSource::StaticsColumn,
            SnowSource::None,
        ] {
            for request in [
                ScopeRequest::default(),
                ScopeRequest {
                    include_trees: true,
                    child_worldspaces: vec![61],
                    area: None,
                },
                ScopeRequest {
                    include_trees: false,
                    child_worldspaces: vec![61],
                    area: Some(CellArea {
                        worldspace_id: 60,
                        min: (0, 0),
                        max: (0, 0),
                    }),
                },
            ] {
                let mut authority = CoreAuthority::open(
                    reference_database(snow),
                    Path::new("."),
                    [],
                    &request,
                    "builder".into(),
                )
                .unwrap();
                let ids = 256..=266; // Includes an absent id after all fixture rows.
                let bulk_rows = scoped_references_by_ids(
                    &authority.connection,
                    &authority.sources,
                    &ids.clone().collect(),
                    request.include_trees,
                )
                .unwrap();
                for id in ids.clone() {
                    let old = scoped_reference_by_id(
                        &authority.connection,
                        &authority.sources,
                        id,
                        request.include_trees,
                    )
                    .ok()
                    .flatten();
                    assert_eq!(
                        format!("{:?}", bulk_rows[&id]),
                        format!("{old:?}"),
                        "raw row id={id}, snow={snow:?}, request={request:?}"
                    );
                }
                let before: BTreeMap<_, _> = ids
                    .clone()
                    .map(|id| (id, authority.reference(id)))
                    .collect();
                let examined = authority.examined_references();
                assert!(authority.preloaded_reference(256).is_none());
                assert!(authority.preload_references().is_none());
                assert_eq!(authority.examined_references(), examined);
                for id in ids.clone() {
                    assert_eq!(
                        authority.reference(id),
                        before[&id],
                        "id={id}, snow={snow:?}, request={request:?}"
                    );
                }
                authority.preload_snapshots().unwrap();
                let view = authority.preloaded_authority().unwrap();
                fn requires_sync(_: &impl Sync) {}
                requires_sync(&view);
                assert_eq!(view.examined_references(), authority.examined_references());
                assert_eq!(view.classifier_scope(), authority.classifier_scope());
                assert_eq!(view.core_build_identity(), authority.core_build_identity());
                assert_eq!(view.builder_identity(), authority.builder_identity());
                assert_eq!(view.lod_origin(60), authority.lod_origin(60));
                for id in ids {
                    assert_eq!(view.reference(id), authority.reference(id));
                    assert_eq!(view.reference_view(id), authority.reference_view(id));
                    assert_eq!(
                        authority.reference(id),
                        before[&id],
                        "parallel id={id}, snow={snow:?}, request={request:?}"
                    );
                }
                assert_eq!(
                    authority
                        .preloaded_reference(257)
                        .unwrap()
                        .refusal
                        .as_deref(),
                    Some("initially_disabled")
                );
                assert_eq!(
                    authority
                        .preloaded_reference(258)
                        .unwrap()
                        .refusal
                        .as_deref(),
                    Some("no_model")
                );
                assert!(authority.preloaded_reference(259).is_none());
                assert!(authority.preloaded_reference(261).is_none());
                assert!(authority.preloaded_reference(263).is_none());
                assert!(authority.preloaded_reference(265).is_none());
                if request.child_worldspaces.contains(&61) {
                    let child = authority.preloaded_reference(260).unwrap();
                    assert_eq!(child.block_worldspace_id, 60);
                    assert_eq!(child.placed_worldspace_id, 61);
                } else {
                    assert!(authority.preloaded_reference(260).is_none());
                }
                assert_eq!(
                    authority.preloaded_reference(264).is_some(),
                    request.include_trees
                );
                assert_eq!(
                    authority.preloaded_reference(262).is_none(),
                    request.area.is_some()
                );
            }
        }
    }

    #[test]
    fn a_bulk_query_error_keeps_the_authoritys_unavailable_reference_result() {
        let mut authority =
            open_declared(database(SnowSource::None), declared(SnowSource::None)).unwrap();
        // This schema lacks the columns the reference query needs. The per-row authority
        // deliberately turns this query error into None, and the preload must do the same.
        assert!(authority.reference(256).is_none());
        assert!(authority.preload_references().is_some());
        assert!(authority.reference(256).is_none());
        assert!(authority.preloaded_reference(256).is_none());
        authority.preload_snapshots().unwrap();
        assert!(authority.reference(256).is_none());
    }

    #[test]
    fn a_bulk_query_storage_limit_reports_recovery_and_preserves_each_original_answer() {
        let mut authority = CoreAuthority::open(
            reference_database(SnowSource::None),
            Path::new("."),
            [],
            &ScopeRequest::default(),
            "builder".into(),
        )
        .unwrap();
        // The large bound ID array exceeds the limit, while individual source rows fit.
        // A batch-level error must not discard the valid earlier source reference.
        authority.examined.extend(1000..1100);
        let expected: BTreeMap<_, _> = authority
            .examined
            .iter()
            .map(|&id| (id, authority.reference(id)))
            .collect();
        assert!(expected[&256].is_some());
        authority
            .connection
            .set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, 200);
        assert!(authority.preload_references().is_some());
        authority.preload_snapshots().unwrap();
        for (&id, expected) in &expected {
            assert_eq!(&authority.reference(id), expected, "reference={id}");
        }
    }

    #[test]
    fn bulk_static_lod_metadata_matches_per_row_without_changing_snapshots() {
        let request = ScopeRequest {
            include_trees: true,
            ..Default::default()
        };
        let baseline = CoreAuthority::open(
            reference_database(SnowSource::None),
            Path::new("."),
            [],
            &request,
            "builder".into(),
        )
        .unwrap();
        for (schema, expected, static_is_tree) in [
            ("", None, false),
            ("ALTER TABLE statics ADD COLUMN flags INTEGER;", None, false),
            (
                "ALTER TABLE statics ADD COLUMN flags INTEGER DEFAULT 0;",
                Some(false),
                false,
            ),
            (
                "ALTER TABLE statics ADD COLUMN flags INTEGER DEFAULT 32768;",
                Some(true),
                false,
            ),
            (
                "ALTER TABLE statics ADD COLUMN flags INTEGER DEFAULT 64;",
                Some(false),
                true,
            ),
            (
                "ALTER TABLE statics ADD COLUMN flags INTEGER DEFAULT 32832;",
                Some(true),
                true,
            ),
        ] {
            let connection = reference_database(SnowSource::None);
            connection.execute_batch(schema).unwrap();
            let mut authority =
                CoreAuthority::open(connection, Path::new("."), [], &request, "builder".into())
                    .unwrap();
            let legacy =
                scoped_reference_by_id(&authority.connection, &authority.sources, 256, true)
                    .unwrap()
                    .unwrap();
            assert_eq!(legacy.base_has_distant_lod, expected);
            assert_eq!(legacy.is_tree, static_is_tree);
            assert_eq!(authority.reference(256), baseline.reference(256));
            assert!(authority.preload_references().is_none());
            let loaded = authority.preloaded_reference(256).unwrap();
            assert_eq!(loaded.base_has_distant_lod, expected);
            assert_eq!(loaded.is_tree, static_is_tree);
            assert_eq!(format!("{legacy:?}"), format!("{loaded:?}"));
            assert_eq!(authority.reference(256), baseline.reference(256));
            let tree = authority.preloaded_reference(264).unwrap();
            assert_eq!(tree.base_has_distant_lod, expected);
            assert!(tree.is_tree);
            assert_eq!(authority.reference(264), baseline.reference(264));
            authority.preload_snapshots().unwrap();
            for id in [256, 264] {
                assert_eq!(authority.reference(id), baseline.reference(id));
            }
        }
    }

    #[test]
    fn snapshots_require_preloaded_rows_and_reloading_invalidates_the_snapshot_cache() {
        let mut authority = CoreAuthority::open(
            reference_database(SnowSource::None),
            Path::new("."),
            [],
            &ScopeRequest::default(),
            "builder".into(),
        )
        .unwrap();
        assert!(authority.preloaded_authority().is_err());
        assert!(authority.preload_snapshots().is_err());
        assert!(authority.preload_references().is_none());
        authority.preload_snapshots().unwrap();
        let expected = authority.reference(256);
        authority.preload_snapshots().unwrap();
        assert_eq!(authority.reference(256), expected);
        assert!(authority.preload_references().is_none());
        assert!(authority.preloaded_snapshots.is_none());
        assert_eq!(authority.reference(256), expected);
        authority.preload_snapshots().unwrap();
        assert_eq!(authority.reference(256), expected);
    }

    #[test]
    fn parallel_cold_catalog_preserves_model_texture_and_alias_refusals() {
        use crate::cluster_mapping::{
            Eligibility, REASON_MODEL_CONTENT_CHANGED, REASON_MODEL_NOT_LOADED,
            REASON_TEXTURE_CONTENT_CHANGED, REASON_TEXTURE_NOT_LOADED,
        };
        use sha2::{Digest, Sha256};
        use std::{fs, path::PathBuf};

        struct Files(PathBuf);
        impl Drop for Files {
            fn drop(&mut self) {
                assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
                assert!(
                    self.0
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("mudcrab-parallel-authority-")
                );
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mudcrab-parallel-authority-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let files = Files(root);
        fs::write(
            files.0.join("OWNER.txt"),
            b"owner: cert-speed parallel source authority test\npurpose: generated model/texture refusal fixtures\nclass: A, regenerable\n",
        )
        .unwrap();
        fs::create_dir_all(files.0.join("meshes/architecture")).unwrap();
        fs::create_dir(files.0.join("textures")).unwrap();
        let document = serde_json::json!({
            "asset": {"version":"2.0"}, "scene":0,
            "scenes":[{"nodes":[0]}], "nodes":[{"mesh":0}],
            "meshes":[{"primitives":[{"material":0}]}],
            "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],
            "textures":[{"source":0}],
            "images":[{"uri":"../../textures/a.opensky-srgb.opensky-wrap1.ktx2"}],
        });
        let mut json = serde_json::to_vec(&document).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(20u32 + json.len() as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        let texture = b"invented bytes, certification hashes without decoding images";
        let model_path = files.0.join("meshes/architecture/test.glb");
        let source_path = files.0.join("textures/a.ktx2");
        let alias_path = files.0.join("textures/a.opensky-srgb.opensky-wrap1.ktx2");
        let hash = |bytes: &[u8]| crate::cluster::hex(&Sha256::digest(bytes));
        for (failure, reason) in [
            ("none", None),
            ("model_missing", Some(REASON_MODEL_NOT_LOADED)),
            ("model_changed", Some(REASON_MODEL_CONTENT_CHANGED)),
            ("model_invalid", Some(REASON_MODEL_NOT_LOADED)),
            ("alias_missing", Some(REASON_TEXTURE_NOT_LOADED)),
            ("alias_changed", Some(REASON_TEXTURE_CONTENT_CHANGED)),
            ("source_missing", Some(REASON_TEXTURE_NOT_LOADED)),
            ("source_changed", Some(REASON_TEXTURE_CONTENT_CHANGED)),
        ] {
            fs::write(&model_path, &glb).unwrap();
            fs::write(&source_path, texture).unwrap();
            fs::write(&alias_path, texture).unwrap();
            let mut model_hash = hash(&glb);
            match failure {
                "model_missing" => fs::remove_file(&model_path).unwrap(),
                "model_changed" => fs::write(&model_path, b"changed model").unwrap(),
                "model_invalid" => {
                    fs::write(&model_path, b"not a GLB").unwrap();
                    model_hash = hash(b"not a GLB");
                }
                "alias_missing" => fs::remove_file(&alias_path).unwrap(),
                "alias_changed" => fs::write(&alias_path, b"changed alias").unwrap(),
                "source_missing" => fs::remove_file(&source_path).unwrap(),
                "source_changed" => fs::write(&source_path, b"changed source").unwrap(),
                "none" => {}
                _ => unreachable!(),
            }
            let published = vec![
                ("meshes/architecture/test.glb".to_string(), model_hash),
                ("textures/a.ktx2".to_string(), hash(texture)),
            ];
            let open = || {
                let connection = reference_database(SnowSource::None);
                let data =
                    rkyv::to_bytes::<rkyv::rancor::Error>(&crate::records::ArchivedRecordData {
                        subrecords: vec![],
                    })
                    .unwrap();
                connection
                    .execute("UPDATE records SET data = ?1", [data.as_slice()])
                    .unwrap();
                connection
                    .execute("UPDATE \"references\" SET data = ?1", [data.as_slice()])
                    .unwrap();
                CoreAuthority::open(
                    connection,
                    &files.0,
                    published.clone(),
                    &ScopeRequest::default(),
                    "builder".into(),
                )
                .unwrap()
            };
            let legacy = open();
            let mut parallel = open();
            assert!(parallel.preload_references().is_none());
            parallel.preload_snapshots().unwrap();
            for id in 256..=266 {
                assert_eq!(
                    parallel.reference(id),
                    legacy.reference(id),
                    "{failure}: {id}"
                );
            }
            let expected = reason.map_or(Eligibility::Eligible, |reason| {
                Eligibility::Unknown(reason.into())
            });
            assert_eq!(
                parallel.reference(256).unwrap().eligibility,
                expected,
                "{failure}"
            );
        }
    }

    #[test]
    fn a_declared_scope_matching_the_database_opens_and_answers_as_declared() {
        for snow in [
            SnowSource::StaticMaterials,
            SnowSource::StaticsColumn,
            SnowSource::None,
        ] {
            let scope = declared(snow);
            let authority = open_declared(database(snow), scope.clone()).unwrap();
            assert_eq!(authority.classifier_scope(), scope.json());
            assert_eq!(authority.core_build_identity().as_deref(), Some("core-1"));
            assert_eq!(authority.builder_identity(), "builder");
            assert_eq!(authority.lod_origin(60), Some((-32, -32)));
            assert_eq!(authority.examined_references(), BTreeSet::from([256]));
        }
    }

    #[test]
    fn a_stale_declared_snow_source_is_refused() {
        let all = [
            SnowSource::StaticMaterials,
            SnowSource::StaticsColumn,
            SnowSource::None,
        ];
        for current in all {
            for stale in all.into_iter().filter(|stale| *stale != current) {
                let error = open_declared(database(current), declared(stale))
                    .err()
                    .unwrap_or_else(|| panic!("declared {stale:?} opened over {current:?}"));
                assert!(
                    format!("{error:#}").contains("declared snow source"),
                    "{error:#}"
                );
            }
        }
    }

    #[test]
    fn the_declared_and_requested_paths_agree_over_the_same_database() {
        let request = ScopeRequest {
            include_trees: false,
            child_worldspaces: vec![],
            area: None,
        };
        let requested = CoreAuthority::open(
            database(SnowSource::StaticMaterials),
            Path::new("."),
            [],
            &request,
            "builder".into(),
        )
        .unwrap();
        let declared = open_declared(
            database(SnowSource::StaticMaterials),
            declared(SnowSource::StaticMaterials),
        )
        .unwrap();
        assert_eq!(requested.classifier_scope(), declared.classifier_scope());
        assert_eq!(
            requested.examined_references(),
            declared.examined_references()
        );
    }
}

//! Which data set extensions this engine reads, and what a feature does without its extension.
//!
//! A converted data set is `main`'s core plus the extensions listed in its
//! `conversion-manifest.json` (`shared::extensions`, `docs/specs/converters/extensions.md`). Each
//! engine feature that reads extension data names the extension and the versions it reads
//! ([`Feature`]). When the data set is opened, [`open_features`] asks the manifest about every
//! feature once:
//!
//! * **present** (listed, a version this engine reads, derived from the data set's current core): the
//!   feature's reader runs. A listed extension whose table cannot be read is a corrupt data set, an
//!   error that stops the run;
//! * **missing, stale or unsupported version**: nothing of the feature is built (no system, no
//!   resource, no query), one `warn!` line says why and what to do (convert again), and an
//!   [`ExtensionNotice`] is kept for the settings menu to grey the entry with the same reason.
//!
//! Door links are now upstream core output. Without a legacy declaration, the database reader
//! discovers the complete core door schema. A stale or unsupported legacy declaration still
//! disables its reader; it cannot be bypassed through core discovery.
//!
//! Design: `local/research/op6-one-data-set-design-2026-10-16.md`, section 3.3.

use std::ops::RangeInclusive;
use std::path::Path;

use bevy::prelude::*;
use color_eyre::eyre::{Result, WrapErr};
use shared::extensions::{DOOR_LINKS, DataExtensions, ExtensionStatus, MATERIAL};

use crate::world::database::{DoorLinksAccess, WorldDatabaseFeatures, has_door_links};

/// An engine feature that needs a data set extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Feature {
    /// The portal graph and every load door's destination (`door_links`): [`crate::portal_graph`],
    /// [`crate::world::database`].
    DoorLinks,
    /// UV transform, effect falloff, blend factors and material animation in the meshes'
    /// `MUDCRAB_material` glTF extension. The mesh readers treat an absent extension as identity
    /// values when this data-set capability is unusable, even if a mesh carries those keys.
    Material,
}

impl Feature {
    /// Every feature, in the order notices are listed.
    pub const ALL: [Self; 2] = [Self::DoorLinks, Self::Material];

    /// The extension id the feature reads.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::DoorLinks => DOOR_LINKS,
            Self::Material => MATERIAL,
        }
    }

    /// What the settings menu and the log call the feature.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DoorLinks => "load-door links and portal graph",
            Self::Material => "extra material data (UV transform, falloff, blend, animation)",
        }
    }

    /// The versions of the extension this engine reads.
    pub const fn versions(self) -> RangeInclusive<u32> {
        1..=1
    }
}

/// One feature that is off because its extension is not usable in the loaded data set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionNotice {
    /// The extension id (`mudcrab.door_links`).
    pub extension: &'static str,
    /// The feature's name, for the menu entry ([`Feature::label`]).
    pub feature: &'static str,
    /// Why it is off and what to do, the [`ExtensionStatus`] text.
    pub reason: String,
}

/// Every feature that is off for lack of its extension, in [`Feature::ALL`] order. The settings menu
/// greys an entry with the matching notice's reason.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtensionNotices(Vec<ExtensionNotice>);

impl ExtensionNotices {
    pub fn iter(&self) -> impl Iterator<Item = &ExtensionNotice> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The notice for a feature, if it is off.
    pub fn for_feature(&self, feature: Feature) -> Option<&ExtensionNotice> {
        self.0
            .iter()
            .find(|notice| notice.extension == feature.extension())
    }
}

/// The extensions of the loaded data set, read once when it is opened.
#[derive(Resource, Clone, Debug, Default)]
pub struct DataSetExtensions {
    data: DataExtensions,
}

impl DataSetExtensions {
    pub fn new(data: DataExtensions) -> Self {
        Self { data }
    }

    /// The status of the extension a feature needs, for the versions this engine reads.
    pub fn status(&self, feature: Feature) -> ExtensionStatus {
        self.data.status(feature.extension(), feature.versions())
    }

    pub fn data(&self) -> &DataExtensions {
        &self.data
    }
}

/// Reads the extensions list of `assets_dir`'s `conversion-manifest.json` (and the core build
/// identity from its database). A data set without a manifest (a fixture, or a run started with
/// `--allow-incomplete-assets`) lists no extensions, so every feature reports missing; a manifest
/// that cannot be read is an error.
pub fn read_data_extensions(assets_dir: &Path) -> Result<DataExtensions> {
    if !assets_dir.join("conversion-manifest.json").is_file() {
        return Ok(DataExtensions::default());
    }
    DataExtensions::load(assets_dir)
        .wrap_err_with(|| format!("cannot read the extensions of {}", assets_dir.display()))
}

/// The text kept in a notice and logged: the status, plus what to do when the status alone does not
/// say it.
fn reason(status: &ExtensionStatus) -> String {
    match status {
        ExtensionStatus::UnsupportedVersion { .. } => {
            format!("{status}; update the engine, or convert the data set again with this build")
        }
        _ => status.to_string(),
    }
}

/// One line of `world-inspect`'s extension list.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ExtensionLine {
    pub id: String,
    /// The version the data set lists; `None` for a known extension the data set lacks.
    pub version: Option<u32>,
    /// The feature that reads it, when this engine has one.
    pub feature: Option<&'static str>,
    /// The [`ExtensionStatus`] text, or "not read by this engine" for an unknown extension.
    pub status: String,
}

/// The extension list `world-inspect` prints: every extension the data set lists (with the status
/// this engine gives it) followed by every extension the engine knows and the data set lacks. Empty
/// for a data set that lists none (a `main` conversion), where there is nothing to print.
pub fn describe(data: &DataExtensions) -> Vec<ExtensionLine> {
    if data.entries().next().is_none() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for entry in data.entries() {
        let feature = Feature::ALL
            .into_iter()
            .find(|feature| feature.extension() == entry.id);
        lines.push(ExtensionLine {
            id: entry.id.clone(),
            version: Some(entry.version),
            feature: feature.map(Feature::label),
            status: feature.map_or_else(
                || "not read by this engine".to_owned(),
                |feature| {
                    data.status(feature.extension(), feature.versions())
                        .to_string()
                },
            ),
        });
    }
    for feature in Feature::ALL {
        if data.get(feature.extension()).is_none() {
            lines.push(ExtensionLine {
                id: feature.extension().to_owned(),
                version: None,
                feature: Some(feature.label()),
                status: ExtensionStatus::Missing.to_string(),
            });
        }
    }
    lines
}

/// The public capability state used by the database and mesh readers.
/// Private extension catalogs are installed by their introducing adapters.
pub struct RuntimeFeatures {
    pub extensions: DataSetExtensions,
    pub notices: ExtensionNotices,
    door_links: DoorLinksAccess,
    has_doors: bool,
}

impl RuntimeFeatures {
    pub fn world_database(&self) -> WorldDatabaseFeatures {
        WorldDatabaseFeatures {
            door_links: self.door_links,
        }
    }

    pub fn enabled(&self, feature: Feature) -> bool {
        if feature == Feature::DoorLinks {
            self.has_doors && self.door_links != DoorLinksAccess::Disabled
        } else {
            self.extensions.status(feature).is_present()
        }
    }

    pub fn install(self, app: &mut App) {
        app.insert_resource(self.extensions)
            .insert_resource(self.notices);
    }
}

/// Opens the public capability gates without importing private feature readers.
/// An invalid legacy declaration cannot bypass the gate through core discovery.
pub fn open_features(assets_dir: &Path, data: DataExtensions) -> Result<RuntimeFeatures> {
    let extensions = DataSetExtensions::new(data);
    let database = assets_dir.join("skyrim_world.db");
    let has_doors = if database.is_file() {
        let connection = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        has_door_links(&connection)?
    } else {
        false
    };
    let declared = extensions.data().get(DOOR_LINKS).is_some();
    let door_links = if !declared {
        DoorLinksAccess::Core
    } else if extensions.status(Feature::DoorLinks).is_present() {
        color_eyre::eyre::ensure!(
            has_doors,
            "extension {} is listed but its door_links table lacks columns",
            DOOR_LINKS
        );
        DoorLinksAccess::LegacyExtension
    } else {
        DoorLinksAccess::Disabled
    };
    let mut notices = Vec::new();
    for feature in Feature::ALL {
        if feature == Feature::DoorLinks && !declared && has_doors {
            continue;
        }
        let status = extensions.status(feature);
        if !status.is_present() {
            let reason = reason(&status);
            warn!(
                feature = feature.label(),
                extension = feature.extension(),
                "{} is off: extension {}: {reason}",
                feature.label(),
                feature.extension()
            );
            notices.push(ExtensionNotice {
                extension: feature.extension(),
                feature: feature.label(),
                reason,
            });
        }
    }
    Ok(RuntimeFeatures {
        extensions,
        notices: ExtensionNotices(notices),
        door_links,
        has_doors,
    })
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use rusqlite::Connection;
    use shared::extensions::{ExtensionEntry, ExtensionOwns};
    use std::collections::BTreeMap;

    /// Every table the four features read, in the shape the converter writes them, plus the core
    /// tables the portal graph joins.
    pub const TABLES: &str = "
        CREATE TABLE cells(id INTEGER PRIMARY KEY, worldspace_id INTEGER);
        CREATE TABLE \"references\"(id INTEGER PRIMARY KEY, cell_id INTEGER NOT NULL,
            pos_x REAL NOT NULL, pos_y REAL NOT NULL, pos_z REAL NOT NULL);
        CREATE TABLE door_links(ref_id INTEGER PRIMARY KEY, destination_ref_id INTEGER NOT NULL,
            pos_x REAL NOT NULL, pos_y REAL NOT NULL, pos_z REAL NOT NULL,
            rot_x REAL NOT NULL, rot_y REAL NOT NULL, rot_z REAL NOT NULL,
            destination_cell_id INTEGER, destination_worldspace_id INTEGER);
        CREATE TABLE static_materials(id INTEGER PRIMARY KEY,
            material_object INTEGER NOT NULL, material_max_angle REAL NOT NULL);
        CREATE TABLE matos(id INTEGER PRIMARY KEY, editor_id TEXT,
            falloff_scale REAL NOT NULL, falloff_bias REAL NOT NULL,
            noise_uv_scale REAL NOT NULL, material_uv_scale REAL NOT NULL,
            dir_proj_x REAL NOT NULL, dir_proj_y REAL NOT NULL, dir_proj_z REAL NOT NULL,
            normal_dampener REAL NOT NULL,
            single_pass_color INTEGER NOT NULL, single_pass INTEGER NOT NULL);
        CREATE TABLE space_lighting(
            space_id INTEGER PRIMARY KEY, is_interior INTEGER NOT NULL, template_id INTEGER,
            ambient INTEGER, directional INTEGER, fog INTEGER,
            fog_near REAL, fog_far REAL, fog_power REAL, fog_clip REAL,
            direction_rot_xy INTEGER, direction_rot_z INTEGER, direction_fade REAL,
            sky_upper INTEGER, sky_fog INTEGER, sky_lower INTEGER,
            sun INTEGER, sun_illuminance REAL, climate_id INTEGER, weather_id INTEGER,
            has_sky INTEGER NOT NULL,
            fog_far_color INTEGER, fog_max REAL, light_fade_begin REAL, light_fade_end REAL);
        INSERT INTO cells VALUES(5, 60);
        INSERT INTO cells VALUES(100, NULL);
        INSERT INTO \"references\" VALUES(1, 5, 10, 20, 30);
        INSERT INTO \"references\" VALUES(2, 100, 1, 2, 3);
        INSERT INTO door_links VALUES(1, 2, 1, 2, 3, 0, 0, 1.5, 100, NULL);
        INSERT INTO door_links VALUES(2, 1, 10, 20, 30, 0, 0, 0.5, 5, 60);
        INSERT INTO matos VALUES(151849, 'SnowMaterialObject1P', 0.35, 0.4, 48.0, 170.66667,
            0.0, 0.0, -1.0, 0.4, 8287339, 1);
        INSERT INTO static_materials VALUES(900, 151849, 120.0);
        INSERT INTO space_lighting(space_id, is_interior, has_sky) VALUES(7, 1, 0);
    ";

    /// The manifest of a `main` data set: no `extensions` key.
    pub const MAIN_MANIFEST: &str = r#"{"schema_version": 26, "complete": true}"#;

    /// A manifest text listing `entries` as `(id, version, built_from)`.
    pub fn manifest(entries: &[(&str, u32, &str)]) -> String {
        let extensions: Vec<ExtensionEntry> = entries
            .iter()
            .map(|(id, version, built_from)| ExtensionEntry {
                id: (*id).to_owned(),
                version: *version,
                owns: ExtensionOwns::default(),
                sha256: String::new(),
                built_from: (*built_from).to_owned(),
                stats: BTreeMap::new(),
            })
            .collect();
        serde_json::json!({"schema_version": 26, "complete": true, "extensions": extensions})
            .to_string()
    }

    /// A data set folder: the manifest text and a database holding [`TABLES`] (without the ones in
    /// `without`), whose `lod_build` identity is `core` when it is given.
    pub fn data_set(
        manifest_text: &str,
        core: Option<&str>,
        without: &[&str],
    ) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("conversion-manifest.json"),
            manifest_text,
        )
        .unwrap();
        let connection = Connection::open(directory.path().join("skyrim_world.db")).unwrap();
        connection.execute_batch(TABLES).unwrap();
        for table in without {
            connection
                .execute_batch(&format!("DROP TABLE \"{table}\";"))
                .unwrap();
        }
        if let Some(core) = core {
            connection
                .execute_batch(&format!(
                    "CREATE TABLE lod_build(id INTEGER PRIMARY KEY CHECK (id=1), build_identity TEXT NOT NULL);
                     INSERT INTO lod_build VALUES(1, '{core}');"
                ))
                .unwrap();
        }
        directory
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{MAIN_MANIFEST, data_set, manifest};
    use super::*;
    use shared::extensions::LOD_INVENTORY;
    fn open(directory: &tempfile::TempDir) -> Result<RuntimeFeatures> {
        open_features(
            directory.path(),
            read_data_extensions(directory.path()).unwrap(),
        )
    }

    fn only(feature: Feature, version: u32, built_from: &str) -> String {
        manifest(&[(feature.extension(), version, built_from)])
    }

    #[test]
    fn core_door_links_need_no_legacy_extension_declaration() {
        let directory = data_set(MAIN_MANIFEST, None, &[]);
        let features = open(&directory).unwrap();
        assert!(features.enabled(Feature::DoorLinks));
        assert!(features.notices.for_feature(Feature::DoorLinks).is_none());
    }

    #[test]
    fn converter_core_schema_opens_without_a_door_extension() {
        let directory = tempfile::tempdir().unwrap();
        let connection =
            rusqlite::Connection::open(directory.path().join("skyrim_world.db")).unwrap();
        converter::esm::exporter::create_tables(&connection).unwrap();
        drop(connection);
        std::fs::write(directory.path().join("conversion-manifest.json"),
            serde_json::json!({"schema_version": shared::LOD_CONVERTER_SCHEMA_VERSION, "complete": true}).to_string()).unwrap();
        let features = open(&directory).unwrap();
        assert!(features.enabled(Feature::DoorLinks));
        assert_eq!(features.world_database().door_links, DoorLinksAccess::Core);
        assert!(features.extensions.data().get(DOOR_LINKS).is_none());
        crate::world::database::WorldDatabase::open_with(
            &directory.path().join("skyrim_world.db"),
            features.world_database(),
        )
        .unwrap();
    }

    #[test]
    fn a_missing_manifest_lists_no_extensions() {
        let directory = tempfile::tempdir().unwrap();
        let data = read_data_extensions(directory.path()).unwrap();
        assert_eq!(data.entries().count(), 0);
    }

    #[test]
    fn a_manifest_that_cannot_be_read_is_an_error() {
        let directory = data_set("{ not json", None, &[]);
        assert!(read_data_extensions(directory.path()).is_err());
    }

    #[test]
    fn a_stale_extension_is_off_with_the_reason() {
        for feature in Feature::ALL {
            let directory = data_set(&only(feature, 1, "older"), Some("core"), &[]);
            let features = open(&directory).unwrap();
            let notice = features.notices.for_feature(feature).unwrap();
            assert!(
                notice
                    .reason
                    .contains("built from another conversion (older)")
                    && notice.reason.contains("convert the data set again"),
                "{}",
                notice.reason
            );
            assert!(!features.enabled(feature));
            if feature == Feature::DoorLinks {
                assert_eq!(
                    features.world_database().door_links,
                    DoorLinksAccess::Disabled
                );
            }
        }
    }

    #[test]
    fn an_unsupported_version_is_off_with_the_reason() {
        for feature in Feature::ALL {
            let directory = data_set(&only(feature, 2, "core"), Some("core"), &[]);
            let features = open(&directory).unwrap();
            let notice = features.notices.for_feature(feature).unwrap();
            assert!(
                notice
                    .reason
                    .contains("version 2, but this build reads versions 1 to 1")
                    && notice.reason.contains("convert the data set again"),
                "{}",
                notice.reason
            );
            assert!(!features.enabled(feature));
            if feature == Feature::DoorLinks {
                assert_eq!(
                    features.world_database().door_links,
                    DoorLinksAccess::Disabled
                );
            }
        }
    }

    #[test]
    fn extensions_the_engine_does_not_read_are_ignored() {
        let directory = data_set(
            &manifest(&[(LOD_INVENTORY, 1, "core"), ("someone.else", 9, "x")]),
            Some("core"),
            &[],
        );
        let features = open(&directory).unwrap();
        assert_eq!(features.notices.len(), 1);
        assert!(features.enabled(Feature::DoorLinks));
    }
    #[test]
    fn a_declared_door_extension_requires_its_complete_table() {
        let directory = data_set(
            &only(Feature::DoorLinks, 1, "core"),
            Some("core"),
            &["door_links"],
        );
        let error = open(&directory)
            .err()
            .expect("missing declared table must fail");
        assert!(format!("{error:#}").contains("door_links"));
    }
}

//! Per-user conversion choices, independent of the geometry mode of an existing output.

use super::{ConversionStatus, GamePathConfig};
use crate::{LauncherSet, mo2_settings::SourceSettings};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversionSettings {
    pub clustered_geometry: bool,
}

impl ConversionSettings {
    /// Applies the same geometry choice to a new conversion and a resumed conversion.
    pub fn apply(&self, config: &mut converter::PipelineConfig) {
        config.lod_mode = if self.clustered_geometry {
            converter::cluster::LodMode::Cluster
        } else {
            converter::cluster::LodMode::Baked
        };
        config.cluster_terrain = self.clustered_geometry;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedMo2 {
    instance_path: PathBuf,
    profile: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct SavedSettings {
    conversion: ConversionSettings,
    skyrim_data_path: Option<PathBuf>,
    converted_assets_path: PathBuf,
    use_mo2: bool,
    mo2: Option<SavedMo2>,
}

impl Default for SavedSettings {
    fn default() -> Self {
        Self {
            conversion: ConversionSettings::default(),
            skyrim_data_path: None,
            converted_assets_path: GamePathConfig::default().converted_assets_path,
            use_mo2: false,
            mo2: None,
        }
    }
}

impl SavedSettings {
    fn capture(
        settings: &ConversionSettings,
        paths: &GamePathConfig,
        source: &SourceSettings,
    ) -> Self {
        Self {
            conversion: settings.clone(),
            skyrim_data_path: paths.skyrim_data_path.clone(),
            converted_assets_path: paths.converted_assets_path.clone(),
            use_mo2: source.use_mo2,
            mo2: source.selection.as_ref().map(|selection| SavedMo2 {
                instance_path: selection.instance_path.clone(),
                profile: selection.profile.clone(),
            }),
        }
    }

    fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        // Truncation cannot corrupt the previous settings if writing the replacement fails.
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let result =
            std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, path));
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}

#[derive(Resource)]
struct SettingsFile {
    path: PathBuf,
    observed: SavedSettings,
}

fn settings_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let folder = std::env::var_os("APPDATA");
    #[cfg(not(windows))]
    let folder = std::env::var_os("XDG_CONFIG_HOME").or_else(|| std::env::var_os("HOME"));
    folder.map(|folder| PathBuf::from(folder).join("mudcrab-launcher.json"))
}

/// Installs disk persistence only for the running launcher, leaving headless logic reusable.
pub fn install(app: &mut App) {
    app.add_systems(
        Startup,
        load_settings.before(super::panel::detect_skyrim_at_start),
    )
    .add_systems(Update, save_settings.in_set(LauncherSet::Draw));
}

fn load_settings(world: &mut World) {
    let Some(path) = settings_path() else {
        world.resource_mut::<ConversionStatus>().push_notice(
            "Cannot locate the user configuration folder; conversion settings cannot be saved.",
        );
        return;
    };
    load_settings_from(path, world);
}

fn load_settings_from(path: PathBuf, world: &mut World) {
    let saved = match SavedSettings::load(&path) {
        Ok(saved) => saved,
        Err(error) => {
            world.resource_mut::<ConversionStatus>().push_notice(&format!(
                "Cannot read launcher settings at {}: {error}. Saving is disabled until the file is repaired and the launcher restarted.", path.display()));
            return;
        }
    };
    *world.resource_mut::<ConversionSettings>() = saved.conversion;
    {
        let mut paths = world.resource_mut::<GamePathConfig>();
        paths.skyrim_data_path = saved.skyrim_data_path;
        paths.converted_assets_path = saved.converted_assets_path;
    }
    let restored = {
        let mut source = world.resource_mut::<SourceSettings>();
        source.use_mo2 = saved.use_mo2;
        source.restore_selection(saved.mo2.map(|selection| mo2::Selection {
            instance_path: selection.instance_path,
            profile: selection.profile,
        }))
    };
    if let Err(error) = restored {
        world.resource_mut::<ConversionStatus>().push_notice(&error);
    }
    let observed = SavedSettings::capture(
        world.resource::<ConversionSettings>(),
        world.resource::<GamePathConfig>(),
        world.resource::<SourceSettings>(),
    );
    world.insert_resource(SettingsFile { path, observed });
}

fn save_settings(
    file: Option<ResMut<SettingsFile>>,
    settings: Res<ConversionSettings>,
    paths: Res<GamePathConfig>,
    source: Res<SourceSettings>,
    mut status: ResMut<ConversionStatus>,
) {
    let Some(mut file) = file else { return };
    if !settings.is_changed() && !paths.is_changed() && !source.is_changed() {
        return;
    }
    let saved = SavedSettings::capture(&settings, &paths, &source);
    if saved == file.observed {
        return;
    }
    file.observed = saved.clone();
    if let Err(error) = saved.save(&file.path) {
        status.push_notice(&format!(
            "Cannot save launcher settings at {}: {error}",
            file.path.display()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_settings_default_to_baked_and_map_both_choices() {
        let mut config = converter::PipelineConfig::new("data", "output");
        ConversionSettings {
            clustered_geometry: true,
        }
        .apply(&mut config);
        assert_eq!(config.lod_mode, converter::cluster::LodMode::Cluster);
        assert!(config.cluster_terrain);
        ConversionSettings::default().apply(&mut config);
        assert_eq!(config.lod_mode, converter::cluster::LodMode::Baked);
        assert!(!config.cluster_terrain);
    }

    #[test]
    fn conversion_choices_round_trip_with_paths_and_source() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join("OWNER.txt"),
            "Owner: #2144 launcher settings fixture; lifetime: test.\n",
        )
        .unwrap();
        let path = folder.path().join("settings.json");
        let saved = SavedSettings {
            conversion: ConversionSettings {
                clustered_geometry: true,
            },
            skyrim_data_path: Some("Data".into()),
            converted_assets_path: "converted".into(),
            use_mo2: true,
            mo2: Some(SavedMo2 {
                instance_path: "instance".into(),
                profile: "profile".into(),
            }),
        };
        saved.save(&path).unwrap();
        let loaded = SavedSettings::load(&path).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&saved).unwrap()
        );
        assert!(
            !serde_json::from_str::<SavedSettings>("{}")
                .unwrap()
                .conversion
                .clustered_geometry
        );
    }

    #[test]
    fn corrupt_settings_are_reported_instead_of_replaced() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join("OWNER.txt"),
            "Owner: #2144 corrupt settings fixture; lifetime: test.\n",
        )
        .unwrap();
        let path = folder.path().join("settings.json");
        std::fs::write(&path, b"bad json").unwrap();
        assert!(SavedSettings::load(&path).is_err());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ConversionSettings>()
            .init_resource::<GamePathConfig>()
            .init_resource::<SourceSettings>()
            .init_resource::<ConversionStatus>()
            .add_systems(Update, save_settings);
        load_settings_from(path.clone(), app.world_mut());
        // Startup autodetection must not turn a read failure into an overwrite with defaults.
        app.world_mut()
            .resource_mut::<GamePathConfig>()
            .skyrim_data_path = Some("detected/Data".into());
        app.update();
        assert!(!app.world().contains_resource::<SettingsFile>());
        assert_eq!(std::fs::read(path).unwrap(), b"bad json");
    }
}

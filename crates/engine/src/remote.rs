//! Opt-in Bevy Remote Protocol (BRP) server for inspecting a running engine.
//!
//! Built only with the engine's `remote` cargo feature and started only with `--remote`. The server
//! listens on 127.0.0.1 and has no authentication: it is a local development tool, never something
//! to expose on a network.

use crate::config::EngineConfig;
use bevy::prelude::*;
use color_eyre::Result;

/// The port Bevy's remote HTTP plugin always uses for the render world's server. It is not
/// configurable in Bevy 0.19, so `--remote-port` must not take it.
pub const REMOTE_RENDER_PORT: u16 = 15703;

/// The server settings asked for on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteSettings {
    pub port: u16,
}

impl RemoteSettings {
    /// Returns the settings when `--remote` was given, `None` when it was not, and an error when it
    /// cannot be honoured: the engine was built without the `remote` feature, or the port is one
    /// Bevy's server cannot use.
    pub fn from_config(config: &EngineConfig) -> Result<Option<Self>> {
        if !config.remote {
            return Ok(None);
        }
        if !cfg!(feature = "remote") {
            color_eyre::eyre::bail!(
                "--remote needs an engine built with the `remote` feature; rebuild with \
                 `cargo build -p engine --features remote`"
            );
        }
        if config.remote_port == 0 || config.remote_port == REMOTE_RENDER_PORT {
            color_eyre::eyre::bail!(
                "--remote-port {} is not usable: pick a fixed port other than 0 and \
                 {REMOTE_RENDER_PORT} (Bevy's render-world server port)",
                config.remote_port
            );
        }
        Ok(Some(Self {
            port: config.remote_port,
        }))
    }
}

/// Adds the Bevy Remote Protocol plugins, bound to 127.0.0.1, when `settings` is present.
#[cfg(feature = "remote")]
pub fn add_remote_plugins(app: &mut App, settings: Option<RemoteSettings>) {
    use crate::{
        doors::{DoorLeaf, DoorState, LoadDoor},
        player::{Player, PlayerMode},
        portal::StreamedCellKey,
        streaming::{ActiveCell, AssetFailure, RenderOrigin, StreamingMetrics},
        transition::CrossingHeld,
        world::components::{
            CellRef, DistantTerrainRoot, ExpectedModelBounds, ExteriorCellGrid, FormId,
            InstanceBounds, MaterialHandle, MeshHandle, StreamedCellRoot, StreamingCamera,
            TerrainPatch, WaterSurface, WorldPosition, WorldTransform,
        },
    };
    use bevy::remote::{RemotePlugin, http::RemoteHttpPlugin};
    use std::net::Ipv4Addr;

    let Some(settings) = settings else {
        return;
    };
    app.add_plugins((
        RemotePlugin::default(),
        RemoteHttpPlugin::default()
            .with_address(Ipv4Addr::LOCALHOST)
            .with_port(settings.port),
    ))
    // Only reflected types are visible to `world.query` and friends. Automatic registration covers
    // these on most platforms; registering them here keeps the list explicit.
    .register_type::<FormId>()
    .register_type::<CellRef>()
    .register_type::<WorldPosition>()
    .register_type::<WorldTransform>()
    .register_type::<MeshHandle>()
    .register_type::<MaterialHandle>()
    .register_type::<InstanceBounds>()
    .register_type::<ExpectedModelBounds>()
    .register_type::<StreamingCamera>()
    .register_type::<StreamedCellRoot>()
    .register_type::<ExteriorCellGrid>()
    .register_type::<TerrainPatch>()
    .register_type::<WaterSurface>()
    .register_type::<RenderOrigin>()
    .register_type::<StreamingMetrics>()
    .register_type::<AssetFailure>()
    // The portal's own state: the space the camera is in, the load doors and their leaves, the
    // player controller and a crossing in progress.
    .register_type::<DistantTerrainRoot>()
    .register_type::<ActiveCell>()
    .register_type::<StreamedCellKey>()
    .register_type::<LoadDoor>()
    .register_type::<DoorState>()
    .register_type::<DoorLeaf>()
    .register_type::<Player>()
    .register_type::<PlayerMode>()
    .register_type::<CrossingHeld>();
    info!(
        "Bevy Remote Protocol listening on http://{}:{} (render world on port {REMOTE_RENDER_PORT}); \
         no authentication, local development only",
        Ipv4Addr::LOCALHOST,
        settings.port
    );
}

/// Without the `remote` feature there is nothing to add; [`RemoteSettings::from_config`] has
/// already refused `--remote`.
#[cfg(not(feature = "remote"))]
pub fn add_remote_plugins(_app: &mut App, _settings: Option<RemoteSettings>) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote_config(port: u16) -> EngineConfig {
        EngineConfig {
            remote: true,
            remote_port: port,
            ..EngineConfig::default()
        }
    }

    #[test]
    fn no_server_without_the_flag() {
        assert_eq!(
            RemoteSettings::from_config(&EngineConfig::default()).unwrap(),
            None
        );
    }

    #[cfg(not(feature = "remote"))]
    #[test]
    fn the_flag_explains_the_missing_feature() {
        let error = RemoteSettings::from_config(&remote_config(crate::config::DEFAULT_REMOTE_PORT))
            .unwrap_err()
            .to_string();
        assert!(error.contains("--features remote"), "{error}");
    }

    #[cfg(feature = "remote")]
    #[test]
    fn default_ports_match_bevy() {
        assert_eq!(
            crate::config::DEFAULT_REMOTE_PORT,
            bevy::remote::http::DEFAULT_PORT
        );
        assert_eq!(REMOTE_RENDER_PORT, bevy::remote::http::DEFAULT_RENDER_PORT);
    }

    #[cfg(feature = "remote")]
    #[test]
    fn rejects_ports_the_server_cannot_use() {
        assert!(RemoteSettings::from_config(&remote_config(0)).is_err());
        assert!(RemoteSettings::from_config(&remote_config(REMOTE_RENDER_PORT)).is_err());
        assert_eq!(
            RemoteSettings::from_config(&remote_config(16_000)).unwrap(),
            Some(RemoteSettings { port: 16_000 })
        );
    }

    #[cfg(feature = "remote")]
    #[test]
    fn adds_the_remote_plugins_headless() {
        use bevy::remote::{RemotePlugin, http::RemoteHttpPlugin};

        // The server itself starts in `Startup`, so building the plugin set binds no socket.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        add_remote_plugins(&mut app, Some(RemoteSettings { port: 16_002 }));
        assert!(app.is_plugin_added::<RemotePlugin>());
        assert!(app.is_plugin_added::<RemoteHttpPlugin>());
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(
            registry
                .get(std::any::TypeId::of::<crate::world::components::FormId>())
                .is_some()
        );
    }
}

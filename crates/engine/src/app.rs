use crate::{
    atmosphere::{
        CAVERN_AMBIENT_BRIGHTNESS, CAVERN_AMBIENT_COLOR, DAY_SUN_ILLUMINANCE,
        SKY_AMBIENT_BRIGHTNESS, SKY_AMBIENT_COLOR, SKY_COLOR, atmosphere_applies,
    },
    config::EngineConfig,
    metrics::AcceptanceMetricsPlugin,
    physics::{MovementTuning, PhysicsFixturePlugin, WorldPlayerPlugin},
    pose_capture::PoseCapturePlugin,
    profiling::{ProfilingPlugin, ProfilingState},
    remote::{RemoteSettings, add_remote_plugins},
    render::{
        LIGHT_LAYERS, MAIN_VIEW_LAYERS, RendererMetrics, TerrainExtension, TerrainMaterial,
        VercidiumRendererPlugin, WATER_LAYER, WaterExtension, WaterMaterial,
        WaterReflectionTexture, terrain_layer_sampler,
    },
    shots::{ShotsFile, ShotsRun},
    sky::SkyCamera,
    streaming::{
        AssetFailure, RenderOrigin, StreamingMetrics, StreamingPlugin, StreamingWorld,
        build_terrain_quadrant_mesh, camera_grid, validate_standard_material,
    },
    world::{
        cache::{CellCache, TerrainLayerSnapshot, TerrainSnapshot},
        components::{
            CellRef, ExpectedModelBounds, FormId, InstanceBounds, StreamedCellRoot, StreamingCamera,
        },
        database::{AssetCatalog, CellKey, WorldDatabase},
        lighting::SpaceLightingCatalog,
    },
};
use bevy::{
    asset::{AssetPlugin, RenderAssetUsages},
    camera::ClearColorConfig,
    camera::primitives::MeshAabb,
    camera::visibility::RenderLayers,
    core_pipeline::prepass::DepthPrepass,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder},
    log::{Level, LogPlugin},
    prelude::*,
    render::diagnostic::RenderDiagnosticsPlugin,
    render::occlusion_culling::OcclusionCulling,
    render::render_asset::RenderAssetBytesPerFrame,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    render::view::screenshot::{Screenshot, save_to_disk},
    tasks::{IoTaskPool, TaskPoolBuilder},
    window::{PresentMode, WindowPlugin, WindowPosition, WindowResolution},
    winit::WinitSettings,
};
use color_eyre::Result;
use color_eyre::eyre::WrapErr;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Resource)]
struct InitialCameraGroundHeight(f32);

#[derive(Resource)]
struct AssetDirectoryReadLock {
    _guard: shared::asset_lock::AssetLock,
}

pub fn run(mut config: EngineConfig) -> Result<()> {
    validate_fixture_selection(&config)?;
    let remote = RemoteSettings::from_config(&config)?;
    configure_io_task_pool();
    // The graphics settings (--graphics, the knob flags, --tonemapper and the settings file): a
    // value that is not one is fatal first, with the valid ones, before anything is opened or
    // written, rather than a silent default (`crate::graphics_settings`).
    let graphics =
        crate::graphics_settings::from_config(&config).map_err(color_eyre::eyre::Report::msg)?;
    let interactive_world_physics = world_physics_player(&config);
    let streaming_fixture_dir = if config.streaming_fixture {
        let fixture = StreamingFixtureDirectory::create(config.worldspace_id, config.start_grid)?;
        config.assets_dir = fixture.path.clone();
        Some(fixture)
    } else {
        None
    };
    let reads_asset_tree = config.streaming_fixture
        || !(config.benchmark_only
            || config.material_fixture
            || config.terrain_water_fixture
            || config.transform_bounds_fixture
            || config.renderer_fixture
            || config.physics_fixture);
    let asset_lock = if reads_asset_tree {
        Some(
            shared::asset_lock::AssetLock::acquire_shared(&config.assets_dir).wrap_err_with(
                || {
                    format!(
                        "engine cannot read assets at {}",
                        config.assets_dir.display()
                    )
                },
            )?,
        )
    } else {
        None
    };
    let runtime_data = if config.streaming_fixture {
        let database_path = config.assets_dir.join("skyrim_world.db");
        Some((
            WorldDatabase::open(&database_path)?,
            AssetCatalog::open(&database_path)?,
            SpaceLightingCatalog::open(&database_path),
            CellCache::open(&config.assets_dir.join("cell_cache.rkyv"))?,
            InitialCameraGroundHeight(0.0),
        ))
    } else if config.benchmark_only
        || config.material_fixture
        || config.terrain_water_fixture
        || config.transform_bounds_fixture
        || config.renderer_fixture
        || config.physics_fixture
    {
        None
    } else {
        validate_runtime_assets(&config)?;
        let database_path = config.assets_dir.join("skyrim_world.db");
        let cache = CellCache::open(&config.assets_dir.join("cell_cache.rkyv"))?;
        let ground_height = initial_camera_ground_height(&config, &database_path, &cache)?;
        Some((
            WorldDatabase::open(&database_path)?,
            AssetCatalog::open(&database_path)?,
            SpaceLightingCatalog::open(&database_path),
            cache,
            InitialCameraGroundHeight(ground_height),
        ))
    };
    // --shots: the file is read before the window exists, because it sets the window's size, and a
    // shots file the engine cannot use is fatal before anything is rendered or written.
    let shots = if let Some(path) = config.portal.shots.clone() {
        color_eyre::eyre::ensure!(
            !config.headless,
            "--shots renders a window-sized image; it cannot run with --headless"
        );
        color_eyre::eyre::ensure!(
            config.portal.demo_tour.is_none() && !config.streaming_fixture,
            "--shots poses the camera itself; the demo tour and the streaming fixture move it"
        );
        color_eyre::eyre::ensure!(
            runtime_data.is_some(),
            "--shots needs the world database and cell cache, which a fixture or benchmark-only \
             run does not open"
        );
        let file = ShotsFile::load(&path)
            .wrap_err_with(|| format!("cannot use the shots file {}", path.display()))?;
        let output_dir = config
            .portal
            .shots_output_dir()
            .expect("--shots names a file, so it has an output folder");
        fs::create_dir_all(&output_dir)
            .wrap_err_with(|| format!("failed to create {}", output_dir.display()))?;
        info!(
            shots = file.shots.len(),
            output = %output_dir.display(),
            "rendering reference shots"
        );
        Some(ShotsRun::new(file, output_dir))
    } else {
        None
    };
    let shots_active = shots.is_some();
    // --start-shot: the shot the run is started at, read here for the same reason the shots file is
    // - a file or a name the run cannot use is fatal before the window exists - and refused beside
    // the flags that pose the camera too (`crate::pose_capture`). A run with no such flag resolves
    // to nothing and renders as it always did.
    let start_shot = crate::pose_capture::start_shot_run(&config)?;
    color_eyre::eyre::ensure!(
        start_shot.is_none() || runtime_data.is_some(),
        "--start-shot poses the camera in the world; a fixture or benchmark-only run does not open \
         the world database and cell cache"
    );
    let movement_tuning = (interactive_world_physics
        && runtime_data.is_some()
        && !config.streaming_fixture)
        .then(|| MovementTuning::from_world_database(&config.assets_dir.join("skyrim_world.db")))
        .transpose()?;
    let asset_path = config.assets_dir.to_string_lossy().into_owned();
    let benchmark_active =
        config.benchmark_frames.is_some() || config.benchmark_duration_secs.is_some();
    configure_benchmark_priority(benchmark_active)?;
    let window = (!config.headless).then(|| Window {
        title: config.window_title(),
        resolution: shots.as_ref().map_or_else(
            || WindowResolution::new(1600, 900),
            ShotsRun::window_resolution,
        ),
        // A `--tour-bench` run measures frame times too, and vsync would pin every state to the
        // display's refresh (impl-195 read 16.6 ms for all three), so it runs unsynced as well, and
        // so does a `--portal-bench` run (`EngineConfig::times_frames`).
        present_mode: if config.times_frames() {
            PresentMode::AutoNoVsync
        } else {
            PresentMode::AutoVsync
        },
        // A tour or a shots run starts parked: placed where no monitor is and not focused, but a
        // full-size window with its taskbar entry, so it renders, its screenshots are unchanged, and
        // clicking the entry brings it on screen (`crate::window_parking`). A minimised window would
        // render at 1x1 pixels.
        // A timing run opens on screen, in the middle, to be watched (the user asked, 2026-09-26).
        position: if config.window_offscreen() {
            WindowPosition::At(crate::window_parking::PARKED_POSITION)
        } else if config.times_frames() {
            WindowPosition::Centered(bevy::window::MonitorSelection::Primary)
        } else {
            WindowPosition::Automatic
        },
        focused: !config.window_offscreen(),
        ..default()
    });
    let origin = RenderOrigin(IVec2::new(config.start_grid.0, config.start_grid.1));
    // A shots run is interactive in the same sense the demo tour is: sky and underground lighting,
    // portals and lights, but no player controller, no fly camera and no objective text. It poses
    // the camera itself, so nothing else may move it.
    let shots_mode = shots.is_some();
    // Interactive walking only; acceptance and benchmark runs keep the scripted fly camera.
    let walk = config.walks();
    let graphics_cycle = config.portal.graphics_cycle;
    let mut app = App::new();
    if let Some(tuning) = movement_tuning {
        app.insert_resource(tuning);
    }
    if let Some(asset_lock) = asset_lock {
        app.insert_resource(AssetDirectoryReadLock { _guard: asset_lock });
    }
    if config.times_frames() || shots_active {
        // Acceptance runs are commonly left unfocused while the campaign driver
        // advances through its scenarios. Bevy's game default throttles an
        // unfocused window to 60 Hz, which makes a 16.67 ms P95 gate measure the
        // event-loop sleep instead of renderer performance. A shots run left behind other windows would likewise starve its settle rule of frames.
        // A `--tour-bench` run is
        // parked unfocused off screen too, and read 16.67 ms in every state until
        // it ran continuously as well (2026-09-25).
        app.insert_resource(WinitSettings::continuous());
    }
    let render_asset_budget = upload_budget(&config);
    // The run fingerprint is measurement data: an ordinary play session neither builds one nor
    // pays for the startup system that reads it.
    let records_measurements = crate::fingerprint::records_measurements(&config);
    app.insert_resource(config)
        .insert_resource(origin)
        .insert_resource(render_asset_budget)
        .init_resource::<StreamingMetrics>()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_path,
                    ..default()
                })
                .set(world_gltf_plugin())
                .set(WindowPlugin {
                    primary_window: window,
                    ..default()
                })
                .set(LogPlugin {
                    level: Level::INFO,
                    // Bevy's default filter plus: gilrs logs an unknown-gamepad-mapping line and
                    // this engine has no gamepad input.
                    filter: "wgpu=error,naga=warn,gilrs=off".to_string(),
                    ..default()
                }),
        )
        .add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin {
                debug: true,
                ..default()
            },
            AcceptanceMetricsPlugin,
            ProfilingPlugin,
            RenderDiagnosticsPlugin,
        ))
        .add_plugins((
            VercidiumRendererPlugin,
            crate::nif_material::NifMaterialPlugin,
        ))
        // Registered for every run, lights or not: the plugin owns the budget, not the spawning,
        // and `--lights` is what `streaming::spawn_cell` reads to place anything for it to budget.
        .add_plugins(crate::lights::LightsPlugin)
        .add_systems(Update, capture_acceptance_screenshot);
    add_remote_plugins(&mut app, remote);
    if records_measurements {
        app.add_plugins(crate::fingerprint::RunFingerprintPlugin);
    }
    let demo_tour = app
        .world()
        .resource::<EngineConfig>()
        .portal
        .demo_tour
        .clone();
    // The runs that are looked at rather than measured: sky and underground lighting, portals and
    // lights, and no acceptance capture. A pure function of the configuration (H7), so this gate
    // and the portal's own wiring cannot disagree about the sort of run this is.
    let interactive = app.world().resource::<EngineConfig>().interactive();
    // A demo tour's output folder, made before anything renders: a folder the engine cannot write
    // is fatal, and this is where the run can still report it as an error - a plugin build cannot.
    if let Some(output_dir) = demo_tour.as_ref() {
        std::fs::create_dir_all(output_dir)
            .wrap_err_with(|| format!("failed to create {}", output_dir.display()))?;
    }
    if let Some(run) = shots {
        // `--shots` hands its run over as a resource: the file was loaded above, before the window
        // existed, and the plugin that runs it is the portal (H3).
        app.insert_resource(run);
    }
    if !walk && !shots_mode {
        // The scripted fly camera: a walked run hands the camera to the player's own controller
        // (`PlayerPlugin`, added by the portal plugin), and a shots run poses the camera itself, at
        // an exact Creation position, and takes no input.
        app.add_systems(Update, fly_camera);
    }
    if atmosphere_applies(interactive, app.world().resource::<EngineConfig>()) {
        // The sky, the fog, the ambient and the per-space lighting, in one call: the plugin owns
        // the clear colour and the system that writes them (`crate::atmosphere`).
        app.add_plugins(crate::atmosphere::AtmospherePlugin);
        // Each sky camera's dome, coloured from the space that camera shows (`crate::sky_dome`).
        // Upstream main's `SkyPlugin` is not added: it writes every camera's clear colour and fog
        // from one global palette, which is `crate::atmosphere`'s and `crate::portal`'s job here.
        app.add_plugins(crate::sky_dome::SkyDomePlugin);
    }
    if let Some((database, catalog, space_lighting, cache, ground_height)) = runtime_data {
        app.insert_resource(database)
            .insert_resource(catalog)
            .insert_resource(space_lighting)
            .insert_resource(cache)
            .insert_resource(ground_height)
            .add_plugins(StreamingPlugin);
        // A walked run's world collision: the streamer builds the colliders and enables only the
        // player's space (`crate::portal::gate_cell_colliders`), and `crate::player` walks on them
        // through rapier ray casts. Only rapier itself is added - not main's capsule player
        // (`WorldPlayerPlugin`), which knows nothing of doors.
        if crate::streaming::builds_world_colliders(app.world().resource::<EngineConfig>()) {
            app.add_plugins(crate::physics::PhysicsCorePlugin);
        }
        // The portal, in one call: the crossing for every run that opened the world, and everything
        // else the demo is - the doorway image, the door and model animation, the player, the demo
        // tour, the shots run - for the runs that are looked at rather than measured. Which of them
        // this run gets is the plugin's own decision (docs/design/portal-plugin.md).
        app.add_plugins(crate::portal::PortalPlugin);
        // The portal graph: every load door and where it leads, read once from the world
        // database (`crate::portal_graph`). The transition layer reads it every frame to rank the
        // doors the camera is most likely to take next and pre-stream what is behind them
        // (`transition::predicted_doors`).
        app.add_plugins(crate::portal_graph::PortalGraphPlugin);
        // The graphics settings, applied to the main, portal and water cameras and the sun, for
        // every run that opened the world (a `--shots` run included); `M` cycles the tonemapper
        // (`crate::graphics_settings`). Auto exposure is not in Bevy's default plugins.
        app.add_plugins((
            bevy::post_process::auto_exposure::AutoExposurePlugin,
            crate::graphics_settings::GraphicsSettingsPlugin { settings: graphics },
        ));
        // `--graphics-cycle <seconds>`: the anti-aliasing stepped through every switch on its own,
        // to reproduce a crash in switching it live (impl-239).
        if let Some(period) = graphics_cycle {
            app.insert_resource(crate::graphics_settings::GraphicsCycle::new(period));
        }
        // `--portal-bench`: the portal timed at a fixed list of dense doors, in GPU, CPU and frame
        // time (`crate::portal_bench`). The doors file is read here, where a file the run cannot
        // use is still an error rather than a panic.
        let portal_bench = app.world().resource::<EngineConfig>().portal.clone();
        if let Some(path) = portal_bench.portal_bench {
            let doors = crate::portal_bench::DoorsFile::load(&path)?;
            let output = portal_bench
                .bench_out
                .unwrap_or_else(|| PathBuf::from(crate::config::DEFAULT_PORTAL_BENCH_OUT));
            let timing_seconds = portal_bench
                .bench_seconds
                .unwrap_or(crate::portal_bench::DEFAULT_TIMING_SECONDS);
            app.add_plugins(crate::portal_bench::PortalBenchPlugin {
                doors,
                output,
                timing_seconds,
            });
        }
        if interactive {
            // The pose capture: `P` saves the camera's pose to `local/reference/manual-poses.jsonl`,
            // and `--start-shot` starts the run at one (`crate::pose_capture`). Added here rather
            // than by the portal plugin, which is what adds the other camera-driving runs' plugins:
            // it is not the portal's - it reads the camera, the space it stands in and the
            // shots-file contract - and it needs the world the streaming plugin has just opened.
            app.add_plugins(PoseCapturePlugin { start: start_shot });
            // (Skyrim's LIGH references: `LightsPlugin` is registered for every run above.)
            // Light spilling through the doorway the portal draws, both ways
            // (`crate::portal_spill`). Opt-in (`--portal-light-spill`) until it passes on
            // doorways it was not fitted on: it softens the colour seam everywhere but brightens
            // interiors that are already brighter than the porch (impl-210's held-out set).
            if app.world().resource::<EngineConfig>().portal.light_spill {
                app.add_plugins(crate::portal_spill::PortalSpillPlugin);
            }
        }
        if !app.world().resource::<EngineConfig>().headless {
            // Field notes: `F12` takes a screenshot, writes the pose and a state snapshot, and
            // asks for a note (`crate::field_notes`). Every windowed run that has opened the
            // world gets it, not only the ones `interactive` covers - a person can be at the
            // keyboard of a plain flight too - which is why this is its own gate rather than
            // riding the one above.
            app.add_plugins(crate::field_notes::FieldNotesPlugin);
            app.add_plugins(crate::graphics_panel::GraphicsPanelPlugin);
        }
        app.add_systems(Startup, setup_world);
        if interactive_world_physics {
            // main's own door crossing (`crate::door_crossing`, #128) is compiled but not
            // registered: portal's doors (`crate::doors`, `crate::transition`) are the door
            // system, and a second `E` handler would cross twice.
            app.add_plugins(WorldPlayerPlugin);
        }
        if app.world().resource::<EngineConfig>().streaming_fixture {
            app.init_resource::<StreamingFixtureState>()
                .add_systems(Startup, setup_streaming_fixture_visual)
                .add_systems(
                    PreUpdate,
                    (drive_streaming_fixture, cross_streaming_fixture_interior).chain(),
                )
                .add_systems(PostUpdate, validate_streaming_fixture);
        }
    } else if app.world().resource::<EngineConfig>().material_fixture {
        app.add_systems(Startup, setup_material_fixture)
            .add_systems(Update, validate_material_fixture);
    } else if app.world().resource::<EngineConfig>().terrain_water_fixture {
        app.add_systems(PostStartup, setup_terrain_water_fixture)
            .add_systems(Update, validate_terrain_water_fixture);
    } else if app
        .world()
        .resource::<EngineConfig>()
        .transform_bounds_fixture
    {
        app.add_systems(Startup, setup_transform_bounds_fixture)
            .add_systems(Update, validate_transform_bounds_fixture);
    } else if app.world().resource::<EngineConfig>().renderer_fixture {
        app.add_systems(Startup, setup_renderer_fixture)
            .add_systems(Update, validate_renderer_fixture);
    } else if app.world().resource::<EngineConfig>().physics_fixture {
        app.add_plugins(PhysicsFixturePlugin);
    } else {
        app.add_systems(Startup, setup_world);
        app.add_systems(Startup, setup_synthetic_benchmark);
    }
    let exit = app.run();
    drop(app);
    drop(streaming_fixture_dir);
    finished(&exit)
}

/// What the process reports after an [`App`] has run.
///
/// A shots run that could not write an image, and an acceptance run whose gates did not pass, end
/// with [`AppExit::Error`]; dropping that would report success to whatever ran the process - the
/// acceptance script reads the exit code - so the error is handed back to `main` instead.
fn finished(exit: &AppExit) -> Result<()> {
    match exit {
        AppExit::Success => Ok(()),
        AppExit::Error(code) => Err(color_eyre::eyre::eyre!(
            "the run failed (exit code {code:?})"
        )),
    }
}

/// Bevy's per-frame render-asset byte budget, seeded from the run's option.
///
/// Textures and meshes over the budget wait for a later frame instead of being
/// prepared the moment they load, so a cell's new models arrive over a few
/// frames rather than in one upload burst. Deferred assets are never dropped,
/// and a single asset larger than the whole budget is still prepared. Images
/// are prepared before meshes and share the one budget, so while new images
/// use it up, new meshes wait for a later frame.
///
/// A capture run (`--shots`, `--demo-tour`) has no budget: its settle gate
/// counts what the streamer still has to spawn, not what the render world still
/// has to upload, and a loaded machine left whole rooms undrawn in the frame it
/// photographed (research-283). The timing runs keep the configured budget.
fn upload_budget(config: &EngineConfig) -> RenderAssetBytesPerFrame {
    let captures = config.portal.shots.is_some() || config.portal.demo_tour.is_some();
    RenderAssetBytesPerFrame {
        max_bytes: if captures {
            None
        } else {
            config.max_upload_bytes_per_frame()
        },
    }
}

/// Matches LAND's sampler for shared repeating images before either loader caches them.
/// Bevy retains authored U/V wrapping and requires linear filters for anisotropy.
fn world_gltf_plugin() -> bevy::gltf::GltfPlugin {
    bevy::gltf::GltfPlugin {
        default_sampler: terrain_layer_sampler(),
        ..default()
    }
}

fn validate_fixture_selection(config: &EngineConfig) -> Result<()> {
    let selected = [
        config.material_fixture,
        config.terrain_water_fixture,
        config.transform_bounds_fixture,
        config.renderer_fixture,
        config.streaming_fixture,
        config.physics_fixture,
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    color_eyre::eyre::ensure!(selected <= 1, "select only one fixture mode");
    Ok(())
}

#[cfg(windows)]
fn configure_benchmark_priority(benchmark_active: bool) -> Result<()> {
    if benchmark_active {
        use windows_sys::Win32::System::Threading::{
            ABOVE_NORMAL_PRIORITY_CLASS, GetCurrentProcess, SetPriorityClass,
        };
        // SAFETY: GetCurrentProcess returns the current process pseudo-handle,
        // which is valid for SetPriorityClass and must not be closed.
        let configured =
            unsafe { SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS) };
        if configured == 0 {
            return Err(std::io::Error::last_os_error())
                .wrap_err("failed to set benchmark process priority");
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn configure_benchmark_priority(_benchmark_active: bool) -> Result<()> {
    Ok(())
}

/// The stack each IO task pool thread reserves.
///
/// Asset loads nest on these stacks. bevy_asset runs every load as a task on the IO pool, and
/// bevy_gltf's loader loads a file's textures inside `IoTaskPool::scope`, whose `block_on` ticks
/// the pool's shared executor on the calling thread while it waits. So a glTF load waiting for its
/// textures picks up the next queued glTF load and runs it on the same stack, that one does the
/// same, and so on: the nesting is as deep as the queue of model loads. Measured on the portal
/// bench's Markarth door (`--profile quick`): each nested load costs about 85 KiB, and one thread
/// reached 8,074 KiB (about 95 loads deep) and overflowed the 8 MiB this used to be. A dense cell
/// queues hundreds of distinct models at once (the largest interior has 522), plus its
/// neighbours, so the reservation has to cover the whole queue, not a typical load.
///
/// 128 MiB is room for about 1,500 nested loads. It is address space, not memory: Rust asks
/// Windows for a stack *reservation*, and pages are committed only as deep as a thread reaches.
const IO_TASK_STACK_BYTES: usize = 128 * 1024 * 1024;

fn io_task_pool_builder(threads: usize) -> TaskPoolBuilder {
    TaskPoolBuilder::new()
        .num_threads(threads)
        .thread_name("IO Task Pool".to_owned())
        .stack_size(IO_TASK_STACK_BYTES)
}

fn configure_io_task_pool() {
    let threads = std::thread::available_parallelism()
        .map(|count| count.get().div_ceil(4).clamp(1, 4))
        .unwrap_or(1);
    IoTaskPool::get_or_init(|| io_task_pool_builder(threads).build());
}

/// The interior cell the streaming fixture loads by id. An interior carries no grid square and
/// belongs to no worldspace, and its id sits past the block the exterior cells are numbered in.
const STREAMING_FIXTURE_INTERIOR_CELL_ID: u32 = 0x0001_0000;
/// The reference placed inside that interior cell. It has no model, so the fixture still needs no
/// converted assets; the crossing is observed through the root and this reference.
const STREAMING_FIXTURE_INTERIOR_REFERENCE_ID: u32 = STREAMING_FIXTURE_INTERIOR_CELL_ID + 1;
/// The frame the fixture loads the interior on, after the first teleport has moved the camera
/// several cells away from the grid it starts on.
const STREAMING_FIXTURE_INTERIOR_FRAME: u32 = 8;

struct StreamingFixtureDirectory {
    path: PathBuf,
}

impl StreamingFixtureDirectory {
    fn create(worldspace_id: u32, start_grid: (i32, i32)) -> Result<Self> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path =
            std::env::temp_dir().join(format!("mudcrab-streaming-{}-{suffix}", std::process::id()));
        fs::create_dir(&path).wrap_err_with(|| format!("failed to create {}", path.display()))?;
        let fixture = Self { path };
        fixture.populate(worldspace_id, start_grid)?;
        Ok(fixture)
    }

    fn populate(&self, worldspace_id: u32, start_grid: (i32, i32)) -> Result<()> {
        let database_path = self.path.join("skyrim_world.db");
        let connection = Connection::open(&database_path)?;
        connection.execute_batch(&format!(
            r#"CREATE TABLE schema_info(version INTEGER NOT NULL);
            INSERT INTO schema_info VALUES({});
            CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,interior_name TEXT);
            CREATE TABLE worldspaces(id INTEGER PRIMARY KEY,editor_id TEXT,parent_world INTEGER,flags INTEGER,lod_origin_x INTEGER,lod_origin_y INTEGER);
            CREATE TABLE land(cell_id INTEGER PRIMARY KEY);
            CREATE TABLE statics(id INTEGER PRIMARY KEY,model_path TEXT,bounds_min_x REAL,bounds_min_y REAL,bounds_min_z REAL,bounds_max_x REAL,bounds_max_y REAL,bounds_max_z REAL,bounds_valid INTEGER NOT NULL);
            CREATE TABLE "references"(id INTEGER PRIMARY KEY,cell_id INTEGER,base_form_id INTEGER,pos_x REAL,pos_y REAL,pos_z REAL,rot_x REAL,rot_y REAL,rot_z REAL,scale REAL);
            CREATE VIRTUAL TABLE exterior_spatial USING rtree(id,minX,maxX,minY,maxY,minZ,maxZ,+cell_id,+worldspace_id);
            CREATE TABLE lod_build(id INTEGER PRIMARY KEY CHECK (id=1),build_identity TEXT NOT NULL);
            INSERT INTO lod_build VALUES(1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
            CREATE TABLE lod_chunks(worldspace_id INTEGER,tier INTEGER,anchor_x INTEGER,anchor_y INTEGER,payload_path TEXT,content_hash TEXT,bounds_min_x REAL,bounds_min_y REAL,bounds_min_z REAL,bounds_max_x REAL,bounds_max_y REAL,bounds_max_z REAL,source_cells TEXT,PRIMARY KEY(worldspace_id,tier,anchor_x,anchor_y));
            CREATE VIRTUAL TABLE lod_chunks_spatial USING rtree(id,minX,maxX,minY,maxY,+worldspace_id,+tier,+anchor_x,+anchor_y);
            CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT);
            CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER);
            CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT);"#,
            shared::WORLD_DATABASE_SCHEMA_VERSION,
        ))?;
        connection.execute(
            "INSERT INTO worldspaces(id,editor_id,parent_world,flags,lod_origin_x,lod_origin_y) VALUES(?1,'Fixture',NULL,0,?2,?3)",
            params![worldspace_id, start_grid.0, start_grid.1],
        )?;
        let mut insert = connection
            .prepare("INSERT INTO cells(id,worldspace_id,grid_x,grid_y) VALUES(?1,?2,?3,?4)")?;
        let mut cell_id = 1u32;
        for grid_y in start_grid.1.saturating_sub(48)..=start_grid.1.saturating_add(48) {
            for grid_x in start_grid.0.saturating_sub(48)..=start_grid.0.saturating_add(48) {
                insert.execute(params![cell_id, worldspace_id, grid_x, grid_y])?;
                cell_id += 1;
            }
        }
        drop(insert);
        connection.execute(
            "INSERT INTO cells(id,worldspace_id,grid_x,grid_y,interior_name) VALUES(?1,NULL,NULL,NULL,'Fixture Hall')",
            params![STREAMING_FIXTURE_INTERIOR_CELL_ID],
        )?;
        connection.execute(
            "INSERT INTO \"references\"(id,cell_id,base_form_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,scale)
             VALUES(?1,?2,?3,256.0,0.0,192.0,0.0,0.0,0.0,1.0)",
            params![
                STREAMING_FIXTURE_INTERIOR_REFERENCE_ID,
                STREAMING_FIXTURE_INTERIOR_CELL_ID,
                STREAMING_FIXTURE_INTERIOR_REFERENCE_ID
            ],
        )?;
        drop(connection);
        let cache = shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: Vec::new(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cache)
            .wrap_err("failed to archive streaming fixture cache")?;
        fs::write(self.path.join("cell_cache.rkyv"), bytes)?;
        Ok(())
    }
}

impl Drop for StreamingFixtureDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path) {
            warn!(%error, path = %self.path.display(), "failed to remove streaming fixture directory");
        }
    }
}

#[derive(Resource, Default)]
struct StreamingFixtureState {
    frames: u32,
    total_x: i32,
    total_y: i32,
    finished: bool,
    interior: InteriorCrossing,
    /// The exterior grid the camera stood on when the interior was loaded: the place it has to
    /// leave for the interior to be observed from far outside the unload radius.
    interior_center: IVec2,
}

/// What the fixture observed of the exterior/interior crossing. The interior is loaded by id the
/// way a door crossing will load one; the camera then carries on over exteriors far outside the
/// unload radius and comes back. The contract is that the interior never exists twice and its
/// references match its root. Today it also stays loaded throughout (an interior has no grid
/// square, so [`cell_within_unload_radius`](crate::streaming) keeps it, and only a space switch
/// unloads one); that is current behaviour, not part of the contract.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct InteriorCrossing {
    requested_frame: Option<u32>,
    resident_frame: Option<u32>,
    /// Frames on which the camera stood more than `unload_radius` cells from the grid the interior
    /// was loaded from, and the interior root counts seen on them.
    away_samples: u32,
    min_away_roots: usize,
    max_away_roots: usize,
}

impl InteriorCrossing {
    fn observe(&mut self, away: bool, roots: usize) {
        if !away {
            return;
        }
        self.away_samples = self.away_samples.saturating_add(1);
        if self.away_samples == 1 {
            self.min_away_roots = roots;
            self.max_away_roots = roots;
        } else {
            self.min_away_roots = self.min_away_roots.min(roots);
            self.max_away_roots = self.max_away_roots.max(roots);
        }
    }
}

/// The crossing contract, as the fixture's own observations and the final root and reference
/// counts express it: the interior was loaded, the camera was observed far away from it, it never
/// had two roots, and its references match its root (one each, or none if it was unloaded).
/// Orphaned and missing roots are the lifecycle validator's, which the run also requires at zero.
fn interior_crossing_valid(crossing: &InteriorCrossing, roots: usize, references: usize) -> bool {
    crossing.requested_frame.is_some()
        && crossing.resident_frame.is_some()
        && crossing.away_samples > 0
        && crossing.max_away_roots <= 1
        && roots <= 1
        && references == roots
}

fn setup_streaming_fixture_visual(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    let mesh = Mesh3d(meshes.add(Cuboid::new(180.0, 480.0, 180.0)));
    let material = MeshMaterial3d(materials.add(TerrainMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.22, 0.48, 0.18),
            perceptual_roughness: 0.88,
            ..default()
        },
        extension: TerrainExtension::default(),
    }));
    commands.spawn_batch((0..64).map(move |index| {
        let x = index % 8;
        let z = index / 8;
        (
            mesh.clone(),
            material.clone(),
            Transform::from_xyz(700.0 + x as f32 * 360.0, 240.0, -700.0 - z as f32 * 360.0),
        )
    }));
}

fn drive_streaming_fixture(
    mut state: ResMut<StreamingFixtureState>,
    mut camera: Query<&mut Transform, With<StreamingCamera>>,
    mut profiler: ResMut<ProfilingState>,
) {
    if state.finished {
        return;
    }
    state.frames = state.frames.saturating_add(1);
    let Some((x, y, label)) = (match state.frames {
        4 => Some((6, 0, "rapid_traversal")),
        5 => Some((0, -7, "rapid_traversal")),
        6 => Some((18, 12, "teleport")),
        14 => Some((-30, -9, "teleport")),
        22 => Some((9, 5, "rapid_traversal")),
        30 => Some((-state.total_x, -state.total_y, "return_to_origin")),
        _ => None,
    }) else {
        return;
    };
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    // A move of whole cells east and north, which is how far a cell root that many grids away sits.
    camera.translation += crate::streaming::grid_translation(IVec2::new(x, y), IVec2::ZERO);
    state.total_x += x;
    state.total_y += y;
    profiler.event("streaming-fixture", label, None);
}

/// Loads the fixture's interior by id, through the loader path the camera planner uses, and
/// watches it while the camera keeps crossing exteriors around it.
#[allow(clippy::too_many_arguments)]
fn cross_streaming_fixture_interior(
    config: Res<EngineConfig>,
    mut state: ResMut<StreamingFixtureState>,
    database: Res<WorldDatabase>,
    origin: Res<RenderOrigin>,
    mut streaming: ResMut<StreamingWorld>,
    mut metrics: ResMut<StreamingMetrics>,
    camera: Query<&Transform, With<StreamingCamera>>,
    roots: Query<&CellRef, With<StreamedCellRoot>>,
    mut profiler: ResMut<ProfilingState>,
) {
    if state.finished {
        return;
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let center = camera_grid(camera.translation, origin.0);
    if state.interior.requested_frame.is_none() {
        if state.frames < STREAMING_FIXTURE_INTERIOR_FRAME {
            return;
        }
        streaming.request_cell(
            &database,
            CellKey::Interior(STREAMING_FIXTURE_INTERIOR_CELL_ID),
            &mut metrics,
            &mut profiler,
        );
        state.interior.requested_frame = Some(state.frames);
        state.interior_center = center;
        profiler.event("streaming-fixture", "interior_requested", None);
        return;
    }
    let roots = roots
        .iter()
        .filter(|cell| cell.0 == STREAMING_FIXTURE_INTERIOR_CELL_ID)
        .count();
    // Only a loaded interior can be unloaded against the rule: the frames between the request and
    // the commit have no root to count yet.
    if state.interior.resident_frame.is_none() {
        if roots > 0 {
            state.interior.resident_frame = Some(state.frames);
            profiler.event("streaming-fixture", "interior_resident", None);
        }
        return;
    }
    let away = (center.x - state.interior_center.x).abs() > config.unload_radius
        || (center.y - state.interior_center.y).abs() > config.unload_radius;
    state.interior.observe(away, roots);
}

/// Whether this run's camera is main's rapier walking player ([`WorldPlayerPlugin`]).
///
/// Off on the portal branch because main's capsule player knows nothing of doors: it cannot pass
/// through a load door's doorway into the room beyond or follow the portal's space changes. Portal
/// runs keep their own walking player (`crate::player::PlayerPlugin`, `--walk`), which does, and
/// the free camera instead. (The streamer does build the terrain and static colliders for walked
/// runs; the gate is not about them.)
const WORLD_PHYSICS_PLAYER: bool = false;

fn world_physics_player(config: &EngineConfig) -> bool {
    WORLD_PHYSICS_PLAYER && config.interactive_world_physics()
}

fn validate_streaming_fixture(
    config: Res<EngineConfig>,
    mut state: ResMut<StreamingFixtureState>,
    mut metrics: ResMut<StreamingMetrics>,
    roots: Query<&CellRef, With<StreamedCellRoot>>,
    references: Query<&FormId>,
    mut profiler: ResMut<ProfilingState>,
) {
    if state.finished || state.frames < 90 {
        return;
    }
    let expected_resident = ((config.stream_radius * 2 + 1).max(0) as usize).pow(2);
    // The plan holds the inner grid in full, and around it the terrain-only ring, which lives one
    // cell beyond the ring's own edge. A full cell's band is `unload_radius`, wider than the ring's
    // only when the ring is off; the plan keeps whichever is the wider of the two.
    let outer_radius = config
        .terrain_radius
        .saturating_add(1)
        .max(config.unload_radius);
    let maximum_resident = ((outer_radius * 2 + 1).max(0) as usize).pow(2);
    let interior_roots = roots
        .iter()
        .filter(|cell| cell.0 == STREAMING_FIXTURE_INTERIOR_CELL_ID)
        .count();
    let interior_references = references
        .iter()
        .filter(|form_id| form_id.0 == STREAMING_FIXTURE_INTERIOR_REFERENCE_ID)
        .count();
    let settled = metrics.active_requests == 0 && metrics.loading_cells == 0;
    let valid = settled
        && metrics.requests_submitted > expected_resident as u64
        && metrics.responses_received > 0
        && metrics.stale_responses > 0
        && metrics.unloaded_cells > 0
        && metrics.origin_rebases >= 6
        && metrics.resident_cells >= expected_resident
        && metrics.resident_cells <= maximum_resident
        && metrics.resident_roots == metrics.resident_cells
        && metrics.out_of_range_cell_roots == 0
        && metrics.streaming_invariant_failures == 0
        && metrics.commit_frames > 0
        && interior_crossing_valid(&state.interior, interior_roots, interior_references);
    if valid {
        metrics.streaming_fixture_validated = true;
        profiler.event("streaming-fixture", "validated", None);
        state.finished = true;
    } else if state.frames >= 300 {
        metrics.streaming_fixture_failures = metrics.streaming_fixture_failures.saturating_add(1);
        error!(
            ?metrics,
            "streaming fixture did not settle or violated its lifecycle contract"
        );
        profiler.event("streaming-fixture", "failed", None);
        state.finished = true;
    }
}

#[derive(Component, Debug, Clone, Copy)]
enum CanonicalMaterialKind {
    Opaque,
    Cutout,
    Blend,
    Emissive,
    DoubleSided,
    NormalMapped,
}

#[derive(Resource, Default)]
struct CanonicalMaterialFixtureState {
    finished: bool,
}

fn fixture_image(data: Vec<u8>, srgb: bool) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::linear();
    image
}

/// A terrain layer image of the synthetic fixture: one flat colour, carried by a sampler that
/// repeats like the one a streamed layer gets, since the shader tiles every layer across a
/// cell. Bevy's default sampler clamps to the edge, which stretches the outermost texels over the
/// rest of the tiles.
fn terrain_fixture_image(pixel: [u8; 4]) -> Image {
    let mut image = fixture_image((0..16).flat_map(|_| pixel).collect(), true);
    image.sampler = bevy::image::ImageSampler::Descriptor(terrain_layer_sampler());
    image
}

fn setup_material_fixture(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.init_resource::<CanonicalMaterialFixtureState>();
    let checker = images.add(fixture_image(
        (0..16)
            .flat_map(|index| {
                let alpha = if (index + index / 4) % 2 == 0 { 255 } else { 0 };
                [78, 166, 88, alpha]
            })
            .collect(),
        true,
    ));
    let normal = images.add(fixture_image(
        (0..16).flat_map(|_| [128, 128, 255, 255]).collect(),
        false,
    ));
    let definitions = [
        (
            CanonicalMaterialKind::Opaque,
            StandardMaterial {
                base_color: Color::srgb(0.55, 0.42, 0.25),
                perceptual_roughness: 0.75,
                ..default()
            },
        ),
        (
            CanonicalMaterialKind::Cutout,
            StandardMaterial {
                base_color_texture: Some(checker),
                alpha_mode: AlphaMode::Mask(0.5),
                ..default()
            },
        ),
        (
            CanonicalMaterialKind::Blend,
            StandardMaterial {
                base_color: Color::srgba(0.15, 0.45, 0.9, 0.45),
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
        ),
        (
            CanonicalMaterialKind::Emissive,
            StandardMaterial {
                base_color: Color::srgb(0.08, 0.08, 0.08),
                // A *converted* emissive, at the magnitude the converter writes and put through the
                // same exposure `render::SkyrimMaterialHandler` puts every streamed material
                // through: the Blackreach mushroom caps publish `[0.212, 0.992, 1.0]` at a strength
                // of 2.0 (`docs/research/visual-gaps-spec.md`, gap 1). The box is what a glowing
                // surface of the game looks like - which is what the check on it has to be able to
                // tell from a glow that has lost its scale.
                emissive: crate::render::exposed_emissive(LinearRgba::new(0.424, 1.984, 2.0, 1.0)),
                ..default()
            },
        ),
        (
            CanonicalMaterialKind::DoubleSided,
            StandardMaterial {
                base_color: Color::srgb(0.75, 0.2, 0.18),
                double_sided: true,
                cull_mode: None,
                ..default()
            },
        ),
        (
            CanonicalMaterialKind::NormalMapped,
            StandardMaterial {
                base_color: Color::srgb(0.45, 0.48, 0.52),
                normal_map_texture: Some(normal),
                ..default()
            },
        ),
    ];
    let mesh = meshes.add(Cuboid::new(2.2, 2.2, 2.2));
    for (index, (kind, material)) in definitions.into_iter().enumerate() {
        commands.spawn((
            Name::new(format!("Canonical {kind:?}")),
            kind,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(materials.add(material)),
            Transform::from_xyz((index as f32 - 2.5) * 2.8, 0.0, 0.0),
        ));
    }
    commands.spawn((
        Camera3d::default(),
        crate::color_pipeline::SceneColorPipeline::default(),
        Transform::from_xyz(0.0, 5.0, 18.0).looking_at(Vec3::ZERO, Vec3::Y),
        StreamingCamera,
        Msaa::Off,
        DepthPrepass,
        OcclusionCulling,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        RenderLayers::from_layers(&LIGHT_LAYERS),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.7, -0.5, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 120.0,
        ..default()
    });
}

/// Whether an emissive is at the magnitude the engine's lighting works in: brighter than a mid-grey
/// surface of the dimmest lit space, so that a converted glow is a glow, and not so far above one
/// that it is a lamp of its own.
///
/// The comparison is in Bevy's own units, where the ambient of `crate::atmosphere` puts a surface
/// at `albedo * brightness * colour`, and both that surface and the emissive are then multiplied by
/// the camera's exposure on the way to the screen (`bevy_pbr/src/render/pbr_functions.wgsl:863` for
/// the lit sum and `:840` for an emissive whose alpha is 1), so the two are directly comparable
/// without knowing the exposure. The reference is a mid-grey surface of Blackreach, the dimmest lit
/// space the game has ([`CAVERN_AMBIENT_BRIGHTNESS`], [`CAVERN_AMBIENT_COLOR`]).
///
/// A converted emissive that never met `crate::lights::EMISSIVE_EXPOSURE` is a thousandth of a
/// surface's brightness and reads as black - which is what this check is for
/// (`docs/research/visual-gaps-spec.md`, gap 1) - and the band's upper edge is what stops the
/// scale from being raised until a glow is a lamp that clips every frame it is in.
pub(crate) fn emissive_is_visible(emissive: LinearRgba) -> bool {
    let luminance = 0.2126 * emissive.red + 0.7152 * emissive.green + 0.0722 * emissive.blue;
    let mid_grey_surface = 0.5 * CAVERN_AMBIENT_BRIGHTNESS * {
        let linear = LinearRgba::from(CAVERN_AMBIENT_COLOR);
        0.2126 * linear.red + 0.7152 * linear.green + 0.0722 * linear.blue
    };
    (mid_grey_surface * 0.3..=mid_grey_surface * 8.0).contains(&luminance)
}

fn validate_material_fixture(
    query: Query<(&CanonicalMaterialKind, &MeshMaterial3d<StandardMaterial>)>,
    materials: Res<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    mut state: ResMut<CanonicalMaterialFixtureState>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    if state.finished || query.iter().count() != 6 {
        return;
    }
    let mut validated_images = 0usize;
    for (kind, handle) in &query {
        let result = materials
            .get(handle)
            .ok_or_else(|| "material is not loaded".to_owned())
            .and_then(|material| {
                match kind {
                    CanonicalMaterialKind::Opaque if material.alpha_mode != AlphaMode::Opaque => {
                        Err("opaque mode was not preserved".to_owned())
                    }
                    CanonicalMaterialKind::Cutout
                        if !matches!(material.alpha_mode, AlphaMode::Mask(_)) =>
                    {
                        Err("mask mode was not preserved".to_owned())
                    }
                    CanonicalMaterialKind::Blend if material.alpha_mode != AlphaMode::Blend => {
                        Err("blend mode was not preserved".to_owned())
                    }
                    CanonicalMaterialKind::Emissive if !emissive_is_visible(material.emissive) => {
                        Err("emissive intensity was lost".to_owned())
                    }
                    CanonicalMaterialKind::DoubleSided
                        if !material.double_sided || material.cull_mode.is_some() =>
                    {
                        Err("double-sided culling was not preserved".to_owned())
                    }
                    CanonicalMaterialKind::NormalMapped
                        if material.normal_map_texture.is_none() =>
                    {
                        Err("normal map was not preserved".to_owned())
                    }
                    _ => Ok(()),
                }?;
                validate_standard_material(material, &images)
            });
        match result {
            Ok(count) => validated_images += count,
            Err(reason) => {
                metrics.asset_load_failures += 1;
                metrics.material_validation_failures += 1;
                metrics.asset_failures.push(AssetFailure {
                    model_path: format!("canonical-material-fixture/{kind:?}"),
                    reference_form_id: 0,
                    base_form_id: 0,
                    cell_id: 0,
                    dependency_chain: vec![reason],
                });
                profiler.increment("assets/load_failures", 1);
            }
        }
    }
    metrics.materials_validated += 6;
    metrics.images_validated += validated_images as u64;
    metrics.canonical_fixture_validated = metrics.material_validation_failures == 0;
    state.finished = true;
}

#[derive(Component)]
struct TerrainWaterFixtureTerrain;

#[derive(Component)]
struct TerrainWaterFixtureWater;

#[derive(Resource, Default)]
struct TerrainWaterFixtureState {
    finished: bool,
}

/// The synthetic LAND snapshot the terrain/water fixture draws: a rolling height field whose four
/// quadrants each carry a full base-and-five-overlays stack, every overlay strongest around its own
/// centre so the weight field is visible in the scene. Built without game data, like every fixture.
fn terrain_water_fixture_snapshot() -> TerrainSnapshot {
    let mut layers = Vec::new();
    for quadrant in 0..4 {
        layers.push(TerrainLayerSnapshot {
            texture_form_id: 1,
            quadrant,
            layer: 0,
            is_base: true,
            weights: Vec::new(),
        });
        for layer in 1..=5u16 {
            let weights = (0usize..17 * 17)
                .filter_map(|vertex| {
                    let x = vertex % 17;
                    let y = vertex / 17;
                    let center = (layer as usize * 3).min(16);
                    let distance = x.abs_diff(center).min(y.abs_diff(center));
                    (distance < 3).then(|| (vertex as u16, (3 - distance) as f32 * 0.12))
                })
                .collect();
            layers.push(TerrainLayerSnapshot {
                texture_form_id: u32::from(layer) + 1,
                quadrant,
                layer,
                is_base: false,
                weights,
            });
        }
    }
    TerrainSnapshot {
        cell_id: 0xF170_0001,
        width: 33,
        height: 33,
        heights: (0..33 * 33)
            .map(|index| {
                let x = (index % 33) as f32 - 16.0;
                let y = (index / 33) as f32 - 16.0;
                45.0 * (x * 0.22).sin() + 35.0 * (y * 0.18).cos()
            })
            .collect(),
        normals: (0..33 * 33).flat_map(|_| [0, 0, 127]).collect(),
        vertex_colors: (0..33 * 33)
            .flat_map(|index| {
                let shade = 190 + (index % 33) as u8;
                [shade, shade, shade]
            })
            .collect(),
        layers,
        water_height: Some(12.0),
        water_type_form_id: Some(1),
    }
}

fn setup_terrain_water_fixture(
    mut commands: Commands,
    reflection: Res<WaterReflectionTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.init_resource::<TerrainWaterFixtureState>();
    let palette = [
        [82, 116, 58, 255],
        [122, 101, 70, 255],
        [83, 92, 102, 255],
        [146, 138, 103, 255],
        [60, 91, 54, 255],
        [113, 82, 62, 255],
    ];
    let texture_handles: [Handle<Image>; 6] =
        palette.map(|pixel| images.add(terrain_fixture_image(pixel)));
    let flow_normal = images.add(fixture_image(
        (0..16)
            .flat_map(|index| {
                if index % 2 == 0 {
                    [150, 110, 255, 255]
                } else {
                    [110, 150, 255, 255]
                }
            })
            .collect(),
        false,
    ));
    let terrain = terrain_water_fixture_snapshot();
    for quadrant in 0..4 {
        commands.spawn((
            Name::new(format!("Terrain/water fixture quadrant {quadrant}")),
            Mesh3d(
                meshes.add(
                    build_terrain_quadrant_mesh(&terrain, quadrant)
                        .expect("canonical terrain fixture must build"),
                ),
            ),
            MeshMaterial3d(terrain_materials.add(TerrainMaterial {
                base: StandardMaterial {
                    base_color: Color::WHITE,
                    perceptual_roughness: 0.92,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                },
                extension: TerrainExtension::fixture(texture_handles.clone()),
            })),
            TerrainWaterFixtureTerrain,
        ));
    }
    commands.spawn((
        Name::new("Terrain/water fixture water"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(2200.0, 2200.0))),
        MeshMaterial3d(water_materials.add(WaterMaterial {
            base: StandardMaterial {
                base_color: Color::srgba(0.04, 0.2, 0.32, 0.7),
                metallic: 0.15,
                perceptual_roughness: 0.06,
                reflectance: 0.9,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            extension: WaterExtension::with_reflection(reflection.0.clone(), Some(flow_normal)),
        })),
        Transform::from_xyz(CELL_SIZE_HALF, 12.0, -CELL_SIZE_HALF),
        crate::world::components::WaterSurface,
        TerrainWaterFixtureWater,
        RenderLayers::layer(WATER_LAYER),
    ));
    let target = Vec3::new(CELL_SIZE_HALF, 0.0, -CELL_SIZE_HALF);
    commands.spawn((
        Camera3d::default(),
        crate::color_pipeline::SceneColorPipeline::default(),
        Transform::from_xyz(CELL_SIZE_HALF, 1800.0, 2600.0).looking_at(target, Vec3::Y),
        StreamingCamera,
        Msaa::Off,
        DepthPrepass,
        OcclusionCulling,
        RenderLayers::from_layers(&MAIN_VIEW_LAYERS),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: DAY_SUN_ILLUMINANCE,
            shadow_maps_enabled: true,
            ..default()
        },
        RenderLayers::from_layers(&LIGHT_LAYERS),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: SKY_AMBIENT_COLOR,
        brightness: SKY_AMBIENT_BRIGHTNESS,
        ..default()
    });
}

fn validate_terrain_water_fixture(
    terrain: Query<(&Mesh3d, &MeshMaterial3d<TerrainMaterial>), With<TerrainWaterFixtureTerrain>>,
    water: Query<&MeshMaterial3d<WaterMaterial>, With<TerrainWaterFixtureWater>>,
    meshes: Res<Assets<Mesh>>,
    terrain_materials: Res<Assets<TerrainMaterial>>,
    water_materials: Res<Assets<WaterMaterial>>,
    mut state: ResMut<TerrainWaterFixtureState>,
    mut metrics: ResMut<StreamingMetrics>,
) {
    if state.finished || terrain.iter().count() != 4 || water.iter().count() != 1 {
        return;
    }
    let valid_terrain = terrain.iter().all(|(mesh, handle)| {
        terrain_materials.get(handle).is_some() && meshes.get(mesh).is_some()
    });
    let valid_water = water
        .single()
        .ok()
        .and_then(|material| water_materials.get(material))
        .is_some();
    if valid_terrain && valid_water {
        metrics.terrain_patches_validated += 4;
        metrics.water_surfaces_validated += 1;
        metrics.materials_validated += 5;
        metrics.images_validated += 7;
        metrics.terrain_water_fixture_validated = true;
    } else {
        metrics.terrain_validation_failures += (!valid_terrain) as u64;
        metrics.water_validation_failures += (!valid_water) as u64;
    }
    state.finished = true;
}

#[derive(Component)]
struct TransformBoundsFixtureRoot;

#[derive(Resource, Default)]
struct TransformBoundsFixtureState {
    frames: u8,
    finished: bool,
}

fn setup_transform_bounds_fixture(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.init_resource::<TransformBoundsFixtureState>();
    let beam_mesh = meshes.add(Cuboid::new(2.0, 4.0, 1.5));
    let cube_mesh = meshes.add(Cuboid::new(2.0, 2.0, 2.0));
    let cap_mesh = meshes.add(Cuboid::new(6.5, 0.7, 1.2));
    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.38, 0.46, 0.58),
        perceptual_roughness: 0.72,
        ..default()
    });
    let bronze = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.39, 0.12),
        metallic: 0.45,
        perceptual_roughness: 0.42,
        ..default()
    });
    let moss = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.48, 0.24),
        perceptual_roughness: 0.86,
        ..default()
    });

    let left = Transform::from_xyz(-2.5, 0.0, 0.0)
        .with_rotation(Quat::from_rotation_z(0.28))
        .with_scale(Vec3::new(1.0, 1.35, 0.75));
    let group = Transform::from_xyz(2.0, 0.5, 0.0)
        .with_rotation(Quat::from_rotation_y(-0.42))
        .with_scale(Vec3::new(0.8, 1.3, 0.65));
    let nested = Transform::from_xyz(1.0, 1.0, 0.0)
        .with_rotation(Quat::from_rotation_x(0.31))
        .with_scale(Vec3::new(1.2, 0.5, 1.7));
    let cap = Transform::from_xyz(0.0, 3.8, 0.0)
        .with_rotation(Quat::from_euler(EulerRot::YXZ, 0.18, -0.12, 0.08))
        .with_scale(Vec3::new(1.05, 0.8, 1.25));

    let mut expected_min = Vec3::splat(f32::INFINITY);
    let mut expected_max = Vec3::splat(f32::NEG_INFINITY);
    for bounds in [
        InstanceBounds::transformed(
            Vec3::new(-1.0, -2.0, -0.75),
            Vec3::new(1.0, 2.0, 0.75),
            left.to_matrix(),
        ),
        InstanceBounds::transformed(
            Vec3::splat(-1.0),
            Vec3::splat(1.0),
            group.to_matrix() * nested.to_matrix(),
        ),
        InstanceBounds::transformed(
            Vec3::new(-3.25, -0.35, -0.6),
            Vec3::new(3.25, 0.35, 0.6),
            cap.to_matrix(),
        ),
    ] {
        expected_min = expected_min.min(bounds.min);
        expected_max = expected_max.max(bounds.max);
    }

    commands
        .spawn((
            Name::new("Canonical transform/bounds assembly"),
            TransformBoundsFixtureRoot,
            ExpectedModelBounds {
                min: expected_min,
                max: expected_max,
            },
            Transform::from_xyz(0.0, -1.0, 0.0)
                .with_rotation(Quat::from_rotation_y(0.48))
                .with_scale(Vec3::new(1.1, 0.9, 1.2)),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Rotated left support"),
                Mesh3d(beam_mesh),
                MeshMaterial3d(stone),
                left,
            ));
            parent
                .spawn((
                    Name::new("Non-uniform hierarchy pivot"),
                    group,
                    Visibility::default(),
                ))
                .with_child((
                    Name::new("Nested rotated support"),
                    Mesh3d(cube_mesh),
                    MeshMaterial3d(bronze),
                    nested,
                ));
            parent.spawn((
                Name::new("Rotated top cap"),
                Mesh3d(cap_mesh),
                MeshMaterial3d(moss),
                cap,
            ));
        });
    commands.spawn((
        Camera3d::default(),
        crate::color_pipeline::SceneColorPipeline::default(),
        Transform::from_xyz(2.0, 5.5, 16.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        StreamingCamera,
        Msaa::Off,
        DepthPrepass,
        OcclusionCulling,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: DAY_SUN_ILLUMINANCE,
            shadow_maps_enabled: true,
            ..default()
        },
        RenderLayers::from_layers(&LIGHT_LAYERS),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.7, -0.55, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 140.0,
        ..default()
    });
}

#[allow(clippy::too_many_arguments)]
fn validate_transform_bounds_fixture(
    roots: Query<
        (Entity, &ExpectedModelBounds, &GlobalTransform),
        With<TransformBoundsFixtureRoot>,
    >,
    children: Query<&Children>,
    nodes: Query<(&Transform, &GlobalTransform, Option<&Mesh3d>)>,
    meshes: Res<Assets<Mesh>>,
    mut state: ResMut<TransformBoundsFixtureState>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    if state.finished {
        return;
    }
    state.frames = state.frames.saturating_add(1);
    if state.frames < 3 {
        return;
    }
    let result = (|| -> Result<(usize, usize), String> {
        let (root, expected, root_global) = roots
            .single()
            .map_err(|_| "canonical transform fixture root is missing".to_owned())?;
        let root_inverse = root_global.affine().inverse();
        let mut actual_min = Vec3::splat(f32::INFINITY);
        let mut actual_max = Vec3::splat(f32::NEG_INFINITY);
        let mut node_count = 0usize;
        let mut mesh_count = 0usize;
        for descendant in children.iter_descendants(root) {
            let (local, global, mesh) = nodes
                .get(descendant)
                .map_err(|_| format!("fixture node {descendant:?} has no transform"))?;
            if !local.to_matrix().is_finite()
                || !global.to_matrix().is_finite()
                || local.scale.abs().min_element() <= 1.0e-6
            {
                return Err(format!(
                    "fixture node {descendant:?} has an invalid transform"
                ));
            }
            node_count += 1;
            let Some(mesh) = mesh else { continue };
            let aabb = meshes
                .get(mesh)
                .and_then(MeshAabb::compute_aabb)
                .ok_or_else(|| format!("fixture mesh {:?} has no bounds", mesh.id()))?;
            let center = Vec3::from(aabb.center);
            let half = Vec3::from(aabb.half_extents);
            let bounds = InstanceBounds::transformed(
                center - half,
                center + half,
                Mat4::from(root_inverse * global.affine()),
            );
            actual_min = actual_min.min(bounds.min);
            actual_max = actual_max.max(bounds.max);
            mesh_count += 1;
        }
        let error = (actual_min - expected.min)
            .abs()
            .max((actual_max - expected.max).abs())
            .max_element();
        (mesh_count == 3 && error <= 1.0e-4)
            .then_some((node_count, mesh_count))
            .ok_or_else(|| {
                format!(
                    "hierarchy bounds mismatch: expected {:?}..{:?}, actual {:?}..{:?}",
                    expected.min, expected.max, actual_min, actual_max
                )
            })
    })();
    match result {
        Ok((nodes, meshes)) => {
            metrics.transform_instances_validated += 1;
            metrics.transform_nodes_validated += nodes as u64;
            metrics.bounds_validated += meshes as u64;
            metrics.transform_bounds_fixture_validated = true;
            profiler.increment("transforms/fixture_validated", 1);
        }
        Err(reason) => {
            metrics.asset_load_failures += 1;
            metrics.transform_bounds_validation_failures += 1;
            metrics.asset_failures.push(AssetFailure {
                model_path: "fixtures/transform-bounds-assembly".to_owned(),
                reference_form_id: 0,
                base_form_id: 0,
                cell_id: 0,
                dependency_chain: vec![reason],
            });
            profiler.increment("transforms/validation_failures", 1);
        }
    }
    state.finished = true;
}

#[derive(Component)]
struct RendererFixtureCenterVisible;

#[derive(Component)]
struct RendererFixtureRightVisible;

#[derive(Component)]
struct RendererFixtureLeftVisible;

#[derive(Resource, Default)]
struct RendererFixtureState {
    frames: u16,
    phase_started: u16,
    phase: u8,
    center_seen: bool,
    right_seen: bool,
    left_seen: bool,
    finished: bool,
}

fn setup_renderer_fixture(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.init_resource::<RendererFixtureState>();
    let cube = meshes.add(Cuboid::new(2.0, 2.0, 2.0));
    let wall = meshes.add(Cuboid::new(12.0, 10.0, 1.0));
    let opaque = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.3, 0.34),
        perceptual_roughness: 0.9,
        ..default()
    });
    let green = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.8, 0.2),
        ..default()
    });
    let red = materials.add(StandardMaterial {
        base_color: Color::srgb(0.85, 0.08, 0.05),
        ..default()
    });
    let blue = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.35, 0.9),
        ..default()
    });
    let gold = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.55, 0.08),
        metallic: 0.25,
        ..default()
    });
    commands.spawn((
        Name::new("Renderer fixture occluder"),
        Mesh3d(wall),
        MeshMaterial3d(opaque),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    commands.spawn((
        Name::new("Renderer fixture front visible"),
        RendererFixtureCenterVisible,
        Mesh3d(cube.clone()),
        MeshMaterial3d(green),
        Transform::from_xyz(0.0, 0.0, 5.0),
    ));
    commands.spawn((
        Name::new("Renderer fixture fully occluded"),
        Mesh3d(cube.clone()),
        MeshMaterial3d(red),
        Transform::from_xyz(0.0, 0.0, -4.0),
    ));
    commands.spawn((
        Name::new("Renderer fixture visible after right turn"),
        RendererFixtureRightVisible,
        Mesh3d(cube.clone()),
        MeshMaterial3d(blue),
        Transform::from_xyz(10.0, 0.0, -2.0)
            .with_rotation(Quat::from_rotation_y(0.45))
            .with_scale(Vec3::new(1.8, 0.7, 1.2)),
    ));
    commands.spawn((
        Name::new("Renderer fixture visible after left turn"),
        RendererFixtureLeftVisible,
        Mesh3d(cube),
        MeshMaterial3d(gold),
        Transform::from_xyz(-10.0, 0.0, -2.0)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.25, -0.5, 0.18))
            .with_scale(Vec3::new(0.65, 2.1, 1.4)),
    ));
    commands.spawn((
        Camera3d::default(),
        crate::color_pipeline::SceneColorPipeline::default(),
        Transform::from_xyz(0.0, 1.5, 16.0).looking_at(Vec3::ZERO, Vec3::Y),
        StreamingCamera,
        Msaa::Off,
        DepthPrepass,
        OcclusionCulling,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: DAY_SUN_ILLUMINANCE,
            shadow_maps_enabled: true,
            ..default()
        },
        RenderLayers::from_layers(&LIGHT_LAYERS),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.65, -0.45, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 130.0,
        ..default()
    });
}

fn validate_renderer_fixture(
    mut camera: Query<&mut Transform, With<StreamingCamera>>,
    center: Query<&ViewVisibility, With<RendererFixtureCenterVisible>>,
    right: Query<&ViewVisibility, With<RendererFixtureRightVisible>>,
    left: Query<&ViewVisibility, With<RendererFixtureLeftVisible>>,
    mut state: ResMut<RendererFixtureState>,
    mut renderer: ResMut<RendererMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    if state.finished {
        return;
    }
    state.frames = state.frames.saturating_add(1);
    let phase_frames = state.frames.saturating_sub(state.phase_started);
    let center_visible = center.single().is_ok_and(|visibility| visibility.get());
    let right_visible = right.single().is_ok_and(|visibility| visibility.get());
    let left_visible = left.single().is_ok_and(|visibility| visibility.get());
    match state.phase {
        0 if phase_frames >= 10 && renderer.final_path_active() && center_visible => {
            state.center_seen = true;
            if let Ok(mut camera) = camera.single_mut() {
                *camera = Transform::from_xyz(0.0, 1.5, 16.0)
                    .looking_at(Vec3::new(10.0, 0.0, -2.0), Vec3::Y);
            }
            state.phase = 1;
            state.phase_started = state.frames;
        }
        1 if phase_frames >= 8 && right_visible => {
            state.right_seen = true;
            if let Ok(mut camera) = camera.single_mut() {
                *camera = Transform::from_xyz(0.0, 1.5, 16.0)
                    .looking_at(Vec3::new(-10.0, 0.0, -2.0), Vec3::Y);
            }
            state.phase = 2;
            state.phase_started = state.frames;
        }
        2 if phase_frames >= 8 && left_visible => {
            state.left_seen = true;
            if let Ok(mut camera) = camera.single_mut() {
                *camera = Transform::from_xyz(0.0, 1.5, 16.0).looking_at(Vec3::ZERO, Vec3::Y);
            }
            state.phase = 3;
            state.phase_started = state.frames;
        }
        3 if phase_frames >= 8 && center_visible && renderer.final_path_active() => {
            renderer.renderer_fixture_validated =
                state.center_seen && state.right_seen && state.left_seen;
            renderer.renderer_validation_failures += (!renderer.renderer_fixture_validated) as u64;
            profiler.increment("renderer/fixture_validated", 1);
            state.finished = true;
        }
        _ if state.frames >= 180 => {
            renderer.renderer_validation_failures =
                renderer.renderer_validation_failures.saturating_add(1);
            profiler.increment("renderer/validation_failures", 1);
            state.finished = true;
        }
        _ => {}
    }
}

#[derive(Deserialize)]
struct RuntimeManifest {
    schema_version: u32,
    complete: bool,
}

#[derive(Deserialize)]
struct RuntimeIntegrationReport {
    schema_version: u32,
    passed: bool,
}

fn validate_runtime_assets(config: &EngineConfig) -> Result<()> {
    for required in ["skyrim_world.db", "cell_cache.rkyv"] {
        color_eyre::eyre::ensure!(
            config.assets_dir.join(required).is_file(),
            "converted asset set is missing {required}: {}",
            config.assets_dir.display()
        );
    }
    if config.allow_incomplete_assets {
        crate::world::database::validate_lod_build_contract(
            &config.assets_dir,
            converter_schema_version(),
        )?;
        return Ok(());
    }
    let manifest_path = config.assets_dir.join("conversion-manifest.json");
    let manifest: RuntimeManifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .wrap_err_with(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .wrap_err("invalid conversion manifest")?;
    if !(shared::MIN_RUNTIME_CONVERTER_SCHEMA_VERSION..=converter_schema_version())
        .contains(&manifest.schema_version)
    {
        let rejection = if manifest.schema_version < shared::MIN_RUNTIME_CONVERTER_SCHEMA_VERSION {
            AssetSetRejection::ConverterSchemaOlder {
                found: manifest.schema_version,
            }
        } else {
            AssetSetRejection::ConverterSchemaNewer {
                found: manifest.schema_version,
            }
        };
        color_eyre::eyre::bail!(
            "{}",
            asset_set_rejection_message(&config.assets_dir, rejection)
        );
    }
    color_eyre::eyre::ensure!(
        manifest.complete,
        "{}",
        asset_set_rejection_message(
            &config.assets_dir,
            AssetSetRejection::IncompleteConversion {
                schema: manifest.schema_version
            }
        )
    );
    let report_path = config.assets_dir.join("integration-report.json");
    let report: RuntimeIntegrationReport = serde_json::from_slice(
        &std::fs::read(&report_path)
            .wrap_err_with(|| format!("failed to read {}", report_path.display()))?,
    )
    .wrap_err("invalid integration report")?;
    if !shared::supports_runtime_world_database_schema(report.schema_version) {
        color_eyre::eyre::bail!(
            "{}",
            asset_set_rejection_message(
                &config.assets_dir,
                AssetSetRejection::WorldDatabaseSchema {
                    found: report.schema_version,
                }
            )
        );
    }
    color_eyre::eyre::ensure!(
        report.passed,
        "{}",
        asset_set_rejection_message(
            &config.assets_dir,
            AssetSetRejection::IntegrationReportFailed
        )
    );
    let database_schema = crate::world::database::validate_lod_build_contract(
        &config.assets_dir,
        converter_schema_version(),
    )?;
    color_eyre::eyre::ensure!(
        database_schema == report.schema_version,
        "world database schema {database_schema} does not match integration report schema {}",
        report.schema_version
    );
    Ok(())
}

/// Why the runtime refused a converted asset set.
///
/// Each situation needs a different fix, so each gets its own message instead
/// of one "incomplete or stale" error covering all of them.
#[derive(Clone, Copy, Debug)]
enum AssetSetRejection {
    /// The converter that wrote the set is older than every schema this engine accepts.
    ConverterSchemaOlder { found: u32 },
    /// The converter that wrote the set is newer than every schema this engine accepts.
    ConverterSchemaNewer { found: u32 },
    /// The converter stopped early or skipped inputs (`complete: false`); `schema` is the one the
    /// manifest names.
    IncompleteConversion { schema: u32 },
    /// The integration report names a world database schema outside the accepted range.
    WorldDatabaseSchema { found: u32 },
    /// The integration report ran and reported failures.
    IntegrationReportFailed,
}

/// Names the failed check, what it found, the range it accepts, and the
/// command that fixes it.
fn asset_set_rejection_message(assets_dir: &Path, rejection: AssetSetRejection) -> String {
    let converter_min = shared::MIN_RUNTIME_CONVERTER_SCHEMA_VERSION;
    let converter_max = converter_schema_version();
    let database_min = shared::MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION;
    let database_max = shared::WORLD_DATABASE_SCHEMA_VERSION;
    let manifest = assets_dir.join("conversion-manifest.json");
    let report = assets_dir.join("integration-report.json");
    // The converter's usage string takes the Skyrim Data folder first and the
    // output directory second. The engine knows only the directory it was
    // given, so the Data folder stays a placeholder.
    let reconvert = format!(
        "cargo run --release -p converter -- <Skyrim Data folder> \"{}\"",
        assets_dir.display()
    );
    match rejection {
        AssetSetRejection::ConverterSchemaOlder { found } => format!(
            "converted assets are stale: {} was written by converter schema {found}, but this \
             engine accepts converter schemas {converter_min} through {converter_max}; reconvert \
             with `{reconvert}` (the converter reuses what it can from the previous conversion)",
            manifest.display()
        ),
        AssetSetRejection::ConverterSchemaNewer { found } => format!(
            "converted assets are newer than this engine: {} was written by converter schema \
             {found}, but this engine understands only converter schemas {converter_min} through \
             {converter_max}; update the engine and rebuild it (`cargo build --release -p \
             engine`), or reconvert with a converter at schema {converter_max}",
            manifest.display()
        ),
        AssetSetRejection::IncompleteConversion { schema } => format!(
            "the asset conversion did not finish: {} reports complete=false at converter schema \
             {schema}, so the converter stopped early or skipped inputs; rerun `{reconvert}` (it \
             reuses unchanged work), or start the engine with --allow-incomplete-assets to use \
             what is there",
            manifest.display()
        ),
        AssetSetRejection::WorldDatabaseSchema { found } => format!(
            "the converted assets use an unsupported world database schema: {} reports world \
             database schema {found} is unsupported; this engine accepts world database schemas \
             {database_min} through {database_max}; reconvert with a converter built from the \
             same revision as this engine: `{reconvert}`",
            report.display()
        ),
        AssetSetRejection::IntegrationReportFailed => format!(
            "the asset integration report did not pass: {} reports passed=false; read that report \
             for the failing check, then reconvert with `{reconvert}`",
            report.display()
        ),
    }
}

const fn converter_schema_version() -> u32 {
    // Shared with the producer without linking the converter into the runtime.
    shared::LOD_CONVERTER_SCHEMA_VERSION
}

fn setup_synthetic_benchmark(
    mut commands: Commands,
    config: Res<EngineConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = std::time::Instant::now();
    let mesh = Mesh3d(meshes.add(Cuboid::new(18.0, 60.0, 18.0)));
    let material = MeshMaterial3d(terrain_materials.add(TerrainMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.16, 0.36, 0.12),
            perceptual_roughness: 0.9,
            ..default()
        },
        extension: TerrainExtension::default(),
    }));
    let side = (config.synthetic_instances as f64).sqrt().ceil() as usize;
    commands.spawn_batch((0..config.synthetic_instances).map(move |index| {
        let x = index % side;
        let z = index / side;
        (
            mesh.clone(),
            material.clone(),
            Transform::from_xyz(x as f32 * 32.0, 30.0, -(z as f32 * 32.0)),
        )
    }));
    info!(
        instances = config.synthetic_instances,
        "synthetic indirect-render benchmark initialized"
    );
    profiler.increment("synthetic/instances", config.synthetic_instances as u64);
    profiler.record_elapsed("startup/synthetic_scene", started);
}

/// How far the camera draws: the far corner of the terrain ring and a margin, so the whole ring is
/// inside it. With the ring off (a `terrain_radius` at or below `stream_radius`) this is what it
/// was before the ring existed, `CELL_SIZE * (stream_radius + 2) * 2`.
///
/// A long far plane costs almost nothing: Bevy uses reverse-Z, whose precision at a given distance
/// depends on the near plane and the float's mantissa, not on where the far plane is.
pub(crate) fn camera_far_plane(config: &EngineConfig) -> f32 {
    let radius = config.terrain_radius.max(config.stream_radius).max(1);
    crate::world::components::CELL_SIZE * (radius + 2) as f32 * 2.0
}

/// The distance fog of an exterior: clear up to the edge of the full-detail grid, fully opaque at
/// the far edge of the terrain ring, in [`SKY_COLOR`] or the underground colour of the space. The
/// ring then fades into the sky it is drawn against instead of ending in a cliff.
///
/// With no ring - a `terrain_radius` at or below `stream_radius`, which is the engine before the
/// ring existed - there is nothing beyond the full-detail grid to fade, and the fog is off.
///
/// `lod_reach` is how far distant terrain reaches ([`crate::streaming::LodHorizon`]) when the
/// world has LOD data, and `None` when it has none. The terrain LOD draws beyond the ring, under
/// the ring's own cells where those are ready and past them where they are not, so fog that is
/// opaque at the ring's edge would hide all of it as sky colour. With LOD the fog is still clear
/// up to the full-detail grid but ends where the LOD does, ring or no ring: the horizon fades into
/// the sky instead of ending in a cliff, and the ring keeps what it is for, full-detail landscape
/// close in. The end is never nearer than the ring's own.
pub(crate) fn exterior_fog(
    stream_radius: i32,
    terrain_radius: i32,
    lod_reach: Option<f32>,
    color: Color,
) -> DistanceFog {
    let cell = crate::world::components::CELL_SIZE;
    let start = cell * (stream_radius.max(0) + 1) as f32;
    let ring_end =
        (terrain_radius > stream_radius).then(|| cell * (terrain_radius.max(0) + 1) as f32);
    let end = match (lod_reach, ring_end) {
        (Some(reach), ring) => reach.max(ring.unwrap_or(0.0)),
        (None, Some(ring)) => ring,
        (None, None) => return fog_off(color),
    };
    if !end.is_finite() || end <= start {
        return fog_off(color);
    }
    DistanceFog {
        color,
        falloff: FogFalloff::Linear { start, end },
        directional_light_color: Color::NONE,
        directional_light_exponent: 8.0,
    }
}

/// Fog that cannot be reached: it starts past any far plane the camera has, so no mesh in the space
/// is ever fogged. Used indoors, where the fog would be wrong - a Dwemer hall three thousand units
/// across has no haze - while the component stays on the camera, so the shader pipeline (and the
/// frame that follows a crossing) does not have to be rebuilt when it comes back.
pub(crate) fn fog_off(color: Color) -> DistanceFog {
    const OFF_START: f32 = crate::world::components::CELL_SIZE * 100.0;
    DistanceFog {
        color,
        falloff: FogFalloff::Linear {
            start: OFF_START,
            end: OFF_START * 2.0,
        },
        directional_light_color: Color::NONE,
        directional_light_exponent: 8.0,
    }
}

/// Creation units in a metre: Skyrim's unit is 1.43 cm and this engine renders one Creation unit as
/// one Bevy world unit, so a distance a renderer's default expresses in metres is
/// [`CREATION_UNITS_PER_METRE`] times larger here. `crate::lights` converts the point lights'
/// intensity at the same ratio.
const CREATION_UNITS_PER_METRE: f32 = 70.0;

/// The cascades the sun's shadow map is split into. Bevy supports four (`MAX_CASCADES_PER_LIGHT`,
/// bevy_pbr-0.19.0 `src/render/light.rs:228`) and clamps a light that asks for more, and
/// [`sun_shadow_cascades`] reaches 248 m, so fewer cascades would mean larger ones and visibly
/// coarser shadows everywhere the player looks.
pub(crate) const SUN_SHADOW_CASCADES: usize = 4;

/// A cascade's shadow map, in texels a side. This is Bevy's own default
/// (`bevy_light-0.19.0` `src/directional_light.rs:202-206`), written out rather than left implicit
/// so that it is chosen together with the cascade distances that were fitted to it: at
/// [`SUN_SHADOW_CASCADES`] the nearest cascade draws a centimetre of the world per texel and the
/// farthest 24 cm, and 4096 - four times the memory and four times the fill - is not what this
/// world's geometry is short of.
pub(crate) const SUN_SHADOW_MAP_SIZE: usize = 2048;

/// The sun's shadow cascades, fitted to a world whose unit is 1.43 cm.
///
/// # Where the shadows were
///
/// Nothing in the world cast one, and this is why. `DirectionalLight::shadow_maps_enabled` was
/// already true, but the sun had no [`CascadeShadowConfig`], so it used Bevy's default - which is
/// built for a metre-scale world. `CascadeShadowConfigBuilder::default` (bevy_light-0.19.0
/// `src/cascade.rs:135-158`) is four cascades from a first far bound of 10 to a maximum distance
/// of 150, split geometrically into the bounds 10 / 24.7 / 60.8 / 150. Read in Creation units that
/// is a shadow map spent on the 2.1 metres of ground around the camera, with the first cascade
/// ending 14 cm in front of the lens: every house, tree and mill of the demo stood outside the
/// last cascade, so the sun lit the whole scene and none of it fell in shadow.
///
/// # The distances
///
/// The first two are Bevy's defaults read in metres and converted at [`CREATION_UNITS_PER_METRE`]:
/// a first cascade far bound of 10 m (700 units) and a near clamp of 0.1 m (7 units), which is where
/// the first cascade's ortho box starts, so anything nearer the camera than 10 cm is unshadowed -
/// the same distance Bevy's default works at.
///
/// The maximum distance is not a conversion but a property of this engine. `streaming::wanted_cells`
/// holds the camera's cell and `stream_radius` cells around it, so the camera - anywhere inside its
/// own cell - stands at most `stream_radius + 1` cells from the far edge of the full-detail grid and
/// sqrt(2) times that from its far corner. Setting the range to that corner puts every mesh the
/// streamer holds in full inside the last cascade, whichever way the camera faces, and it is also
/// where the range has to stop: past the grid there are no casters, because the terrain ring is
/// deliberately `NotShadowCaster` (`streaming.rs`, the ring's `patch.insert`), so covering the ring
/// would take texel size away from the shadows that are visible without adding one.
///
/// At the default `stream_radius` of 2 that is 17,378 units, or 248 m, split into the far bounds
/// 700 / 2,042 / 5,959 / 17,378. At the engine's 45 degree vertical field of view those four
/// cascades draw 0.96 cm, 2.8 cm, 8.2 cm and 24 cm of the world per shadow texel. The fifth of a
/// cascade that each overlaps the previous one by is Bevy's default and is left as it is: it is the
/// band the shader blends two cascades over, and it should stay a proportion of the cascade.
///
/// # The cost, and the knobs
///
/// This is the first configuration in which the shadow pass draws the world at all. The union of
/// the cascades is the camera's frustum out to 17,378 units, which is everything the full-detail
/// grid holds, so the pass costs about what the main opaque pass costs again; the portal camera
/// renders cascades of its own on top (`crate::portal`), so a doorway in view adds a second set.
/// `streaming.rs` has this engine's one measurement of that: taking the ring's terrain out of the
/// shadow pass took a twelve-cell Pale view from 30 to 59 frames a second. A frame rate that
/// suffers is turned back up by, in order of how much they give: [`maximum_distance`] here, which
/// is the geometry the pass draws at all; [`SUN_SHADOW_CASCADES`], which is how many passes it is
/// split over; and [`SUN_SHADOW_MAP_SIZE`], which is fill rate rather than geometry.
///
/// # The biases are left alone, on the numbers
///
/// `DirectionalLight::shadow_depth_bias` is 0.02 (`bevy_light-0.19.0`
/// `src/directional_light.rs:165`) and is added to the fragment's world position along the light's
/// direction (`bevy_pbr-0.19.0` `src/render/shadows.wgsl:192`), so in this world it is a 0.29 mm
/// step - three orders of magnitude below the texel-sized depth error it would have to correct, and
/// a constant that would have to grow with the cascade to mean anything at all. The work is done
/// by `shadow_normal_bias`, 1.8 (`src/directional_light.rs:167`), which is scale-free as it stands:
/// the shader multiplies it by the cascade's own texel size (`shadows.wgsl:191`), and the
/// extraction multiplies it by sqrt(2) for the worst case on the diagonal (`render/light.rs:811`),
/// so it is 2.5 texels of offset along the surface normal in every cascade - about 2 cm under the
/// player and 42 units in the last one, where a texel is 24 cm across anyway. That is the same
/// trade Bevy's default makes in a metre world, so neither value was changed on a guess: if a run
/// shows acne on distant ground the knob is `shadow_normal_bias`, and if small objects lose their
/// contact shadows it is too high. Neither could be judged here, where no GPU was in the loop.
///
/// [`maximum_distance`]: CascadeShadowConfigBuilder::maximum_distance
///
/// `cascades` and `distance` are the graphics settings' (`crate::graphics_settings`):
/// [`SUN_SHADOW_CASCADES`] and `None` - the reach above - are the demo's look.
pub(crate) fn sun_shadow_cascades(
    stream_radius: i32,
    cascades: usize,
    distance: Option<f32>,
) -> CascadeShadowConfig {
    // The `+ 1` is the camera's own cell: the grid is `stream_radius` cells around the camera's
    // cell rather than around the camera, and the camera may stand at the far edge of its own. A
    // radius the command line can be given as negative leaves nothing to load; clamped to 0 it is
    // the camera's own cell alone, which still leaves the first cascade inside the last.
    let full_detail = crate::world::components::CELL_SIZE * (stream_radius.max(0) + 1) as f32;
    let first_cascade_far_bound = 10.0 * CREATION_UNITS_PER_METRE;
    CascadeShadowConfigBuilder {
        minimum_distance: 0.1 * CREATION_UNITS_PER_METRE,
        // A reach the settings name is kept past the first cascade, which the builder requires.
        maximum_distance: distance.map_or(full_detail * std::f32::consts::SQRT_2, |distance| {
            distance.max(first_cascade_far_bound * 1.01)
        }),
        first_cascade_far_bound,
        num_cascades: cascades.max(1),
        overlap_proportion: 0.2,
    }
    .into()
}

/// Where `setup_world` stands the camera relative to the point it looks at, the centre of the cell
/// at the origin of the world grid: the acceptance screenshot is taken from far above it, the
/// walk-around view from behind and above it. [`sun_shadow_cascades`] measures its range from the
/// camera, so the offsets live here rather than in two places.
fn camera_offset(config: &EngineConfig) -> Vec3 {
    if config.acceptance_screenshot.is_some() {
        config
            .screenshot_camera_offset
            .map(Vec3::from)
            .unwrap_or(Vec3::new(0.0, 20_000.0, 1000.0))
    } else {
        Vec3::new(0.0, 1200.0, 2500.0)
    }
}
fn setup_world(
    mut commands: Commands,
    config: Res<EngineConfig>,
    ground_height: Option<Res<InitialCameraGroundHeight>>,
    start_pose: Option<Res<crate::portal::StartPose>>,
    tuning: Option<Res<MovementTuning>>,
) {
    let ground_height = ground_height.as_deref().map_or(0.0, |height| height.0);
    let target = Vec3::new(CELL_SIZE_HALF, ground_height, -CELL_SIZE_HALF);
    let camera_offset = if world_physics_player(&config) {
        let tuning = tuning.as_deref().cloned().unwrap_or_default();
        Vec3::Y * (tuning.eye_height + tuning.capsule_standing_height * 0.5 + 16.0)
    } else {
        camera_offset(&config)
    };
    let camera_position = target + camera_offset;
    // A run that named a start (--demo / --start-position) leaves the pose it wants as a resource,
    // in the render space of the start grid: an explicit start is a Creation-engine foot position
    // with the eye above it, facing the requested heading. Without one, the camera looks at the
    // middle of the start cell from above - or, in an interactive world-physics run, straight ahead
    // along -Z from where the player stands.
    let camera_transform = start_pose.as_deref().map_or_else(
        || {
            if world_physics_player(&config) {
                Transform::from_translation(camera_position)
                    .looking_at(camera_position + Vec3::NEG_Z, Vec3::Y)
            } else {
                Transform::from_translation(camera_position).looking_at(target, Vec3::Y)
            }
        },
        |pose| pose.0,
    );
    let far = camera_far_plane(&config);
    commands.spawn((
        Camera3d::default(),
        crate::color_pipeline::SceneColorPipeline::default(),
        // The backdrop, per camera. `crate::atmosphere::update_atmosphere` rewrites both this and
        // the `ClearColor` resource as soon as it knows which space the camera is in; the resource
        // is what a view that is not the main camera gets (the water reflection camera renders the
        // same space), and the component is what makes a doorway able to clear to a different
        // colour from the room it opens off (`crate::portal`).
        Camera {
            clear_color: ClearColorConfig::Custom(SKY_COLOR),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection { far, ..default() }),
        camera_transform,
        StreamingCamera,
        // This camera draws the streamed world, so the weather's distance fog covers it.
        // The sky draws a dome around this camera and clears it to the weather's fog colour.
        SkyCamera,
        Msaa::Off,
        DepthPrepass,
        OcclusionCulling,
        RenderLayers::from_layers(&[0, 1]),
        // Glow. `Hdr` gives the frame an intermediate format with room above white: the
        // sun of a daylight exterior, the light pools of `crate::lights` (a converted light
        // delivers `LIGHT_EXPOSURE` = 10 times the ambient an interior applies -
        // `crate::atmosphere`'s `INTERIOR_AMBIENT_BRIGHTNESS` at `INTERIOR_AMBIENT_LEVEL` - at
        // half its radius) and a
        // converted glow all reach past 1.0, and without a float target they would flatten there
        // before the tonemapper could roll them off. `Bloom::NATURAL` spreads the brightest of those
        // values into their neighbours, which is the soft halo the reference screenshots show
        // around the mushroom caps and the `BlackreachSun01` orb. `NATURAL` is Bevy's
        // energy-conserving preset: it moves brightness between neighbouring pixels instead of
        // adding to the frame, so a daylight exterior keeps its exposure - measured on the nine
        // `SR-place-Alftand*` shots, whose pooled median moves by 0.8 %.
        //
        // The portal camera deliberately gets neither. It leaves tonemapping to this camera
        // (`crate::portal`), so a bloom pass of its own would composite into the doorway image
        // and be bloomed a second time here.
        //
        // The bloom itself, the tonemapper, the exposure and the rest are the graphics settings'
        // (`crate::graphics_settings`, whose `current` preset is `Bloom::NATURAL` and Bevy's
        // defaults): they are written to this camera in the frame it appears. `Hdr` itself comes
        // with main's `SceneColorPipeline` above.
    ));
    // The sun's shadows. `shadow_maps_enabled` was never the missing piece - the cascades were:
    // without a configuration of its own the sun gets Bevy's, which covers the two metres in front
    // of the camera, and a shadow map spent on that patch is a world lit flat
    // (`sun_shadow_cascades`). The shadow map's size (`DirectionalLightShadowMap`, today's
    // [`SUN_SHADOW_MAP_SIZE`]) and the cascades' count and reach are the graphics settings', which
    // rewrite these from the `MainSun` marker when they differ.
    commands.spawn((
        DirectionalLight {
            illuminance: DAY_SUN_ILLUMINANCE,
            shadow_maps_enabled: true,
            ..default()
        },
        sun_shadow_cascades(config.stream_radius, SUN_SHADOW_CASCADES, None),
        crate::graphics_settings::MainSun,
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: SKY_AMBIENT_COLOR,
        brightness: SKY_AMBIENT_BRIGHTNESS,
        ..default()
    });
    info!(
        assets = %config.assets_dir.display(),
        worldspace = format_args!("{:08X}", config.worldspace_id),
        ground_height,
        camera = ?camera_position,
        target = ?target,
        "Mudcrab runtime initialized"
    );
}

fn initial_camera_ground_height(
    config: &EngineConfig,
    database_path: &std::path::Path,
    cache: &CellCache,
) -> Result<f32> {
    Ok(cell_centre_ground_height(
        config.worldspace_id,
        config.start_grid,
        database_path,
        cache,
    )?
    .unwrap_or(0.0))
}

/// The terrain height at the middle of one exterior cell, or `None` when the cell has no terrain
/// (or the cache has no height there).
fn cell_centre_ground_height(
    worldspace_id: u32,
    grid: (i32, i32),
    database_path: &std::path::Path,
    cache: &CellCache,
) -> Result<Option<f32>> {
    let connection = Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .wrap_err_with(|| format!("failed to open {}", database_path.display()))?;
    let cell_id = connection
        .query_row(
            crate::world::database::EXTERIOR_CELL_ID_SQL,
            params![worldspace_id, grid.0, grid.1],
            |row| row.get::<_, u32>(0),
        )
        .optional()?;
    let Some(terrain) = cell_id.and_then(|cell_id| cache.terrain(cell_id)) else {
        return Ok(None);
    };
    let width = usize::from(terrain.width);
    let height = usize::from(terrain.height);
    let center = (height / 2)
        .checked_mul(width)
        .and_then(|row| row.checked_add(width / 2));
    Ok(center.and_then(|index| terrain.heights.get(index)).copied())
}

/// Where `--benchmark-jump` puts the camera: the middle of exterior cell `grid`, at that cell's
/// ground height plus the same height offset `setup_world` gives the start position, in the
/// coordinates of the current render `origin`.
pub(crate) fn benchmark_jump_position(
    config: &EngineConfig,
    cache: &CellCache,
    origin: IVec2,
    grid: (i32, i32),
) -> Vec3 {
    let database_path = config.assets_dir.join("skyrim_world.db");
    let ground_height =
        match cell_centre_ground_height(config.worldspace_id, grid, &database_path, cache) {
            Ok(Some(height)) => height,
            // A jump target with no terrain is legitimate (a cell past the map edge the run may
            // still want to view), but a silent 0 puts the camera at the wrong height, so say so.
            Ok(None) => {
                warn!(
                    grid_x = grid.0,
                    grid_y = grid.1,
                    "benchmark jump: no terrain at the target cell, using ground height 0"
                );
                0.0
            }
            Err(error) => {
                warn!(%error, "benchmark jump: ground height unavailable, using 0");
                0.0
            }
        };
    jump_camera_position(grid, origin, ground_height + camera_offset(config).y)
}

/// The middle of grid square `grid` at height `y`, in the render space around `origin`: +x is east,
/// +grid_y runs towards -z.
fn jump_camera_position(grid: (i32, i32), origin: IVec2, y: f32) -> Vec3 {
    Vec3::new(
        ((grid.0 - origin.x) as f32 + 0.5) * crate::world::components::CELL_SIZE,
        y,
        -((grid.1 - origin.y) as f32 + 0.5) * crate::world::components::CELL_SIZE,
    )
}

const CELL_SIZE_HALF: f32 = crate::world::components::CELL_SIZE * 0.5;
const AUTO_FLIGHT_HALF_SPAN: f32 = crate::world::components::CELL_SIZE * 4.0;

#[derive(Default)]
struct AutoFlightState {
    initialized: bool,
    axis: Vec3,
    sign: f32,
    offset: f32,
}

fn bounded_auto_flight_direction(
    forward: Vec3,
    step_distance: f32,
    state: &mut AutoFlightState,
) -> Vec3 {
    if !state.initialized {
        state.initialized = true;
        state.axis = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::NEG_Z);
        state.sign = 1.0;
    }
    let next_offset = state.offset + state.sign * step_distance.max(0.0);
    if next_offset >= AUTO_FLIGHT_HALF_SPAN {
        state.sign = -1.0;
    } else if next_offset <= -AUTO_FLIGHT_HALF_SPAN {
        state.sign = 1.0;
    }
    state.offset += state.sign * step_distance.max(0.0);
    state.axis * state.sign
}

fn fly_camera(
    time: Res<Time>,
    config: Res<EngineConfig>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<&mut Transform, With<StreamingCamera>>,
    mut profiler: ResMut<ProfilingState>,
    mut auto_flight: Local<AutoFlightState>,
) {
    // Interactive player paths own the camera; automated camera paths keep legacy controls.
    if config.physics_fixture || world_physics_player(&config) {
        return;
    }
    let started = std::time::Instant::now();
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };
    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        direction += *transform.forward();
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction += *transform.back();
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction += *transform.left();
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += *transform.right();
    }
    if keyboard.pressed(KeyCode::Space) {
        direction += Vec3::Y;
    }
    if keyboard.pressed(KeyCode::ShiftLeft) {
        direction -= Vec3::Y;
    }
    let acceptance_capture_pending = config
        .acceptance_screenshot
        .as_ref()
        .is_some_and(|path| !path.is_file());
    let speed = if config.auto_fly_speed > 0.0 {
        config.auto_fly_speed
    } else if keyboard.pressed(KeyCode::ControlLeft) {
        4000.0
    } else {
        900.0
    };
    if config.auto_fly_speed > 0.0 && !acceptance_capture_pending {
        direction += bounded_auto_flight_direction(
            *transform.forward(),
            speed * time.delta_secs(),
            &mut auto_flight,
        );
    }
    transform.translation += direction.normalize_or_zero() * speed * time.delta_secs();
    profiler.record_elapsed("world/fly_camera", started);
}

fn capture_acceptance_screenshot(
    mut commands: Commands,
    config: Res<EngineConfig>,
    mut state: Local<ScreenshotCaptureState>,
    streaming: Option<Res<StreamingMetrics>>,
    world_database: Option<Res<WorldDatabase>>,
    renderer: Res<RendererMetrics>,
    windows: Query<(), With<Window>>,
) {
    let Some(path) = &config.acceptance_screenshot else {
        return;
    };
    state.frames = state.frames.saturating_add(1);
    let gpu_warmed_up = state
        .started
        .get_or_insert_with(std::time::Instant::now)
        .elapsed()
        >= std::time::Duration::from_secs(2);
    if state.captured
        || state.frames < config.benchmark_warmup_frames.saturating_add(10)
        || !gpu_warmed_up
        || windows.is_empty()
    {
        return;
    }
    let assets_ready = streaming
        .as_deref()
        .is_none_or(|metrics| screenshot_assets_ready(metrics, world_database.is_some(), &config));
    let renderer_ready = renderer.final_path_active()
        && (!config.renderer_fixture || renderer.renderer_fixture_validated);
    if !assets_ready || !renderer_ready {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        error!(%error, path = %path.display(), "failed to create screenshot directory");
        return;
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.clone()));
    state.captured = true;
}

fn screenshot_assets_ready(
    metrics: &StreamingMetrics,
    world_streaming_active: bool,
    config: &EngineConfig,
) -> bool {
    metrics.pending_asset_instances == 0
        && metrics.pending_surface_instances == 0
        && metrics.loading_cells == 0
        && metrics.pending_lod_queries == 0
        && metrics.pending_lod_chunks == 0
        && metrics.failed_cells == 0
        && metrics.unrecovered_lod_queries == 0
        && metrics.unrecovered_lod_chunks == 0
        && (!world_streaming_active || metrics.resident_cells > 0)
        && metrics.asset_load_failures == 0
        && metrics.material_validation_failures == 0
        && metrics.transform_bounds_validation_failures == 0
        && metrics.diagnostic_fallbacks == 0
        && metrics.streaming_invariant_failures == 0
        && metrics.streaming_fixture_failures == 0
        && (!config.material_fixture || metrics.canonical_fixture_validated)
        && (!config.terrain_water_fixture || metrics.terrain_water_fixture_validated)
        && (!config.transform_bounds_fixture || metrics.transform_bounds_fixture_validated)
        && (!config.streaming_fixture || metrics.streaming_fixture_validated)
        && (!config.physics_fixture || metrics.physics_fixture_validated)
}

#[derive(Default)]
struct ScreenshotCaptureState {
    frames: u32,
    captured: bool,
    started: Option<std::time::Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atmosphere::UNDERGROUND_COLOR;
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::world_serialization::WorldSerializationPlugin;

    /// The origin cell's centre is (2048, y, -2048), the start position's convention, and +grid_y
    /// runs towards -z, so the sign and the half-cell offset are pinned per axis.
    #[test]
    fn jump_position_is_the_target_cells_centre_around_the_origin() {
        for (grid, origin, expected) in [
            ((0, 0), IVec2::ZERO, Vec3::new(2048.0, 9.0, -2048.0)),
            ((1, 0), IVec2::ZERO, Vec3::new(6144.0, 9.0, -2048.0)),
            ((0, 1), IVec2::ZERO, Vec3::new(2048.0, 9.0, -6144.0)),
            ((-1, -1), IVec2::ZERO, Vec3::new(-2048.0, 9.0, 2048.0)),
            (
                (4, -21),
                IVec2::new(4, -12),
                Vec3::new(2048.0, 9.0, 34816.0),
            ),
        ] {
            assert_eq!(
                jump_camera_position(grid, origin, 9.0),
                expected,
                "grid {grid:?} origin {origin:?}"
            );
        }
    }

    /// Uses real loaders with a generated external normal image, without game assets or a GPU.
    fn world_normal_loader_fixture(wrap_s: u32, wrap_t: u32) -> (tempfile::TempDir, App) {
        use bevy::{
            image::{CompressedImageFormats, ImageLoader, ImagePlugin},
            scene::ScenePlugin,
        };

        let directory = tempfile::tempdir().unwrap();
        Image::new_fill(
            Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[128, 128, 255, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )
        .try_into_dynamic()
        .unwrap()
        .save(directory.path().join("shared.png"))
        .unwrap();
        fs::write(
            directory.path().join("road.gltf"),
            serde_json::to_vec(&serde_json::json!({
                "asset": {"version": "2.0"},
                "images": [{"uri": "shared.png"}],
                "samplers": [{
                    "wrapS": wrap_s, "wrapT": wrap_t,
                    "magFilter": 9728, "minFilter": 9984
                }],
                "textures": [{"source": 0, "sampler": 0}],
                "materials": [{"normalTexture": {"index": 0}}]
            }))
            .unwrap(),
        )
        .unwrap();

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: directory.path().to_string_lossy().into_owned(),
                ..default()
            },
            ImagePlugin::default(),
            ScenePlugin,
            world_gltf_plugin(),
        ))
        .init_asset::<Mesh>()
        .register_asset_loader(ImageLoader::new(CompressedImageFormats::NONE));
        app.finish();
        app.cleanup();
        (directory, app)
    }

    /// Waits for dependencies too, so each test controls which consumer finishes loading first.
    fn wait_for_world_asset<A: Asset>(app: &mut App, handle: &Handle<A>) {
        use bevy::asset::LoadState;

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            app.update();
            let server = app.world().resource::<AssetServer>();
            if let LoadState::Failed(error) = server.load_state(handle.id()) {
                panic!("fixture load failed: {error}");
            }
            if server.is_loaded_with_dependencies(handle.id()) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "fixture load timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn loaded_world_normal(app: &App, handle: &Handle<bevy::gltf::Gltf>) -> Handle<Image> {
        use bevy::gltf::{Gltf, GltfMaterial};

        let gltf = app.world().resource::<Assets<Gltf>>().get(handle).unwrap();
        app.world()
            .resource::<Assets<GltfMaterial>>()
            .get(&gltf.materials[0])
            .unwrap()
            .normal_map_texture
            .as_ref()
            .unwrap()
            .clone()
    }

    #[test]
    fn shared_world_normal_sampler_is_independent_of_first_consumer() {
        use bevy::{
            gltf::Gltf,
            image::{ImageLoaderSettings, ImageSampler},
        };

        for land_first in [false, true] {
            let (_directory, mut app) = world_normal_loader_fixture(10497, 10497);
            let server = app.world().resource::<AssetServer>().clone();
            let load_land = || {
                server
                    .load_builder()
                    .with_settings(|settings: &mut ImageLoaderSettings| {
                        settings.is_srgb = false;
                        settings.sampler = ImageSampler::Descriptor(terrain_layer_sampler());
                    })
                    .load::<Image>("shared.png")
            };
            let (land, road) = if land_first {
                let land = load_land();
                wait_for_world_asset(&mut app, &land);
                let road = server.load::<Gltf>("road.gltf");
                wait_for_world_asset(&mut app, &road);
                (land, road)
            } else {
                let road = server.load::<Gltf>("road.gltf");
                wait_for_world_asset(&mut app, &road);
                let land = load_land();
                wait_for_world_asset(&mut app, &land);
                (land, road)
            };
            assert_eq!(loaded_world_normal(&app, &road), land);
            let image = app.world().resource::<Assets<Image>>().get(&land).unwrap();
            assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
            assert_eq!(
                image.sampler,
                ImageSampler::Descriptor(terrain_layer_sampler())
            );
        }
    }

    #[test]
    fn v15_distinct_wrap_aliases_keep_samplers_in_both_load_orders() {
        use bevy::{
            gltf::Gltf,
            image::{ImageAddressMode, ImageSampler},
        };
        for clamp_first in [false, true] {
            let (directory, mut app) = world_normal_loader_fixture(10497, 10497);
            fs::copy(
                directory.path().join("shared.png"),
                directory.path().join("shared.opensky-wrap0.png"),
            )
            .unwrap();
            let mut document: serde_json::Value =
                serde_json::from_slice(&fs::read(directory.path().join("road.gltf")).unwrap())
                    .unwrap();
            document["images"][0]["uri"] = serde_json::json!("shared.opensky-wrap0.png");
            document["samplers"][0]["wrapS"] = serde_json::json!(33071);
            document["samplers"][0]["wrapT"] = serde_json::json!(33071);
            fs::write(
                directory.path().join("clamp.gltf"),
                serde_json::to_vec(&document).unwrap(),
            )
            .unwrap();
            let names = if clamp_first {
                ["clamp.gltf", "road.gltf"]
            } else {
                ["road.gltf", "clamp.gltf"]
            };
            let mut retained = Vec::new();
            for name in names {
                let handle = app.world().resource::<AssetServer>().load::<Gltf>(name);
                wait_for_world_asset(&mut app, &handle);
                retained.push((name, handle));
            }
            assert_ne!(
                loaded_world_normal(&app, &retained[0].1),
                loaded_world_normal(&app, &retained[1].1)
            );
            for (name, handle) in &retained {
                let image_handle = loaded_world_normal(&app, handle);
                let image = app
                    .world()
                    .resource::<Assets<Image>>()
                    .get(&image_handle)
                    .unwrap();
                let ImageSampler::Descriptor(sampler) = &image.sampler else {
                    panic!("missing sampler")
                };
                let expected = if *name == "clamp.gltf" {
                    ImageAddressMode::ClampToEdge
                } else {
                    ImageAddressMode::Repeat
                };
                assert_eq!(sampler.address_mode_u, expected);
                assert_eq!(sampler.address_mode_v, expected);
            }
        }
    }

    #[test]
    fn world_gltf_sampler_preserves_authored_wrapping_with_linear_anisotropy() {
        use bevy::{
            gltf::Gltf,
            image::{ImageAddressMode, ImageFilterMode, ImageSampler},
        };

        for (wrap_s, wrap_t, expected_u, expected_v) in [
            (
                33071,
                33648,
                ImageAddressMode::ClampToEdge,
                ImageAddressMode::MirrorRepeat,
            ),
            (
                33648,
                33071,
                ImageAddressMode::MirrorRepeat,
                ImageAddressMode::ClampToEdge,
            ),
        ] {
            let (_directory, mut app) = world_normal_loader_fixture(wrap_s, wrap_t);
            let road = app
                .world()
                .resource::<AssetServer>()
                .load::<Gltf>("road.gltf");
            wait_for_world_asset(&mut app, &road);
            let normal = loaded_world_normal(&app, &road);
            let image = app
                .world()
                .resource::<Assets<Image>>()
                .get(&normal)
                .unwrap();
            let ImageSampler::Descriptor(sampler) = &image.sampler else {
                panic!("glTF image sampler descriptor missing");
            };
            let default_sampler = terrain_layer_sampler();
            assert_eq!(sampler.address_mode_u, expected_u);
            assert_eq!(sampler.address_mode_v, expected_v);
            assert_eq!(sampler.anisotropy_clamp, default_sampler.anisotropy_clamp);
            assert_eq!(sampler.mag_filter, ImageFilterMode::Linear);
            assert_eq!(sampler.min_filter, ImageFilterMode::Linear);
            assert_eq!(sampler.mipmap_filter, ImageFilterMode::Linear);
            assert_eq!(sampler.lod_min_clamp, default_sampler.lod_min_clamp);
            assert_eq!(sampler.lod_max_clamp, default_sampler.lod_max_clamp);
        }
    }

    #[test]
    fn a_failed_run_is_an_error_and_a_successful_one_is_not() {
        assert!(finished(&AppExit::Success).is_ok());
        let message = finished(&AppExit::error())
            .expect_err("a failed run must not report success")
            .to_string();
        assert!(message.contains("exit code"), "{message}");
    }

    /// The streaming fixture's own systems over its own fixture database, with no window, GPU or
    /// game data: the camera crosses exteriors, the fixture loads its interior by id, and the
    /// camera finishes on a fully streamed exterior ring around the grid it started on.
    fn streaming_fixture_app() -> (App, StreamingFixtureDirectory) {
        let mut config = EngineConfig {
            streaming_fixture: true,
            ..default()
        };
        let directory =
            StreamingFixtureDirectory::create(config.worldspace_id, config.start_grid).unwrap();
        config.assets_dir = directory.path.clone();
        let database_path = config.assets_dir.join("skyrim_world.db");
        let cache_path = config.assets_dir.join("cell_cache.rkyv");
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                watch_for_changes_override: Some(false),
                ..default()
            },
            WorldSerializationPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_asset::<TerrainMaterial>()
        .init_asset::<WaterMaterial>()
        .insert_resource(config)
        .insert_resource(WorldDatabase::open(&database_path).unwrap())
        .insert_resource(AssetCatalog::open(&database_path).unwrap())
        .insert_resource(CellCache::open(&cache_path).unwrap())
        .insert_resource(RenderOrigin(IVec2::ZERO))
        .insert_resource(WaterReflectionTexture(Handle::<Image>::default()))
        .init_resource::<ProfilingState>()
        .init_resource::<StreamingFixtureState>()
        .add_plugins(StreamingPlugin)
        .add_systems(
            PreUpdate,
            (drive_streaming_fixture, cross_streaming_fixture_interior).chain(),
        )
        .add_systems(PostUpdate, validate_streaming_fixture);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        (app, directory)
    }

    #[test]
    fn streaming_fixture_loads_its_interior_by_id_and_keeps_it_while_the_camera_crosses_exteriors()
    {
        let (mut app, _directory) = streaming_fixture_app();
        let expected_resident = {
            let config = app.world().resource::<EngineConfig>();
            ((config.stream_radius * 2 + 1).max(0) as usize).pow(2)
        };
        let mut settled_frames = 0;
        let mut updates = 0;
        while updates < 1_200 {
            app.update();
            // The fixture's frames are vsynced in the acceptance run, and its frame budget assumes
            // that pacing. Pace the headless loop like a 60 Hz frame, so a loaded test machine
            // cannot make the fixture give up before the database worker has answered its cells.
            std::thread::sleep(std::time::Duration::from_millis(16));
            updates += 1;
            let metrics = app.world().resource::<StreamingMetrics>();
            let state = app.world().resource::<StreamingFixtureState>();
            // Settling alone can come before the validator's frame 90 on a fast database, so also
            // wait for the validator's verdict, pass or fail.
            let judged =
                metrics.streaming_fixture_validated || metrics.streaming_fixture_failures > 0;
            let crossed_back = judged
                && state.interior.resident_frame.is_some()
                && metrics.resident_cells >= expected_resident
                && metrics.active_requests == 0
                && metrics.loading_cells == 0;
            settled_frames = if crossed_back { settled_frames + 1 } else { 0 };
            if settled_frames >= 5 {
                break;
            }
        }
        assert!(settled_frames >= 5, "the fixture never settled");

        let interior = app.world().resource::<StreamingFixtureState>().interior;
        assert_eq!(
            interior.requested_frame,
            Some(STREAMING_FIXTURE_INTERIOR_FRAME)
        );
        assert!(interior.resident_frame.is_some());
        assert!(interior.away_samples > 0, "camera never left the grid");
        assert!(
            interior.max_away_roots <= 1,
            "the interior must never have two roots"
        );
        // Current behaviour, not the contract: nothing unloads an interior on this tree, so it is
        // still one root when the camera comes back.
        assert_eq!(interior.min_away_roots, 1);

        let world = app.world_mut();
        let mut roots = world.query_filtered::<&CellRef, With<StreamedCellRoot>>();
        let interior_roots = roots
            .iter(world)
            .filter(|cell| cell.0 == STREAMING_FIXTURE_INTERIOR_CELL_ID)
            .count();
        let mut form_ids = world.query::<&FormId>();
        let interior_references = form_ids
            .iter(world)
            .filter(|form_id| form_id.0 == STREAMING_FIXTURE_INTERIOR_REFERENCE_ID)
            .count();
        assert_eq!(interior_roots, 1);
        assert_eq!(interior_references, 1);

        let metrics = app.world().resource::<StreamingMetrics>();
        assert!(
            metrics.streaming_fixture_validated,
            "the fixture's own validation did not accept the run"
        );
        assert_eq!(metrics.duplicate_cell_roots, 0);
        assert_eq!(metrics.orphaned_cell_roots, 0);
        assert_eq!(metrics.missing_cell_roots, 0);
        assert_eq!(metrics.streaming_invariant_failures, 0);
        assert!(metrics.unloaded_cells > 0);
        assert!(metrics.commit_frames > 0);
    }

    #[test]
    fn interior_crossing_fails_on_a_duplicated_or_mismatched_interior() {
        let crossed = InteriorCrossing {
            requested_frame: Some(STREAMING_FIXTURE_INTERIOR_FRAME),
            resident_frame: Some(40),
            away_samples: 12,
            min_away_roots: 1,
            max_away_roots: 1,
        };
        assert!(interior_crossing_valid(&crossed, 1, 1));
        // A second root for the same cell, or a second copy of its reference.
        assert!(!interior_crossing_valid(&crossed, 2, 1));
        assert!(!interior_crossing_valid(&crossed, 1, 2));
        // Never loaded.
        assert!(!interior_crossing_valid(&InteriorCrossing::default(), 1, 1));
        // Unloaded again is allowed: the contract does not require an interior to stay loaded.
        assert!(interior_crossing_valid(&crossed, 0, 0));
        // A reference left behind without its root, or a root without its reference.
        assert!(!interior_crossing_valid(&crossed, 0, 1));
        assert!(!interior_crossing_valid(&crossed, 1, 0));
        assert!(!interior_crossing_valid(
            &InteriorCrossing {
                requested_frame: Some(STREAMING_FIXTURE_INTERIOR_FRAME),
                ..default()
            },
            1,
            1
        ));
        // Gone on some frames while the camera was away is allowed too.
        assert!(interior_crossing_valid(
            &InteriorCrossing {
                min_away_roots: 0,
                ..crossed
            },
            1,
            1
        ));
        assert!(!interior_crossing_valid(
            &InteriorCrossing {
                max_away_roots: 2,
                ..crossed
            },
            1,
            1
        ));
        // No observation from outside the unload radius at all: the final count alone would pass
        // for an interior that was loaded and dropped again.
        assert!(!interior_crossing_valid(
            &InteriorCrossing {
                away_samples: 0,
                min_away_roots: 0,
                max_away_roots: 0,
                ..crossed
            },
            1,
            1
        ));
    }

    fn write_runtime_asset_database(path: &std::path::Path) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE schema_info(version INTEGER NOT NULL);
                 INSERT INTO schema_info VALUES({});
                 CREATE TABLE lod_chunks(id INTEGER PRIMARY KEY);",
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
    }

    #[test]
    fn runtime_converter_schema_matches_the_actual_producer() {
        assert_eq!(
            converter_schema_version(),
            converter::cache::CONVERTER_SCHEMA_VERSION
        );
    }

    /// The IO pool's stack has to hold a whole queue of loads nested inside one another, because a
    /// scope opened on a pool thread (bevy_gltf's texture scope) runs the other queued tasks on its
    /// own stack while it waits. This queues `LOADS` tasks behind a blocked single-thread pool,
    /// each of which takes a 64 KiB frame and then opens a scope, the shape of a glTF load. They
    /// nest far past the 8 MiB the pool used to have (the portal bench's overflow), and must finish
    /// on [`IO_TASK_STACK_BYTES`].
    #[test]
    fn the_io_pool_stack_holds_a_queue_of_loads_nested_in_scopes() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        };

        const LOADS: usize = 300;
        const FRAME_BYTES: usize = 64 * 1024;

        fn load(pool: &bevy::tasks::TaskPool, depth: &AtomicUsize, deepest: &AtomicUsize) {
            let frame = [0u8; FRAME_BYTES];
            std::hint::black_box(&frame);
            let now = depth.fetch_add(1, Ordering::SeqCst) + 1;
            deepest.fetch_max(now, Ordering::SeqCst);
            pool.scope(|scope| scope.spawn(async {}));
            depth.fetch_sub(1, Ordering::SeqCst);
            std::hint::black_box(&frame);
        }

        let pool = Arc::new(io_task_pool_builder(1).build());
        let depth = Arc::new(AtomicUsize::new(0));
        let deepest = Arc::new(AtomicUsize::new(0));

        // Hold the only thread until every load is queued, so the nesting does not depend on
        // how fast this thread can spawn.
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let blocker = pool.spawn(async move {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        started_rx.recv().unwrap();

        let tasks: Vec<_> = (0..LOADS)
            .map(|_| {
                let (pool_ref, depth, deepest) = (pool.clone(), depth.clone(), deepest.clone());
                pool.spawn(async move { load(&pool_ref, &depth, &deepest) })
            })
            .collect();
        release_tx.send(()).unwrap();
        bevy::tasks::block_on(blocker);
        for task in tasks {
            bevy::tasks::block_on(task);
        }

        let deepest = deepest.load(Ordering::SeqCst);
        assert!(
            deepest * FRAME_BYTES > 8 * 1024 * 1024,
            "the loads nested only {deepest} deep, too shallow to test the stack"
        );
    }

    /// The canonical fixture's emissive box is a converted material, and this is what catches a
    /// glow that has lost its scale: the same emissive at the magnitude the converter publishes -
    /// the Blackreach caps' `[0.212, 0.992, 1.0]` times their strength of 2.0 - has to read as
    /// invisible, and the same value after the engine's exposure as visible. A check that only
    /// asked for a positive channel passed while every glow of the game rendered black.
    ///
    /// Together with the upper edge this brackets `EMISSIVE_EXPOSURE` on both sides, which is what
    /// makes it worth having: below about 20 the converted glows are still lost in the ambient, and
    /// above about 240 every frame a glow is in clips.
    #[test]
    fn the_canonical_emissive_is_checked_against_the_ambient_it_sits_in() {
        let converted = LinearRgba::new(0.424, 1.984, 2.0, 1.0);
        assert!(
            !emissive_is_visible(converted),
            "a raw glTF emissive is exactly the state this fixture has to fail on"
        );
        assert!(
            emissive_is_visible(crate::render::exposed_emissive(converted)),
            "and the same material after the engine's emissive scale is the state it has to pass"
        );
        assert!(
            !emissive_is_visible(LinearRgba::NONE),
            "a material with no emissive is not a glow"
        );
        assert!(
            !emissive_is_visible(LinearRgba::new(1.0e6, 1.0e6, 1.0e6, 1.0)),
            "and neither is a lamp of its own: the band has an upper edge"
        );
    }

    #[test]
    fn rejects_conflicting_fixture_modes() {
        let mut config = EngineConfig {
            physics_fixture: true,
            ..Default::default()
        };
        assert!(validate_fixture_selection(&config).is_ok());
        config.streaming_fixture = true;
        assert!(validate_fixture_selection(&config).is_err());
    }

    #[test]
    fn screenshot_readiness_requires_resident_cells_only_with_world_streaming() {
        let metrics = StreamingMetrics::default();
        let config = EngineConfig::default();

        assert!(screenshot_assets_ready(&metrics, false, &config));
        assert!(!screenshot_assets_ready(&metrics, true, &config));

        let mut settled_metrics = metrics;
        settled_metrics.resident_cells = 1;
        assert!(screenshot_assets_ready(&settled_metrics, true, &config));

        let mut failed_metrics = settled_metrics.clone();
        failed_metrics.failed_cells = 1;
        assert!(!screenshot_assets_ready(&failed_metrics, true, &config));

        let mut pending_lod = settled_metrics.clone();
        pending_lod.pending_lod_queries = 1;
        assert!(!screenshot_assets_ready(&pending_lod, true, &config));

        let mut failed_lod = settled_metrics;
        failed_lod.failed_lod_chunks = 1;
        assert!(screenshot_assets_ready(&failed_lod, true, &config));
        failed_lod.unrecovered_lod_chunks = 1;
        assert!(!screenshot_assets_ready(&failed_lod, true, &config));
        failed_lod.unrecovered_lod_chunks = 0;
        failed_lod.unrecovered_lod_queries = 1;
        assert!(!screenshot_assets_ready(&failed_lod, true, &config));
    }

    #[test]
    fn v1_world_and_visual_fixture_cameras_have_explicit_hdr_output() {
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<TerrainMaterial>>()
            .init_resource::<Assets<WaterMaterial>>()
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .add_systems(
                Startup,
                (
                    setup_world,
                    setup_material_fixture,
                    setup_terrain_water_fixture,
                    setup_transform_bounds_fixture,
                    setup_renderer_fixture,
                ),
            );
        app.update();
        crate::color_pipeline::assert_scene_camera_output(app.world_mut(), 5);
    }

    /// The sun is what has to carry the cascades: without a configuration of its own it gets
    /// Bevy's, which covers the two metres in front of the camera.
    #[test]
    fn setup_world_spawns_the_sun_with_its_cascades() {
        let config = EngineConfig::default();
        let mut app = App::new();
        app.insert_resource(config.clone())
            .add_systems(Startup, setup_world);
        app.update();

        let world = app.world_mut();
        let mut suns = world.query::<(&DirectionalLight, &CascadeShadowConfig)>();
        let Ok((light, cascades)) = suns.single(world) else {
            panic!("setup_world spawns one shadow-casting directional light");
        };
        assert!(light.shadow_maps_enabled);
        let expected = sun_shadow_cascades(config.stream_radius, SUN_SHADOW_CASCADES, None);
        assert_eq!(cascades.bounds, expected.bounds);
        assert_eq!(cascades.minimum_distance, expected.minimum_distance);
    }

    /// The streaming camera draws the world (layer 0) and the water planes ([`WATER_LAYER`]):
    /// whatever lives on a layer it does not render vanishes from the player's own view.
    #[test]
    fn the_streaming_camera_renders_every_layer_the_world_uses() {
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .add_systems(Startup, setup_world);
        app.update();

        let mut cameras = app
            .world_mut()
            .query_filtered::<&RenderLayers, With<StreamingCamera>>();
        let layers = cameras
            .single(app.world())
            .expect("the streaming camera must be spawned");
        for layer in MAIN_VIEW_LAYERS {
            assert!(
                layers.intersects(&RenderLayers::layer(layer)),
                "the streaming camera must render layer {layer}"
            );
        }
    }

    /// Main's capsule player knows nothing of doors, so no run may switch it on; the portal's own
    /// `PlayerPlugin` walks instead.
    #[test]
    fn the_world_physics_player_is_off_because_it_knows_nothing_of_doors() {
        assert!(EngineConfig::default().interactive_world_physics());
        assert!(!world_physics_player(&EngineConfig::default()));
    }

    #[test]
    fn automatic_flight_reverses_before_leaving_the_representative_world_area() {
        let mut state = AutoFlightState::default();
        let direction = bounded_auto_flight_direction(Vec3::new(0.0, -1.0, -1.0), 1.0, &mut state);
        assert_eq!(direction, Vec3::NEG_Z);

        assert_eq!(
            bounded_auto_flight_direction(Vec3::NEG_Z, AUTO_FLIGHT_HALF_SPAN, &mut state),
            Vec3::Z
        );
        assert_eq!(
            bounded_auto_flight_direction(Vec3::NEG_Z, 2.0, &mut state),
            Vec3::NEG_Z
        );
        assert!(state.offset.abs() <= AUTO_FLIGHT_HALF_SPAN);
    }

    /// The numbers the default run draws the ring with, and the two properties that make the pair
    /// work: the fog is clear where the full-detail grid ends and opaque where the camera can no
    /// longer see terrain, and everything the fog fades is inside the far plane.
    #[test]
    fn the_default_far_plane_and_fog_cover_the_terrain_ring() {
        let config = EngineConfig::default();
        let far = camera_far_plane(&config);
        let cell = crate::world::components::CELL_SIZE;
        assert_eq!(config.stream_radius, 2);
        assert_eq!(config.terrain_radius, crate::config::DEFAULT_TERRAIN_RADIUS);
        assert_eq!(far, cell * (config.terrain_radius + 2) as f32 * 2.0);

        let DistanceFog {
            falloff: FogFalloff::Linear { start, end },
            color,
            ..
        } = exterior_fog(config.stream_radius, config.terrain_radius, None, SKY_COLOR)
        else {
            panic!("an exterior with a ring fogs linearly");
        };
        assert_eq!(
            color, SKY_COLOR,
            "the ring fades into the sky it is drawn against"
        );
        assert_eq!(
            start,
            cell * 3.0,
            "clear up to the far edge of the full-detail grid"
        );
        assert_eq!(
            end,
            cell * (config.terrain_radius + 1) as f32,
            "opaque at the far edge of the ring, where the last ring cell is"
        );
        assert!(
            end < far,
            "the fog is opaque before the far plane clips anything: a cliff of fog is as bad as a \
             cliff of terrain"
        );

        // The whole ring is inside the far plane, corner included: the corner cell's far side is
        // sqrt(2) * (terrain_radius + 1) cells from the camera cell's centre.
        let corner = (2.0_f32).sqrt() * (config.terrain_radius as f32 + 1.0) * cell;
        assert!(
            corner < far,
            "corner {corner} is outside the far plane {far}"
        );
    }

    /// No ring means nothing beyond the full-detail grid to fade, so the fog is off and the far
    /// plane is the one the engine drew the inner grid with before the ring existed.
    #[test]
    fn without_a_ring_the_far_plane_and_the_fog_are_what_they_were() {
        let far = camera_far_plane(&EngineConfig {
            terrain_radius: 2,
            ..EngineConfig::default()
        });
        assert_eq!(far, crate::world::components::CELL_SIZE * 4.0 * 2.0);

        let DistanceFog {
            falloff: FogFalloff::Linear { start, .. },
            ..
        } = exterior_fog(2, 2, None, SKY_COLOR)
        else {
            panic!("the falloff is linear whether the fog is on or off");
        };
        assert!(
            start > far,
            "with no ring the fog starts past the far plane, so nothing is ever fogged"
        );
    }

    /// With the world's LOD data the fog still starts where the full-detail grid ends but reaches
    /// out to the LOD's horizon, so distant terrain beyond the ring is drawn and not fogged out as
    /// sky colour at the ring's edge; without LOD data it is the ring's, as before.
    #[test]
    fn the_exterior_fog_reaches_the_lod_horizon_when_the_world_has_lod() {
        let cell = crate::world::components::CELL_SIZE;
        let linear = |fog: DistanceFog| {
            let DistanceFog {
                falloff: FogFalloff::Linear { start, end },
                ..
            } = fog
            else {
                panic!("the exterior fog is linear");
            };
            (start, end)
        };
        let defaults = EngineConfig::default();
        let horizon = cell * 91.0;

        assert_eq!(
            linear(exterior_fog(
                defaults.stream_radius,
                defaults.terrain_radius,
                None,
                SKY_COLOR
            )),
            (cell * 3.0, cell * (defaults.terrain_radius + 1) as f32),
            "no LOD data: the ring's own fog"
        );
        assert_eq!(
            linear(exterior_fog(
                defaults.stream_radius,
                defaults.terrain_radius,
                Some(horizon),
                SKY_COLOR
            )),
            (cell * 3.0, horizon),
            "LOD data: clear up to the grid's edge, opaque at the LOD's horizon"
        );
        assert_eq!(
            linear(exterior_fog(
                defaults.stream_radius,
                defaults.terrain_radius,
                Some(cell * 2.0),
                SKY_COLOR
            ))
            .1,
            cell * (defaults.terrain_radius + 1) as f32,
            "a horizon nearer than the ring never pulls the fog in over the ring"
        );
        // No ring, but LOD: the horizon is what there is to fade, so it is fogged too; a LOD
        // horizon inside the grid has nothing to fade and the fog stays off.
        assert_eq!(
            linear(exterior_fog(2, 2, Some(horizon), SKY_COLOR)).1,
            horizon
        );
        assert!(linear(exterior_fog(2, 2, Some(cell), SKY_COLOR)).0 > camera_far_plane(&defaults));
    }

    /// The sun's cascades as the demo's look (`current`) asks for them.
    fn today_cascades(config: &EngineConfig) -> CascadeShadowConfig {
        sun_shadow_cascades(config.stream_radius, SUN_SHADOW_CASCADES, None)
    }

    /// The sun's shadows reach every cell the streamer holds in full, whichever way the camera
    /// faces, and the shader's view of them is a usable one: far bounds that increase (its
    /// `get_cascade_index` returns the first bound the fragment is inside, so a bound out of order
    /// hands every fragment beyond it to a cascade that cannot see it) and a near clamp below the
    /// first bound. The two near distances are Bevy's own defaults read in metres; the last is this
    /// engine's full-detail grid, which is the part a run of the demo depends on.
    #[test]
    fn the_sun_shadows_cover_the_full_detail_grid() {
        let config = EngineConfig::default();
        let cascades = today_cascades(&config);
        let cell = crate::world::components::CELL_SIZE;
        assert_eq!(config.stream_radius, 2);
        assert_eq!(cascades.bounds.len(), SUN_SHADOW_CASCADES);

        assert_eq!(
            cascades.minimum_distance,
            0.1 * CREATION_UNITS_PER_METRE,
            "Bevy's 10 cm near clamp, in Creation units"
        );
        assert_eq!(
            cascades.bounds[0],
            10.0 * CREATION_UNITS_PER_METRE,
            "and its 10 m first cascade, which is 700 units here"
        );

        // The camera stands somewhere inside its own cell and the grid is `stream_radius` cells
        // around that cell, so the grid's far corner is `stream_radius + 1` cells away on each of
        // the two axes.
        let axis = cell * (config.stream_radius as f32 + 1.0);
        let corner = axis * std::f32::consts::SQRT_2;
        assert!(
            cascades.bounds[3] > axis,
            "the grid straight ahead is shadowed: {} covers {axis}",
            cascades.bounds[3]
        );
        // `calculate_cascade_bounds` reaches the maximum distance geometrically, so the last bound
        // is the requested distance to within a rounding error rather than to the bit.
        assert!(
            (cascades.bounds[3] - corner).abs() < 1.0,
            "and its far corner is inside the last cascade too: {} against {corner}",
            cascades.bounds[3]
        );

        assert!(
            cascades.bounds.windows(2).all(|pair| pair[0] < pair[1]),
            "the far bounds increase: {:?}",
            cascades.bounds
        );
        assert!(cascades.minimum_distance < cascades.bounds[0]);
        assert!((0.0..1.0).contains(&cascades.overlap_proportion));
    }

    /// The ring is outside the sun's range on purpose - it casts nothing, so covering it would take
    /// texel size from the shadows that can be seen without adding a caster - so a larger
    /// `--terrain-radius` does not move the range, and the coverage above holds at every radius the
    /// command line accepts. A larger `--stream-radius` does move it: the range follows what the
    /// streamer holds in full, which is what the shadows are for.
    #[test]
    fn the_shadow_range_follows_the_full_detail_grid_and_not_the_ring() {
        let cell = crate::world::components::CELL_SIZE;
        let default = today_cascades(&EngineConfig::default()).bounds;

        for radius in [2, 8, 16, crate::config::MAX_TERRAIN_RADIUS] {
            let config =
                EngineConfig::run_from_args(["--terrain-radius".to_owned(), radius.to_string()]);
            assert_eq!(config.terrain_radius, radius);
            assert_eq!(
                today_cascades(&config).bounds,
                default,
                "a {radius} cell ring is the fog's and the far plane's business, not the sun's"
            );
        }

        let wider = today_cascades(&EngineConfig::run_from_args([
            "--stream-radius".to_owned(),
            "4".to_owned(),
        ]));
        assert!(
            wider.bounds[3] > default[3],
            "a wider full-detail grid is more world to shadow"
        );
        assert!((wider.bounds[3] - cell * 5.0 * std::f32::consts::SQRT_2).abs() < 1.0);

        // A stream radius as negative as the command line allows loads nothing, and the distance
        // derived from it would be negative - which `CascadeShadowConfigBuilder::build` rejects by
        // panic. The engine clamps it and starts.
        let nothing = EngineConfig {
            stream_radius: -4,
            ..EngineConfig::default()
        };
        assert!(today_cascades(&nothing).bounds[3] > 0.0);
    }

    /// An interior has no ring and no haze, and the fog it gets cannot reach anything the space
    /// draws - while staying on the camera, so a crossing does not rebuild the pipeline.
    #[test]
    fn the_fog_of_an_interior_cannot_reach_its_geometry() {
        let far = camera_far_plane(&EngineConfig::default());
        let DistanceFog {
            falloff: FogFalloff::Linear { start, end },
            ..
        } = fog_off(UNDERGROUND_COLOR)
        else {
            panic!("the falloff is linear whether the fog is on or off");
        };
        assert!(
            start > far,
            "start {start} must be past the far plane {far}"
        );
        assert!(start < end);
    }

    #[test]
    fn the_upload_budget_resource_carries_the_configured_option() {
        let mut app = App::new();
        app.insert_resource(upload_budget(&EngineConfig::default()));
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            Some(16 * 1024 * 1024)
        );

        let unlimited = EngineConfig {
            max_upload_mib_per_frame: 0,
            ..default()
        };
        app.insert_resource(upload_budget(&unlimited));
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            None
        );
    }

    /// A capture is photographed once its streaming has settled, and nothing in that gate waits
    /// for the render world's upload backlog, so a capture run uploads unthrottled.
    #[test]
    fn capture_runs_upload_without_a_budget() {
        let with = |set: fn(&mut EngineConfig)| {
            let mut config = EngineConfig::default();
            set(&mut config);
            upload_budget(&config).max_bytes
        };
        assert_eq!(
            with(|c| c.portal.shots = Some(PathBuf::from("shots.json"))),
            None
        );
        assert_eq!(
            with(|c| c.portal.demo_tour = Some(PathBuf::from("tour.json"))),
            None
        );
        assert_eq!(
            with(|c| c.portal.portal_bench = Some(PathBuf::from("bench.json"))),
            Some(16 * 1024 * 1024)
        );
    }

    /// An assets directory the engine was given.
    const EXAMPLE_ASSETS: &str = "converted-assets";

    fn asset_set_message(rejection: AssetSetRejection) -> String {
        asset_set_rejection_message(Path::new(EXAMPLE_ASSETS), rejection)
    }

    /// Runs the real gate against a temporary asset set and returns its error.
    fn runtime_asset_error(manifest: &str, report: &str) -> String {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("skyrim_world.db"), []).unwrap();
        std::fs::write(directory.path().join("cell_cache.rkyv"), []).unwrap();
        std::fs::write(
            directory.path().join("conversion-manifest.json"),
            manifest.as_bytes(),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("integration-report.json"),
            report.as_bytes(),
        )
        .unwrap();
        let config = EngineConfig {
            assets_dir: directory.path().to_owned(),
            ..default()
        };
        format!("{:#}", validate_runtime_assets(&config).unwrap_err())
    }

    #[test]
    fn rejects_stale_or_incomplete_runtime_assets() {
        let directory = tempfile::tempdir().unwrap();
        write_runtime_asset_database(&directory.path().join("skyrim_world.db"));
        std::fs::write(directory.path().join("cell_cache.rkyv"), []).unwrap();
        std::fs::write(
            directory.path().join("conversion-manifest.json"),
            br#"{"schema_version":3,"complete":true}"#,
        )
        .unwrap();
        std::fs::write(
            directory.path().join("integration-report.json"),
            format!(
                r#"{{"schema_version":{},"passed":true}}"#,
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ),
        )
        .unwrap();
        let config = EngineConfig {
            assets_dir: directory.path().to_owned(),
            ..default()
        };
        assert!(validate_runtime_assets(&config).is_err());
    }

    #[test]
    fn older_converter_schema_names_the_accepted_range_and_the_reconvert_command() {
        let oldest = shared::MIN_RUNTIME_CONVERTER_SCHEMA_VERSION;
        let newest = converter_schema_version();
        let message =
            asset_set_message(AssetSetRejection::ConverterSchemaOlder { found: oldest - 1 });
        assert!(message.contains("are stale"), "{message}");
        assert!(
            message.contains(&format!("was written by converter schema {}", oldest - 1)),
            "{message}"
        );
        assert!(
            message.contains(&format!(
                "accepts converter schemas {oldest} through {newest}"
            )),
            "{message}"
        );
        assert!(
            message.contains(&format!(
                "cargo run --release -p converter -- <Skyrim Data folder> \"{EXAMPLE_ASSETS}\""
            )),
            "{message}"
        );
        assert!(message.contains("reuses what it can"), "{message}");
    }

    #[test]
    fn newer_converter_schema_names_the_accepted_range_and_the_engine_rebuild() {
        let oldest = shared::MIN_RUNTIME_CONVERTER_SCHEMA_VERSION;
        let newest = converter_schema_version();
        let message =
            asset_set_message(AssetSetRejection::ConverterSchemaNewer { found: newest + 1 });
        assert!(message.contains("newer than this engine"), "{message}");
        assert!(
            message.contains(&format!("was written by converter schema {}", newest + 1)),
            "{message}"
        );
        assert!(
            message.contains(&format!(
                "understands only converter schemas {oldest} through {newest}"
            )),
            "{message}"
        );
        assert!(
            message.contains("cargo build --release -p engine"),
            "{message}"
        );
    }

    #[test]
    fn incomplete_conversion_names_the_rerun_and_the_incomplete_assets_option() {
        let engine = converter_schema_version();
        let message = asset_set_message(AssetSetRejection::IncompleteConversion { schema: engine });
        assert!(message.contains("did not finish"), "{message}");
        assert!(message.contains("complete=false"), "{message}");
        assert!(
            message.contains(&format!("at converter schema {engine}")),
            "{message}"
        );
        assert!(
            message.contains(&format!(
                "cargo run --release -p converter -- <Skyrim Data folder> \"{EXAMPLE_ASSETS}\""
            )),
            "{message}"
        );
        assert!(message.contains("--allow-incomplete-assets"), "{message}");
    }

    #[test]
    fn incomplete_conversion_names_the_schema_the_manifest_reports() {
        // On the portal branch the oldest accepted converter schema is the current one: the
        // portal runtime loads only its own output.
        let oldest = shared::MIN_RUNTIME_CONVERTER_SCHEMA_VERSION;
        let message = runtime_asset_error(
            &format!(r#"{{"schema_version":{oldest},"complete":false}}"#),
            &format!(
                r#"{{"schema_version":{},"passed":true}}"#,
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ),
        );
        assert!(
            message.contains(&format!("complete=false at converter schema {oldest},")),
            "{message}"
        );
    }

    #[test]
    fn world_database_schema_mismatch_names_the_accepted_range_and_the_reconvert_command() {
        let oldest = shared::MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION;
        let newest = shared::WORLD_DATABASE_SCHEMA_VERSION;
        let message =
            asset_set_message(AssetSetRejection::WorldDatabaseSchema { found: oldest - 1 });
        assert!(message.contains("world database schema"), "{message}");
        assert!(
            message.contains(&format!("world database schema {}", oldest - 1)),
            "{message}"
        );
        assert!(
            message.contains(&format!(
                "accepts world database schemas {oldest} through {newest}"
            )),
            "{message}"
        );
        assert!(
            message.contains(&format!(
                "cargo run --release -p converter -- <Skyrim Data folder> \"{EXAMPLE_ASSETS}\""
            )),
            "{message}"
        );
    }

    #[test]
    fn failed_integration_report_points_at_the_report_file() {
        let message = asset_set_message(AssetSetRejection::IntegrationReportFailed);
        assert!(
            message.contains("integration report did not pass"),
            "{message}"
        );
        assert!(message.contains("passed=false"), "{message}");
        assert!(message.contains("integration-report.json"), "{message}");
        assert!(message.contains(EXAMPLE_ASSETS), "{message}");
        assert!(
            message.contains(&format!(
                "cargo run --release -p converter -- <Skyrim Data folder> \"{EXAMPLE_ASSETS}\""
            )),
            "{message}"
        );
    }

    #[test]
    fn stale_incomplete_and_failed_runtime_assets_report_distinct_reasons() {
        let engine = converter_schema_version();
        let world = shared::WORLD_DATABASE_SCHEMA_VERSION;
        let passing_report = format!(r#"{{"schema_version":{world},"passed":true}}"#);
        let stale =
            runtime_asset_error(r#"{"schema_version":14,"complete":true}"#, &passing_report);
        let incomplete = runtime_asset_error(
            &format!(r#"{{"schema_version":{engine},"complete":false}}"#),
            &passing_report,
        );
        let failed_report = runtime_asset_error(
            &format!(r#"{{"schema_version":{engine},"complete":true}}"#),
            &format!(r#"{{"schema_version":{world},"passed":false}}"#),
        );
        let schema_report = runtime_asset_error(
            &format!(r#"{{"schema_version":{engine},"complete":true}}"#),
            &format!(
                r#"{{"schema_version":{},"passed":true}}"#,
                shared::MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION - 1
            ),
        );

        assert!(stale.contains("converted assets are stale"), "{stale}");
        assert!(
            incomplete.contains("--allow-incomplete-assets"),
            "{incomplete}"
        );
        assert!(failed_report.contains("passed=false"), "{failed_report}");
        assert!(
            schema_report.contains("world database schema"),
            "{schema_report}"
        );
        for (left, right) in [
            (&stale, &incomplete),
            (&stale, &failed_report),
            (&stale, &schema_report),
            (&incomplete, &failed_report),
            (&incomplete, &schema_report),
            (&failed_report, &schema_report),
        ] {
            assert_ne!(left, right);
        }
    }

    /// Current, complete assets load, and an integration report newer than the engine is rejected.
    #[test]
    fn accepts_current_complete_runtime_assets() {
        let directory = tempfile::tempdir().unwrap();
        write_runtime_asset_database(&directory.path().join("skyrim_world.db"));
        std::fs::write(directory.path().join("cell_cache.rkyv"), []).unwrap();
        std::fs::write(
            directory.path().join("conversion-manifest.json"),
            format!(
                r#"{{"schema_version":{},"complete":true}}"#,
                converter_schema_version()
            ),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("integration-report.json"),
            format!(
                r#"{{"schema_version":{},"passed":true}}"#,
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ),
        )
        .unwrap();
        let config = EngineConfig {
            assets_dir: directory.path().to_owned(),
            ..default()
        };
        validate_runtime_assets(&config).unwrap();
        std::fs::write(
            directory.path().join("integration-report.json"),
            format!(
                r#"{{"schema_version":{},"passed":true}}"#,
                shared::WORLD_DATABASE_SCHEMA_VERSION + 1
            ),
        )
        .unwrap();
        assert!(
            validate_runtime_assets(&config)
                .unwrap_err()
                .to_string()
                .contains(&format!(
                    "schema {} is unsupported",
                    shared::WORLD_DATABASE_SCHEMA_VERSION + 1
                ))
        );
    }

    #[test]
    fn accepts_a_passing_integration_report() {
        let directory = tempfile::tempdir().unwrap();
        for required in ["skyrim_world.db", "cell_cache.rkyv"] {
            std::fs::write(directory.path().join(required), []).unwrap();
        }
        // The current schema with its (empty) LOD tables, as the converter writes it.
        write_runtime_asset_database(&directory.path().join("skyrim_world.db"));
        std::fs::write(
            directory.path().join("conversion-manifest.json"),
            format!(
                r#"{{"schema_version":{},"complete":true}}"#,
                converter_schema_version()
            ),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("integration-report.json"),
            format!(
                r#"{{"schema_version":{},"passed":true}}"#,
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ),
        )
        .unwrap();
        let config = EngineConfig {
            assets_dir: directory.path().to_owned(),
            ..default()
        };
        validate_runtime_assets(&config).unwrap();
    }

    #[test]
    fn rejects_truncated_manifest_and_integration_report() {
        for truncated_file in ["conversion-manifest.json", "integration-report.json"] {
            let directory = tempfile::tempdir().unwrap();
            write_runtime_asset_database(&directory.path().join("skyrim_world.db"));
            std::fs::write(directory.path().join("cell_cache.rkyv"), []).unwrap();
            std::fs::write(
                directory.path().join("conversion-manifest.json"),
                format!(
                    r#"{{"schema_version":{},"complete":true}}"#,
                    converter_schema_version()
                ),
            )
            .unwrap();
            std::fs::write(
                directory.path().join("integration-report.json"),
                format!(
                    r#"{{"schema_version":{},"passed":true}}"#,
                    shared::WORLD_DATABASE_SCHEMA_VERSION
                ),
            )
            .unwrap();
            std::fs::write(directory.path().join(truncated_file), b"{").unwrap();
            let config = EngineConfig {
                assets_dir: directory.path().to_owned(),
                ..default()
            };
            assert!(
                validate_runtime_assets(&config).is_err(),
                "{truncated_file}"
            );
        }
    }

    #[test]
    fn ground_height_query_reads_the_right_cell() {
        let database_directory = tempfile::tempdir().unwrap();
        let database_path = database_directory.path().join("skyrim_world.db");
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER);
                 CREATE TABLE land(cell_id INTEGER PRIMARY KEY);
                 INSERT INTO cells(id,worldspace_id,grid_x,grid_y) VALUES(42,7,0,0);
                 INSERT INTO land(cell_id) VALUES(42);",
            )
            .unwrap();
        drop(connection);

        let cache_directory = tempfile::tempdir().unwrap();
        let cache_path = cache_directory.path().join("cell_cache.rkyv");
        let source = shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: vec![shared::CachedLand {
                cell_id: 42,
                width: 2,
                height: 2,
                heights: vec![1.0, 2.0, 3.0, 4.0],
                normals: vec![0; 12],
                vertex_colors: vec![255; 12],
                layers: vec![],
                water_height: None,
                water_type_form_id: None,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&source).unwrap();
        std::fs::write(&cache_path, bytes).unwrap();
        let cache = CellCache::open(&cache_path).unwrap();

        let config = EngineConfig {
            worldspace_id: 7,
            start_grid: (0, 0),
            ..default()
        };

        let ground_height = initial_camera_ground_height(&config, &database_path, &cache).unwrap();
        assert_eq!(ground_height, 4.0);
    }

    #[test]
    fn ground_height_query_does_not_create_a_missing_database() {
        // The old code opened with `Connection::open`, which is
        // read-write-and-create-if-missing: querying a database that does not
        // exist yet silently created an empty one as a side effect. An
        // explicit read-only open must instead fail without creating
        // anything.
        let database_directory = tempfile::tempdir().unwrap();
        let database_path = database_directory.path().join("skyrim_world.db");
        assert!(!database_path.exists());

        let cache_directory = tempfile::tempdir().unwrap();
        let cache_path = cache_directory.path().join("cell_cache.rkyv");
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: vec![],
        })
        .unwrap();
        std::fs::write(&cache_path, bytes).unwrap();
        let cache = CellCache::open(&cache_path).unwrap();

        let config = EngineConfig {
            worldspace_id: 7,
            start_grid: (0, 0),
            ..default()
        };

        assert!(initial_camera_ground_height(&config, &database_path, &cache).is_err());
        assert!(
            !database_path.exists(),
            "a read-only ground-height query must not create a missing database file"
        );
    }

    /// The shader tiles every terrain layer `8` times across a cell, so the fixture's layer images
    /// need the sampler a streamed layer is loaded with. Bevy's default clamps to the edge, which
    /// stretches the outermost texels over the rest of the tiles.
    #[test]
    fn terrain_water_fixture_layers_are_sampled_with_a_repeating_sampler() {
        let expected = bevy::image::ImageSampler::Descriptor(terrain_layer_sampler());
        assert_eq!(terrain_fixture_image([82, 116, 58, 255]).sampler, expected);
    }
}

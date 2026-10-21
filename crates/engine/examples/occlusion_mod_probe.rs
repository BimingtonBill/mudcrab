//! What Bevy's GPU occlusion culling would wrongly remove from a modded scene.
//!
//! One scene, drawn twice in the same frames by two cameras with separate targets: camera `on`
//! carries `OcclusionCulling`, camera `off` does not. Everything else is shared, so any pixel that
//! differs between the two images was removed by occlusion culling alone.
//!
//! A big wall stands in the middle. The cases stand beside it, in view:
//!
//! * `control`: an ordinary cube. Drawn by both cameras.
//! * `leaf_gap`: a cube seen through the transparent half of an alpha-tested leaf card. The card's
//!   depth prepass must discard the same texels the main pass does, or the card would occlude what
//!   the gap shows. Drawn by both cameras.
//! * `leaf_card`: the opaque half of that card. Drawn by both cameras.
//! * `wrong_aabb`: a cube whose `Aabb` component is not where its vertices are (what a vertex
//!   shader that moves vertices far from the rest pose, a morph target or a bad conversion leaves
//!   behind): the stored box sits behind the wall, the drawn cube beside it. Occlusion culling
//!   tests the box, so the camera `on` loses the cube; the camera `off` keeps it.
//! * `no_aabb`: a cube with `NoAutoAabb` and no `Aabb`. Bevy gives it infinite bounds, so it is
//!   never culled. Drawn by both cameras.
//!
//! `wrong_aabb` also proves the culling is live on this adapter: if the probe says `culling_live:
//! false`, the cases above say nothing about occlusion. Usage: `--output DIR`.
use bevy::{
    camera::{RenderTarget, primitives::Aabb, visibility::NoAutoAabb},
    core_pipeline::{prepass::DepthPrepass, tonemapping::Tonemapping},
    prelude::*,
    render::{
        occlusion_culling::OcclusionCulling,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::{ExitCondition, WindowPlugin},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const CAMERA_Z: f32 = 10.0;
/// Vertical field of view of Bevy's default perspective projection (`PI / 4`).
const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;
const FRAMES_BEFORE_CAPTURE: u32 = 120;

#[derive(Resource)]
struct Probe {
    output: PathBuf,
    on: Handle<Image>,
    off: Handle<Image>,
    frames: u32,
    start: Instant,
    captured: Vec<(&'static str, Vec<u8>)>,
}

fn main() {
    let mut output = PathBuf::from("occlusion-mod-probe");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = args.next().expect("output directory").into(),
            _ => panic!("unknown argument {arg}"),
        }
    }
    std::fs::create_dir_all(&output).unwrap();
    let mut app = App::new();
    // The engine's cull counter reads what the GPU culling kept; it is the second witness that
    // occlusion culling is doing something in this scene.
    let counter_config = engine::config::EngineConfig {
        cull_counter: true,
        ..default()
    };
    app.add_plugins(
        DefaultPlugins
            .set(bevy::render::RenderPlugin {
                debug_flags: engine::cull_counter::CullCounterPlugin::render_debug_flags(
                    &counter_config,
                ),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::audio::AudioPlugin>(),
    )
    .add_plugins((
        bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_millis(16)),
        engine::cull_counter::CullCounterPlugin::from_config(&counter_config),
    ));
    app.sub_app_mut(bevy::render::RenderApp)
        .add_systems(bevy::render::Render, report_culling_mode);
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    let target = |images: &mut Assets<Image>| {
        images.add(Image::new_target_texture(
            WIDTH,
            HEIGHT,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ))
    };
    let (on, off) = (target(&mut images), target(&mut images));
    let result = app
        .insert_resource(Probe {
            output,
            on,
            off,
            frames: 0,
            start: Instant::now(),
            captured: Vec::new(),
        })
        .add_systems(Startup, setup)
        .add_systems(Update, capture)
        .run();
    if !result.is_success() {
        std::process::exit(1);
    }
}

/// Prints, once, whether Bevy decided on GPU occlusion culling for this adapter and how many views
/// have a depth pyramid.
fn report_culling_mode(
    support: Option<Res<bevy::render::batching::gpu_preprocessing::GpuPreprocessingSupport>>,
    pyramids: Query<&OcclusionCulling>,
    mut printed: Local<u32>,
) {
    *printed += 1;
    if *printed == FRAMES_BEFORE_CAPTURE {
        println!(
            "gpu preprocessing culling supported: {:?}, occlusion-culling views in the render world: {}",
            support.map(|support| {
                support.max_supported_mode
                    == bevy::render::batching::gpu_preprocessing::GpuPreprocessingMode::Culling
            }),
            pyramids.iter().count()
        );
    }
}

/// An unlit, opaque material of one flat colour.
fn flat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        unlit: true,
        ..default()
    })
}

/// Where a world point lands in the 800x600 image for a camera at (0, 0, [`CAMERA_Z`]) looking down
/// -Z with Bevy's default projection.
fn pixel(point: Vec3) -> (u32, u32) {
    let depth = CAMERA_Z - point.z;
    let half_height = depth * (FOV_Y / 2.0).tan();
    let half_width = half_height * WIDTH as f32 / HEIGHT as f32;
    let x = (point.x / half_width * 0.5 + 0.5) * WIDTH as f32;
    let y = (0.5 - point.y / half_height * 0.5) * HEIGHT as f32;
    (x as u32, y as u32)
}

const CUBE: f32 = 0.8;
const CONTROL: Vec3 = Vec3::new(-3.6, -1.5, 0.0);
const WRONG_AABB: Vec3 = Vec3::new(3.6, -1.5, 0.0);
const NO_AABB: Vec3 = Vec3::new(3.6, 1.5, 0.0);
const LEAF_CUBE: Vec3 = Vec3::new(-3.6, 1.5, 0.0);
/// The card sits nearer the camera, on the line of sight from the camera to [`LEAF_CUBE`], so the
/// cube is centred behind it.
const LEAF_CARD_Z: f32 = 3.0;

fn setup(
    mut commands: Commands,
    probe: Res<Probe>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let cube = meshes.add(Cuboid::new(CUBE, CUBE, CUBE));
    let wall = meshes.add(Cuboid::new(2.0, 8.0, 0.2));
    commands.spawn((
        Mesh3d(wall),
        MeshMaterial3d(flat(&mut materials, Color::srgb(0.3, 0.3, 0.35))),
        Transform::from_xyz(0.0, 0.0, 5.0),
    ));
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(flat(&mut materials, Color::srgb(1.0, 0.1, 0.1))),
        Transform::from_translation(CONTROL),
    ));
    // The wrong box: the cube is drawn at `WRONG_AABB`, its stored box is centred at the world
    // origin, behind the wall, so the box (not the cube) is what the culling tests.
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(flat(&mut materials, Color::srgb(0.1, 0.9, 0.1))),
        Transform::from_translation(WRONG_AABB),
        Aabb::from_min_max(
            Vec3::splat(-CUBE / 2.0) - WRONG_AABB,
            Vec3::splat(CUBE / 2.0) - WRONG_AABB,
        ),
        // Without this Bevy's `calculate_bounds` recomputes the box from the mesh on the first
        // frame (it does for every `Changed<Mesh3d>`) and the wrong box is gone.
        NoAutoAabb,
    ));
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(flat(&mut materials, Color::srgb(0.1, 0.4, 1.0))),
        Transform::from_translation(NO_AABB),
        NoAutoAabb,
    ));
    // The leaf: a card whose left texel is opaque green and right texel transparent, tested with
    // `AlphaMode::Mask`. The cube behind it is centred on the card's seam.
    let mut leaf_texture = Image::new(
        Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![30, 200, 40, 255, 30, 200, 40, 0],
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    leaf_texture.sampler = bevy::image::ImageSampler::nearest();
    let leaf_texture = images.add(leaf_texture);
    let scale = (CAMERA_Z - LEAF_CARD_Z) / (CAMERA_Z - LEAF_CUBE.z);
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(1.6, 1.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(leaf_texture),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(LEAF_CUBE.x * scale, LEAF_CUBE.y * scale, LEAF_CARD_Z),
    ));
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(flat(&mut materials, Color::srgb(1.0, 0.8, 0.0))),
        Transform::from_translation(LEAF_CUBE),
    ));
    // A cube with a correct box that stands behind the wall: nothing to see, but the counter
    // should count it as submitted and not drawn on the camera with occlusion culling.
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(flat(&mut materials, Color::srgb(1.0, 1.0, 1.0))),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    for (target, occlusion) in [(&probe.on, true), (&probe.off, false)] {
        let mut camera = commands.spawn((
            Camera3d::default(),
            RenderTarget::Image(target.clone().into()),
            Tonemapping::None,
            Msaa::Off,
            DepthPrepass,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, CAMERA_Z),
        ));
        if occlusion {
            camera.insert(OcclusionCulling);
        }
    }
}

fn capture(mut commands: Commands, mut probe: ResMut<Probe>, mut exit: MessageWriter<AppExit>) {
    if probe.start.elapsed() > Duration::from_secs(90) {
        exit.write(AppExit::error());
        return;
    }
    probe.frames += 1;
    if probe.frames == FRAMES_BEFORE_CAPTURE {
        commands.spawn(Screenshot::image(probe.on.clone())).observe(
            |event: On<ScreenshotCaptured>,
             mut probe: ResMut<Probe>,
             adapter: Res<bevy::render::renderer::RenderAdapterInfo>,
             counter: Res<engine::cull_counter::CullCounterBridge>,
             mut exit: MessageWriter<AppExit>| {
                keep(&mut probe, "on", &event);
                evaluate(&probe, &adapter.name, &counter, &mut exit);
            },
        );
        commands
            .spawn(Screenshot::image(probe.off.clone()))
            .observe(
                |event: On<ScreenshotCaptured>,
                 mut probe: ResMut<Probe>,
                 adapter: Res<bevy::render::renderer::RenderAdapterInfo>,
                 counter: Res<engine::cull_counter::CullCounterBridge>,
                 mut exit: MessageWriter<AppExit>| {
                    keep(&mut probe, "off", &event);
                    evaluate(&probe, &adapter.name, &counter, &mut exit);
                },
            );
    }
}

/// Saves the screenshot as `<label>.png` and keeps its RGB bytes for the pixel samples.
fn keep(probe: &mut Probe, label: &'static str, event: &ScreenshotCaptured) {
    let image = event.image.clone().try_into_dynamic().unwrap().to_rgb8();
    image
        .save(probe.output.join(format!("{label}.png")))
        .unwrap();
    probe.captured.push((label, image.into_raw()));
}

/// The RGB of pixel (x, y) in a captured image.
fn rgb(bytes: &[u8], x: u32, y: u32) -> [u8; 3] {
    let at = ((y * WIDTH + x) * 3) as usize;
    [bytes[at], bytes[at + 1], bytes[at + 2]]
}

fn evaluate(
    probe: &Probe,
    adapter: &str,
    counter: &engine::cull_counter::CullCounterBridge,
    exit: &mut MessageWriter<AppExit>,
) {
    // Both observers call this; only the one that finds both images does the work.
    let find = |name: &str| {
        probe
            .captured
            .iter()
            .find(|(label, _)| *label == name)
            .map(|(_, bytes)| bytes.as_slice())
    };
    let (Some(on), Some(off)) = (find("on"), find("off")) else {
        return;
    };
    // The card is centred on the cube's line of sight; the gap and the opaque half are 0.2 units
    // either side of that line at the card's depth.
    let gap_slope = 0.2 / (CAMERA_Z - LEAF_CARD_Z);
    let along = |slope: f32, point: Vec3| point + Vec3::new(slope * (CAMERA_Z - point.z), 0.0, 0.0);
    let cases = [
        ("control", pixel(CONTROL)),
        ("no_aabb", pixel(NO_AABB)),
        ("wrong_aabb", pixel(WRONG_AABB)),
        // On the cube's line of sight, right of its centre: behind the card's transparent half.
        ("leaf_gap", pixel(along(gap_slope, LEAF_CUBE))),
        // Left of its centre: behind the card's opaque half, so the card's colour shows.
        ("leaf_card", pixel(along(-gap_slope, LEAF_CUBE))),
    ];
    let mut results = Vec::new();
    for (name, (x, y)) in cases {
        let (a, b) = (rgb(on, x, y), rgb(off, x, y));
        results.push(serde_json::json!({
            "case": name, "pixel": [x, y], "occlusion_on": a, "occlusion_off": b,
            "removed_by_occlusion_culling": b.iter().any(|v| *v > 20) && a.iter().all(|v| *v <= 20),
            "differs": a != b,
        }));
    }
    let removed = |name: &str| {
        results
            .iter()
            .any(|r| r["case"] == name && r["removed_by_occlusion_culling"] == true)
    };
    let differs = |name: &str| {
        results
            .iter()
            .any(|r| r["case"] == name && r["differs"] == true)
    };
    let culling_live = removed("wrong_aabb");
    let wrongly_removed: Vec<_> = ["control", "no_aabb", "leaf_gap", "leaf_card"]
        .into_iter()
        .filter(|name| differs(name))
        .collect();
    let passed = culling_live && wrongly_removed.is_empty();
    let report = serde_json::json!({
        "kind": "occlusion-mod-probe", "adapter": adapter,
        "culling_live": culling_live, "wrongly_removed": wrongly_removed,
        "cases": results, "passed": passed,
        "cull_counter": counter.snapshot(),
    });
    std::fs::write(
        probe.output.join("probe.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
    exit.write(if passed {
        AppExit::Success
    } else {
        AppExit::error()
    });
}

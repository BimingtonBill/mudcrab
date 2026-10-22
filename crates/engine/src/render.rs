use crate::{
    color_pipeline::{DEFAULT_SCENE_EV100, SceneColorPipeline},
    config::WaterReflectionPreset,
    lights::EMISSIVE_EXPOSURE,
    profiling::ProfilingState,
    view_policy::{
        SecondaryViewTarget, ViewDemand, ViewPolicy, ViewPolicySystems, ViewRequest, ViewShadows,
    },
    world::{
        cache::{TerrainLayerSnapshot, TerrainSnapshot},
        database::AssetCatalog,
    },
};
use bevy::{
    app::{HierarchyPropagatePlugin, PropagateOver, PropagateSet},
    asset::{LoadContext, embedded_asset},
    camera::{
        Exposure, RenderTarget,
        primitives::{Aabb, Frustum},
        visibility::{RenderLayers, VisibilitySystems},
    },
    core_pipeline::{mip_generation::experimental::depth::ViewDepthPyramid, prepass::DepthPrepass},
    gltf::{
        GltfMaterial,
        extensions::{ErasedGltfExtensionHandler, GltfExtensionHandler, GltfExtensionHandlers},
    },
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    light::{Cascades, SimulationLightSystems},
    pbr::{
        ExtendedMaterial, ExtractedDirectionalLight, LightEntity, MaterialExtension, ShadowView,
        prepare_lights,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        batching::gpu_preprocessing::{
            GpuPreprocessingMode, GpuPreprocessingSupport, IndirectParametersBuffers,
        },
        occlusion_culling::OcclusionCulling,
        render_resource::{AsBindGroup, ShaderType},
    },
    shader::ShaderRef,
    transform::helper::TransformHelper,
};
use serde::Serialize;
use shared::LAND_TEXTURE_REPEATS_PER_CELL;
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;
/// The snow material of a `MATO`-covered static. Its type, its shader and its formula live in
/// [`crate::snow`], which is also where the tables behind it are read; this alias keeps the three
/// materials of the renderer side by side.
pub type SnowMaterial = crate::snow::SnowMaterial;
use crate::effect_palette::{EffectPalette, EffectPaletteMaterial, EffectPaletteRegistry};

/// How far the procedural waves tilt the water's normal. Skyrim's water is close to flat at a
/// distance; a stronger tilt striped lakes with bright and dark bands.
pub(crate) const WAVE_STRENGTH: f32 = 0.05;

/// Skyrim's DefaultWater after Update.esm: Fresnel Amount 0.10 and a Reflectivity Amount of 0.8
/// (WATR `DNAM`). Used when a water has no decoded colours yet, e.g. a database converted before
/// `crates/converter` started reading them.
pub const DEFAULT_WATER_FRESNEL: f32 = 0.10;
pub const DEFAULT_WATER_REFLECTIVITY: f32 = 0.8;
/// DefaultWater's Reflection Color after Update.esm (`#778C9D`): what the water reflects where the
/// reflection image holds nothing, and what a water with no decoded colours falls back to.
pub const DEFAULT_WATER_REFLECTION_COLOR: [u8; 3] = [0x77, 0x8C, 0x9D];

fn water_reflection_color(rgb: [u8; 3]) -> Vec4 {
    let linear = Color::srgb_u8(rgb[0], rgb[1], rgb[2]).to_linear();
    Vec4::new(linear.red, linear.green, linear.blue, 0.0)
}

pub struct VercidiumRendererPlugin;

impl Plugin for VercidiumRendererPlugin {
    fn build(&self, app: &mut App) {
        add_placed_object_layer_propagation(app);
        embedded_asset!(app, "shaders/terrain.wgsl");
        embedded_asset!(app, "shaders/water.wgsl");
        embedded_asset!(app, "shaders/effect_palette.wgsl");
        // The policy owns every secondary camera's activity, and this plugin is what spawns the
        // first of them, so the two are registered together: without it the reflection camera would
        // be spawned with a demand and never switched on. The shared render-schedule skip
        // (`crate::render_skip`) goes with them: it is the render-world half of a cadence skip,
        // for the policy's views and for the portal's camera alike, so a run that renders at all
        // has it.
        app.add_plugins((
            crate::view_policy::SecondaryViewPolicyPlugin,
            crate::shadow_route::ShadowRoutePlugin,
            crate::render_skip::RenderSkipPlugin,
            // `--water-reflection ssr`: the screen-space reflection of the water, a module that
            // knows nothing of this branch (see [`portal_water_ssr_hook`] for what joins it to
            // the portal's views). Under `planar` (the default) it registers nothing at all.
            crate::water_ssr::WaterSsrPlugin,
        ));
        // The one place this branch's planar reflection and the screen-space one meet.
        let planar_replaced = portal_water_ssr_hook(app);
        install_material_animation(app);
        app.add_plugins((
            crate::light_falloff::SkyrimLightFalloffPlugin,
            crate::billboard::BillboardPlugin,
            crate::shadow_layers::ShadowViewLayersPlugin,
            MaterialPlugin::<TerrainMaterial>::default(),
            MaterialPlugin::<WaterMaterial>::default(),
            crate::snow::SnowMaterialPlugin,
            MaterialPlugin::<EffectPaletteMaterial>::default(),
            crate::prepass_vertex_alpha::PrepassVertexAlphaPlugin,
        ))
        .init_resource::<RendererMetrics>()
        .add_systems(Startup, setup_water_reflection)
        .add_systems(
            Update,
            (sync_renderer_metrics, apply_water_settings).after(crate::settings_ui::SettingsUiSet),
        )
        // The reflection is prepared in `PostUpdate` before transform propagation, not in
        // `Update`: a camera teleported this frame must be mirrored by *this* frame's pose, and
        // the pose written just before propagation is the one the frame renders with.
        .add_systems(
            PostUpdate,
            prepare_water_reflection
                .in_set(ViewPolicySystems::Prepare)
                .run_if(move || !planar_replaced),
        );

        // `--reflection-cascades N` installs a limit only when explicitly requested.
        // The default, `all`, and 4 keep every cascade and leave the limiting system out.
        add_configured_water_reflection_cascade_limit(app);

        let bridge = RendererProofBridge::default();
        app.insert_resource(bridge.clone());
        register_skyrim_material_handler(app);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(bridge)
                .add_systems(
                    Render,
                    sample_renderer_path.after(RenderSystems::PrepareResourcesCollectPhaseBuffers),
                )
                .add_systems(
                    Render,
                    give_shadow_views_their_light_layers
                        .in_set(RenderSystems::CreateViews)
                        .after(prepare_lights),
                );
        }
    }
}

/// A shadow view Bevy created without render layers.
type UnlayeredShadowView = (With<ShadowView>, Without<RenderLayers>);

/// Gives each directional shadow cascade the render layers of the light that owns it.
///
/// Bevy 0.19 builds a cascade's shadow view without a `RenderLayers` component, and
/// `queue_shadows` tests every caster's layers against that view's, defaulting to layer 0 when it
/// has none. The CPU side (`check_dir_light_mesh_visibility`) does use the light's layers, so a
/// caster on [`PLACED_OBJECT_LAYER`] is collected for the cascade and then dropped before it is
/// queued: none of the placed objects - buildings, trees, rocks - would cast a shadow. Copying the
/// light's layers onto its views makes the queue test agree with the collection test.
fn give_shadow_views_their_light_layers(
    mut commands: Commands,
    views: Query<(Entity, &LightEntity), UnlayeredShadowView>,
    lights: Query<&ExtractedDirectionalLight>,
) {
    for (view, owner) in &views {
        let LightEntity::Directional { light_entity, .. } = owner else {
            continue;
        };
        if let Ok(light) = lights.get(*light_entity) {
            commands.entity(view).insert(light.render_layers.clone());
        }
    }
}

#[derive(Resource, Debug, Clone, Default, Serialize)]
pub struct RendererMetrics {
    pub gpu_preprocessing_active: bool,
    pub gpu_culling_active: bool,
    pub indirect_drawing_active: bool,
    pub occlusion_culling_views: u64,
    pub hzb_views: u64,
    pub indirect_phase_buffers: u64,
    pub indirect_batch_sets: u64,
    pub proof_frames: u64,
    pub renderer_fixture_validated: bool,
    pub renderer_validation_failures: u64,
    /// What the GPU culling did, per render phase: instances submitted, drawn and rejected. Empty
    /// unless the run asked for `--cull-counter` ([`crate::cull_counter`]).
    pub cull: crate::cull_counter::CullTotals,
}

impl RendererMetrics {
    pub fn final_path_active(&self) -> bool {
        self.gpu_preprocessing_active
            && self.gpu_culling_active
            && self.indirect_drawing_active
            && self.occlusion_culling_views > 0
            && self.hzb_views > 0
            && self.indirect_phase_buffers > 0
            && self.indirect_batch_sets > 0
            && self.proof_frames > 0
            && self.renderer_validation_failures == 0
    }
}

#[derive(Default)]
struct RendererProofState {
    gpu_preprocessing: AtomicBool,
    gpu_culling: AtomicBool,
    indirect_drawing: AtomicBool,
    occlusion_views: AtomicU64,
    hzb_views: AtomicU64,
    indirect_phases: AtomicU64,
    indirect_batch_sets: AtomicU64,
    frames: AtomicU64,
}

#[derive(Resource, Clone, Default)]
struct RendererProofBridge(Arc<RendererProofState>);

fn sample_renderer_path(
    bridge: Res<RendererProofBridge>,
    support: Option<Res<GpuPreprocessingSupport>>,
    indirect: Option<Res<IndirectParametersBuffers>>,
    views: Query<Option<&ViewDepthPyramid>, With<OcclusionCulling>>,
) {
    let Some(support) = support else { return };
    let preprocessing = support.is_available();
    let culling = support.max_supported_mode == GpuPreprocessingMode::Culling;
    let (phase_count, batch_sets, indirect_active) =
        indirect.as_deref().map_or((0, 0, false), |buffers| {
            let phases = buffers.len() as u64;
            let batch_sets = buffers
                .values()
                .map(|phase| {
                    phase.batch_set_count(true) as u64 + phase.batch_set_count(false) as u64
                })
                .sum::<u64>();
            let active = buffers.values().any(|phase| {
                phase.indexed.data_buffer().is_some() || phase.non_indexed.data_buffer().is_some()
            });
            (phases, batch_sets, active)
        });
    let occlusion_views = views.iter().count() as u64;
    let hzb_views = views.iter().filter(|pyramid| pyramid.is_some()).count() as u64;
    bridge
        .0
        .gpu_preprocessing
        .fetch_or(preprocessing, Ordering::Relaxed);
    bridge.0.gpu_culling.fetch_or(culling, Ordering::Relaxed);
    bridge
        .0
        .indirect_drawing
        .fetch_or(indirect_active, Ordering::Relaxed);
    bridge
        .0
        .occlusion_views
        .fetch_max(occlusion_views, Ordering::Relaxed);
    bridge.0.hzb_views.fetch_max(hzb_views, Ordering::Relaxed);
    bridge
        .0
        .indirect_phases
        .fetch_max(phase_count, Ordering::Relaxed);
    bridge
        .0
        .indirect_batch_sets
        .fetch_max(batch_sets, Ordering::Relaxed);
    bridge.0.frames.fetch_add(1, Ordering::Relaxed);
}

fn sync_renderer_metrics(
    bridge: Res<RendererProofBridge>,
    cull: Option<Res<crate::cull_counter::CullCounterBridge>>,
    mut metrics: ResMut<RendererMetrics>,
) {
    if let Some(cull) = cull {
        metrics.cull = cull.snapshot();
    }
    metrics.gpu_preprocessing_active = bridge.0.gpu_preprocessing.load(Ordering::Relaxed);
    metrics.gpu_culling_active = bridge.0.gpu_culling.load(Ordering::Relaxed);
    metrics.indirect_drawing_active = bridge.0.indirect_drawing.load(Ordering::Relaxed);
    metrics.occlusion_culling_views = bridge.0.occlusion_views.load(Ordering::Relaxed);
    metrics.hzb_views = bridge.0.hzb_views.load(Ordering::Relaxed);
    metrics.indirect_phase_buffers = bridge.0.indirect_phases.load(Ordering::Relaxed);
    metrics.indirect_batch_sets = bridge.0.indirect_batch_sets.load(Ordering::Relaxed);
    metrics.proof_frames = bridge.0.frames.load(Ordering::Relaxed);
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExtension {
    #[texture(100)]
    layer_0: Option<Handle<Image>>,
    #[texture(102)]
    layer_1: Option<Handle<Image>>,
    #[texture(104)]
    layer_2: Option<Handle<Image>>,
    #[texture(106)]
    layer_3: Option<Handle<Image>>,
    #[texture(108)]
    layer_4: Option<Handle<Image>>,
    #[texture(110)]
    layer_5: Option<Handle<Image>>,
    // LAND images share one repeating sampler. Use the first available color layer so an
    // untextured base does not select Bevy's clamping fallback sampler. AsBindGroup requires a
    // texture on the sampler's field; binding 119 supplies it without another image allocation.
    #[texture(119)]
    #[sampler(101)]
    sampler_source: Option<Handle<Image>>,
    #[uniform(112)]
    settings: TerrainSettings,
    // Each layer's normal map, sampled through `sampler_source`: every terrain layer image is
    // loaded with the same repeating sampler (`terrain_layer_sampler`), so the six need no sampler
    // bindings of their own. Which layers have one is in `TerrainSettings::normal_layers_*`, since
    // Bevy's stand-in for a missing texture is white, which decodes to a steep tilt.
    #[texture(113)]
    normal_0: Option<Handle<Image>>,
    #[texture(114)]
    normal_1: Option<Handle<Image>>,
    #[texture(115)]
    normal_2: Option<Handle<Image>>,
    #[texture(116)]
    normal_3: Option<Handle<Image>>,
    #[texture(117)]
    normal_4: Option<Handle<Image>>,
    #[texture(118)]
    normal_5: Option<Handle<Image>>,
}

#[derive(ShaderType, Reflect, Debug, Clone)]
struct TerrainSettings {
    tiling_and_layer_count: Vec4,
    quadrant_origin: Vec4,
    fallback_weights_0: Vec4,
    fallback_weights_1: Vec4,
    weight_source: Vec4,
    /// 1.0 in component `i` when layer `i` (0-3) has a normal map bound, else 0.0.
    normal_layers_0: Vec4,
    /// Layers 4 and 5 in x and y.
    normal_layers_1: Vec4,
    weights: [Vec4; WEIGHT_FIELD_WORDS],
}

/// The sample grid of one terrain quadrant: 17x17 opacities on the same 128-unit spacing as the
/// quadrant's mesh columns, which is what LAND's `VTXT` entries index.
pub(crate) const QUADRANT_WEIGHT_SAMPLES: usize = 17;

/// Samples per overlay that a quadrant can carry: the base layer plus five overlays, the runtime's
/// own cap (`quadrant_layers` rejects more).
pub(crate) const OVERLAY_WEIGHT_SLOTS: usize = 5;

/// Vec4s per overlay in the weight field: `289` samples four to a vec4, padded to whole vec4s so
/// every overlay starts on a 16-byte boundary. The shader declares the same numbers
/// (`WEIGHT_GRID_WORDS` in `terrain.wgsl`).
pub(crate) const OVERLAY_WEIGHT_WORDS: usize =
    (QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES).div_ceil(4);

/// The whole field the material's uniform carries: five overlays of [`OVERLAY_WEIGHT_WORDS`].
pub(crate) const WEIGHT_FIELD_WORDS: usize = OVERLAY_WEIGHT_SLOTS * OVERLAY_WEIGHT_WORDS;

/// The descriptor Bevy's asset loader uses for a `.ktx2` that requests no sampler of its own:
/// `ImagePlugin::default`'s `ImageSamplerDescriptor::linear()`, whose address modes are
/// `ImageAddressMode`'s default.
fn default_linear_sampler() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor::linear()
}

/// A render layer index.
pub type Layer = usize;

/// The layer of the world: terrain, and (here) the placed objects too.
pub const WORLD_LAYER: Layer = 0;

/// Full-detail stock objects stay separate from the reflected LOD world. Layers 2 through 4
/// belong to the portal destination, animated door layer, and unshadowed reflection replica.
/// Seamless cells instead receive their explicit role stamp from the portal's layer resolver.
pub const PLACED_OBJECT_LAYER: Layer = 5;

/// What every scene light belongs to, as a layer list (see [`LIGHT_LAYERS`]).
pub const REFLECTION_VIEW_LAYERS: &[Layer] = &[WORLD_LAYER];

/// The value main's placed-object propagation copies onto the meshes below a reference.
pub const PLACED_OBJECT_RENDER_LAYERS: RenderLayers = RenderLayers::layer(PLACED_OBJECT_LAYER);

/// The render layers for a point or spot light spawned below a placed-object reference (main's
/// contract: the propagated layer must not replace the light's own). The sun uses [`LIGHT_LAYERS`]
/// instead; see [`POINT_LIGHT_LAYERS`].
pub fn light_render_layers() -> (RenderLayers, PropagateOver<RenderLayers>) {
    (
        RenderLayers::from_layers(&POINT_LIGHT_LAYERS),
        PropagateOver::default(),
    )
}

/// Propagate the stock reference layer. The portal shares this plugin and adds its own role
/// observers, so either renderer setup may have installed it first.
pub fn add_placed_object_layer_propagation(app: &mut App) {
    if !app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate));
    }
    app.configure_sets(
        PostUpdate,
        PropagateSet::<RenderLayers>::default().before(VisibilitySystems::CheckVisibility),
    );
}

/// The render layer of water planes. The world, its placed objects and the doorway frames sit on
/// layer 0; the destination space of a portal is [`crate::portal::DESTINATION_LAYER`].
pub const WATER_LAYER: usize = 1;

/// The layers the main camera draws.
pub const MAIN_VIEW_LAYERS: [usize; 3] = [WORLD_LAYER, WATER_LAYER, PLACED_OBJECT_LAYER];

/// The layers a light of the active space shines on.
pub const LIGHT_LAYERS: [usize; 3] = [WORLD_LAYER, WATER_LAYER, PLACED_OBJECT_LAYER];

/// The layer only the unshadowed sun copy and a shadowless reflection view are on; see
/// [`crate::shadow_route`]. No main view and no source light is ever on it. World content carries
/// it **beside** [`WORLD_LAYER`] ([`reflectable_layers`]), never instead of it: an entity is on
/// this layer so the shadowless route still draws it, and on the world layer so every other view
/// and the source sun keep drawing it exactly as before.
///
/// Main has this as layer 3. The portal already uses 2 for the destination space
/// ([`crate::portal::DESTINATION_LAYER`]) and 3 for the door layer
/// ([`crate::door_layer::DOOR_LAYER`]), whose doors are on layer 0 as well: the replica takes the
/// next free layer, so a door-layer door's own set and this one stay distinct layers even though
/// the door is drawn by both routes.
///
/// **Engine-reserved: mods must not put content or lights on this layer by hand.** It belongs to
/// the engine's reflection routes. A mod's own world content goes on the world layer
/// ([`WORLD_LAYER`]), which the engine widens through [`reflectable_layers`] so the main view and
/// both reflection routes draw it. A mod's point or spot light takes the engine's
/// [`POINT_LIGHT_LAYERS`], which already include this layer. Content put here alone is drawn by the
/// shadowless reflection route and by no other view.
pub const UNSHADOWED_REFLECTION_LAYER: Layer = 4;

/// What the water reflection camera draws while its policy says shadows off: the reflected world
/// through [`UNSHADOWED_REFLECTION_LAYER`], lit by the unshadowed sun copy instead of the sun. It is
/// meant to be the same content as [`REFLECTION_VIEW_LAYERS`] on the other route.
pub const UNSHADOWED_REFLECTION_VIEW_LAYERS: &[Layer] = &[UNSHADOWED_REFLECTION_LAYER];

/// The layers an entity the water reflection draws belongs to, given the layers it belongs to
/// anyway: `own`, plus [`UNSHADOWED_REFLECTION_LAYER`] whenever the shadowed reflection route draws
/// `own` ([`REFLECTION_VIEW_LAYERS`]).
///
/// The two routes must draw the **same content** - the shadowless one is that picture with the
/// sun's cascades left out, not a thinner scene - so every producer of content the reflection shows
/// widens its layers through here: the plain nodes and terrain of a streamed cell
/// ([`crate::portal`]'s stamping, `crate::streaming`), a sky dome, the LOD ring and the door
/// layer's doors. A point light needs the layer too and names it directly
/// ([`POINT_LIGHT_LAYERS`]), since a light's layers are not content layers. A route that drew less
/// would be a cheaper reflection bought by dropping content rather than by dropping shadows.
///
/// Content no reflection draws is returned untouched, so nothing a doorway alone puts on screen can
/// leak into a reflection of the space the observer is in: a water surface ([`WATER_LAYER`]), the
/// destination space ([`crate::portal::DESTINATION_LAYER`]), the portal's quad and the doorway
/// filler all stay on their own layers.
///
/// This is the one rule: a new producer of content the world layer draws widens its layers here
/// rather than naming [`UNSHADOWED_REFLECTION_LAYER`] itself, or it ships a thing the shadowless
/// route silently drops.
pub fn reflectable_layers(own: &RenderLayers) -> RenderLayers {
    if own.intersects(&RenderLayers::from_layers(REFLECTION_VIEW_LAYERS)) {
        own.clone().with(UNSHADOWED_REFLECTION_LAYER)
    } else {
        own.clone()
    }
}

/// The layers an entity of the world belongs to: the world layer, which the main view and the
/// source sun see, and [`UNSHADOWED_REFLECTION_LAYER`], which the shadowless reflection route
/// sees. Terrain carries it, and so does every node a streamed cell stamps; a water surface never
/// does.
pub fn reflectable_world_layers() -> RenderLayers {
    reflectable_layers(&RenderLayers::layer(WORLD_LAYER))
}

/// What a point or spot light belongs to: [`LIGHT_LAYERS`] plus [`UNSHADOWED_REFLECTION_LAYER`].
///
/// A view's lights are chosen by layer intersection, so a shadowless reflection view, which draws
/// the replica layer only, would otherwise lose every lamp the shadowed reflection shows. A
/// converted point light has no cascades, so lighting that view costs nothing extra. The sun must
/// not carry the replica layer - that is what keeps its cascades out of the shadowless view - so it
/// keeps [`LIGHT_LAYERS`].
pub const POINT_LIGHT_LAYERS: [usize; 4] = [
    WORLD_LAYER,
    WATER_LAYER,
    PLACED_OBJECT_LAYER,
    UNSHADOWED_REFLECTION_LAYER,
];

/// The sampler a terrain layer texture needs: the shader tiles every layer `tiling` times across a
/// cell (`uv * LAND_TEXTURE_REPEATS_PER_CELL`), so the address mode must repeat.
/// Bevy's default sampler clamps to the edge,
/// which stretches a texture's last texel column, row and corner across everything past the first
/// tile - long streaks where the edge column is stretched, and one flat colour where the corner
/// texel covers the rest. A terrain texture that is not loaded through the asset server - the
/// synthetic fixtures build theirs in memory - has to be given the same sampler by hand.
pub(crate) fn terrain_layer_sampler() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: 16,
        ..default_linear_sampler()
    }
}

/// Packs a quadrant's overlay weight grids into the material uniform's field. Sample `s` of
/// overlay `o` lands in component `s % 4` of word `o * OVERLAY_WEIGHT_WORDS + s / 4`, the layout
/// `grid_weight` in `terrain.wgsl` reads.
fn weight_field(overlay_weights: &[Vec<f32>]) -> [Vec4; WEIGHT_FIELD_WORDS] {
    let mut field = [Vec4::ZERO; WEIGHT_FIELD_WORDS];
    for (overlay, grid) in overlay_weights
        .iter()
        .take(OVERLAY_WEIGHT_SLOTS)
        .enumerate()
    {
        for (sample, weight) in grid
            .iter()
            .enumerate()
            .take(QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES)
        {
            field[overlay * OVERLAY_WEIGHT_WORDS + sample / 4][sample % 4] = *weight;
        }
    }
    field
}

impl TerrainSettings {
    /// Marks which layers have a normal map bound, for the shader to blend only those.
    fn set_normal_layers(&mut self, normals: &[Option<Handle<Image>>; 6]) {
        let present = |index: usize| if normals[index].is_some() { 1.0 } else { 0.0 };
        self.normal_layers_0 = Vec4::new(present(0), present(1), present(2), present(3));
        self.normal_layers_1 = Vec4::new(present(4), present(5), 0.0, 0.0);
    }

    /// The settings of one streamed quadrant: the tiling the shader repeats every layer by, the
    /// quadrant's origin inside the cell, and its overlay weight field - which the shader
    /// interpolates itself, so the weights reach the fragment stage exactly as LAND records them.
    fn for_quadrant(quadrant: u8, layers: usize, overlay_weights: &[Vec<f32>]) -> Self {
        Self {
            tiling_and_layer_count: Vec4::new(
                LAND_TEXTURE_REPEATS_PER_CELL,
                LAND_TEXTURE_REPEATS_PER_CELL,
                layers as f32,
                0.0,
            ),
            quadrant_origin: Vec4::new(f32::from(quadrant % 2), f32::from(quadrant / 2), 0.0, 0.0),
            fallback_weights_0: Vec4::X,
            fallback_weights_1: Vec4::ZERO,
            // A quadrant with no overlay has nothing in the field to read, so the shader is told to
            // skip it: the packed attributes carry the same (empty) overlays.
            weight_source: if overlay_weights.is_empty() {
                Vec4::ZERO
            } else {
                Vec4::X
            },
            normal_layers_0: Vec4::ZERO,
            normal_layers_1: Vec4::ZERO,
            weights: weight_field(overlay_weights),
        }
    }

    /// The settings of a material that carries no weight field: only the tiling and the fallback
    /// weight, with the shader reading the packed vertex attributes instead. The synthetic
    /// fixtures are built without an `Assets<Image>` or a LAND snapshot, so this is how they are
    /// rendered; no streamed quadrant uses it.
    fn vertex_weights_only(layers: f32) -> Self {
        Self {
            tiling_and_layer_count: Vec4::new(
                LAND_TEXTURE_REPEATS_PER_CELL,
                LAND_TEXTURE_REPEATS_PER_CELL,
                layers,
                0.0,
            ),
            quadrant_origin: Vec4::ZERO,
            fallback_weights_0: Vec4::X,
            fallback_weights_1: Vec4::ZERO,
            weight_source: Vec4::ZERO,
            normal_layers_0: Vec4::ZERO,
            normal_layers_1: Vec4::ZERO,
            weights: [Vec4::ZERO; WEIGHT_FIELD_WORDS],
        }
    }
}

/// The landscape texture the base layer of `quadrant` samples.
///
/// Some `LAND` quadrants carry no base texture: the converter publishes them with texture form id
/// `0` (`crates/converter/src/esm/cell_cache.rs`), and Bevy binds its 1x1 white fallback to the
/// empty slot, which paints the quadrant's whole base weight as a white square. Such a quadrant
/// borrows a texture that is there instead: the base of the first neighbouring quadrant of the
/// same cell that has one, in quadrant order, else the texture of its own strongest overlay.
/// `0` still means "no texture", for a quadrant with nothing to borrow.
pub(crate) fn quadrant_base_texture_form_id(
    terrain: &TerrainSnapshot,
    quadrant: u8,
) -> Result<u32, String> {
    let layers = crate::streaming::quadrant_layers(terrain, quadrant)?;
    let own_base = layers
        .iter()
        .find(|layer| layer.is_base)
        .filter(|base| base.texture_form_id != 0);
    if let Some(base) = own_base {
        return Ok(base.texture_form_id);
    }
    for neighbour in 0..4u8 {
        if neighbour == quadrant {
            continue;
        }
        let base = crate::streaming::quadrant_layers(terrain, neighbour)?
            .into_iter()
            .find(|layer| layer.is_base)
            .filter(|base| base.texture_form_id != 0);
        if let Some(base) = base {
            return Ok(base.texture_form_id);
        }
    }
    Ok(strongest_overlay_texture(&layers).unwrap_or(0))
}

/// The texture of the overlay that covers most of the quadrant: the one whose `VTXT` opacities sum
/// highest, the earlier layer winning a tie so the choice is deterministic. Layers with no texture
/// of their own cannot stand in for the base and are skipped.
fn strongest_overlay_texture(layers: &[&TerrainLayerSnapshot]) -> Option<u32> {
    let mut strongest: Option<(f32, u32)> = None;
    for layer in layers.iter().filter(|layer| !layer.is_base) {
        if layer.texture_form_id == 0 {
            continue;
        }
        let covered: f32 = layer.weights.iter().map(|(_, opacity)| opacity).sum();
        let replace = match strongest {
            Some((best, _)) => covered > best,
            None => true,
        };
        if replace {
            strongest = Some((covered, layer.texture_form_id));
        }
    }
    strongest.map(|(_, texture_form_id)| texture_form_id)
}

/// The images a terrain quadrant's material waits on, by the colour space each
/// must decode to.
pub struct TerrainImages {
    /// The layers' diffuse images, sRGB.
    pub color: Vec<Handle<Image>>,
    /// The layers' normal maps, linear.
    pub normal: Vec<Handle<Image>>,
}

impl TerrainExtension {
    #[cfg(test)]
    pub(crate) fn with_test_normals(mut self, normals: [Option<Handle<Image>>; 6]) -> Self {
        self.settings.set_normal_layers(&normals);
        [
            self.normal_0,
            self.normal_1,
            self.normal_2,
            self.normal_3,
            self.normal_4,
            self.normal_5,
        ] = normals;
        self
    }

    /// Remove a failed authored normal binding. An absent binding uses the geometric normal,
    /// matching stock terrain behavior, while retaining every valid layer's normal map.
    pub(crate) fn remove_normal(&mut self, failed: bevy::asset::AssetId<Image>) {
        for normal in [
            &mut self.normal_0,
            &mut self.normal_1,
            &mut self.normal_2,
            &mut self.normal_3,
            &mut self.normal_4,
            &mut self.normal_5,
        ] {
            if normal.as_ref().is_some_and(|image| image.id() == failed) {
                *normal = None;
            }
        }
        self.settings.set_normal_layers(&[
            self.normal_0.clone(),
            self.normal_1.clone(),
            self.normal_2.clone(),
            self.normal_3.clone(),
            self.normal_4.clone(),
            self.normal_5.clone(),
        ]);
    }

    /// The exact image resources retained by this quadrant's material.
    pub(crate) fn image_ids(&self) -> impl Iterator<Item = bevy::asset::AssetId<Image>> + '_ {
        [
            self.layer_0.as_ref(),
            self.layer_1.as_ref(),
            self.layer_2.as_ref(),
            self.layer_3.as_ref(),
            self.layer_4.as_ref(),
            self.layer_5.as_ref(),
            self.normal_0.as_ref(),
            self.normal_1.as_ref(),
            self.normal_2.as_ref(),
            self.normal_3.as_ref(),
            self.normal_4.as_ref(),
            self.normal_5.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(Handle::id)
    }

    /// Builds the material without looking up zero texture IDs, retaining those layers' slots
    /// and weights. Nonzero texture IDs still require a converted diffuse image.
    pub fn from_quadrant(
        terrain: &TerrainSnapshot,
        quadrant: u8,
        catalog: &AssetCatalog,
        asset_server: &AssetServer,
    ) -> Result<(Self, TerrainImages), String> {
        let layers = crate::streaming::quadrant_layers(terrain, quadrant)?;
        // The layer slots in the order the shader samples them: slot 0 is the base, which a
        // quadrant without a `BTXT` of its own samples from the texture
        // [`quadrant_base_texture_form_id`] borrows for it.
        let mut slots = [0u32; 6];
        slots[0] = quadrant_base_texture_form_id(terrain, quadrant)?;
        for (slot, layer) in layers.iter().enumerate().skip(1) {
            slots[slot] = layer.texture_form_id;
        }
        let mut textures: [Option<Handle<Image>>; 6] = std::array::from_fn(|_| None);
        let mut normals: [Option<Handle<Image>>; 6] = std::array::from_fn(|_| None);
        let mut handles = Vec::new();
        let mut normal_handles = Vec::new();
        for (slot, texture_form_id) in slots.iter().enumerate() {
            if *texture_form_id == 0 {
                continue;
            }
            let texture_form_id = *texture_form_id;
            // A borrowed base samples the borrowed form id's normal map too, so its detail
            // matches the diffuse it is drawn with.
            if let Some(path) = catalog.landscape_normal(texture_form_id) {
                // A normal map holds directions, not colours: it must not be decoded as sRGB.
                let handle = asset_server
                    .load_builder()
                    .with_settings(|settings: &mut ImageLoaderSettings| {
                        settings.sampler = ImageSampler::Descriptor(terrain_layer_sampler());
                        settings.is_srgb = false;
                    })
                    .load(path.to_owned());
                normals[slot] = Some(handle.clone());
                normal_handles.push(handle);
            }
            let path = catalog.landscape_diffuse(texture_form_id).ok_or_else(|| {
                format!(
                    "LAND {:08X} quadrant {quadrant} texture {:08X} has no diffuse image",
                    terrain.cell_id, texture_form_id
                )
            })?;
            let handle = asset_server
                .load_builder()
                .with_settings(|settings: &mut ImageLoaderSettings| {
                    settings.sampler = ImageSampler::Descriptor(terrain_layer_sampler());
                })
                .load(path.to_owned());
            textures[slot] = Some(handle.clone());
            handles.push(handle);
        }
        let overlay_weights = crate::streaming::quadrant_overlay_weights(terrain, quadrant)?;
        let mut settings = TerrainSettings::for_quadrant(quadrant, layers.len(), &overlay_weights);
        settings.set_normal_layers(&normals);
        Ok((
            Self {
                layer_0: textures[0].clone(),
                layer_1: textures[1].clone(),
                layer_2: textures[2].clone(),
                layer_3: textures[3].clone(),
                layer_4: textures[4].clone(),
                layer_5: textures[5].clone(),
                sampler_source: textures.iter().flatten().next().cloned(),
                settings,
                normal_0: normals[0].clone(),
                normal_1: normals[1].clone(),
                normal_2: normals[2].clone(),
                normal_3: normals[3].clone(),
                normal_4: normals[4].clone(),
                normal_5: normals[5].clone(),
            },
            TerrainImages {
                color: handles,
                normal: normal_handles,
            },
        ))
    }

    pub(crate) fn fixture(
        terrain: &TerrainSnapshot,
        quadrant: u8,
        textures: [Handle<Image>; 6],
    ) -> Result<Self, String> {
        let layers = crate::streaming::quadrant_layers(terrain, quadrant)?;
        let overlay_weights = crate::streaming::quadrant_overlay_weights(terrain, quadrant)?;
        Ok(Self {
            layer_0: Some(textures[0].clone()),
            layer_1: Some(textures[1].clone()),
            layer_2: Some(textures[2].clone()),
            layer_3: Some(textures[3].clone()),
            layer_4: Some(textures[4].clone()),
            layer_5: Some(textures[5].clone()),
            sampler_source: Some(textures[0].clone()),
            settings: TerrainSettings::for_quadrant(quadrant, layers.len(), &overlay_weights),
            normal_0: None,
            normal_1: None,
            normal_2: None,
            normal_3: None,
            normal_4: None,
            normal_5: None,
        })
    }

    /// Whether the shader reads this material's overlay weights from its uniform weight field rather
    /// than from the weights the mesh packs into its vertex attributes. False only for a material
    /// with no overlays to read: a quadrant whose sole layer is its base, or a stand-in terrain.
    #[cfg(test)]
    pub(crate) fn reads_weight_field(&self) -> bool {
        self.settings.weight_source.x > 0.5
    }

    /// Whether any layer has a normal map bound.
    #[cfg(test)]
    pub(crate) fn has_normal_maps(&self) -> bool {
        self.settings.normal_layers_0 != Vec4::ZERO || self.settings.normal_layers_1 != Vec4::ZERO
    }
}

impl Default for TerrainExtension {
    fn default() -> Self {
        Self {
            layer_0: None,
            layer_1: None,
            layer_2: None,
            layer_3: None,
            layer_4: None,
            layer_5: None,
            sampler_source: None,
            settings: TerrainSettings::vertex_weights_only(0.0),
            normal_0: None,
            normal_1: None,
            normal_2: None,
            normal_3: None,
            normal_4: None,
            normal_5: None,
        }
    }
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://engine/shaders/terrain.wgsl".into()
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterExtension {
    #[uniform(100)]
    settings: WaterSettings,
    #[texture(101)]
    #[sampler(102)]
    reflection: Option<Handle<Image>>,
    #[texture(103)]
    #[sampler(104)]
    flow_normal: Option<Handle<Image>>,
}

/// The field order here is the uniform's memory layout and must match `struct WaterSettings` in
/// `shaders/water.wgsl` (encase writes the fields in declaration order); the test
/// `water_settings_fields_match_the_shader_struct` pins it.
#[derive(ShaderType, Reflect, Debug, Clone)]
struct WaterSettings {
    /// x = wave scale, y = wave speed, z = wave strength. w unused.
    wave_scale_speed_strength: Vec4,
    /// xy = flow direction, z unused, w = 1 when the flow normal texture is present.
    flow_direction: Vec4,
    /// x = Fresnel Amount (Schlick F0), y = Reflectivity Amount. z/w unused.
    fresnel_reflectivity: Vec4,
    /// x = the pinned clock in seconds, y = 1 when the water uses it instead of the shared view
    /// clock. z/w unused. See [`WaterExtension::with_pinned_clock`].
    clock: Vec4,
    /// The water's Reflection Color (WATR DNAM), linear. What the reflection fades to where the
    /// reflection image has no coverage; alpha unused.
    reflection_color: Vec4,
}

impl Default for WaterExtension {
    fn default() -> Self {
        Self {
            settings: WaterSettings {
                wave_scale_speed_strength: Vec4::new(0.006, 0.15, WAVE_STRENGTH, 0.0),
                flow_direction: Vec4::new(0.8, 0.35, 0.0, 0.0),
                clock: Vec4::ZERO,
                fresnel_reflectivity: Vec4::new(
                    DEFAULT_WATER_FRESNEL,
                    DEFAULT_WATER_REFLECTIVITY,
                    1.0,
                    0.0,
                ),
                reflection_color: water_reflection_color(DEFAULT_WATER_REFLECTION_COLOR),
            },
            reflection: None,
            flow_normal: None,
        }
    }
}

impl WaterExtension {
    /// Per-cell dependencies. The shared reflection target belongs to the
    /// renderer baseline and must not keep a retired water cell reserved.
    pub(crate) fn image_ids(&self) -> impl Iterator<Item = bevy::asset::AssetId<Image>> + '_ {
        self.flow_normal.iter().map(Handle::id)
    }

    /// Builds a water material with Skyrim's DefaultWater fresnel and reflectivity. Callers that
    /// know a water's own factors (from [`crate::world::database::AssetCatalog::water_colors`])
    /// should use [`Self::with_reflection_and_factors`] instead.
    pub fn with_reflection(reflection: Handle<Image>, flow_normal: Option<Handle<Image>>) -> Self {
        Self::with_reflection_and_factors(
            reflection,
            flow_normal,
            DEFAULT_WATER_FRESNEL,
            DEFAULT_WATER_REFLECTIVITY,
        )
    }

    pub fn with_reflection_and_factors(
        reflection: Handle<Image>,
        flow_normal: Option<Handle<Image>>,
        fresnel: f32,
        reflectivity: f32,
    ) -> Self {
        let has_flow_normal = flow_normal.is_some() as u8 as f32;
        Self {
            reflection: Some(reflection),
            flow_normal,
            settings: WaterSettings {
                wave_scale_speed_strength: Vec4::new(0.006, 0.15, WAVE_STRENGTH, 0.0),
                flow_direction: Vec4::new(0.8, 0.35, 0.0, has_flow_normal),
                clock: Vec4::ZERO,
                // z gates the reflection strength; 1.0 keeps reflections on until settings set it.
                fresnel_reflectivity: Vec4::new(fresnel, reflectivity, 1.0, 0.0),
                reflection_color: water_reflection_color(DEFAULT_WATER_REFLECTION_COLOR),
            },
        }
    }

    /// Sets the water's own Reflection Color (WATR DNAM, as stored bytes).
    pub fn with_reflection_color(mut self, rgb: [u8; 3]) -> Self {
        self.settings.reflection_color = water_reflection_color(rgb);
        self
    }
}

impl WaterExtension {
    /// Pins this water's animation clock to a fixed time instead of the shared clock.
    ///
    /// The waves otherwise read `globals.time_unwrapped` in the shader, the one clock Bevy uploads once a
    /// frame for every view, so the material itself never changes while time advances and Bevy does
    /// not re-prepare its bind group every frame. A pinned clock is for scenes that need a
    /// reproducible still, such as the terrain/water fixture.
    pub fn with_pinned_clock(mut self, seconds: f32) -> Self {
        self.settings.clock = Vec4::new(seconds, 1.0, 0.0, 0.0);
        self
    }
}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://engine/shaders/water.wgsl".into()
    }

    /// Under `--water-reflection ssr` the shader is compiled with the march and its step counts
    /// ([`crate::water_ssr::push_shader_defs`]); under `planar` it gets no definition at all and
    /// is the shader it has always been.
    fn specialize(
        _pipeline: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            crate::water_ssr::push_shader_defs(&mut fragment.shader_defs);
        }
        Ok(())
    }
}

#[derive(Resource, Clone)]
pub struct WaterReflectionTexture(pub Handle<Image>);

/// What every water material is held to. A change re-applies it to all of them.
#[derive(Clone, Copy, PartialEq)]
struct WaterTargets {
    wave: Vec4,
    reflection: f32,
    flow_normals: bool,
    pinned_clock: Option<Vec4>,
}

/// Water materials inspected by [`apply_water_settings`]; a settled frame inspects none.
#[cfg(test)]
#[derive(Resource, Default)]
struct WaterSettingsVisits(usize);

/// Apply live water controls when they change, and to water that arrives or is replaced. The
/// animation needs no material write: the shader reads Bevy's global time, one uniform per frame.
fn apply_water_settings(
    settings: Option<Res<crate::settings::RenderingSettings>>,
    shots: Option<Res<crate::shots::ShotsRun>>,
    mut events: MessageReader<AssetEvent<WaterMaterial>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut applied: Local<Option<WaterTargets>>,
    #[cfg(test)] mut visits: Option<ResMut<WaterSettingsVisits>>,
) {
    let Some(settings) = settings else {
        events.clear();
        return;
    };
    let targets = WaterTargets {
        wave: Vec4::new(
            settings.wave_scale,
            settings.wave_speed,
            if settings.water_waves {
                settings.wave_strength
            } else {
                0.0
            },
            0.0,
        ),
        reflection: if settings.water_reflections {
            settings.water_reflection_strength
        } else {
            0.0
        },
        flow_normals: settings.water_flow_normals,
        // A static capture claims a fixed water phase in its receipt, so every water material,
        // including streamed cells that arrive later, must read that phase and not the shared clock.
        pinned_clock: shots
            .as_ref()
            .and_then(|shots| shots.water_time_seconds())
            .map(|seconds| Vec4::new(seconds, 1.0, 0.0, 0.0)),
    };
    let ids: Vec<_> = if *applied == Some(targets) {
        // Our own writes come back as one `Modified` each; they then match and stop there.
        events
            .read()
            .filter_map(|event| match event {
                AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(*id),
                _ => None,
            })
            .collect()
    } else {
        *applied = Some(targets);
        events.clear();
        materials.ids().collect()
    };
    #[cfg(test)]
    if let Some(visits) = visits.as_mut() {
        visits.0 += ids.len();
    }
    for id in ids {
        let Some(water) = materials.get(id).map(|material| &material.extension) else {
            continue;
        };
        let flow = f32::from(u8::from(
            targets.flow_normals && water.flow_normal.is_some(),
        ));
        let stale = water.settings.wave_scale_speed_strength != targets.wave
            || water.settings.flow_direction.w != flow
            || water.settings.fresnel_reflectivity.z != targets.reflection
            || targets
                .pinned_clock
                .is_some_and(|clock| water.settings.clock != clock);
        if !stale {
            continue;
        }
        if let Some(mut material) = materials.get_mut(id) {
            let water = &mut material.extension;
            water.settings.wave_scale_speed_strength = targets.wave;
            water.settings.flow_direction.w = flow;
            water.settings.fresnel_reflectivity.z = targets.reflection;
            if let Some(clock) = targets.pinned_clock {
                water.settings.clock = clock;
            }
        }
    }
}

#[derive(Component)]
pub(crate) struct WaterReflectionCamera;

/// Slack added to a water plane's bounds before the main camera frustum test, in world units (4096
/// to a cell).
///
/// The frustum is built from the observer's current pose each frame, so the margin is not there to
/// absorb a stale frustum. It keeps the reflection camera on for a plane just outside the view, so
/// that a turn or a move between two frames cannot bring water in before the reflection has
/// rendered once: seen from one cell away, it buys about seven degrees of extra turn per frame - a
/// flick past 400 degrees a second - or, when the camera moves instead of turning, 512 units of
/// travel between two frames. A plane that far off the edge of a 45 degree view is a sliver of the
/// frame, so little of the saving is given back.
const WATER_REFLECTION_FRUSTUM_MARGIN: f32 = 512.0;

/// Spawns the camera that fills [`WaterReflectionTexture`], and the texture it renders into.
///
/// It is the flipped copy of the streaming camera, drawn before the main view, and it renders
/// [`REFLECTION_VIEW_LAYERS`] - or [`UNSHADOWED_REFLECTION_VIEW_LAYERS`] while its policy says
/// shadows off ([`crate::shadow_route`]).
///
/// The camera is managed by the [`ViewPolicy`] it carries: [`prepare_water_reflection`] fills its
/// demand and [`apply_view_policies`](crate::view_policy::apply_view_policies) decides each frame
/// whether it renders, keeping this image alive across the frames it does not.
///
/// The policy is the one `--water-reflection-preset` chose ([`water_reflection_policy`]; baseline
/// when there is no configuration), its target scaled by `--reflection-scale`. The baseline camera
/// keeps the components the reflection cost flags give it
/// ([`ReflectionOptions`](crate::config::ReflectionOptions): depth prepass and occlusion culling
/// on by default). The water preset throttles the camera, and a throttled camera must not keep occlusion
/// history or multisampling: it is spawned without [`OcclusionCulling`], whatever
/// `--reflection-occlusion` says, and with [`Msaa::Off`].
fn setup_water_reflection(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    config: Option<Res<crate::config::EngineConfig>>,
) {
    let options = config
        .as_ref()
        .map(|config| config.reflection.clone())
        .unwrap_or_default();
    let preset = config.map_or(WaterReflectionPreset::Baseline, |config| {
        config.water_reflection_preset
    });
    let mut policy = water_reflection_policy(preset);
    // `--reflection-scale` scales whichever size the preset starts from (1 keeps it).
    let (width, height) = options.scaled_size((policy.target_size.x, policy.target_size.y));
    policy.target_size = UVec2::new(width, height);
    let layers = if policy.shadows == ViewShadows::On {
        REFLECTION_VIEW_LAYERS
    } else {
        UNSHADOWED_REFLECTION_VIEW_LAYERS
    };
    let image = images.add(Image::new_target_texture(
        policy.target_size.x,
        policy.target_size.y,
        bevy::render::render_resource::TextureFormat::Rgba16Float,
        None,
    ));
    commands.insert_resource(WaterReflectionTexture(image.clone()));
    let mut camera = commands.spawn((
        Camera3d::default(),
        SceneColorPipeline::reflection(),
        Camera {
            order: -1,
            // The camera is placed below the water looking up, not mirrored, so its triangles keep
            // their winding: inverting the culling drew the back faces.
            invert_culling: false,
            is_active: false,
            // Nothing drawn means alpha 0, so the water can tell "no reflected geometry" from
            // reflected geometry and fall back to its Reflection Color there.
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::Image(image.clone().into()),
        Transform::default(),
        RenderLayers::from_layers(layers),
        // The policy's shadow choice picks one of these two layer sets every frame.
        crate::shadow_route::ViewShadowRoute {
            shadowed: RenderLayers::from_layers(REFLECTION_VIEW_LAYERS),
            unshadowed: RenderLayers::from_layers(UNSHADOWED_REFLECTION_VIEW_LAYERS),
        },
        // The chosen preset is not yet part of a run's recorded settings; a benchmark comparing
        // presets has to name the preset it ran with.
        policy,
        ViewDemand::default(),
        crate::view_policy::ViewPolicyState::default(),
        SecondaryViewTarget {
            image,
            allocated: policy.target_size,
        },
        WaterReflectionCamera,
        // The exposure and shadow filter of the graphics settings; no AA, no SSAO
        // (`crate::graphics_settings`).
        crate::graphics_settings::GraphicsCamera::WaterReflection,
    ));
    // The prepass is on by default; `--reflection-prepass=off` disables it and occlusion culling.
    // Without the prepass the view has no prepass phase to queue, sort,
    // batch and draw (`bevy_core_pipeline-0.19.0/src/prepass/node.rs`), and a card's soft edge
    // (`effect_palette.wgsl`'s `DEPTH_PREPASS` block) is compiled out for this view. Without
    // occlusion culling the view skips the two-phase early/late prepass split
    // (`prepass/node.rs:41`), the late GPU preprocessing pass (`bevy_pbr-0.19.0/src/render/
    // gpu_preprocess.rs:459`, `:610`) and its depth pyramid (`bevy_core_pipeline-0.19.0/src/
    // mip_generation/experimental/depth.rs:699`).
    if options.prepass {
        camera.insert(DepthPrepass);
    }
    match preset {
        WaterReflectionPreset::Baseline => {
            if options.occlusion_culling() {
                camera.insert(OcclusionCulling);
            }
        }
        // A throttled view keeps neither occlusion history nor multisampling.
        WaterReflectionPreset::Water => {
            camera.insert(Msaa::Off);
        }
    }
}

/// The [`ViewPolicy`] a `--water-reflection-preset` stands for.
pub fn water_reflection_policy(preset: WaterReflectionPreset) -> ViewPolicy {
    match preset {
        WaterReflectionPreset::Baseline => ViewPolicy::baseline(),
        WaterReflectionPreset::Water => ViewPolicy::water(),
    }
}

/// `--reflection-cascades N`: how many of a sun's shadow cascades the reflection view keeps.
#[derive(Resource, Debug, Clone, Copy)]
struct WaterReflectionCascadeLimit(usize);

/// Registers the reflection's cascade limiter from the run's [`EngineConfig`]: one for the limit
/// [`ReflectionOptions::cascades`](crate::config::ReflectionOptions::cascades) holds, and nothing at
/// all for `--reflection-cascades all` (or 4), which is `None` and keeps every cascade the sun has.
/// A run with no `EngineConfig` (some unit tests) is left unlimited too.
fn add_configured_water_reflection_cascade_limit(app: &mut App) {
    if let Some(limit) = app
        .world()
        .get_resource::<crate::config::EngineConfig>()
        .and_then(|config| config.reflection.cascades)
    {
        add_water_reflection_cascade_limit(app, limit);
    }
}

/// Registers the reflection's cascade limiter: [`WaterReflectionCascadeLimit`] and its system, in
/// `PostUpdate` after Bevy's own frustum update (`SimulationLightSystems::UpdateLightFrusta`, where
/// `bevy_light` puts `update_directional_light_frusta`, `bevy_light-0.19.0/src/lib.rs:187-188`).
///
/// The ordering is part of the design, not an aside: a limiter running before the frusta would have
/// them built from the shortened lists, and a view with fewer frusta than the one before it panics
/// Bevy's directional light visibility check (see [`limit_water_reflection_cascades`]).
fn add_water_reflection_cascade_limit(app: &mut App, limit: usize) {
    app.insert_resource(WaterReflectionCascadeLimit(limit))
        .add_systems(
            PostUpdate,
            limit_water_reflection_cascades.after(SimulationLightSystems::UpdateLightFrusta),
        );
}

/// Keeps only the first [`WaterReflectionCascadeLimit`] shadow cascades of each directional light
/// for the reflection view, and none of a light the reflection view cannot see.
///
/// A light's cascade count is one number for every view, so the reflection cannot be given a sun of
/// its own: the world's meshes are on layer 0 and the reflection draws layer 0, so any light that
/// casts their shadows reaches the reflection view too (`bevy_pbr-0.19.0/src/render/light.rs:1782`
/// filters a view's lights by layer). What can differ per view is the list of cascades: Bevy builds
/// it for every active camera (`bevy_light-0.19.0/src/cascade.rs:195-253`), and everything after
/// zips it: `prepare_lights` pairs the view's cascades with its frusta and the light's bounds and
/// spawns one shadow view per pair (`bevy_pbr-0.19.0/src/render/light.rs:1795-1818`), so a cascade
/// dropped here is a shadow view never specialized, queued, batched or drawn on the render thread.
///
/// Only the cascades are shortened, after `update_directional_light_frusta` has built the frusta
/// from them (`bevy_light-0.19.0/src/directional_light.rs:239`). Shortening the frusta as well
/// would also save the main world's per-cascade culling, but it panics:
/// `check_dir_light_mesh_visibility` sizes its thread-local queues to one view's frustum count only
/// on the threads that ran a batch for that view (`bevy_light-0.19.0/src/lib.rs:403-407`), and then
/// indexes every thread's queue by the next view's cascade index (`:466-476`) - a view with fewer
/// frusta than the one before it leaves a short queue behind ("index out of bounds: the len is 0
/// but the index is 0", `lib.rs:477`, seen on a `--shots` run). With every view's frusta the same
/// length, as Bevy always has them, the queues always fit.
///
/// The light's shader record still says the light has its full count (`light.rs:1672-1706` takes it
/// from the config), and the dropped cascades are left at `GpuDirectionalCascade::default`:
/// `far_bound` 0, so `get_cascade_index` (`bevy_pbr-0.19.0/src/render/shadows.wgsl:136-145`)
/// returns "no cascade" past the last kept one and the surface is lit, and the blend into the next
/// cascade over the last kept cascade's overlap samples an identity matrix, which lands outside
/// the shadow map (`shadows.wgsl:151-168`) for any point further than one unit from the world
/// origin. The shadows fade out at the last kept cascade's far bound; 0 keeps none, and the
/// reflection is unshadowed.
///
/// A light whose layers miss the reflection camera's (the doorway's sun, on the portal's layer)
/// has cascades listed for the reflection view too, which `prepare_lights` never turns into shadow
/// views (it skips a light whose layers miss the view's): they are emptied as well, which changes
/// nothing drawn.
///
/// Runs after `update_directional_light_frusta` (`SimulationLightSystems::UpdateLightFrusta`), for
/// the reason above; the lists reach the render world at extraction, after `PostUpdate`.
fn limit_water_reflection_cascades(
    limit: Res<WaterReflectionCascadeLimit>,
    reflection: Query<(Entity, Option<&RenderLayers>), With<WaterReflectionCamera>>,
    mut lights: Query<(&mut Cascades, Option<&RenderLayers>), With<DirectionalLight>>,
) {
    let Ok((camera, camera_layers)) = reflection.single() else {
        return;
    };
    let camera_layers = camera_layers.cloned().unwrap_or_default();
    for (mut cascades, light_layers) in &mut lights {
        let keep = if light_layers
            .cloned()
            .unwrap_or_default()
            .intersects(&camera_layers)
        {
            limit.0
        } else {
            0
        };
        if let Some(view_cascades) = cascades.cascades.get_mut(&camera) {
            view_cascades.truncate(keep);
        }
    }
}

/// PORTAL-SIDE HOOK of the screen-space water reflection (`--water-reflection ssr`,
/// [`crate::water_ssr`]). Everything that joins that module to this branch is here; the module
/// itself, and the `ssr_reflect` function of `shaders/water.wgsl`, know nothing of the portal, the
/// view policy or the planar camera and port to another branch unchanged.
///
/// Under `ssr` it does two things, and returns `true` so the caller leaves
/// [`prepare_water_reflection`] out of the schedule:
///
/// * **The planar camera is never demanded.** Without that system nothing fills the reflection
///   camera's [`ViewDemand`], so the policy never admits it: it is not active, renders nothing and
///   builds no shadow cascades, and its preparation costs nothing either.
///   `--water-reflection-preset` is not consulted.
/// * **The sky is the camera's own.** The water's misses fall back to the sky the camera's dome
///   draws, and the dome's colours are this branch's per-camera [`CameraSky`]
///   ([`crate::sky_dome`]): [`feed_water_ssr_sky`] hands each camera's palette to the module.
///
/// Under `planar` it does nothing and returns `false`.
fn portal_water_ssr_hook(app: &mut App) -> bool {
    let ssr = app
        .world()
        .get_resource::<crate::config::EngineConfig>()
        .is_some_and(|config| {
            config.water_reflection_mode == crate::config::WaterReflectionMode::Ssr
        });
    if ssr {
        app.add_systems(
            PostUpdate,
            feed_water_ssr_sky.before(crate::water_ssr::WaterSsrSystems::Mark),
        );
    }
    ssr
}

/// Part of [`portal_water_ssr_hook`]: copies each camera's sky palette to the component the
/// screen-space reflection reads, writing only when it changed.
fn feed_water_ssr_sky(
    mut commands: Commands,
    cameras: Query<(
        Entity,
        &crate::sky_dome::CameraSky,
        Option<&crate::water_ssr::WaterSsrSky>,
    )>,
) {
    for (camera, sky, current) in &cameras {
        let wanted = crate::water_ssr::WaterSsrSky {
            palette: sky.palette,
        };
        if current != Some(&wanted) {
            commands.entity(camera).insert(wanted);
        }
    }
}

/// The streamed world camera, never the reflection camera.
type WaterReflectionObserver = (
    With<crate::world::components::StreamingCamera>,
    Without<WaterReflectionCamera>,
);

/// What the adapter reads off the world camera: the entity it can ask for a current pose, the pose
/// it had at the last propagation, the projection the reflection has to match, and the layers it
/// draws.
type WaterReflectionObserverView = (
    Entity,
    &'static GlobalTransform,
    Option<&'static Projection>,
    Option<&'static Exposure>,
    Option<&'static RenderLayers>,
);

/// What the gate reads off a water plane: the entity it can ask for a current pose, its propagated
/// pose and bounds for the frustum test, and the layers and inherited visibility that say whether
/// any camera sampling the reflection draws it at all.
type WaterSurfaceView = (
    Entity,
    &'static GlobalTransform,
    Option<&'static Aabb>,
    Option<&'static RenderLayers>,
    Option<&'static InheritedVisibility>,
);

/// The portal's camera, read for whether it draws this frame.
type PortalCameraFilter = (
    With<crate::portal::PortalCamera>,
    Without<WaterReflectionCamera>,
);

/// Fills the reflection camera's [`ViewDemand`] for this frame.
///
/// It picks the water surface nearest the observer for the mirror plane, tests the surfaces against
/// the observer's *current* pose and projection, and hands the result to the policy - this system
/// never touches `Camera::is_active`. The pose comes from the observer's `Transform` through
/// [`TransformHelper`](bevy::transform::helper::TransformHelper) rather than from its
/// `GlobalTransform`, because the camera follow systems and the streaming fixture write
/// `Transform` in `Update` and propagation has not run yet when this system does.
///
/// No observer, or no water at all, clears the demand: the policy then switches the view off and
/// keeps the image it last rendered into.
#[allow(clippy::too_many_arguments)]
fn prepare_water_reflection(
    mut commands: Commands,
    main_camera: Query<WaterReflectionObserverView, WaterReflectionObserver>,
    water: Query<WaterSurfaceView, With<crate::world::components::WaterSurface>>,
    portal_cameras: Query<&Camera, PortalCameraFilter>,
    mut reflection_camera: Query<
        (Entity, &mut ViewDemand, Option<&mut Exposure>),
        With<WaterReflectionCamera>,
    >,
    transforms: TransformHelper,
    mut profiler: ResMut<ProfilingState>,
    config: Option<Res<crate::config::EngineConfig>>,
    settings: Option<Res<crate::settings::RenderingSettings>>,
) {
    let started = std::time::Instant::now();
    // `--water-reflection off`, a measurement switch: no demand, so the policy never admits the
    // camera and the reflection image keeps the texels it was created with (black), which the
    // water shader blends in at its Fresnel weight as no reflection at all.
    if config.is_some_and(|config| !config.water_reflection)
        || settings.is_some_and(|settings| !settings.water_reflections)
    {
        for (_, mut demand, _) in &mut reflection_camera {
            demand.request = None;
        }
        profiler.record_elapsed("render/water_reflection_camera", started);
        return;
    }
    let Ok((observer, observer_pose, projection, exposure, main_layers)) = main_camera.single()
    else {
        for (_, mut demand, _) in &mut reflection_camera {
            demand.request = None;
        }
        profiler.record_elapsed("render/water_reflection_camera", started);
        return;
    };
    let observer_pose = current_global_transform(&transforms, observer, observer_pose);
    // PBR lighting is already exposure-scaled when the reflection is sampled by water.
    // Keep both views in the same domain, including explicit diagnostic exposure changes.
    let ev100 = exposure.map_or(DEFAULT_SCENE_EV100, |value| value.ev100);
    let main_layers = main_layers.cloned().unwrap_or_default();
    let portal_drawn = portal_cameras.iter().any(|camera| camera.is_active);
    // The frustum is built from the pose the observer stands in *now*, not from the one Bevy kept
    // in step with it at the end of the previous frame: a fast turn or a teleport otherwise leaves
    // the gate deciding against a view the camera has already left.
    let frustum = projection.map(|projection| projection.compute_frustum(&observer_pose));
    let far = projection.map(|projection| projection.far());

    // The surface nearest the main camera fixes the mirror plane; a further surface would put the
    // reflection at the wrong height when more than one water level is streamed in.
    let mut mirror_surface = None;
    let mut mirror_distance = f32::INFINITY;
    let mut surface_in_view = false;
    for (entity, surface, bounds, layers, visibility) in &water {
        let Some(seen_by) = water_drawn_by(layers, visibility, &main_layers, portal_drawn) else {
            // Water no camera draws - a pre-streamed exterior's river behind a shut house door -
            // neither fixes the mirror plane nor keeps the reflection rendering.
            continue;
        };
        let surface = current_global_transform(&transforms, entity, surface);
        let distance = (surface.translation().y - observer_pose.translation().y).abs();
        if distance < mirror_distance {
            mirror_distance = distance;
            mirror_surface = Some(surface);
        }
        surface_in_view |= match seen_by {
            WaterSeenBy::Main => {
                water_plane_in_view(&observer_pose, frustum.as_ref(), far, &surface, bounds)
            }
            // The destination is laid out behind the portal's window, not in the main camera's
            // view, so the main frustum says nothing about it: drawn by the portal is enough.
            WaterSeenBy::Portal => true,
        };
    }

    for (entity, mut demand, reflection_exposure) in &mut reflection_camera {
        // Compared before it is written, so a still exposure does not mark the camera changed.
        if let Some(mut reflection_exposure) = reflection_exposure {
            if reflection_exposure.ev100 != ev100 {
                reflection_exposure.ev100 = ev100;
            }
        } else {
            commands.entity(entity).insert(Exposure { ev100 });
        }
        let Some(mirror_surface) = mirror_surface.as_ref().filter(|_| surface_in_view) else {
            demand.request = None;
            continue;
        };
        let mirror_height = mirror_surface.translation().y;
        let pose = reflected_camera_transform(&observer_pose, mirror_height);
        let projection = projection.map(|projection| match projection {
            Projection::Perspective(lens) => {
                Projection::Perspective(reflection_lens(lens, &pose, mirror_height))
            }
            other => other.clone(),
        });
        demand.request = Some(ViewRequest {
            observer_pose,
            pose,
            projection,
            // The mirror's height, not the pose derived from it: a plane that rises under a still
            // observer changes nothing else the policy can compare. Water is a flat surface, so two
            // equal heights give the same mirror however the plane is otherwise placed.
            domain_revision: u64::from(mirror_height.to_bits()),
        });
    }
    profiler.record_elapsed("render/water_reflection_camera", started);
}

/// Which camera sampling the reflection draws a water plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaterSeenBy {
    /// The main camera: the plane is on one of its layers.
    Main,
    /// The portal camera, which draws the destination's cells while a doorway is open.
    Portal,
}

/// Whether any camera that samples the water reflection draws this plane, and which one.
///
/// Only water a camera draws is worth a reflection: the reflection texture is sampled by water
/// surfaces alone, so a plane on no drawn layer - a cell the portal hides, or a destination's
/// river while its door is shut and the portal camera is off - costs a whole second render of the
/// scene for nothing. Standing inside a Riverwood house with the door shut, the exterior behind
/// it stays streamed in for the door, and its river kept the reflection on (research-269).
///
/// A hidden plane (its [`InheritedVisibility`] false) is drawn by nobody. A plane without
/// layers is on layer 0, like every entity; a plane without an [`InheritedVisibility`] - a unit
/// test's bare entity - counts as visible.
fn water_drawn_by(
    layers: Option<&RenderLayers>,
    visibility: Option<&InheritedVisibility>,
    main_layers: &RenderLayers,
    portal_drawn: bool,
) -> Option<WaterSeenBy> {
    if visibility.is_some_and(|visibility| !visibility.get()) {
        return None;
    }
    let layers = layers.cloned().unwrap_or_default();
    if layers.intersects(main_layers) {
        Some(WaterSeenBy::Main)
    } else if portal_drawn
        && layers.intersects(&RenderLayers::layer(crate::portal::DESTINATION_LAYER))
    {
        Some(WaterSeenBy::Portal)
    } else {
        None
    }
}

/// An entity's world transform as it stands this frame.
///
/// [`TransformHelper`](bevy::transform::helper::TransformHelper) is what makes "this frame" true -
/// it walks the entity's `Transform` and its ancestors' rather than reading the propagated
/// [`GlobalTransform`]. An entity whose chain cannot be walked (one spawned with only a
/// `GlobalTransform`, or with an ancestor whose `Transform` is missing) keeps the propagated pose:
/// the last known one is a better mirror than no reflection at all.
fn current_global_transform(
    transforms: &TransformHelper<'_, '_>,
    entity: Entity,
    propagated: &GlobalTransform,
) -> GlobalTransform {
    transforms
        .compute_global_transform(entity)
        .unwrap_or(*propagated)
}

/// Whether a water plane's bounds reach the main camera's view, its bounds grown by
/// [`WATER_REFLECTION_FRUSTUM_MARGIN`] first.
///
/// The frustum half is Bevy's own test against the plane's [`Aabb`]. Bevy's perspective projection
/// is infinite reverse-z, so the [`Frustum`] carries no far plane at all (`from_clip_from_world`
/// leaves its last half space at `(NaN, NaN, NaN, inf)`), and the far distance of the camera's
/// [`Projection`] is applied here by hand through the plane's bounding sphere.
///
/// Missing pieces mean the test cannot run - a plane whose mesh has not produced an [`Aabb`] yet,
/// or an observer without a [`Projection`] to build the frustum from - and the plane then counts as
/// visible, keeping the reflection camera on rather than risk a stale reflection.
fn water_plane_in_view(
    main: &GlobalTransform,
    frustum: Option<&Frustum>,
    far: Option<f32>,
    surface: &GlobalTransform,
    bounds: Option<&Aabb>,
) -> bool {
    let (Some(frustum), Some(far), Some(bounds)) = (frustum, far, bounds) else {
        return true;
    };
    let margin = Vec3::splat(WATER_REFLECTION_FRUSTUM_MARGIN);
    let grown = Aabb::from_min_max(
        Vec3::from(bounds.min()) - margin,
        Vec3::from(bounds.max()) + margin,
    );
    let surface_to_world = surface.affine();
    let centre = Vec3::from(surface_to_world.transform_point3a(grown.center));
    let radius = Vec3::from(grown.half_extents).length() * surface.scale().max_element();
    if (centre - main.translation()).dot(*main.forward()) - radius > far {
        return false;
    }
    frustum.intersects_obb(&grown, &surface_to_world, true, true)
}

/// The reflection camera's lens: the main camera's, clipped at the water plane.
///
/// The reflection camera sits below the surface looking up, so the river bed and the banks lie
/// between it and the sky and would fill the image (the white sheet). Clipping at the water plane
/// keeps only what is above it, as a real mirror shows. Bevy's own mirror example does the same with
/// [`PerspectiveProjection::near_clip_plane`]: the plane's normal in the camera's view space, and
/// the negative distance to it as the fourth component.
fn reflection_lens(
    main_lens: &PerspectiveProjection,
    reflection: &Transform,
    water_y: f32,
) -> PerspectiveProjection {
    // Keep the side the main camera looks at the water from: above it, unless the main camera is
    // underwater (its mirrored camera then sits above the surface).
    let height_above_camera = water_y - reflection.translation.y;
    let normal_world = if height_above_camera >= 0.0 {
        Vec3::Y
    } else {
        Vec3::NEG_Y
    };
    let normal_view = reflection.rotation.inverse() * normal_world;
    PerspectiveProjection {
        near_clip_plane: normal_view.extend(-height_above_camera.abs()),
        ..*main_lens
    }
}

fn reflected_camera_transform(main: &GlobalTransform, water_y: f32) -> Transform {
    let mut position = main.translation();
    position.y = water_y * 2.0 - position.y;
    let mut forward = main.forward().as_vec3();
    forward.y = -forward.y;
    Transform::from_translation(position).looking_to(forward, Vec3::Y)
}

/// The core material extension (the one `main`'s converter writes): shader family, flags and the
/// auxiliary texture slots.
const OPEN_SKYRIM_MATERIAL_EXTENSION: &str = "OPEN_SKYRIM_material";
/// The data-set extension `mudcrab.material` (`crates/converter/src/material.rs`): the static UV
/// transform (`uvOffset`, `uvScale`), the effect falloff keys and the two `NiAlphaProperty` blend
/// factors under `blendSource` and `blendDestination` that glTF's own `BLEND` cannot express,
/// spelled as nif.xml's `AlphaFunction`. A mesh without it has identity UVs, no falloff and the
/// default blend.
const MUDCRAB_MATERIAL_EXTENSION: &str = "MUDCRAB_material";
/// A material's animated shader values (`crate::material_animation`).
const MATERIAL_ANIMATION_EXTENSION: &str = "OPEN_SKYRIM_material_animation";

/// A source or destination blend factor of `NiAlphaProperty` (nif.xml's `AlphaFunction`), which is
/// what the extension publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlendFactor {
    One,
    Zero,
    SourceColor,
    InverseSourceColor,
    DestinationColor,
    InverseDestinationColor,
    SourceAlpha,
    InverseSourceAlpha,
    DestinationAlpha,
    InverseDestinationAlpha,
    SourceAlphaSaturate,
}

impl BlendFactor {
    /// The factor a published name stands for, or `None` for a name this engine does not know.
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "ONE" => Self::One,
            "ZERO" => Self::Zero,
            "SRC_COLOR" => Self::SourceColor,
            "INV_SRC_COLOR" => Self::InverseSourceColor,
            "DEST_COLOR" => Self::DestinationColor,
            "INV_DEST_COLOR" => Self::InverseDestinationColor,
            "SRC_ALPHA" => Self::SourceAlpha,
            "INV_SRC_ALPHA" => Self::InverseSourceAlpha,
            "DEST_ALPHA" => Self::DestinationAlpha,
            "INV_DEST_ALPHA" => Self::InverseDestinationAlpha,
            "SRC_ALPHA_SATURATE" => Self::SourceAlphaSaturate,
            _ => return None,
        })
    }
}

/// What a pair of blend factors renders as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlendInterpretation {
    /// `SRC_ALPHA`/`ONE` and `ONE`/`ONE`: the surface's colour adds to the frame - the glow cards
    /// of torches, braziers and Dwemer lanterns.
    Additive,
    /// `DEST_COLOR`/`ZERO` and `ZERO`/`SRC_COLOR`: the surface multiplies the frame behind it.
    Multiplicative,
    /// `SRC_ALPHA`/`INV_SRC_ALPHA`, which is exactly what glTF `BLEND` already renders.
    StraightAlpha,
    /// A pair neither glTF nor Bevy's material has a mode for: the material stays alpha-over, and
    /// the pair is reported once.
    Unsupported,
}

impl BlendInterpretation {
    fn of(source: BlendFactor, destination: BlendFactor) -> Self {
        match (source, destination) {
            (BlendFactor::SourceAlpha, BlendFactor::One) | (BlendFactor::One, BlendFactor::One) => {
                Self::Additive
            }
            (BlendFactor::DestinationColor, BlendFactor::Zero)
            | (BlendFactor::Zero, BlendFactor::SourceColor) => Self::Multiplicative,
            (BlendFactor::SourceAlpha, BlendFactor::InverseSourceAlpha) => Self::StraightAlpha,
            _ => Self::Unsupported,
        }
    }

    fn alpha_mode(self) -> AlphaMode {
        match self {
            Self::Additive => AlphaMode::Add,
            Self::Multiplicative => AlphaMode::Multiply,
            Self::StraightAlpha | Self::Unsupported => AlphaMode::Blend,
        }
    }
}

/// The blend pairs already reported, so a model whose shapes share one unmatched pair reports it
/// once instead of once per material.
static UNMATCHED_BLEND_PAIRS: Mutex<BTreeSet<(String, String)>> = Mutex::new(BTreeSet::new());

fn report_unmatched_blend_pair(source: &str, destination: &str) {
    let mut reported = UNMATCHED_BLEND_PAIRS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if reported.insert((source.to_owned(), destination.to_owned())) {
        warn!(
            blend_source = %source,
            blend_destination = %destination,
            "the published `NiAlphaProperty` blend factors are not one this engine knows; the material keeps straight alpha-over"
        );
    }
}

/// The two blend factors a material's `MUDCRAB_material` extension publishes, as the converter
/// spelled them. The converter writes both together, and only for a material it published as glTF
/// `BLEND`, so a pair here always means a surface glTF's `BLEND` would draw wrong.
fn published_blend_pair(extension: Option<&serde_json::Value>) -> Option<(&str, &str)> {
    let extension = extension?;
    let source = extension.get("blendSource")?.as_str()?;
    let destination = extension.get("blendDestination")?.as_str()?;
    Some((source, destination))
}

/// The alpha mode a loaded material's blend pair asks for, or `None` for a material that publishes
/// no pair the engine acts on - no pair at all, a pair with a name it does not know, or a pair it
/// cannot express, both of which are reported once and left as alpha-over.
fn skyrim_alpha_mode(
    material: &bevy::gltf::gltf::Material,
    material_extensions: bool,
) -> Option<AlphaMode> {
    if !material_extensions {
        return None;
    }
    let (source, destination) =
        published_blend_pair(material.extension_value(MUDCRAB_MATERIAL_EXTENSION))?;
    let factors = BlendFactor::from_name(source).zip(BlendFactor::from_name(destination));
    let Some((source_factor, destination_factor)) = factors else {
        report_unmatched_blend_pair(source, destination);
        return None;
    };
    match BlendInterpretation::of(source_factor, destination_factor) {
        BlendInterpretation::Unsupported => {
            report_unmatched_blend_pair(source, destination);
            None
        }
        interpretation => Some(interpretation.alpha_mode()),
    }
}

/// The emissive a streamed material is published with: its colour channels at the engine's scale,
/// its alpha kept.
///
/// The alpha is not the glow's to scale. A `StandardMaterial`'s emissive carries an alpha that a
/// `Blend` surface uses as coverage, and `LinearRgba * f32` would scale it with the rest: an alpha
/// of `1.0` would come out at `EMISSIVE_EXPOSURE`, and one of `0.0` would stay black however bright
/// the scale is.
///
/// `app.rs` builds the canonical emissive fixture with this function too, so the fixture is what a
/// converted glow looks like rather than a magnitude of its own.
pub(crate) fn exposed_emissive(emissive: LinearRgba) -> LinearRgba {
    LinearRgba::new(
        emissive.red * EMISSIVE_EXPOSURE,
        emissive.green * EMISSIVE_EXPOSURE,
        emissive.blue * EMISSIVE_EXPOSURE,
        emissive.alpha,
    )
}

/// Whether a streamed material carries a *deliberate glow* - one this engine has to bring up to its
/// own lighting scale - or an emissive that is a surface's own brightness and must stay where the
/// converter put it.
///
/// The line is `KHR_materials_emissive_strength`, which the converter publishes only for a NIF whose
/// emissive multiple is above 1 (`crates/converter/src/material.rs`, `publish_emissive`), and the two
/// classes it separates behave completely differently in this engine:
///
/// * **Above 1: an emitter.** `blackreachgiantmushroom01`'s caps (2.0 to 3.6), the `BlackreachSun01`
///   orb (3.0), a torch's flame card (3.0). These read as small lights, and against an ambient of
///   650 to 800 - and light pools 10 times *that* (`crate::lights::LIGHT_EXPOSURE`) - a published emissive of 2 to
///   3.6 is invisible: nothing renders it (`docs/research/visual-gaps-spec.md`, gap 1).
/// * **Exactly 1: a self-lit surface.** `emissiveFactor [1, 1, 1]` with the glow slot holding the
///   model's own diffuse texture - Skyrim's `SLSF1_Own_Emit`, which the snow-covered trees, the ice
///   of the Alftand ravine and the landscape ice all carry. Skyrim uses it to keep a surface from
///   going dark where the light leaves it, it is already the brightness the game gives it, and it is
///   not this handler's to change.
///
/// Scaling the second class as well is not a smaller or larger version of the same fix: the first
/// attempt at this scaled both, and at 1000 the ice of the Alftand ravine rendered white and a
/// daylight reference pose went from 0.01 % to 45 % of its pixels clipped, while the value that
/// holds that guard leaves the emitters of Tamriel untouched.
/// The brightest a deliberate glow's channel is drawn at, after [`EMISSIVE_EXPOSURE`].
///
/// Past about 93 the tonemapper's lookup table has one cell for everything, so a glow whose channels
/// all exceed it is drawn the same white whatever its colour: Blackreach's cyan mushroom caps
/// (published `[0.42, 1.98, 2.0]`, x100) and hanging strands (`[2.26, 3.59, 3.6]`) burnt out
/// (research-172, look-gaps item 8). 45 keeps the brightest channel a cell below, the others in
/// proportion. Measured 2026-09-24: Blackreach's three shots 1.33 -> 1.14 (flat white 11.5 % ->
/// 6.4 % on the worst), the exterior holdout 0.740 -> 0.724, interiors unchanged (0.658 -> 0.662).
pub const GLOW_PEAK_CEILING: f32 = 45.0;

/// A glow scaled down, colour kept, so its brightest channel is at most [`GLOW_PEAK_CEILING`].
pub(crate) fn capped_glow(emissive: LinearRgba) -> LinearRgba {
    let peak = emissive.red.max(emissive.green).max(emissive.blue);
    if peak <= GLOW_PEAK_CEILING {
        return emissive;
    }
    let scale = GLOW_PEAK_CEILING / peak;
    LinearRgba::new(
        emissive.red * scale,
        emissive.green * scale,
        emissive.blue * scale,
        emissive.alpha,
    )
}

fn is_deliberate_glow(gltf_material: &bevy::gltf::gltf::Material) -> bool {
    gltf_material
        .emissive_strength()
        .is_some_and(|strength| strength > 1.0)
}

/// The material a streamed Skyrim material is published as: its blend pair's `AlphaMode` (when it
/// publishes one the engine can act on) and, for the deliberate glows, its emissive at the engine's
/// scale. `None` when the material is already what it should be, so nothing is republished for
/// nothing.
fn skyrim_material(
    gltf_material: &bevy::gltf::gltf::Material,
    material: &StandardMaterial,
    material_extensions: bool,
) -> Option<StandardMaterial> {
    // A material with no pair this engine acts on - no extension at all, or one it cannot express -
    // keeps the `AlphaMode` glTF's own `alphaMode` gave it: an additive glow card is the point of
    // the pair, and the wrong guess in the other direction would veil the world.
    let alpha_mode =
        skyrim_alpha_mode(gltf_material, material_extensions).unwrap_or(material.alpha_mode);
    let emissive = if is_deliberate_glow(gltf_material) {
        capped_glow(exposed_emissive(material.emissive))
    } else {
        material.emissive
    };
    if alpha_mode == material.alpha_mode && emissive == material.emissive {
        return None;
    }
    Some(StandardMaterial {
        alpha_mode,
        emissive,
        ..material.clone()
    })
}

/// Gives a streamed Skyrim material the `AlphaMode` its blend factors ask for and, for the
/// deliberate glows, the emissive scale the engine lights its world in.
///
/// Bevy's own PBR material handler publishes the loaded material at `"{material_label}/std"`, the
/// label the scene's meshes are then handed, so this handler - registered after that one - replaces
/// the value under that label and changes nothing else about it. Without the blend half, an
/// additive glow card (`SRC_ALPHA`/`ONE`: torch and Dwemer lantern glows) or a multiplicative
/// surface (`ZERO`/`SRC_COLOR`) draws as ordinary alpha-over, a grey veil over the world instead of
/// light.
///
/// It is registered for *every* streamed material, not only the ones with a blend pair: the
/// materials that carry most of the game's glow publish no pair at all (the Blackreach mushroom
/// caps are `OPAQUE` and `MASK`), so an early return on a missing pair would leave exactly those
/// invisible - which is what it did before this handler covered every material.
/// [`is_deliberate_glow`] is what decides which emitted values are that glow and which are a
/// surface's own brightness.
///
/// It also remembers the file's textures by glTF index, because an effect material's palette is a
/// texture no glTF material slot names: only the `OPEN_SKYRIM_material` extension points at it
/// ([`crate::effect_palette`]).
#[derive(Default, Clone)]
struct SkyrimMaterialHandler {
    material_extensions: bool,
    textures: Vec<Option<Handle<Image>>>,
    palettes: EffectPaletteRegistry,
    animations: Option<crate::material_animation::MaterialAnimationRegistry>,
}

impl GltfExtensionHandler for SkyrimMaterialHandler {
    fn dyn_clone(&self) -> Box<dyn ErasedGltfExtensionHandler> {
        Box::new(self.clone())
    }

    fn on_root(
        &mut self,
        _load_context: &mut LoadContext<'_>,
        _gltf: &bevy::gltf::gltf::Gltf,
        _settings: &bevy::gltf::GltfLoaderSettings,
    ) {
        // A new file: the texture indices of the last one mean nothing here.
        self.textures.clear();
    }

    fn on_texture(&mut self, gltf_texture: &bevy::gltf::gltf::Texture, texture: Handle<Image>) {
        let index = gltf_texture.index();
        if self.textures.len() <= index {
            self.textures.resize(index + 1, None);
        }
        self.textures[index] = Some(texture);
    }

    fn on_material(
        &mut self,
        load_context: &mut LoadContext<'_>,
        gltf_material: &bevy::gltf::gltf::Material,
        _material: Handle<GltfMaterial>,
        _material_asset: &GltfMaterial,
        material_label: &str,
    ) {
        let label = format!("{material_label}/std");
        // Reading the material Bevy built back out of the load context keeps every field it
        // filled in (textures, culling, alpha cutoff, unlit flag) and lets the mode and the
        // emissive be the only differences, which is what makes this safe to run over every
        // streamed material.
        let Some(material) = load_context
            .get_labeled(&label)
            .and_then(|asset| asset.get::<StandardMaterial>())
            .cloned()
        else {
            // Once per process: this only happens if Bevy's own material handler stops publishing
            // at this label, and then it happens for every streamed material.
            warn_once!(
                label = %label,
                "a streamed material has no loaded `StandardMaterial` published for it"
            );
            return;
        };
        if let Some(palette) = effect_palette_of(
            gltf_material,
            &material,
            &self.textures,
            self.material_extensions,
        ) {
            self.palettes
                .insert(format!("{}#{label}", load_context.path()), palette);
        }
        if let Some(registry) = &self.animations
            && let Some(animation) = material_animation_of(gltf_material, self.material_extensions)
        {
            registry.insert(format!("{}#{label}", load_context.path()), animation);
        }
        let Some(material) = skyrim_material(gltf_material, &material, self.material_extensions)
        else {
            return;
        };
        load_context.add_labeled_asset(label, material);
    }
}

/// The [`EffectPalette`] of a greyscale-to-palette effect material, or `None` for every other
/// material, and for one whose palette or source texture the file does not carry.
fn effect_palette_of(
    gltf_material: &bevy::gltf::gltf::Material,
    material: &StandardMaterial,
    textures: &[Option<Handle<Image>>],
    material_extensions: bool,
) -> Option<EffectPalette> {
    let extension = gltf_material.extension_value(OPEN_SKYRIM_MATERIAL_EXTENSION)?;
    let (palette_index, settings) = crate::effect_palette::palette_settings(
        extension,
        material_extensions
            .then(|| gltf_material.extension_value(MUDCRAB_MATERIAL_EXTENSION))
            .flatten(),
        gltf_material.emissive_factor(),
        gltf_material.emissive_strength().unwrap_or(1.0),
        gltf_material.pbr_metallic_roughness().base_color_factor()[3],
    )?;
    Some(EffectPalette {
        palette: textures.get(palette_index)?.clone()?,
        // The emissive texture is the source; Bevy leaves it out of a material whose emissive it
        // does not draw, and the converter publishes the same texture as the base colour's.
        source: material
            .emissive_texture
            .clone()
            .or_else(|| material.base_color_texture.clone())?,
        settings,
    })
}

/// Only a validated manifest capability allows the loader to consume material extension fields.
fn material_extensions_enabled(app: &App) -> bool {
    app.world()
        .get_resource::<crate::extensions::DataSetExtensions>()
        .is_some_and(|extensions| {
            extensions
                .status(crate::extensions::Feature::Material)
                .is_present()
        })
}

fn install_material_animation(app: &mut App) {
    if material_extensions_enabled(app) {
        app.add_plugins(crate::material_animation::MaterialAnimationPlugin);
    }
}

fn material_animation_of(
    material: &bevy::gltf::gltf::Material,
    material_extensions: bool,
) -> Option<crate::material_animation::MaterialAnimation> {
    if !material_extensions {
        return None;
    }
    material
        .extension_value(MATERIAL_ANIMATION_EXTENSION)
        .and_then(|animation| {
            crate::material_animation::MaterialAnimation::from_extensions(
                animation,
                material.extension_value(MUDCRAB_MATERIAL_EXTENSION),
            )
        })
        .filter(crate::material_animation::MaterialAnimation::plays_anything)
}

/// Registers the handler after Bevy's PBR handler, capturing the data-set capability for loader
/// tasks. Core material fields remain readable even when extension payloads are unusable.
fn register_skyrim_material_handler(app: &mut App) {
    let material_extensions = material_extensions_enabled(app);
    let palettes = EffectPaletteRegistry::default();
    app.insert_resource(palettes.clone());
    let animations = material_extensions.then(|| {
        let registry = crate::material_animation::MaterialAnimationRegistry::default();
        app.insert_resource(registry.clone());
        registry
    });
    let Some(handlers) = app.world().get_resource::<GltfExtensionHandlers>() else {
        warn!(
            "the glTF extension handlers are unavailable; additive and multiplicative Skyrim materials will render as alpha-over, and no streamed emissive will reach the engine's scale"
        );
        return;
    };
    handlers
        .0
        .write_blocking()
        .push(Box::new(SkyrimMaterialHandler {
            material_extensions,
            textures: Vec::new(),
            palettes,
            animations,
        }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_policy::ViewUpdateRate;
    use crate::world::cache::TerrainLayerSnapshot;

    #[test]
    fn quadrant_uvs_address_the_weight_grid_sample_for_sample() {
        let samples = QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES;
        // A `VTXT` list that names every sample with its own index, so the weight read at a vertex
        // says which sample the shader looked up.
        let terrain = terrain_fixture_with_overlays(
            0x0001_2345,
            &[(0..samples)
                .map(|sample| (sample as u16, sample as f32 / samples as f32))
                .collect()],
        );
        let last = (QUADRANT_WEIGHT_SAMPLES - 1) as f32;
        for quadrant in 0..4u8 {
            let mesh = crate::streaming::build_terrain_quadrant_mesh(&terrain, quadrant).unwrap();
            let grids = crate::streaming::quadrant_overlay_weights(&terrain, quadrant).unwrap();
            let settings = TerrainSettings::for_quadrant(quadrant, 2, &grids);
            let VertexAttributeValues::Float32x2(uvs) =
                mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
            else {
                panic!("terrain UVs must be Float32x2");
            };
            assert_eq!(uvs.len(), samples, "one vertex per grid sample");
            for (sample, uv) in uvs.iter().enumerate() {
                let coordinate = [
                    (uv[0] * 2.0 - settings.quadrant_origin.x) * last,
                    (uv[1] * 2.0 - settings.quadrant_origin.y) * last,
                ];
                let expected = sample as f32 / samples as f32;
                assert!(
                    (shader_weight(&settings, 0, [coordinate[0] / last, coordinate[1] / last])
                        - expected)
                        .abs()
                        < 1.0e-5,
                    "quadrant {quadrant} vertex {sample} reads {coordinate:?}, not its own sample"
                );
            }
        }
    }

    #[test]
    fn shader_weight_index_expressions_address_the_packed_field() {
        let source = include_str!("shaders/terrain.wgsl");
        let side = shader_constant(source, "WEIGHT_GRID_SIDE") as u32;
        let words = shader_constant(source, "WEIGHT_GRID_WORDS") as u32;
        // Every sample carries a value of its own, so an index that lands anywhere but on its own
        // sample reads a different opacity rather than a plausible one.
        let grid = |overlay: u32| -> Vec<f32> {
            (0..side * side)
                .map(|sample| (overlay * side * side + sample) as f32)
                .collect()
        };
        let grids: Vec<Vec<f32>> = (0..OVERLAY_WEIGHT_SLOTS as u32).map(grid).collect();
        let settings = TerrainSettings::for_quadrant(0, OVERLAY_WEIGHT_SLOTS + 1, &grids);

        // How `packed_weight` splits a sample between the word that holds it and the component of
        // that word.
        let packed = shader_function_body(source, "packed_weight");
        let field = packed
            .split_once("terrain.weights[")
            .expect("`packed_weight` must read the weight field")
            .1;
        let (word_of, rest) = field.split_once(']').expect("the word index must close");
        let (_, rest) = rest.split_once('[').expect("the component index must open");
        let component_of = rest.split_once(']').expect("the component must close").0;
        let (word_of, component_of) = (word_of.trim(), component_of.trim());

        // How `grid_weight` names the sample each corner of the coordinate reads, in the order the
        // bilinear blend takes them: west and east of the north row, then of the south row.
        let grid_weight = shader_function_body(source, "grid_weight");
        let corners: Vec<&str> = grid_weight
            .split("packed_weight(overlay, ")
            .skip(1)
            .map(|call| call.split(')').next().expect("a call must close"))
            .collect();
        assert_eq!(
            corners.len(),
            4,
            "the blend reads the four samples around the coordinate"
        );

        // The last row and column have no next sample - the shader clamps `next` there - so the
        // corners are evaluated only where all four neighbours are on the grid.
        for row in [0u32, 1, 7, 15] {
            for column in [0u32, 1, 7, 15] {
                let terms = [
                    ("west", column),
                    ("east", column + 1),
                    ("north", row),
                    ("south", row + 1),
                    ("WEIGHT_GRID_SIDE", side),
                ];
                let samples: Vec<u32> = corners
                    .iter()
                    .map(|corner| evaluate_u32(corner, &terms))
                    .collect();
                assert_eq!(
                    samples,
                    [
                        row * side + column,
                        row * side + column + 1,
                        (row + 1) * side + column,
                        (row + 1) * side + column + 1,
                    ],
                    "the corners of grid point ({column}, {row}) must be its own row and column"
                );
                // Three of the four corners, over three overlays: the field's first, middle and last
                // slots, so an index that reads another overlay's word fails too.
                let probes = [(0u32, samples[0]), (2, samples[3]), (4, samples[2])];
                for (overlay, sample) in probes {
                    let word_terms = [
                        ("overlay", overlay),
                        ("sample", sample),
                        ("WEIGHT_GRID_WORDS", words),
                    ];
                    let word = evaluate_u32(word_of, &word_terms) as usize;
                    let component = evaluate_u32(component_of, &[("sample", sample)]) as usize;
                    assert_eq!(
                        settings.weights[word][component],
                        (overlay * side * side + sample) as f32,
                        "overlay {overlay} sample {sample} must read the Rust side's value"
                    );
                }
            }
        }
    }

    #[test]
    fn reflection_camera_skips_a_cell_sized_plane_past_the_far_plane() {
        // The plane's bounding sphere reaches inside the far distance, but no part of the plane
        // does: its nearest edge, grown by the margin, is still 1,940 units ahead.
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -4_500.0), CELL_WATER_HALF_EXTENTS);
        assert!(!harness.frame().active);
    }

    #[test]
    fn terrain_reservation_observes_all_six_diffuse_and_normal_layers() {
        let mut images = Assets::<Image>::default();
        let handles: [Handle<Image>; 12] = std::array::from_fn(|_| images.add(Image::default()));
        let extension = TerrainExtension {
            layer_0: Some(handles[0].clone()),
            layer_1: Some(handles[1].clone()),
            layer_2: Some(handles[2].clone()),
            layer_3: Some(handles[3].clone()),
            layer_4: Some(handles[4].clone()),
            layer_5: Some(handles[5].clone()),
            normal_0: Some(handles[6].clone()),
            normal_1: Some(handles[7].clone()),
            normal_2: Some(handles[8].clone()),
            normal_3: Some(handles[9].clone()),
            normal_4: Some(handles[10].clone()),
            normal_5: Some(handles[11].clone()),
            ..default()
        };
        assert_eq!(
            extension.image_ids().collect::<Vec<_>>(),
            handles.iter().map(Handle::id).collect::<Vec<_>>()
        );
    }
    use crate::world::components::{StreamingCamera, WaterSurface};
    use bevy::app::Propagate;
    use bevy::camera::{primitives::CascadesFrusta, visibility::ViewVisibility};
    use bevy::image::ImageFilterMode;
    use bevy::light::update_directional_light_frusta;
    use bevy::mesh::VertexAttributeValues;
    use bevy::reflect::{PartialReflect, ReflectRef};

    const MAIN_CAMERA_FAR: f32 = 1000.0;

    /// Runs `setup_water_reflection` under the given command line and returns the app and the
    /// reflection camera it spawned.
    fn spawned_reflection_camera(args: &[&str]) -> (App, Entity) {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .insert_resource(crate::config::EngineConfig::run_from_args(
                args.iter().map(|arg| (*arg).to_owned()),
            ))
            .add_systems(Startup, setup_water_reflection);
        app.update();
        let camera = app
            .world_mut()
            .query_filtered::<Entity, With<WaterReflectionCamera>>()
            .single(app.world())
            .expect("one reflection camera");
        (app, camera)
    }

    fn reflection_target_size(app: &App) -> UVec2 {
        let texture = app.world().resource::<WaterReflectionTexture>();
        app.world()
            .resource::<Assets<Image>>()
            .get(&texture.0)
            .expect("the reflection target")
            .size()
    }

    #[test]
    fn the_default_reflection_camera_keeps_prepass_and_occlusion_and_a_full_target() {
        let (app, camera) = spawned_reflection_camera(&[]);
        let camera = app.world().entity(camera);
        assert!(camera.contains::<DepthPrepass>());
        assert!(camera.contains::<OcclusionCulling>());
        assert_eq!(reflection_target_size(&app), UVec2::new(1024, 576));
    }

    #[test]
    fn the_reflection_prepass_on_flag_preserves_the_prepass_and_occlusion() {
        let (app, camera) = spawned_reflection_camera(&["--reflection-prepass=on"]);
        let camera = app.world().entity(camera);
        assert!(camera.contains::<DepthPrepass>());
        assert!(camera.contains::<OcclusionCulling>());
    }

    #[test]
    fn reflection_flags_drop_the_occlusion_culling_or_the_whole_prepass() {
        let (app, camera) =
            spawned_reflection_camera(&["--reflection-prepass=on", "--reflection-occlusion=off"]);
        assert!(app.world().entity(camera).contains::<DepthPrepass>());
        assert!(!app.world().entity(camera).contains::<OcclusionCulling>());

        let (app, camera) = spawned_reflection_camera(&["--reflection-prepass=off"]);
        assert!(!app.world().entity(camera).contains::<DepthPrepass>());
        assert!(
            !app.world().entity(camera).contains::<OcclusionCulling>(),
            "occlusion culling needs the prepass"
        );
    }

    #[test]
    fn the_reflection_scale_shrinks_the_target() {
        let (app, _) = spawned_reflection_camera(&["--reflection-scale", "0.5"]);
        assert_eq!(reflection_target_size(&app), UVec2::new(512, 288));
    }

    fn four_cascades_for(views: &[Entity]) -> Cascades {
        Cascades {
            cascades: views
                .iter()
                .map(|view| (*view, vec![bevy::light::cascade::Cascade::default(); 4]))
                .collect(),
        }
    }

    /// The app the cascade-limit tests drive: the limiter registered from an `EngineConfig` the way
    /// `VercidiumRendererPlugin` registers it, Bevy's own `update_directional_light_frusta` in the
    /// set the limiter is ordered after (`bevy_light-0.19.0/src/lib.rs:187-188`), and a sun and a
    /// doorway sun (layer 2, out of the reflection's sight) over a main and a reflection view.
    ///
    /// Bevy's system is registered *after* the limiter, the way `VercidiumRendererPlugin` has it,
    /// so the frusta the tests read are the ones the ordering edge produces. Bevy's own update
    /// writes each light's whole frustum map from the cascade lists as they stand when it runs
    /// (`bevy_light-0.19.0/src/directional_light.rs:239-251`), so a limiter that ran before it
    /// would leave the reflection view with only the cascades it kept - which is what the frustum
    /// assertions below would see.
    struct CascadeLimitApp {
        app: App,
        main: Entity,
        reflection: Entity,
        sun: Entity,
        doorway_sun: Entity,
    }

    impl CascadeLimitApp {
        fn new(args: &[&str]) -> Self {
            let mut app = App::new();
            app.insert_resource(crate::config::EngineConfig::run_from_args(
                args.iter().map(|arg| (*arg).to_owned()),
            ));
            add_configured_water_reflection_cascade_limit(&mut app);
            app.add_systems(
                PostUpdate,
                update_directional_light_frusta.in_set(SimulationLightSystems::UpdateLightFrusta),
            );
            let main = app.world_mut().spawn_empty().id();
            let reflection = app
                .world_mut()
                .spawn((WaterReflectionCamera, RenderLayers::layer(0)))
                .id();
            let light = || {
                (
                    DirectionalLight {
                        // Without shadow maps the frusta are left alone
                        // (`bevy_light-0.19.0/src/directional_light.rs:235`).
                        shadow_maps_enabled: true,
                        ..default()
                    },
                    ViewVisibility::VISIBLE,
                    CascadesFrusta::default(),
                )
            };
            let sun = app
                .world_mut()
                .spawn((light(), four_cascades_for(&[main, reflection])))
                .id();
            let doorway_sun = app
                .world_mut()
                .spawn((
                    light(),
                    four_cascades_for(&[main, reflection]),
                    RenderLayers::layer(2),
                ))
                .id();
            app.update();
            Self {
                app,
                main,
                reflection,
                sun,
                doorway_sun,
            }
        }

        /// The cascades `light` lists for `view`.
        fn cascades(&self, light: Entity, view: Entity) -> usize {
            self.app.world().get::<Cascades>(light).unwrap().cascades[&view].len()
        }

        /// The frusta `light` built for `view`.
        fn frusta(&self, light: Entity, view: Entity) -> usize {
            self.app
                .world()
                .get::<CascadesFrusta>(light)
                .unwrap()
                .frusta[&view]
                .len()
        }
    }

    #[test]
    fn the_reflection_view_keeps_only_the_cascades_asked_for_and_every_frustum() {
        let run = CascadeLimitApp::new(&["--reflection-cascades", "1"]);

        assert_eq!(
            run.cascades(run.sun, run.reflection),
            1,
            "the sun keeps one cascade for the reflection"
        );
        assert_eq!(
            run.cascades(run.sun, run.main),
            4,
            "the main camera keeps every cascade"
        );
        assert_eq!(
            run.cascades(run.doorway_sun, run.reflection),
            0,
            "a light the reflection cannot see keeps none for it"
        );
        assert_eq!(run.cascades(run.doorway_sun, run.main), 4);

        // The frusta are left as Bevy's own update built them, one per cascade the sun has for the
        // view: this is the ordering `add_water_reflection_cascade_limit` buys, since Bevy writes
        // the whole map from the cascade lists as they stand when it runs. Shortening them would be
        // the panic `check_dir_light_mesh_visibility` takes: it sizes its thread-local queues to one
        // view's frustum count and indexes them by the next view's cascade index
        // (`bevy_light-0.19.0/src/lib.rs:403-407`, `:466-476`).
        assert_eq!(
            run.frusta(run.sun, run.reflection),
            4,
            "the reflection's frusta are left full"
        );
        assert_eq!(run.frusta(run.sun, run.main), 4);
        assert_eq!(
            run.frusta(run.doorway_sun, run.reflection),
            4,
            "a light kept out of the reflection keeps every frustum too"
        );
    }

    #[test]
    fn all_the_cascades_leaves_the_limiter_out_of_the_frame() {
        let run = CascadeLimitApp::new(&["--reflection-cascades", "all"]);
        assert_eq!(
            run.cascades(run.sun, run.reflection),
            4,
            "`all` keeps every cascade and installs no limiter"
        );
        assert_eq!(run.cascades(run.doorway_sun, run.reflection), 4);
        assert_eq!(run.frusta(run.sun, run.reflection), 4);
    }

    /// The mesh bounds of the water plane streaming.rs spawns: a `Plane3d` quad a cell wide with no
    /// thickness at all.
    const CELL_WATER_HALF_EXTENTS: Vec3 = Vec3::new(2048.0, 0.0, 2048.0);

    /// The observer and the reflection camera the adapter and the policy drive, with no renderer.
    /// The observer's pose, projection and the water surfaces are the whole input, so a headless
    /// `App` is enough to run its frames; the reflection camera's `is_active`, pose and image are
    /// the whole output.
    struct ReflectionHarness {
        app: App,
        main_camera: Entity,
        reflection_camera: Entity,
    }

    impl ReflectionHarness {
        fn new(main: Transform) -> Self {
            let mut app = App::new();
            app.init_resource::<ProfilingState>()
                .init_resource::<Assets<Image>>()
                .init_resource::<Time>()
                .insert_resource(crate::streaming::RenderOrigin(IVec2::ZERO))
                .insert_resource(crate::sky::CameraSpace::Exterior)
                .add_plugins(crate::view_policy::SecondaryViewPolicyPlugin)
                .add_systems(
                    PostUpdate,
                    prepare_water_reflection.in_set(ViewPolicySystems::Prepare),
                );
            let main_camera = app
                .world_mut()
                .spawn((
                    main,
                    GlobalTransform::from(main),
                    main_camera_projection(),
                    StreamingCamera,
                ))
                .id();
            let image =
                app.world_mut()
                    .resource_mut::<Assets<Image>>()
                    .add(Image::new_target_texture(
                        crate::view_policy::BASELINE_TARGET_SIZE.x,
                        crate::view_policy::BASELINE_TARGET_SIZE.y,
                        bevy::render::render_resource::TextureFormat::Rgba16Float,
                        None,
                    ));
            let reflection_camera = app
                .world_mut()
                .spawn((
                    Camera {
                        order: -1,
                        is_active: false,
                        ..default()
                    },
                    Transform::default(),
                    Projection::default(),
                    RenderTarget::Image(image.clone().into()),
                    ViewPolicy::baseline(),
                    crate::view_policy::ViewPolicyState::default(),
                    ViewDemand::default(),
                    SecondaryViewTarget {
                        image: image.clone(),
                        allocated: crate::view_policy::BASELINE_TARGET_SIZE,
                    },
                    WaterReflectionCamera,
                ))
                .id();
            app.insert_resource(WaterReflectionTexture(image));
            Self {
                app,
                main_camera,
                reflection_camera,
            }
        }

        /// Moves the observer, as the camera follow systems and the streaming fixture do: the
        /// `Transform` is the new pose and the `GlobalTransform` is left at the last one propagation
        /// wrote, which is what the adapter has to look past.
        fn aim(&mut self, main: Transform) {
            self.app
                .world_mut()
                .entity_mut(self.main_camera)
                .insert(main);
        }

        fn set_projection(&mut self, projection: Projection) {
            self.app
                .world_mut()
                .entity_mut(self.main_camera)
                .insert(projection);
        }

        fn set_policy(&mut self, policy: ViewPolicy) {
            self.app
                .world_mut()
                .entity_mut(self.reflection_camera)
                .insert(policy);
        }

        fn spawn_water(&mut self, translation: Vec3, half_extents: Vec3) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Transform::from_translation(translation),
                    GlobalTransform::from_translation(translation),
                    water_bounds(half_extents),
                    WaterSurface,
                ))
                .id()
        }

        /// Raises or lowers a surface, as a different water plane streaming in would.
        fn move_water(&mut self, water: Entity, height: f32) {
            self.app
                .world_mut()
                .entity_mut(water)
                .insert(Transform::from_translation(Vec3::new(0.0, height, -800.0)));
        }

        /// A water plane as a cell's role leaves it: on these layers, and drawn or hidden.
        fn spawn_water_on(&mut self, translation: Vec3, layers: RenderLayers, visible: bool) {
            self.app.world_mut().spawn((
                GlobalTransform::from(Transform::from_translation(translation)),
                water_bounds(CELL_WATER_HALF_EXTENTS),
                layers,
                if visible {
                    InheritedVisibility::VISIBLE
                } else {
                    InheritedVisibility::HIDDEN
                },
                WaterSurface,
            ));
        }

        /// The portal's camera, drawing this frame or not.
        fn spawn_portal_camera(&mut self, drawing: bool) {
            self.app.world_mut().spawn((
                Camera {
                    order: -2,
                    is_active: drawing,
                    ..default()
                },
                crate::portal::PortalCamera,
            ));
        }

        fn frame(&mut self) -> ReflectionFrame {
            self.app.update();
            let world = self.app.world();
            let camera = world.get::<Camera>(self.reflection_camera).unwrap();
            let transform = world.get::<Transform>(self.reflection_camera).unwrap();
            let projection = world.get::<Projection>(self.reflection_camera).unwrap();
            let render_target = world.get::<RenderTarget>(self.reflection_camera).unwrap();
            let RenderTarget::Image(target) = render_target else {
                panic!("the reflection camera renders into an image");
            };
            let image = world
                .resource::<Assets<Image>>()
                .get(&target.handle)
                .expect("the reflection image lives as long as the camera");
            ReflectionFrame {
                active: camera.is_active,
                transform: *transform,
                projection: projection.clone(),
                image: target.handle.clone(),
                image_size: image.size(),
            }
        }
    }

    struct ReflectionFrame {
        active: bool,
        transform: Transform,
        projection: Projection,
        image: Handle<Image>,
        image_size: UVec2,
    }

    fn main_camera_projection() -> Projection {
        Projection::Perspective(PerspectiveProjection {
            far: MAIN_CAMERA_FAR,
            ..default()
        })
    }

    /// The `Frustum` `update_frusta` derives from a camera's projection and pose.
    fn main_camera_frustum(main: &Transform) -> Frustum {
        let projection = main_camera_projection();
        let global = GlobalTransform::from(*main);
        let clip_from_world = projection.get_clip_from_view() * global.to_matrix().inverse();
        Frustum(ViewFrustum::from_clip_from_world(&clip_from_world))
    }

    /// A water plane's bounds in its own space, as `calculate_bounds` leaves them on the entity.
    fn water_bounds(half_extents: Vec3) -> Aabb {
        Aabb::from_min_max(-half_extents, half_extents)
    }

    #[test]
    fn a_spawned_camera_carries_the_components_the_gate_reads() {
        let mut app = App::new();
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), StreamingCamera))
            .id();
        let world = app.world();
        assert!(world.get::<Frustum>(camera).is_some());
        assert!(world.get::<Projection>(camera).is_some());
    }

    #[test]
    fn reflection_target_preserves_linear_hdr_until_the_main_view() {
        use bevy::camera::Hdr;
        use bevy::core_pipeline::tonemapping::Tonemapping;
        use bevy::render::render_resource::TextureFormat;

        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .add_systems(Startup, setup_water_reflection);
        app.update();
        let target = &app.world().resource::<WaterReflectionTexture>().0;
        let image = app.world().resource::<Assets<Image>>().get(target).unwrap();
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba16Float);
        assert!(image.texture_view_descriptor.is_none());
        let mut cameras = app
            .world_mut()
            .query_filtered::<(&Hdr, &Tonemapping, &Exposure), With<WaterReflectionCamera>>();
        let (_, tonemapping, exposure) = cameras.single(app.world()).unwrap();
        assert_eq!(*tonemapping, Tonemapping::None);
        assert_eq!(exposure.ev100, 9.7);
    }

    #[test]
    fn reflection_tracks_exposure_changes_even_while_not_visible() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        for ev100 in [7.0, 12.0, 9.7] {
            harness
                .app
                .world_mut()
                .entity_mut(harness.main_camera)
                .insert(Exposure { ev100 });
            assert!(!harness.frame().active);
            assert_eq!(
                harness
                    .app
                    .world()
                    .get::<Exposure>(harness.reflection_camera)
                    .unwrap()
                    .ev100,
                ev100
            );
        }
        harness
            .app
            .world_mut()
            .entity_mut(harness.main_camera)
            .remove::<Exposure>();
        harness.frame();
        assert_eq!(
            harness
                .app
                .world()
                .get::<Exposure>(harness.reflection_camera)
                .unwrap()
                .ev100,
            9.7
        );
    }

    #[test]
    fn stationary_reflection_does_not_mark_camera_state_changed() {
        #[derive(Resource, Default)]
        struct Changes {
            transform: bool,
            camera: bool,
            exposure: bool,
        }

        type ReflectionChanges<'w> = (Ref<'w, Transform>, Ref<'w, Camera>, Ref<'w, Exposure>);

        fn observe_changes(
            reflection: Query<ReflectionChanges<'_>, With<WaterReflectionCamera>>,
            mut changes: ResMut<Changes>,
        ) {
            let (transform, camera, exposure) = reflection.single().unwrap();
            *changes = Changes {
                transform: transform.is_changed(),
                camera: camera.is_changed(),
                exposure: exposure.is_changed(),
            };
        }

        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness
            .app
            .init_resource::<Changes>()
            .add_systems(Last, observe_changes);
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), Vec3::new(200.0, 0.0, 200.0));
        assert!(harness.frame().active);
        assert!(harness.frame().active);
        let changes = harness.app.world().resource::<Changes>();
        assert!(!changes.transform && !changes.camera && !changes.exposure);

        harness.aim(Transform::from_xyz(0.0, 160.0, 0.0));
        let frame = harness.frame();
        assert_eq!(frame.transform.translation.y, -80.0);
        let changes = harness.app.world().resource::<Changes>();
        assert!(changes.transform && !changes.camera && !changes.exposure);

        harness
            .app
            .world_mut()
            .entity_mut(harness.main_camera)
            .insert(Exposure { ev100: 7.0 });
        harness.frame();
        let changes = harness.app.world().resource::<Changes>();
        assert!(!changes.transform && !changes.camera && changes.exposure);

        harness.aim(Transform::from_xyz(0.0, 160.0, 0.0).looking_to(Vec3::Z, Vec3::Y));
        assert!(!harness.frame().active);
        assert!(harness.app.world().resource::<Changes>().camera);
        assert!(!harness.frame().active);
        let changes = harness.app.world().resource::<Changes>();
        assert!(!changes.transform && !changes.camera && !changes.exposure);
    }

    #[test]
    fn v2_reflection_restores_missing_exposure_and_updates_its_pose() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), CELL_WATER_HALF_EXTENTS);
        for exposure in [Some(Exposure { ev100: 7.0 }), None] {
            harness
                .app
                .world_mut()
                .entity_mut(harness.reflection_camera)
                .remove::<Exposure>();
            if let Some(exposure) = exposure {
                harness
                    .app
                    .world_mut()
                    .entity_mut(harness.main_camera)
                    .insert(exposure);
            } else {
                harness
                    .app
                    .world_mut()
                    .entity_mut(harness.main_camera)
                    .remove::<Exposure>();
            }
            let frame = harness.frame();
            assert!(frame.active);
            assert_eq!(frame.transform.translation.y, -40.0);
            assert_eq!(
                harness
                    .app
                    .world()
                    .get::<Exposure>(harness.reflection_camera)
                    .unwrap()
                    .ev100,
                exposure.map_or(9.7, |value| value.ev100)
            );
        }
    }

    #[test]
    fn reflection_camera_renders_while_a_water_plane_is_in_view() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), CELL_WATER_HALF_EXTENTS);
        assert!(harness.frame().active);
    }

    /// Inside a house with its door shut: the exterior behind the door is streamed in on the
    /// destination layer, but the portal camera is off, so nothing draws its river.
    #[test]
    fn reflection_camera_stays_off_for_destination_water_while_the_portal_is_off() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        let destination = RenderLayers::layer(crate::portal::DESTINATION_LAYER);
        harness.spawn_water_on(Vec3::new(0.0, 40.0, -800.0), destination.clone(), true);
        assert!(!harness.frame().active, "no portal camera at all");
        harness.spawn_portal_camera(false);
        assert!(
            !harness.frame().active,
            "a portal camera that is not drawing"
        );
    }

    /// The door opens: the portal draws the destination, and its water samples the reflection.
    #[test]
    fn reflection_camera_renders_destination_water_while_the_portal_draws() {
        // Behind the main camera on purpose: the destination is laid out for the portal's window,
        // so the main frustum must not decide it.
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        let destination = RenderLayers::layer(crate::portal::DESTINATION_LAYER);
        harness.spawn_water_on(Vec3::new(0.0, 40.0, 5_000.0), destination, true);
        harness.spawn_portal_camera(true);
        assert!(harness.frame().active);
    }

    /// A cell the portal hides (no layers, hidden) keeps neither the reflection on nor the mirror
    /// plane: the nearer hidden plane must not set the height the visible one is reflected at.
    #[test]
    fn hidden_water_neither_keeps_the_reflection_on_nor_fixes_the_mirror() {
        let main = Transform::from_xyz(0.0, 120.0, 0.0);
        let mut harness = ReflectionHarness::new(main);
        harness.spawn_water_on(Vec3::new(0.0, 100.0, -800.0), RenderLayers::none(), false);
        assert!(!harness.frame().active, "only hidden water");
        harness.spawn_water_on(Vec3::new(0.0, 40.0, -800.0), RenderLayers::layer(0), true);
        let frame = harness.frame();
        assert!(frame.active, "the visible plane is in view");
        assert_eq!(
            frame.transform,
            reflected_camera_transform(&GlobalTransform::from(main), 40.0),
            "mirrored in the visible plane, not the nearer hidden one"
        );
    }

    #[test]
    fn reflection_camera_skips_water_behind_the_main_camera() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, 5_000.0), Vec3::new(200.0, 0.0, 200.0));
        assert!(!harness.frame().active);
    }

    #[test]
    fn reflection_camera_skips_water_past_the_far_plane() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -4_000.0), Vec3::new(200.0, 0.0, 200.0));
        assert!(!harness.frame().active);
    }

    #[test]
    fn reflection_camera_renders_when_only_a_plane_edge_reaches_the_view() {
        // A cell-sized plane a full cell to the side: its centre is far outside the view, but the
        // near corner of the plane crosses into the frustum.
        let main = Transform::from_xyz(0.0, 0.0, 0.0);
        let mut harness = ReflectionHarness::new(main);
        let centre = Vec3::new(2048.0, 0.0, -700.0);
        harness.spawn_water(centre, CELL_WATER_HALF_EXTENTS);
        assert!(harness.frame().active);

        // Counted as a point at its centre, the same plane stays outside the frustum even with the
        // gate's margin, so only the plane's bounds can carry the test.
        assert!(!water_plane_in_view(
            &GlobalTransform::from(main),
            Some(&main_camera_frustum(&main)),
            Some(MAIN_CAMERA_FAR),
            &GlobalTransform::from_translation(centre),
            Some(&water_bounds(Vec3::ZERO)),
        ));
    }

    #[test]
    fn reflection_camera_stays_off_without_water() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        assert!(!harness.frame().active);
    }

    #[test]
    fn reflection_camera_is_mirrored_in_the_frame_it_becomes_active() {
        let facing_away = Transform::from_xyz(0.0, 300.0, 0.0).looking_to(Vec3::Z, Vec3::Y);
        let mut harness = ReflectionHarness::new(facing_away);
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), Vec3::new(200.0, 0.0, 200.0));
        assert!(!harness.frame().active);

        // Facing the water reopens the gate, and the mirror of that frame reflects the camera of
        // that frame, not the pose the gate closed at.
        harness.aim(Transform::from_xyz(0.0, 120.0, 0.0));
        let frame = harness.frame();
        assert!(frame.active);
        assert!((frame.transform.translation.y + 40.0).abs() < 1.0e-4);
        assert!(frame.transform.forward().z < 0.0);
    }

    #[test]
    fn reflection_camera_renders_until_a_plane_reports_its_bounds() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.app.world_mut().spawn((
            GlobalTransform::from(Transform::from_translation(Vec3::new(0.0, 40.0, -800.0))),
            WaterSurface,
        ));
        assert!(harness.frame().active);
    }

    #[test]
    fn reflection_mirrors_the_observer_pose_written_this_frame() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), CELL_WATER_HALF_EXTENTS);
        assert!((harness.frame().transform.translation.y + 40.0).abs() < 1.0e-4);

        // A teleport: the observer's `Transform` is the new pose, and the `GlobalTransform` left
        // behind by the last propagation is the old one. The mirror has to follow the new pose in
        // the frame it was written, or the teleport is reflected a frame late.
        harness.aim(Transform::from_xyz(0.0, 200.0, 0.0));
        let frame = harness.frame();
        assert!(frame.active);
        assert!((frame.transform.translation.y + 120.0).abs() < 1.0e-4);
    }

    #[test]
    fn the_spawned_reflection_camera_activates_when_water_is_in_view() {
        let mut app = App::new();
        app.init_resource::<ProfilingState>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Time>()
            .insert_resource(crate::streaming::RenderOrigin(IVec2::ZERO))
            .insert_resource(crate::sky::CameraSpace::Exterior)
            .add_plugins(crate::view_policy::SecondaryViewPolicyPlugin)
            .add_systems(Startup, setup_water_reflection)
            .add_systems(
                PostUpdate,
                prepare_water_reflection.in_set(ViewPolicySystems::Prepare),
            );
        let observer = Transform::from_xyz(0.0, 120.0, 0.0);
        app.world_mut().spawn((
            observer,
            GlobalTransform::from(observer),
            main_camera_projection(),
            StreamingCamera,
        ));
        let water = Vec3::new(0.0, 40.0, -800.0);
        app.world_mut().spawn((
            Transform::from_translation(water),
            GlobalTransform::from_translation(water),
            water_bounds(CELL_WATER_HALF_EXTENTS),
            WaterSurface,
        ));
        app.update();
        let world = app.world_mut();
        let mut cameras = world.query_filtered::<&Camera, With<WaterReflectionCamera>>();
        let camera = cameras.single(world).unwrap();
        assert!(
            camera.is_active,
            "the camera the real setup spawns is managed by the policy and renders with water in view"
        );
    }

    /// The spawned reflection camera switches between the shadowed and the unshadowed route as its
    /// policy says, and on neither route reaches the source sun and the copy at once, nor the
    /// water. (On this branch the placed objects share the world layer and are reflected, see
    /// [`PLACED_OBJECT_LAYER`].)
    #[test]
    fn the_spawned_reflection_camera_follows_its_policys_shadow_route() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .add_plugins(crate::shadow_route::ShadowRoutePlugin)
            .add_systems(Startup, setup_water_reflection);
        app.update();
        let camera = app
            .world_mut()
            .query_filtered::<Entity, With<WaterReflectionCamera>>()
            .single(app.world())
            .unwrap();
        let layers = |app: &App| {
            app.world()
                .entity(camera)
                .get::<RenderLayers>()
                .cloned()
                .unwrap()
        };
        let source_sun = RenderLayers::from_layers(&LIGHT_LAYERS);
        let replica = RenderLayers::layer(UNSHADOWED_REFLECTION_LAYER);

        assert_eq!(
            layers(&app),
            RenderLayers::from_layers(REFLECTION_VIEW_LAYERS),
            "the baseline policy keeps shadows on"
        );
        for shadows in [
            crate::view_policy::ViewShadows::Off,
            crate::view_policy::ViewShadows::On,
        ] {
            app.world_mut().entity_mut(camera).insert(ViewPolicy {
                shadows,
                ..ViewPolicy::baseline()
            });
            app.update();
            let layers = layers(&app);
            assert_ne!(
                layers.intersects(&source_sun),
                layers.intersects(&replica),
                "{shadows:?}: the camera is on exactly one sun's route"
            );
            assert!(!layers.intersects(&RenderLayers::layer(WATER_LAYER)));
            assert!(!layers.intersects(&RenderLayers::layer(crate::portal::DESTINATION_LAYER)));
            assert!(layers.intersects(&reflectable_world_layers()));
        }
    }

    fn spawned_preset_reflection_camera(preset: WaterReflectionPreset) -> (App, Entity) {
        spawned_reflection_camera(&["--water-reflection-preset", preset.name()])
    }

    #[test]
    fn the_baseline_preset_keeps_todays_reflection_camera() {
        let (app, camera) = spawned_preset_reflection_camera(WaterReflectionPreset::Baseline);
        let camera = app.world().entity(camera);
        assert_eq!(camera.get::<ViewPolicy>(), Some(&ViewPolicy::baseline()));
        assert!(camera.contains::<DepthPrepass>());
        assert!(camera.contains::<OcclusionCulling>());
        // The spawn sets no `Msaa`; it is not the same as "not multisampled". In the shipped app
        // `CameraPlugin` registers `Msaa` as a required component of `Camera`
        // (`bevy_render-0.19.1/src/camera.rs:65`), so this camera gets Bevy's default `Sample4`
        // (`bevy_render-0.19.1/src/view/mod.rs:240-246`) - a known cost that only
        // the water preset closes with an explicit `Msaa::Off`. A bare test app has no such plugin.
        assert_eq!(
            camera.get::<Msaa>(),
            None,
            "the baseline spawn adds no Msaa of its own, and the water preset does"
        );
        assert_eq!(
            camera.get::<RenderLayers>(),
            Some(&RenderLayers::from_layers(REFLECTION_VIEW_LAYERS))
        );
    }

    /// The reflection cost flags still shape the camera under a preset: the prepass brings
    /// occlusion culling back on the baseline camera, never on the throttled water one, and the
    /// scale applies to whichever size the preset starts from.
    #[test]
    fn the_reflection_flags_apply_on_top_of_the_preset() {
        let (app, camera) = spawned_reflection_camera(&[
            "--water-reflection-preset",
            "baseline",
            "--reflection-prepass=on",
        ]);
        let camera = app.world().entity(camera);
        assert!(camera.contains::<DepthPrepass>());
        assert!(camera.contains::<OcclusionCulling>());

        let (app, camera) = spawned_reflection_camera(&[
            "--water-reflection-preset",
            "water",
            "--reflection-prepass=on",
            "--reflection-scale",
            "0.5",
        ]);
        let half = crate::view_policy::WATER_TARGET_SIZE / 2;
        assert_eq!(reflection_target_size(&app), half);
        let camera = app.world().entity(camera);
        assert!(camera.contains::<DepthPrepass>());
        assert!(
            !camera.contains::<OcclusionCulling>(),
            "a throttled camera must not keep occlusion history"
        );
        assert_eq!(camera.get::<ViewPolicy>().unwrap().target_size, half);
    }

    /// The replica layer is a layer of its own on this branch too: not the water's, not the
    /// portal destination's and not the door layer's. Content carries it *beside* the world layer,
    /// not instead of it, so no view gains or loses a thing from either route's membership.
    #[test]
    fn the_replica_layer_is_no_other_layer() {
        for layer in [
            WORLD_LAYER,
            WATER_LAYER,
            crate::portal::DESTINATION_LAYER,
            crate::door_layer::DOOR_LAYER,
        ] {
            assert_ne!(UNSHADOWED_REFLECTION_LAYER, layer);
        }
        assert!(
            !RenderLayers::from_layers(&MAIN_VIEW_LAYERS)
                .intersects(&RenderLayers::layer(UNSHADOWED_REFLECTION_LAYER)),
            "the main camera is never on the replica layer"
        );
        let world = reflectable_world_layers();
        assert!(world.intersects(&RenderLayers::layer(WORLD_LAYER)));
        assert!(world.intersects(&RenderLayers::layer(UNSHADOWED_REFLECTION_LAYER)));
        // Widening is a union: what a node was already on stays on it.
        for own in [
            RenderLayers::layer(WORLD_LAYER),
            RenderLayers::from_layers(&MAIN_VIEW_LAYERS),
            crate::door_layer::door_layers(),
            RenderLayers::none(),
            RenderLayers::layer(WATER_LAYER),
            RenderLayers::layer(crate::portal::DESTINATION_LAYER),
        ] {
            let widened = reflectable_layers(&own);
            for layer in own.iter() {
                assert!(
                    widened.intersects(&RenderLayers::layer(layer)),
                    "widening dropped layer {layer} of {own:?}"
                );
            }
        }
    }

    /// The shadowless route draws the same content the shadowed one does - only without the sun's
    /// cascades. Every kind of content the world layer holds is here with whether a reflection draws
    /// it at all, and both routes have to agree: a route whose membership is smaller is a cheaper
    /// reflection bought by dropping content (impl-1094: the portal stamped its cells with the world
    /// layer alone, so the shadowless route drew terrain only).
    #[test]
    fn both_reflection_routes_draw_the_same_content() {
        let shadowed = RenderLayers::from_layers(REFLECTION_VIEW_LAYERS);
        let shadowless = RenderLayers::from_layers(UNSHADOWED_REFLECTION_VIEW_LAYERS);
        let producers: [(&str, RenderLayers, bool); 8] = [
            (
                "terrain and the plain nodes of a cell",
                reflectable_world_layers(),
                true,
            ),
            (
                "a reference's model on the placed-object layer",
                reflectable_layers(&RenderLayers::layer(PLACED_OBJECT_LAYER)),
                false,
            ),
            (
                "a node a mod put on the world layer",
                reflectable_layers(&RenderLayers::layer(WORLD_LAYER)),
                true,
            ),
            (
                "a light's own layers",
                RenderLayers::from_layers(&POINT_LIGHT_LAYERS),
                true,
            ),
            (
                "a sky dome on the main camera's layers",
                reflectable_layers(&RenderLayers::from_layers(&MAIN_VIEW_LAYERS)),
                true,
            ),
            (
                "the door layer's doors",
                crate::door_layer::door_layers(),
                true,
            ),
            ("a water surface", RenderLayers::layer(WATER_LAYER), false),
            (
                "the portal's destination space",
                RenderLayers::layer(crate::portal::DESTINATION_LAYER),
                false,
            ),
        ];
        for (what, layers, drawn) in producers {
            assert_eq!(
                layers.intersects(&shadowed),
                drawn,
                "the shadowed route and {what}: {layers:?}"
            );
            assert_eq!(
                layers.intersects(&shadowless),
                drawn,
                "the shadowless route and {what}: {layers:?} - the two routes must draw the same \
                 content"
            );
        }
        // Non-vacuous: the content a reflection is supposed to show is drawn by both, so the
        // agreement above is not two routes agreeing on nothing.
        assert!(reflection_draws_a_whole_world());
    }

    /// Whether the shadowed reflection view draws the world layer at all - the fixture check the
    /// parity test above needs, kept in one place so a change to [`REFLECTION_VIEW_LAYERS`] that
    /// emptied it would fail the test that leans on it rather than making it vacuous.
    fn reflection_draws_a_whole_world() -> bool {
        RenderLayers::from_layers(REFLECTION_VIEW_LAYERS).intersects(&reflectable_world_layers())
    }

    #[test]
    fn the_water_preset_drops_occlusion_history_and_multisampling() {
        let (app, camera) = spawned_preset_reflection_camera(WaterReflectionPreset::Water);
        let camera = app.world().entity(camera);
        assert_eq!(camera.get::<ViewPolicy>(), Some(&ViewPolicy::water()));
        assert!(
            !camera.contains::<OcclusionCulling>(),
            "a throttled camera must not keep occlusion history"
        );
        assert_eq!(camera.get::<Msaa>(), Some(&Msaa::Off));
        assert_eq!(
            camera.get::<RenderLayers>(),
            Some(&RenderLayers::from_layers(
                UNSHADOWED_REFLECTION_VIEW_LAYERS
            )),
            "the water preset starts on the shadowless route"
        );
        let target = app.world().resource::<WaterReflectionTexture>().0.clone();
        let image = app
            .world()
            .resource::<Assets<Image>>()
            .get(&target)
            .unwrap();
        assert_eq!(
            image.texture_descriptor.size.width,
            crate::view_policy::WATER_TARGET_SIZE.x
        );
    }

    #[test]
    fn the_reflection_off_switch_never_activates_the_camera() {
        let mut app = App::new();
        app.init_resource::<ProfilingState>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Time>()
            .insert_resource(crate::config::EngineConfig {
                water_reflection: false,
                ..default()
            })
            .insert_resource(crate::streaming::RenderOrigin(IVec2::ZERO))
            .insert_resource(crate::sky::CameraSpace::Exterior)
            .add_plugins(crate::view_policy::SecondaryViewPolicyPlugin)
            .add_systems(Startup, setup_water_reflection)
            .add_systems(
                PostUpdate,
                prepare_water_reflection.in_set(ViewPolicySystems::Prepare),
            );
        let observer = Transform::from_xyz(0.0, 120.0, 0.0);
        app.world_mut().spawn((
            observer,
            GlobalTransform::from(observer),
            main_camera_projection(),
            StreamingCamera,
        ));
        let water = Vec3::new(0.0, 40.0, -800.0);
        app.world_mut().spawn((
            Transform::from_translation(water),
            GlobalTransform::from_translation(water),
            water_bounds(CELL_WATER_HALF_EXTENTS),
            WaterSurface,
        ));
        for _ in 0..5 {
            app.update();
            let world = app.world_mut();
            let mut cameras = world.query_filtered::<&Camera, With<WaterReflectionCamera>>();
            assert!(
                !cameras.single(world).unwrap().is_active,
                "water is in view, but the reflection was switched off"
            );
        }
    }

    /// A spawned reflection camera, a main camera and a plane in front of it, run for a few frames
    /// under this configuration; every frame's `is_active` of the reflection camera.
    fn reflection_activity_under(config: crate::config::EngineConfig) -> Vec<bool> {
        let mut app = App::new();
        app.init_resource::<ProfilingState>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Time>()
            .insert_resource(config)
            .insert_resource(crate::streaming::RenderOrigin(IVec2::ZERO))
            .insert_resource(crate::sky::CameraSpace::Exterior)
            .add_plugins(crate::view_policy::SecondaryViewPolicyPlugin)
            .add_systems(Startup, setup_water_reflection);
        // As `VercidiumRendererPlugin` registers it: the hook decides whether the planar
        // preparation is in the schedule at all.
        let planar_replaced = portal_water_ssr_hook(&mut app);
        app.add_systems(
            PostUpdate,
            prepare_water_reflection
                .in_set(ViewPolicySystems::Prepare)
                .run_if(move || !planar_replaced),
        );
        let observer = Transform::from_xyz(0.0, 120.0, 0.0);
        app.world_mut().spawn((
            observer,
            GlobalTransform::from(observer),
            main_camera_projection(),
            StreamingCamera,
        ));
        let water = Vec3::new(0.0, 40.0, -800.0);
        app.world_mut().spawn((
            Transform::from_translation(water),
            GlobalTransform::from_translation(water),
            water_bounds(CELL_WATER_HALF_EXTENTS),
            WaterSurface,
        ));
        (0..5)
            .map(|_| {
                app.update();
                let world = app.world_mut();
                let mut cameras = world.query_filtered::<&Camera, With<WaterReflectionCamera>>();
                cameras.single(world).unwrap().is_active
            })
            .collect()
    }

    #[test]
    fn ssr_never_demands_or_activates_the_reflection_camera() {
        use crate::config::{EngineConfig, WaterReflectionMode};
        for preset in [
            WaterReflectionPreset::Baseline,
            WaterReflectionPreset::Water,
        ] {
            let activity = reflection_activity_under(EngineConfig {
                water_reflection_mode: WaterReflectionMode::Ssr,
                water_reflection_preset: preset,
                ..default()
            });
            assert_eq!(
                activity, [false; 5],
                "water is in view, but --water-reflection ssr marches the screen instead ({preset:?})"
            );
        }
    }

    #[test]
    fn planar_still_demands_the_reflection_camera_when_water_is_in_view() {
        use crate::config::{EngineConfig, WaterReflectionMode};
        let planar = reflection_activity_under(EngineConfig {
            water_reflection_mode: WaterReflectionMode::Planar,
            ..default()
        });
        assert!(planar.iter().all(|active| *active), "{planar:?}");
        // The Plus default is ssr, so a run with no flag never demands the reflection camera.
        let default_run = reflection_activity_under(EngineConfig::default());
        assert_eq!(default_run, [false; 5], "{default_run:?}");
    }

    #[test]
    fn the_portal_hook_hands_each_cameras_sky_to_the_water_module_only_under_ssr() {
        use crate::{
            config::{EngineConfig, WaterReflectionMode},
            sky::SkyPalette,
            sky_dome::CameraSky,
            water_ssr::WaterSsrSky,
        };
        for mode in [WaterReflectionMode::Ssr, WaterReflectionMode::Planar] {
            let mut app = App::new();
            app.insert_resource(EngineConfig {
                water_reflection_mode: mode,
                ..default()
            });
            let replaced = portal_water_ssr_hook(&mut app);
            assert_eq!(replaced, mode == WaterReflectionMode::Ssr);
            let palette = SkyPalette::SKYRIM_CLEAR_DAY;
            let sky = app
                .world_mut()
                .spawn(CameraSky {
                    palette: Some(palette),
                })
                .id();
            let none = app.world_mut().spawn(CameraSky { palette: None }).id();
            app.update();
            let fed = |entity| app.world().get::<WaterSsrSky>(entity).copied();
            if replaced {
                assert_eq!(
                    fed(sky),
                    Some(WaterSsrSky {
                        palette: Some(palette)
                    })
                );
                assert_eq!(fed(none), Some(WaterSsrSky { palette: None }));
            } else {
                assert_eq!(fed(sky), None, "planar hands the water module nothing");
            }
        }
    }

    #[test]
    fn ssr_leaves_the_reflection_camera_spawned_but_off() {
        // The camera, its image and its policy exist exactly as under planar (the material still
        // binds the image), so `--water-reflection-preset` is simply not consulted for a demand.
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .insert_resource(crate::config::EngineConfig {
                water_reflection_mode: crate::config::WaterReflectionMode::Ssr,
                ..default()
            });
        app.add_systems(Startup, setup_water_reflection);
        app.update();
        let world = app.world_mut();
        let mut cameras = world.query_filtered::<&Camera, With<WaterReflectionCamera>>();
        assert!(!cameras.single(world).unwrap().is_active);
        assert!(world.get_resource::<WaterReflectionTexture>().is_some());
    }

    #[test]
    fn reflection_renders_with_the_observer_projection() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), CELL_WATER_HALF_EXTENTS);
        let narrow = Projection::Perspective(PerspectiveProjection {
            fov: 0.5,
            far: 5_000.0,
            ..default()
        });
        harness.set_projection(narrow.clone());
        let frame = harness.frame();
        assert!(frame.active);
        let reflection_clip = frame.projection.get_clip_from_view();
        let observer_clip = narrow.get_clip_from_view();
        // Oblique water clipping changes only the depth row. Screen UV and perspective division
        // must still use the observer's projection, or the sampled reflection changes framing.
        for row in [0, 1, 3] {
            assert_eq!(reflection_clip.row(row), observer_clip.row(row));
        }
        assert_ne!(reflection_clip.row(2), observer_clip.row(2));
    }

    #[test]
    fn a_water_level_change_renders_a_skipped_frame_and_moves_the_mirror() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::EveryOtherFrame;
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.set_policy(policy);
        let water = harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), Vec3::new(200.0, 0.0, 200.0));
        assert!(
            harness.frame().active,
            "the first demanded frame has no schedule"
        );

        // The next frame is one the cadence would skip; the plane rises a unit, which is a change
        // neither the observer's pose nor anything else in the demand shows.
        harness.move_water(water, 41.0);
        let frame = harness.frame();
        assert!(frame.active);
        assert!((frame.transform.translation.y + 38.0).abs() < 1.0e-4);
    }

    #[test]
    fn the_reflection_image_keeps_its_handle_across_frames() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), CELL_WATER_HALF_EXTENTS);
        let shared = harness
            .app
            .world()
            .resource::<WaterReflectionTexture>()
            .0
            .clone();
        for _ in 0..3 {
            let frame = harness.frame();
            assert!(frame.active);
            assert_eq!(frame.image, shared, "the water materials hold this handle");
            assert_eq!(frame.image_size, crate::view_policy::BASELINE_TARGET_SIZE);
        }
    }

    #[test]
    fn reflection_camera_stops_rendering_when_the_observer_is_gone() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), CELL_WATER_HALF_EXTENTS);
        assert!(harness.frame().active);

        harness
            .app
            .world_mut()
            .entity_mut(harness.main_camera)
            .remove::<StreamingCamera>();
        assert!(!harness.frame().active, "nothing is left to mirror");
    }

    /// A cell whose four quadrants each carry a base layer and one overlay with the same `VTXT`
    /// list, so a weight measured in one quadrant can be compared with its neighbour's.
    fn overlay_fixture(cell_id: u32, overlay: Vec<(u16, f32)>) -> TerrainSnapshot {
        let layers = (0..4)
            .flat_map(|quadrant| {
                [
                    TerrainLayerSnapshot {
                        texture_form_id: 1,
                        quadrant,
                        layer: 0,
                        is_base: true,
                        weights: Vec::new(),
                    },
                    TerrainLayerSnapshot {
                        texture_form_id: 7,
                        quadrant,
                        layer: 1,
                        is_base: false,
                        weights: overlay.clone(),
                    },
                ]
            })
            .collect();
        TerrainSnapshot {
            cell_id,
            width: 33,
            height: 33,
            heights: vec![0.0; 33 * 33],
            normals: [0, 0, 127].repeat(33 * 33),
            vertex_colors: vec![255; 33 * 33 * 3],
            layers,
            water_height: None,
            water_type_form_id: None,
        }
    }

    /// The VTXT list of an overlay that has full weight on both of a quadrant's x edge columns and
    /// none in between, so the samples one column inside an edge differ from the edge itself.
    fn edge_columns_overlay() -> Vec<(u16, f32)> {
        (0..QUADRANT_WEIGHT_SAMPLES as u16)
            .flat_map(|y| [0u16, 16].map(move |x| (y * 17 + x, 1.0)))
            .collect()
    }

    /// `terrain.wgsl`'s cell uv to quadrant-local uv, for the quadrant at `origin`.
    fn quadrant_uv(cell_uv: [f32; 2], origin: [f32; 2]) -> [f32; 2] {
        [cell_uv[0] * 2.0 - origin[0], cell_uv[1] * 2.0 - origin[1]]
    }

    /// The weight a fragment receives, mirroring `grid_point` and `grid_weight` in `terrain.wgsl`:
    /// the quadrant-local coordinate clamped onto the 17x17 sample square, then read bilinearly,
    /// clamped at the quadrant's edge.
    fn shader_weight(settings: &TerrainSettings, overlay: usize, local_uv: [f32; 2]) -> f32 {
        let last = (QUADRANT_WEIGHT_SAMPLES - 1) as f32;
        let sample = |x: usize, y: usize| -> f32 {
            let index = y * QUADRANT_WEIGHT_SAMPLES + x;
            settings.weights[overlay * OVERLAY_WEIGHT_WORDS + index / 4][index % 4]
        };
        // `grid_point`: clamping before the fraction is taken keeps an edge sample on its own edge
        // instead of blending a neighbour in.
        let corner = [
            (local_uv[0] * last).clamp(0.0, last),
            (local_uv[1] * last).clamp(0.0, last),
        ];
        let blend = [corner[0] - corner[0].floor(), corner[1] - corner[1].floor()];
        let axis = |value: f32| -> (usize, usize) {
            let base = value as usize;
            (base, (base + 1).min(last as usize))
        };
        let (west, east) = axis(corner[0]);
        let (north, south) = axis(corner[1]);
        let top = sample(west, north) * (1.0 - blend[0]) + sample(east, north) * blend[0];
        let bottom = sample(west, south) * (1.0 - blend[0]) + sample(east, south) * blend[0];
        top * (1.0 - blend[1]) + bottom * blend[1]
    }

    /// The value of a `const NAME: u32 = <n>u;` declaration in `terrain.wgsl`.
    fn shader_constant(source: &str, name: &str) -> usize {
        let declaration = format!("const {name}: u32 = ");
        let rest = source
            .split_once(&declaration)
            .unwrap_or_else(|| panic!("terrain.wgsl declares no `{declaration}`"))
            .1;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits
            .parse()
            .unwrap_or_else(|_| panic!("`{declaration}` is not followed by a number: {rest}"))
    }

    /// The element count of a `field: array<vec4<f32>, <n>>` declaration in `terrain.wgsl`.
    fn shader_array_length(source: &str, field: &str) -> usize {
        let declaration = format!("{field}: array<vec4<f32>, ");
        let rest = source
            .split_once(&declaration)
            .unwrap_or_else(|| panic!("terrain.wgsl declares no `{declaration}`"))
            .1;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits
            .parse()
            .unwrap_or_else(|_| panic!("`{declaration}` is not followed by a number: {rest}"))
    }

    /// The field names of `struct NAME { .. }` in `terrain.wgsl`, in declaration order, so the
    /// uniform's layout can be compared with the Rust struct that is written into it.
    fn shader_struct_fields(source: &str, name: &str) -> Vec<String> {
        let body = source
            .split_once(&format!("struct {name} {{"))
            .unwrap_or_else(|| panic!("terrain.wgsl declares no `struct {name}`"))
            .1
            .split_once('}')
            .expect("the struct declaration must be closed")
            .0;
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .map(|line| {
                line.split_once(':')
                    .unwrap_or_else(|| panic!("`{line}` is not a field declaration"))
                    .0
                    .to_owned()
            })
            .collect()
    }

    /// The expression a `const NAME: u32 = <expression>;` declaration in `terrain.wgsl` holds, so a
    /// constant derived from another is read as the shader writes it.
    fn shader_constant_expression<'a>(source: &'a str, name: &str) -> &'a str {
        let declaration = format!("const {name}: u32 = ");
        source
            .split_once(&declaration)
            .unwrap_or_else(|| panic!("terrain.wgsl declares no `{declaration}`"))
            .1
            .split_once(';')
            .expect("a const declaration must end with a semicolon")
            .0
            .trim()
    }

    /// The body of `fn NAME(..) { <body> }` in `terrain.wgsl`, for the helpers whose whole body is
    /// the arithmetic under test.
    fn shader_function_body<'a>(source: &'a str, name: &str) -> &'a str {
        source
            .split_once(&format!("fn {name}("))
            .unwrap_or_else(|| panic!("terrain.wgsl declares no `fn {name}`"))
            .1
            .split_once('{')
            .expect("a function declaration must open its body")
            .1
            .split_once('}')
            .expect("a function body must be closed")
            .0
    }

    /// `source` without its whitespace, so a pinned expression survives a reformat of the file.
    fn without_whitespace(source: &str) -> String {
        source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    }

    /// The value of one of `terrain.wgsl`'s `u32` expressions over the named `terms`: `+`, `-`, `*`,
    /// `/`, `%`, parentheses, literals with their `u` suffix, and identifiers. This runs the
    /// shader's own arithmetic rather than a copy of it, which is the only way a test can notice a
    /// transposed index - a copy of the expression transposes with it.
    fn evaluate_u32(expression: &str, terms: &[(&str, u32)]) -> u32 {
        /// The expression left to read, and what the shader's identifiers stand for.
        struct Parser<'a> {
            characters: Vec<char>,
            position: usize,
            terms: &'a [(&'a str, u32)],
        }

        impl<'a> Parser<'a> {
            fn skip_whitespace(&mut self) {
                while self
                    .characters
                    .get(self.position)
                    .is_some_and(|character| character.is_whitespace())
                {
                    self.position += 1;
                }
            }

            /// Reads `expected` if it is next, whitespace aside.
            fn eat(&mut self, expected: char) -> bool {
                self.skip_whitespace();
                if self.characters.get(self.position) == Some(&expected) {
                    self.position += 1;
                    return true;
                }
                false
            }

            /// `<operand> (('*' | '/' | '%') <operand>)*`, left to right as WGSL binds it.
            fn product(&mut self) -> u32 {
                let mut value = self.operand();
                loop {
                    if self.eat('*') {
                        value *= self.operand();
                    } else if self.eat('/') {
                        value /= self.operand();
                    } else if self.eat('%') {
                        value %= self.operand();
                    } else {
                        return value;
                    }
                }
            }

            /// `<product> (('+' | '-') <product>)*`.
            fn sum(&mut self) -> u32 {
                let mut value = self.product();
                loop {
                    if self.eat('+') {
                        value += self.product();
                    } else if self.eat('-') {
                        value -= self.product();
                    } else {
                        return value;
                    }
                }
            }

            /// The whole expression, for the messages of the assertions below.
            fn text(&self) -> String {
                self.characters.iter().collect()
            }

            /// A literal, one of the named terms, or a parenthesised sum.
            fn operand(&mut self) -> u32 {
                self.skip_whitespace();
                if self.eat('(') {
                    let value = self.sum();
                    assert!(
                        self.eat(')'),
                        "`{}` must close its parentheses",
                        self.text()
                    );
                    return value;
                }
                let mut name = String::new();
                while let Some(&character) = self.characters.get(self.position) {
                    if !character.is_ascii_alphanumeric() && character != '_' {
                        break;
                    }
                    name.push(character);
                    self.position += 1;
                }
                // A literal may carry WGSL's `u` suffix, which says nothing about its value.
                if let Ok(literal) = name.trim_end_matches('u').parse::<u32>() {
                    return literal;
                }
                let (_, value) = self
                    .terms
                    .iter()
                    .find(|(term, _)| *term == name.as_str())
                    .unwrap_or_else(|| {
                        panic!(
                            "`{}` names `{name}`, which the test does not define",
                            self.text()
                        )
                    });
                *value
            }
        }

        let mut parser = Parser {
            characters: expression.chars().collect(),
            position: 0,
            terms,
        };
        let value = parser.sum();
        parser.skip_whitespace();
        assert_eq!(
            parser.position,
            parser.characters.len(),
            "`{expression}` has a suffix the test cannot evaluate"
        );
        value
    }

    #[test]
    fn reflects_camera_above_and_below_the_water_plane() {
        let above = GlobalTransform::from(
            Transform::from_xyz(2.0, 10.0, 4.0).looking_to(Vec3::new(0.0, -0.5, -1.0), Vec3::Y),
        );
        let reflected = reflected_camera_transform(&above, 3.0);
        assert!((reflected.translation.y + 4.0).abs() < 1.0e-5);
        assert!(reflected.forward().y > 0.0);

        let below = GlobalTransform::from(
            Transform::from_xyz(2.0, -4.0, 4.0).looking_to(Vec3::new(0.0, 0.5, -1.0), Vec3::Y),
        );
        let reflected = reflected_camera_transform(&below, 3.0);
        assert!((reflected.translation.y - 10.0).abs() < 1.0e-5);
        assert!(reflected.forward().y < 0.0);
    }

    /// Whether a world point is inside the reflection camera's clipped view (reverse-z, so a point
    /// in front of the near or oblique clip plane has `z > w`).
    fn in_reflection_view(lens: &PerspectiveProjection, camera: &Transform, point: Vec3) -> bool {
        use bevy::camera::CameraProjection;
        let view = camera.compute_affine().inverse().transform_point3(point);
        let clip = lens.get_clip_from_view() * view.extend(1.0);
        clip.w > 0.0 && clip.z >= 0.0 && clip.z <= clip.w
    }

    #[test]
    fn the_reflection_lens_keeps_what_is_above_the_water_and_drops_what_is_below() {
        let main = GlobalTransform::from(
            Transform::from_xyz(0.0, 10.0, 0.0).looking_to(Vec3::NEG_Z, Vec3::Y),
        );
        let water_y = 3.0;
        let reflection = reflected_camera_transform(&main, water_y);
        let lens = reflection_lens(&PerspectiveProjection::default(), &reflection, water_y);
        // The mirrored camera sits at y = -4 looking level; the points are straight ahead of it.
        assert!(in_reflection_view(
            &lens,
            &reflection,
            Vec3::new(0.0, 5.0, -20.0)
        ));
        assert!(!in_reflection_view(
            &lens,
            &reflection,
            Vec3::new(0.0, 1.0, -20.0)
        ));
        assert!(!in_reflection_view(
            &lens,
            &reflection,
            Vec3::new(0.0, -2.0, -20.0)
        ));
    }

    #[test]
    fn the_reflection_lens_for_a_camera_below_the_water_keeps_what_is_below_it() {
        let main = GlobalTransform::from(
            Transform::from_xyz(0.0, -4.0, 0.0).looking_to(Vec3::NEG_Z, Vec3::Y),
        );
        let water_y = 3.0;
        let reflection = reflected_camera_transform(&main, water_y);
        let lens = reflection_lens(&PerspectiveProjection::default(), &reflection, water_y);
        // The mirrored camera sits at y = 10 looking level: the world under the surface is kept
        // and the air above it is dropped.
        assert!(in_reflection_view(
            &lens,
            &reflection,
            Vec3::new(0.0, -2.0, -20.0)
        ));
        assert!(!in_reflection_view(
            &lens,
            &reflection,
            Vec3::new(0.0, 5.0, -20.0)
        ));
    }

    #[test]
    fn the_reflection_system_gives_the_reflection_camera_the_clipped_main_lens() {
        use bevy::camera::CameraProjection;
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 120.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 40.0, -800.0), CELL_WATER_HALF_EXTENTS);
        assert!(harness.frame().active);
        let Projection::Perspective(lens) = harness
            .app
            .world()
            .get::<Projection>(harness.reflection_camera)
            .unwrap()
        else {
            panic!("the reflection camera lost its perspective projection");
        };
        let Projection::Perspective(main_lens) = main_camera_projection() else {
            unreachable!();
        };
        assert_eq!(lens.far, main_lens.far);
        assert!(lens.near_clip_plane.xyz().length() > 0.99);
        assert!(lens.near_clip_plane.w < 0.0);
        let _ = lens.get_clip_from_view();
    }

    #[test]
    fn the_reflection_lens_keeps_the_main_cameras_field_of_view() {
        let main_lens = PerspectiveProjection {
            fov: 1.3,
            aspect_ratio: 4.0 / 3.0,
            near: 0.5,
            far: 5000.0,
            ..default()
        };
        let reflection = Transform::from_xyz(0.0, -4.0, 0.0);
        let lens = reflection_lens(&main_lens, &reflection, 3.0);
        assert_eq!(lens.fov, main_lens.fov);
        assert_eq!(lens.aspect_ratio, main_lens.aspect_ratio);
        assert_eq!(lens.far, main_lens.far);
    }

    /// The reflection pass is what costs 1.72 ms of a 4.41 ms frame at the rural cell, because it
    /// redraws every placed object; the camera must be spawned on the world layer alone. Asserting
    /// on the spawned camera rather than on the constant keeps the two from drifting apart.
    /// Stock full-detail objects and water stay outside the reflected LOD world. Seamless cells
    /// use their explicit role layers; they do not change the stock placed-object contract.
    #[test]
    fn the_reflection_camera_renders_the_world_layer_only() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .add_systems(Startup, setup_water_reflection);
        app.update();

        let mut cameras = app
            .world_mut()
            .query_filtered::<&RenderLayers, With<WaterReflectionCamera>>();
        let layers = cameras
            .single(app.world())
            .expect("the reflection camera must be spawned");
        assert!(layers.intersects(&RenderLayers::layer(WORLD_LAYER)));
        assert!(!layers.intersects(&RenderLayers::layer(WATER_LAYER)));
        assert!(!layers.intersects(&PLACED_OBJECT_RENDER_LAYERS));
    }

    /// The other half of the contract: the main camera is what still draws the placed objects and
    /// the water, so a new layer has to be added to its list and not only taken out of the
    /// reflection camera's.
    #[test]
    fn the_main_view_renders_every_layer() {
        let main_view = RenderLayers::from_layers(&MAIN_VIEW_LAYERS);
        assert!(main_view.intersects(&RenderLayers::layer(WORLD_LAYER)));
        assert!(main_view.intersects(&RenderLayers::layer(WATER_LAYER)));
        assert!(main_view.intersects(&PLACED_OBJECT_RENDER_LAYERS));
    }

    /// A light needs the world layer for both cameras and the placed-object layer for its shadow
    /// cascades: `check_dir_light_mesh_visibility` only collects casters that share a layer with
    /// the light.
    #[test]
    fn lights_cover_both_cameras_and_the_placed_objects() {
        let light = RenderLayers::from_layers(&POINT_LIGHT_LAYERS);
        for view in [
            MAIN_VIEW_LAYERS.as_slice(),
            REFLECTION_VIEW_LAYERS,
            UNSHADOWED_REFLECTION_VIEW_LAYERS,
        ] {
            assert!(
                light.intersects(&RenderLayers::from_layers(view)),
                "a light must reach {view:?}"
            );
        }
        assert!(light.intersects(&PLACED_OBJECT_RENDER_LAYERS));
        // The sun keeps its own contract: both shadowed views and the placed objects it shadows,
        // and never the replica layer, which is what keeps its cascades out of the shadowless view.
        let sun = RenderLayers::from_layers(&LIGHT_LAYERS);
        for view in [MAIN_VIEW_LAYERS.as_slice(), REFLECTION_VIEW_LAYERS] {
            assert!(
                sun.intersects(&RenderLayers::from_layers(view)),
                "the sun must reach {view:?}"
            );
        }
        assert!(sun.intersects(&PLACED_OBJECT_RENDER_LAYERS));
        assert!(!sun.intersects(&RenderLayers::from_layers(
            UNSHADOWED_REFLECTION_VIEW_LAYERS
        )));
    }

    /// A point light is a child of its placed-object reference, whose layer propagates down the
    /// hierarchy; the light must keep [`POINT_LIGHT_LAYERS`] anyway, or it stops lighting the terrain
    /// and the reflection view.
    #[test]
    fn a_light_below_a_reference_keeps_the_light_layers() {
        let mut app = App::new();
        add_placed_object_layer_propagation(&mut app);
        let reference = app
            .world_mut()
            .spawn(Propagate(PLACED_OBJECT_RENDER_LAYERS))
            .id();
        let light = app
            .world_mut()
            .spawn((light_render_layers(), ChildOf(reference)))
            .id();
        app.update();
        app.update();
        assert_eq!(
            app.world().entity(reference).get::<RenderLayers>(),
            Some(&PLACED_OBJECT_RENDER_LAYERS)
        );
        assert_eq!(
            app.world().entity(light).get::<RenderLayers>(),
            Some(&RenderLayers::from_layers(&POINT_LIGHT_LAYERS)),
            "the propagated placed-object layer must not replace the light's layers"
        );
    }

    /// The propagation has to be in place before visibility compares layers, and the meshes it
    /// carries arrive from the glb spawner in `SpawnScene` - between `Update` and `PostUpdate`. A
    /// descendant spawned after its reference must still inherit the layer in that same frame.
    #[test]
    fn a_descendant_spawned_after_its_reference_inherits_the_placed_object_layer() {
        let mut app = App::new();
        add_placed_object_layer_propagation(&mut app);
        let reference = app
            .world_mut()
            .spawn(Propagate(PLACED_OBJECT_RENDER_LAYERS))
            .id();
        app.update();
        // The glb is spawned frames after the reference: a scene root with the mesh primitives
        // below it.
        let scene_root = app.world_mut().spawn(ChildOf(reference)).id();
        let mesh = app
            .world_mut()
            .spawn((Mesh3d(Handle::default()), ChildOf(scene_root)))
            .id();

        app.update();

        for entity in [reference, scene_root, mesh] {
            assert_eq!(
                app.world().entity(entity).get::<RenderLayers>(),
                Some(&PLACED_OBJECT_RENDER_LAYERS),
                "a reference and everything below it must carry the placed-object layer"
            );
        }
    }

    /// The waves read the shared `globals.time_unwrapped`, so a frame in which only time advanced must not
    /// touch a water material: any write marks the asset modified, and Bevy then re-extracts and
    /// re-prepares its bind group, once per streamed water cell, every frame.
    #[test]
    fn advancing_time_leaves_water_materials_unmodified() {
        use bevy::time::TimeUpdateStrategy;
        use std::time::Duration;

        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 50.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 0.0, -500.0), CELL_WATER_HALF_EXTENTS);
        harness
            .app
            .add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                bevy::time::TimePlugin,
                AssetPlugin::default(),
            ))
            .init_asset::<WaterMaterial>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )));
        let handle = harness
            .app
            .world_mut()
            .resource_mut::<Assets<WaterMaterial>>()
            .add(WaterMaterial {
                base: StandardMaterial::default(),
                extension: WaterExtension::default(),
            });
        // Let the creation event and the first frames' time settle, then count from here.
        for _ in 0..3 {
            harness.frame();
        }
        harness
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<WaterMaterial>>>()
            .clear();
        let before = harness.app.world().resource::<Time>().elapsed_secs();
        for _ in 0..10 {
            harness.frame();
        }
        let world = harness.app.world_mut();
        assert!(
            world.resource::<Time>().elapsed_secs() > before,
            "the frames must have advanced time"
        );
        let events: Vec<_> = world
            .resource_mut::<Messages<AssetEvent<WaterMaterial>>>()
            .drain()
            .collect();
        assert!(
            events.is_empty(),
            "time advancing alone modified a water material: {events:?}"
        );
        assert!(
            world
                .resource::<Assets<WaterMaterial>>()
                .get(&handle)
                .is_some()
        );
    }

    /// An unchanged pose and active flag are not written, so the reflection camera is not marked
    /// changed on every still frame.
    #[test]
    fn a_still_view_does_not_rewrite_the_reflection_camera() {
        let mut harness = ReflectionHarness::new(Transform::from_xyz(0.0, 50.0, 0.0));
        harness.spawn_water(Vec3::new(0.0, 0.0, -500.0), CELL_WATER_HALF_EXTENTS);
        harness.frame();
        harness.frame();
        let ticks = |harness: &ReflectionHarness| {
            let entity = harness.app.world().entity(harness.reflection_camera);
            (
                entity.get_ref::<Transform>().unwrap().last_changed(),
                entity.get_ref::<Camera>().unwrap().last_changed(),
            )
        };
        let settled = ticks(&harness);
        harness.frame();
        harness.frame();
        assert_eq!(ticks(&harness), settled);
    }

    /// The water uniform is written in the Rust struct's declaration order and read in the WGSL
    /// struct's, so the two lists of fields must be the same, name for name. A reorder on one side
    /// only makes every water read another field's values (the clock arriving in the fresnel
    /// slot froze the waves and lost the reflection).
    #[test]
    fn water_settings_fields_match_the_shader_struct() {
        use bevy::reflect::structs::Struct;

        let source = include_str!("shaders/water.wgsl");
        let block = source
            .split("struct WaterSettings {")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("water.wgsl must declare struct WaterSettings");
        let shader_fields: Vec<&str> = block
            .lines()
            .map(|line| line.split("//").next().unwrap_or("").trim())
            .filter(|line| !line.is_empty())
            .map(|line| line.split(':').next().unwrap_or("").trim())
            .collect();
        let settings = WaterExtension::default().settings;
        let rust_fields: Vec<&str> = (0..settings.field_len())
            .map(|index| settings.name_at(index).expect("field name"))
            .collect();
        assert_eq!(rust_fields, shader_fields);
    }

    /// Bevy declares the view's fog binding only for a view that has `DistanceFog`, and a measured
    /// run installs no atmosphere, so its main camera has none. The screen-space reflection water
    /// must compile both ways: a water pipeline that fails never draws, so the world never counts
    /// as ready (the fly-fast route waited 400 s at its first waypoint, 2026-10-20).
    #[test]
    fn the_ssr_water_shader_compiles_with_and_without_fog() {
        use crate::far_terrain_shadows::{bindings_audit::add, lighting_tests};
        use naga_oil::compose::{
            Composer, NagaModuleDescriptor, ShaderDefValue, get_preprocessor_data,
        };

        let source = include_str!("shaders/water.wgsl");
        let sources = lighting_tests::sources();
        for fog in [true, false] {
            let mut defs = lighting_tests::defs(false, 4, &["WATER_SSR", "DEPTH_PREPASS"]);
            defs.insert("WATER_SSR_LINEAR_STEPS".into(), ShaderDefValue::UInt(24));
            defs.insert("WATER_SSR_BISECTION_STEPS".into(), ShaderDefValue::UInt(4));
            if !fog {
                defs.remove("DISTANCE_FOG");
            }
            let mut composer =
                Composer::default().with_capabilities(wgpu::naga::valid::Capabilities::all());
            for import in get_preprocessor_data(source).1 {
                add(&import.import, &sources, &mut composer, &defs);
            }
            composer
                .make_naga_module(NagaModuleDescriptor {
                    source,
                    file_path: "engine/shaders/water.wgsl",
                    shader_defs: defs,
                    ..Default::default()
                })
                .unwrap_or_else(|error| panic!("fog {fog}: {}", error.emit_to_string(&composer)));
        }
    }

    /// A pinned clock gives a reproducible still, and the shared clock is the default.
    #[test]
    fn only_a_pinned_water_clock_reads_the_material_time() {
        let shared = WaterExtension::default();
        assert_eq!(shared.settings.clock.y, 0.0);
        let pinned = WaterExtension::default().with_pinned_clock(1.0);
        assert_eq!(pinned.settings.clock.x, 1.0);
        assert_eq!(pinned.settings.clock.y, 1.0);
        assert_eq!(
            pinned.settings.flow_direction,
            shared.settings.flow_direction
        );
        assert_eq!(
            pinned.settings.wave_scale_speed_strength,
            shared.settings.wave_scale_speed_strength
        );
    }

    fn water_settings_app() -> App {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), AssetPlugin::default()))
            .init_asset::<WaterMaterial>()
            .init_resource::<crate::settings::RenderingSettings>()
            .add_systems(Update, apply_water_settings);
        app
    }

    fn water_shots_run() -> crate::shots::ShotsRun {
        crate::shots::ShotsRun::new(
            crate::shots::ShotsFile {
                width: 64,
                height: 64,
                shots: Vec::new(),
            },
            std::path::PathBuf::from("unused-shots-output"),
            0,
        )
    }

    fn add_test_water(app: &mut App, extension: WaterExtension) -> Handle<WaterMaterial> {
        app.world_mut()
            .resource_mut::<Assets<WaterMaterial>>()
            .add(WaterMaterial {
                base: StandardMaterial::default(),
                extension,
            })
    }

    #[test]
    fn static_shots_pin_existing_and_newly_arriving_water_materials() {
        let mut app = water_settings_app();
        app.insert_resource(water_shots_run());
        let mut handles = vec![
            add_test_water(&mut app, WaterExtension::default()),
            add_test_water(&mut app, WaterExtension::default().with_pinned_clock(7.0)),
        ];
        app.update();
        for handle in &handles {
            let water = app
                .world()
                .resource::<Assets<WaterMaterial>>()
                .get(handle)
                .unwrap();
            assert_eq!(
                water.extension.settings.clock,
                Vec4::new(1.0, 1.0, 0.0, 0.0)
            );
        }
        handles.push(add_test_water(&mut app, WaterExtension::default()));
        app.update();
        for handle in &handles {
            let water = app
                .world()
                .resource::<Assets<WaterMaterial>>()
                .get(handle)
                .unwrap();
            assert_eq!(
                water.extension.settings.clock,
                Vec4::new(1.0, 1.0, 0.0, 0.0)
            );
        }
    }

    #[test]
    fn static_shots_do_not_modify_water_materials_on_repeated_frames() {
        let mut app = water_settings_app();
        app.insert_resource(water_shots_run());
        let handle = add_test_water(&mut app, WaterExtension::default());
        // Flush creation and first-update events before checking steady frames.
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<Assets<WaterMaterial>>()
                .get(&handle)
                .unwrap()
                .extension
                .settings
                .clock,
            Vec4::new(1.0, 1.0, 0.0, 0.0)
        );
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<WaterMaterial>>>()
            .clear();
        for _ in 0..3 {
            app.update();
            let events: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<AssetEvent<WaterMaterial>>>()
                .drain()
                .collect();
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, AssetEvent::Modified { .. })),
                "unchanged static-shot water was modified: {events:?}"
            );
        }
    }

    #[test]
    fn settled_water_frames_inspect_no_material() {
        let mut app = water_settings_app();
        app.init_resource::<WaterSettingsVisits>();
        app.insert_resource(water_shots_run());
        for _ in 0..3 {
            add_test_water(&mut app, WaterExtension::default());
        }
        // The first frame applies everything; the next ones consume the asset events, including
        // the echo of its own writes.
        for _ in 0..4 {
            app.update();
        }
        let settled = app.world().resource::<WaterSettingsVisits>().0;
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<WaterSettingsVisits>().0,
            settled,
            "a settled frame inspected water materials"
        );
        let arriving = add_test_water(&mut app, WaterExtension::default());
        app.update();
        assert_eq!(
            app.world()
                .resource::<Assets<WaterMaterial>>()
                .get(&arriving)
                .unwrap()
                .extension
                .settings
                .clock,
            Vec4::new(1.0, 1.0, 0.0, 0.0),
            "arriving water still gets the pinned clock"
        );
        for _ in 0..3 {
            app.update();
        }
        let after_arrival = app.world().resource::<WaterSettingsVisits>().0;
        assert!(
            after_arrival <= settled + 2,
            "an arrival inspects only itself"
        );
        app.world_mut()
            .resource_mut::<crate::settings::RenderingSettings>()
            .wave_speed = 0.25;
        app.update();
        assert_eq!(
            app.world().resource::<WaterSettingsVisits>().0,
            after_arrival + 4,
            "a settings change re-applies every water material once"
        );
    }

    #[test]
    fn ordinary_and_route_water_keep_shared_and_authored_clocks() {
        for route in [false, true] {
            let mut app = water_settings_app();
            if route {
                let mut run = water_shots_run();
                run.route_checkpoints = Some(BTreeSet::from([0]));
                app.insert_resource(run);
            }
            let shared = add_test_water(&mut app, WaterExtension::default());
            let authored =
                add_test_water(&mut app, WaterExtension::default().with_pinned_clock(7.0));
            // Force a real settings update, so preservation is not an early-return artifact.
            app.world_mut()
                .resource_mut::<crate::settings::RenderingSettings>()
                .wave_speed = 0.25;
            for _ in 0..2 {
                app.update();
                let materials = app.world().resource::<Assets<WaterMaterial>>();
                assert_eq!(
                    materials.get(&shared).unwrap().extension.settings.clock,
                    Vec4::ZERO
                );
                assert_eq!(
                    materials.get(&authored).unwrap().extension.settings.clock,
                    Vec4::new(7.0, 1.0, 0.0, 0.0)
                );
                for handle in [&shared, &authored] {
                    assert_eq!(
                        materials
                            .get(handle)
                            .unwrap()
                            .extension
                            .settings
                            .wave_scale_speed_strength
                            .y,
                        0.25
                    );
                }
            }
        }
    }

    #[test]
    fn animated_water_advances_the_shader_phase() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Assets<WaterMaterial>>()
            .add_message::<AssetEvent<WaterMaterial>>()
            .add_systems(Update, apply_water_settings);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WaterMaterial>>()
            .add(WaterMaterial {
                base: StandardMaterial::default(),
                extension: WaterExtension::default(),
            });
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(2));
        app.update();
        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
        // The shared nonwrapping clock advances; material time stays untouched.
        assert_eq!(app.world().resource::<Time>().elapsed_secs_wrapped(), 2.0);
        assert_eq!(material.extension.settings.clock.y, 0.0);
        assert_eq!(material.extension.settings.wave_scale_speed_strength.w, 0.0);
        let shader = include_str!("shaders/water.wgsl");
        assert!(
            shader.contains("select(globals.time_unwrapped, water.clock.x, water.clock.y > 0.5)")
        );
        assert!(shader.contains("let phase = time * water.wave_scale_speed_strength.y;"));
    }

    #[test]
    fn final_renderer_requires_every_gpu_path_signal() {
        let complete = RendererMetrics {
            gpu_preprocessing_active: true,
            gpu_culling_active: true,
            indirect_drawing_active: true,
            occlusion_culling_views: 1,
            hzb_views: 1,
            indirect_phase_buffers: 1,
            indirect_batch_sets: 1,
            proof_frames: 1,
            ..default()
        };
        assert!(complete.final_path_active());
        assert!(
            !RendererMetrics {
                hzb_views: 0,
                ..complete.clone()
            }
            .final_path_active()
        );
        assert!(
            !RendererMetrics {
                renderer_validation_failures: 1,
                ..complete
            }
            .final_path_active()
        );
    }

    /// The shader tiles every layer by the shared LAND repeats per cell, so the layer textures
    /// must be sampled with a repeating address mode. Bevy's default clamps to the edge, which
    /// stretched the textures in the frames: everything past the first tile read the edge texels.
    #[test]
    fn failed_terrain_normal_removes_only_its_bindings_and_shader_flags() {
        let mut images = Assets::<Image>::default();
        let failed = images.add(Image::default());
        let valid = images.add(Image::default());
        let mut extension = TerrainExtension {
            normal_0: Some(failed.clone()),
            normal_1: Some(valid.clone()),
            normal_4: Some(failed.clone()),
            ..default()
        };
        extension.remove_normal(failed.id());
        assert!(extension.normal_0.is_none());
        assert!(extension.normal_4.is_none());
        assert_eq!(extension.normal_1, Some(valid));
        assert_eq!(extension.settings.normal_layers_0, Vec4::Y);
        assert_eq!(extension.settings.normal_layers_1, Vec4::ZERO);
        assert!(!extension.image_ids().any(|id| id == failed.id()));
    }

    #[test]
    fn authored_terrain_normal_bindings_and_flags_are_preserved() {
        let mut extension = TerrainExtension::default();
        let normals: [Option<Handle<Image>>; 6] =
            std::array::from_fn(|index| (index % 2 == 0).then(Handle::default));
        extension.settings.set_normal_layers(&normals);
        [extension.normal_0, extension.normal_2, extension.normal_4] =
            [normals[0].clone(), normals[2].clone(), normals[4].clone()];
        let copy = extension.clone();
        assert!(copy.has_normal_maps());
        assert_eq!(
            [
                copy.normal_0,
                copy.normal_1,
                copy.normal_2,
                copy.normal_3,
                copy.normal_4,
                copy.normal_5
            ],
            normals
        );
    }

    #[test]
    fn tiled_terrain_layers_are_sampled_with_a_repeating_sampler() {
        let sampler = terrain_layer_sampler();
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_w, ImageAddressMode::Repeat);
        // Repeat and 16x anisotropy retain linear filtering and the default mip range.
        let default_sampler = ImageSamplerDescriptor::linear();
        assert_eq!(sampler.mag_filter, default_sampler.mag_filter);
        assert_eq!(sampler.min_filter, default_sampler.min_filter);
        assert_eq!(sampler.mipmap_filter, default_sampler.mipmap_filter);
        assert_eq!(sampler.lod_min_clamp, default_sampler.lod_min_clamp);
        assert_eq!(sampler.lod_max_clamp, default_sampler.lod_max_clamp);
        assert_eq!(sampler.anisotropy_clamp, 16);
        assert_eq!(sampler.compare, default_sampler.compare);
        assert_eq!(sampler.mag_filter, ImageFilterMode::Linear);
        assert_eq!(
            default_sampler.address_mode_u,
            ImageAddressMode::ClampToEdge
        );
        assert_eq!(
            default_sampler.address_mode_v,
            ImageAddressMode::ClampToEdge
        );
        assert!(
            TerrainSettings::vertex_weights_only(1.0)
                .tiling_and_layer_count
                .x
                > 1.0,
            "layers only need a repeating sampler because the shader tiles them"
        );
    }

    /// Zero overlays keep material slots and weights without image requests; a nonzero LTEX
    /// with no diffuse image still fails without preventing a subsequent placeholder build.
    #[test]
    fn terrain_zero_texture_overlay_material_preserves_slots_without_image_requests() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("placeholder-catalogue.db");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT);
                 CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER);
                 CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT);
                 INSERT INTO texture_sets VALUES(1,'textures/zero-must-not-load.dds');
                 INSERT INTO landscape_textures VALUES(0,1),(2116,NULL);",
            )
            .unwrap();
        drop(connection);
        let catalog = AssetCatalog::open(&path).unwrap();
        assert!(
            catalog.landscape_diffuse(0).is_some(),
            "poison null lookup is observable"
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>();
        let asset_server = app.world().resource::<AssetServer>();
        let mut terrain =
            terrain_fixture_with_overlays(91, &[vec![(0, 0.25), (18, 0.75)], vec![(3, 0.625)]]);
        for layer in &mut terrain.layers {
            layer.texture_form_id = 0;
            if !layer.is_base && layer.layer == 2 {
                layer.layer = 4;
            }
        }
        let (extension, images) =
            TerrainExtension::from_quadrant(&terrain, 0, &catalog, asset_server)
                .expect("zero overlays keep material slots and do not request a texture");
        assert!(images.color.is_empty() && images.normal.is_empty());
        assert!(
            extension.layer_0.is_none()
                && extension.layer_1.is_none()
                && extension.layer_2.is_none()
        );
        assert!(
            extension.layer_3.is_none()
                && extension.layer_4.is_none()
                && extension.layer_5.is_none()
        );
        assert_eq!(extension.settings.tiling_and_layer_count.z, 3.0);
        assert_eq!(extension.settings.normal_layers_0, Vec4::ZERO);
        assert_eq!(extension.settings.normal_layers_1, Vec4::ZERO);
        let sample = |overlay: usize, vertex: usize| {
            extension.settings.weights[overlay * OVERLAY_WEIGHT_WORDS + vertex / 4][vertex % 4]
        };
        assert_eq!(sample(0, 0).to_bits(), 0.25f32.to_bits());
        assert_eq!(sample(0, 18).to_bits(), 0.75f32.to_bits());
        assert_eq!(sample(1, 3).to_bits(), 0.625f32.to_bits());
        assert_eq!(sample(1, 0), 0.0);

        terrain.layers[2].texture_form_id = 0x844;
        let error = TerrainExtension::from_quadrant(&terrain, 0, &catalog, asset_server)
            .err()
            .expect("a nonzero LTEX without a diffuse image still fails");
        assert!(error.contains("00000844") && error.contains("no diffuse image"));
        terrain.layers[2].texture_form_id = 0;
        assert!(TerrainExtension::from_quadrant(&terrain, 0, &catalog, asset_server).is_ok());
    }

    /// The weight field is the quadrant's own `17x17` grids, one per overlay, in the order
    /// `quadrant_layers` returns them: an overlay's `VTXT` list reaches the fragment stage at the
    /// samples it names, and every sample it does not name reads as opacity 0.
    #[test]
    fn weight_field_carries_every_overlay_grid_from_the_layer_list() {
        let terrain = terrain_fixture_with_overlays(
            0x0001_2345,
            &[
                vec![(0, 1.0), (17 + 1, 0.5), (16 * 17 + 16, 0.25)],
                vec![(17 * 8 + 8, 0.75)],
            ],
        );
        let grids = crate::streaming::quadrant_overlay_weights(&terrain, 0).unwrap();
        assert_eq!(grids.len(), 2, "one grid per overlay, base excluded");
        assert_eq!(
            grids[0].len(),
            QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES,
            "a grid covers the quadrant's whole sample square"
        );
        let settings = TerrainSettings::for_quadrant(0, 3, &grids);
        assert_eq!(
            settings.tiling_and_layer_count,
            Vec4::new(24.0, 24.0, 3.0, 0.0)
        );
        assert_eq!(settings.quadrant_origin.xy(), Vec2::ZERO);
        assert_eq!(
            settings.weight_source.x, 1.0,
            "the material carries the weight field"
        );

        let sample = |overlay: usize, x: usize, y: usize| -> f32 {
            let index = y * QUADRANT_WEIGHT_SAMPLES + x;
            settings.weights[overlay * OVERLAY_WEIGHT_WORDS + index / 4][index % 4]
        };
        assert_eq!(sample(0, 0, 0), 1.0);
        assert_eq!(sample(0, 1, 1), 0.5);
        assert_eq!(sample(0, 16, 16), 0.25);
        assert_eq!(sample(0, 5, 5), 0.0, "an unnamed sample is opacity 0");
        assert_eq!(sample(1, 8, 8), 0.75, "the second overlay's own grid");
        assert_eq!(
            sample(1, 0, 0),
            0.0,
            "the two grids do not bleed into each other"
        );
        for overlay in 2..OVERLAY_WEIGHT_SLOTS {
            assert_eq!(
                sample(overlay, 8, 8),
                0.0,
                "an overlay the quadrant does not use leaves its slot empty"
            );
        }
        assert_eq!(
            settings.weights[OVERLAY_WEIGHT_WORDS - 1].w,
            0.0,
            "the padding word after the last sample stays zero"
        );
    }

    /// Each quadrant reads its own origin out of the cell, so the same cell uv addresses the same
    /// ground in every quadrant.
    #[test]
    fn quadrant_origin_places_each_quadrant_inside_the_cell() {
        let origins: Vec<[f32; 2]> = (0..4)
            .map(|quadrant| {
                let settings = TerrainSettings::for_quadrant(quadrant, 2, &[]);
                assert_eq!(
                    settings.quadrant_origin.z, 0.0,
                    "only xy carries the origin"
                );
                [settings.quadrant_origin.x, settings.quadrant_origin.y]
            })
            .collect();
        assert_eq!(
            origins,
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            "the mesh's quadrants in the same order: west/east then south/north"
        );
    }

    /// The weight field is only worth reading where there are overlays to read. A quadrant whose only
    /// layer is its base has none, and the packed vertex attributes carry none either, so the shader
    /// is told to skip the field's five lookups altogether.
    #[test]
    fn weight_source_is_set_only_for_quadrants_with_overlays() {
        let grid = vec![1.0; QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES];
        assert_eq!(
            TerrainSettings::for_quadrant(0, 2, &[grid]).weight_source,
            Vec4::X
        );
        assert_eq!(
            TerrainSettings::for_quadrant(0, 1, &[]).weight_source,
            Vec4::ZERO,
            "a base-only quadrant has no overlay to read"
        );
        assert_eq!(
            TerrainSettings::vertex_weights_only(6.0).weight_source,
            Vec4::ZERO,
            "a material with no weight field reads the packed attributes"
        );
    }

    /// A fixture material carries the quadrant's own weight field, so the no-game-data scene draws
    /// its overlays through the interpolation the streamed path uses instead of the packed tangent.
    #[test]
    fn fixture_materials_carry_the_quadrants_weight_field() {
        let sample = QUADRANT_WEIGHT_SAMPLES * 8 + 8;
        let terrain =
            terrain_fixture_with_overlays(0x0001_2345, &[vec![(0, 1.0), (sample as u16, 0.5)]]);
        for quadrant in 0..4u8 {
            let extension = TerrainExtension::fixture(
                &terrain,
                quadrant,
                std::array::from_fn(|_| Handle::<Image>::default()),
            )
            .expect("the fixture material must build");
            assert!(
                extension.reads_weight_field(),
                "quadrant {quadrant} must render its overlay through the weight field"
            );
            assert_eq!(
                extension.settings.quadrant_origin.xy(),
                Vec2::new(f32::from(quadrant % 2), f32::from(quadrant / 2)),
                "each quadrant reads its own origin"
            );
            assert_eq!(extension.settings.weights[0][0], 1.0);
            assert_eq!(
                extension.settings.weights[sample / 4][sample % 4],
                0.5,
                "the fixture's own VTXT opacity reaches the field"
            );
        }
    }

    /// Defect A. The overlay weight field is the quadrant's own 17x17 sample grid. A layer that is
    /// full strength on one sample and absent on the next must reach the fragment as the linear
    /// interpolation of the two - `0.5` halfway between them, `0.25`/`0.75` at the quarter points -
    /// not as the sharpened `t^2` ramp the packed vertex attributes produce.
    #[test]
    fn weight_field_interpolates_linearly_between_samples() {
        let terrain = overlay_fixture(0x0001_2345, vec![(0, 1.0)]);
        let overlay_weights = crate::streaming::quadrant_overlay_weights(&terrain, 0).unwrap();
        assert_eq!(overlay_weights.len(), 1, "one overlay, one weight grid");
        let settings = TerrainSettings::for_quadrant(0, 2, &overlay_weights);
        assert_eq!(settings.weight_source, Vec4::X, "the field is in charge");

        let sample = |x: usize, y: usize| -> f32 {
            let index = y * QUADRANT_WEIGHT_SAMPLES + x;
            settings.weights[index / 4][index % 4]
        };
        assert_eq!(sample(0, 0), 1.0, "the sample itself");
        assert_eq!(sample(1, 0), 0.0, "and the empty sample beside it");
        assert_eq!(shader_weight(&settings, 0, [0.0, 0.0]), 1.0);

        // Vertex 0 is the quadrant's (x=0, y=0) sample; its x neighbour is empty, so travelling
        // one sample step in x crosses the 1.0 -> 0.0 edge.
        let step = 1.0 / 16.0;
        let halfway = shader_weight(&settings, 0, [0.5 * step, 0.0]);
        assert!(
            (halfway - 0.5).abs() < 0.01,
            "the midpoint of a 1.0 -> 0.0 edge must read 0.5, not the packed tangent's 0.25: {halfway}"
        );
        let quarter = shader_weight(&settings, 0, [0.25 * step, 0.0]);
        let three_quarters = shader_weight(&settings, 0, [0.75 * step, 0.0]);
        assert!(
            (quarter - 0.75).abs() < 0.01,
            "a quarter of the way in: {quarter}"
        );
        assert!(
            (three_quarters - 0.25).abs() < 0.01,
            "three quarters of the way in: {three_quarters}"
        );

        // Both axes: the field is bilinear, so the centre of the quad whose only full sample is
        // the corner reads 0.25 - the value Skyrim's interpolation gives, and not the one the
        // triangle split would if the weights stayed per vertex.
        let centre = shader_weight(&settings, 0, [0.5 * step, 0.5 * step]);
        assert!((centre - 0.25).abs() < 0.01, "the quad centre: {centre}");

        // The overlay slots the quadrant does not use stay empty, so an unused slot cannot leak
        // weight into the blend.
        for overlay in 1..OVERLAY_WEIGHT_SLOTS {
            assert_eq!(shader_weight(&settings, overlay, [0.0, 0.0]), 0.0);
        }
    }

    /// A coordinate outside the sample square reads the sample it is clamped onto. `grid_point`
    /// clamps before `grid_weight` takes its blend fraction, so a coordinate an ulp below the
    /// quadrant's west or south edge reads that edge sample rather than blending its neighbour in,
    /// and one past the far edge reads the last sample instead of the next quadrant's.
    #[test]
    fn weight_field_clamps_coordinates_onto_the_sample_square() {
        let last = (QUADRANT_WEIGHT_SAMPLES - 1) as f32;
        let edge = QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES - 1;
        let terrain = overlay_fixture(0x0001_2345, vec![(0, 1.0), (edge as u16, 0.25)]);
        let overlay_weights = crate::streaming::quadrant_overlay_weights(&terrain, 0).unwrap();
        let settings = TerrainSettings::for_quadrant(0, 2, &overlay_weights);

        // Sample `s` sits at `s / 16` in quadrant-local uv, so an ulp outside the square is
        // `1.0e-7 / 16` beyond its edge.
        let below = -1.0e-7 / last;
        let past = 1.0 + 1.0e-7 / last;
        assert_eq!(shader_weight(&settings, 0, [below, 0.0]), 1.0);
        assert_eq!(shader_weight(&settings, 0, [0.0, below]), 1.0);
        assert_eq!(shader_weight(&settings, 0, [past, 1.0]), 0.25);
        assert_eq!(shader_weight(&settings, 0, [1.0, past]), 0.25);
    }

    /// The uniform's Rust layout and the shader's must say the same numbers, and list the same
    /// fields in the same order - an insertion on one side alone shifts every field after it, and
    /// the shader then reads its neighbour's bytes.
    #[test]
    fn terrain_uniform_layout_matches_the_shader_source() {
        let source = include_str!("shaders/terrain.wgsl");
        assert_eq!(
            shader_constant(source, "WEIGHT_GRID_SIDE"),
            QUADRANT_WEIGHT_SAMPLES
        );
        assert_eq!(
            shader_constant(source, "WEIGHT_GRID_WORDS"),
            OVERLAY_WEIGHT_WORDS
        );
        assert_eq!(shader_array_length(source, "weights"), WEIGHT_FIELD_WORDS);
        assert_eq!(
            OVERLAY_WEIGHT_WORDS, 73,
            "289 samples four to a vec4, rounded up"
        );
        assert_eq!(WEIGHT_FIELD_WORDS, 365, "five overlays of 73 vec4s");

        let settings = TerrainSettings::for_quadrant(
            0,
            2,
            &[vec![1.0; QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES]],
        );
        let ReflectRef::Struct(fields) = settings.reflect_ref() else {
            panic!("TerrainSettings must reflect as a struct");
        };
        let rust_fields: Vec<String> = (0..fields.field_len())
            .map(|index| {
                fields
                    .name_at(index)
                    .expect("every field is named")
                    .to_owned()
            })
            .collect();
        assert_eq!(rust_fields, shader_struct_fields(source, "TerrainSettings"));
    }

    /// The parts of `terrain.wgsl`'s weight arithmetic the evaluator cannot run, pinned as the
    /// shader writes them: the two places that bound the sample grid, where a bound of
    /// `WEIGHT_GRID_SIDE` instead of the last sample would read the next quadrant's edge texels
    /// along this one, and the fragment's quadrant-local coordinate.
    #[test]
    fn shader_weight_field_bounds_its_grid_at_the_last_sample() {
        let source = include_str!("shaders/terrain.wgsl");
        let side = evaluate_u32(shader_constant_expression(source, "WEIGHT_GRID_SIDE"), &[]);
        let last = evaluate_u32(
            shader_constant_expression(source, "GRID_LAST_SAMPLE"),
            &[("WEIGHT_GRID_SIDE", side)],
        );
        assert_eq!(
            last,
            side - 1,
            "the grid's far edge is its last sample index, not the grid's side"
        );

        let point = "clamp(coordinate, vec2<f32>(0.0), vec2<f32>(f32(GRID_LAST_SAMPLE)))";
        assert!(
            without_whitespace(shader_function_body(source, "grid_point"))
                .contains(&without_whitespace(point)),
            "a coordinate must be clamped onto the sample square before its blend fraction is taken"
        );
        let next = "min(base + vec2<u32>(1u), vec2<u32>(GRID_LAST_SAMPLE))";
        assert!(
            without_whitespace(shader_function_body(source, "grid_weight"))
                .contains(&without_whitespace(next)),
            "the blend must pair the far edge's last sample with itself"
        );
        let coordinate = "(in.uv * 2.0 - terrain.quadrant_origin.xy) * f32(GRID_LAST_SAMPLE)";
        assert!(
            without_whitespace(source).contains(&without_whitespace(coordinate)),
            "the fragment's coordinate must be quadrant-local, on the grid's own scale"
        );
    }

    /// Defect A, the mechanism this replaced. `build_terrain_quadrant_mesh` still packs overlays
    /// 1-3 into `ATTRIBUTE_TANGENT` for materials without a weight field, and Bevy re-normalizes
    /// `world_tangent.xyz` in the vertex shader. Interpolating that carrier halfway across the
    /// same 1.0 -> 0.0 edge yields 0.25: the sharpened ramp the user sees as hard, stair-stepped
    /// transitions. Streamed quadrants carry the weight field above instead.
    #[test]
    fn packed_vertex_weights_sharpen_the_midpoint_to_a_quarter() {
        let terrain = overlay_fixture(0x0001_2345, vec![(0, 1.0)]);
        let mesh = crate::streaming::build_terrain_quadrant_mesh(&terrain, 0).unwrap();
        let VertexAttributeValues::Float32x4(tangents) =
            mesh.attribute(Mesh::ATTRIBUTE_TANGENT).unwrap()
        else {
            panic!("terrain tangents must be Float32x4");
        };
        let full = tangents[0];
        let empty = tangents[1];
        // The vertex shader normalizes xyz per vertex and interpolates the result; the fragment
        // shader only multiplies the interpolated xyz by the interpolated magnitude.
        let midpoint: Vec<f32> = (0..4)
            .map(|axis| (full[axis] + empty[axis]) * 0.5)
            .collect();
        let reconstructed = midpoint[0] * midpoint[3].abs();
        assert!(
            (reconstructed - 0.25).abs() < 0.01,
            "the packed path reads {reconstructed} where the linear weight is 0.5"
        );
    }

    /// Defect A, at a quadrant and at a cell boundary. Each quadrant's weight field holds that
    /// quadrant's own samples clamped at its edge, so where neighbouring data agrees on the shared
    /// column the two sides read the same weight - and they read the *edge* sample, not a blend
    /// with the column behind it.
    #[test]
    fn adjacent_quadrants_and_cells_agree_on_their_shared_edge() {
        let west_cell = overlay_fixture(0x0001_2345, edge_columns_overlay());
        let east_cell = overlay_fixture(0x0001_2346, edge_columns_overlay());
        let settings_of = |terrain: &TerrainSnapshot, quadrant: u8| {
            let weights = crate::streaming::quadrant_overlay_weights(terrain, quadrant).unwrap();
            TerrainSettings::for_quadrant(quadrant, weights.len() + 1, &weights)
        };
        let south_west = settings_of(&west_cell, 0);
        let south_east = settings_of(&west_cell, 1);
        let next_cell_south_west = settings_of(&east_cell, 0);

        // The quadrant boundary inside one cell: quadrant 0's east column and quadrant 1's west
        // column are the same column of ground, at cell uv x = 0.5. Both sides must read it.
        for cell_y in [0.0_f32, 0.25, 0.5] {
            let left = shader_weight(&south_west, 0, quadrant_uv([0.5, cell_y], [0.0, 0.0]));
            let right = shader_weight(&south_east, 0, quadrant_uv([0.5, cell_y], [1.0, 0.0]));
            assert!(
                (left - 1.0).abs() < 0.01,
                "quadrant 0's east column at cell y {cell_y}: {left}"
            );
            assert!(
                (right - 1.0).abs() < 0.01,
                "quadrant 1's west column at cell y {cell_y}: {right}"
            );
        }

        // The cell boundary: this cell's east edge against the next cell's west edge.
        for cell_y in [0.0_f32, 0.25, 0.5] {
            let left = shader_weight(&south_east, 0, quadrant_uv([1.0, cell_y], [1.0, 0.0]));
            let right = shader_weight(
                &next_cell_south_west,
                0,
                quadrant_uv([0.0, cell_y], [0.0, 0.0]),
            );
            assert!(
                (left - right).abs() < 0.01,
                "adjacent cells disagree at cell y {cell_y}: {left} against {right}"
            );
            assert!((left - 1.0).abs() < 0.01, "the shared cell edge: {left}");
        }

        // Half a sample inside that edge the field is halfway between its two samples, which is
        // what makes the measurements above evidence that the edge sample itself is read. Half a
        // sample is `0.5 / 16` of a quadrant, and half that again in cell units.
        let inside = shader_weight(
            &south_east,
            0,
            quadrant_uv([1.0 - 0.5 / 32.0, 0.0], [1.0, 0.0]),
        );
        assert!(
            (inside - 0.5).abs() < 0.01,
            "half a sample inside the edge the weight ramps to 0.5: {inside}"
        );
    }

    /// Defect B's mechanism on the CPU, with the tiling the material uses. A four-texel layer
    /// sampled at `uv = cell_uv * tiling`: clamping reads the last texel column (and, past the
    /// first tile in both axes, the one corner texel) wherever the tiling has moved past the
    /// texture, which is the flat colour and the stretched streaks of the frames; repeating reads
    /// the texel the tiling actually points at.
    #[test]
    fn a_clamping_sampler_smears_a_tiled_layer_across_the_cell() {
        let tiling = TerrainExtension::default()
            .settings
            .tiling_and_layer_count
            .x;
        let texels = 4i32;
        // A layer whose column index is its red channel, so a sample says which column it read.
        let layer = |x: i32, _y: i32| [x as u8 * 60, 0, 0, 255];
        let read = |cell_uv: f32, repeat: bool| -> i32 {
            let uv = cell_uv * tiling;
            let texel = uv * texels as f32 - 0.5;
            let column = if repeat {
                texel.floor().rem_euclid(texels as f32)
            } else {
                texel.floor().clamp(0.0, texels as f32 - 1.0)
            };
            i32::from(layer(column as i32, 0)[0]) / 60
        };

        // Every position past the first tile reads the edge column when clamping...
        for cell_uv in [0.2_f32, 0.5, 0.9] {
            assert_eq!(
                read(cell_uv, false),
                texels - 1,
                "cell uv {cell_uv} clamps to the layer's edge column"
            );
        }
        // ...while repeating keeps sampling the layer the tiling points at, so the texel read
        // changes with the position instead of sticking at the edge.
        assert_ne!(read(0.2, true), read(0.2, false));
        assert_ne!(read(0.5, true), read(0.9, true));
    }

    /// Each blend pair the engine knows renders the way `NiAlphaProperty` asks; the pair glTF's
    /// `BLEND` already expresses keeps it, and a pair neither can express does not silently become
    /// an additive or multiplicative surface.
    #[test]
    fn blend_pairs_map_to_the_alpha_mode_that_renders_them() {
        let mode = |source, destination| BlendInterpretation::of(source, destination).alpha_mode();
        assert_eq!(
            mode(BlendFactor::SourceAlpha, BlendFactor::One),
            AlphaMode::Add,
            "the additive glow card of a torch"
        );
        assert_eq!(mode(BlendFactor::One, BlendFactor::One), AlphaMode::Add);
        assert_eq!(
            mode(BlendFactor::DestinationColor, BlendFactor::Zero),
            AlphaMode::Multiply
        );
        assert_eq!(
            mode(BlendFactor::Zero, BlendFactor::SourceColor),
            AlphaMode::Multiply
        );
        assert_eq!(
            mode(BlendFactor::SourceAlpha, BlendFactor::InverseSourceAlpha),
            AlphaMode::Blend,
            "straight alpha-over is what glTF `BLEND` already renders"
        );

        // A pair with no mode of its own stays alpha-over instead of being rendered as something
        // stronger: the direction is wrong for additive and multiplicative alike.
        for (source, destination) in [
            (BlendFactor::InverseSourceAlpha, BlendFactor::One),
            (BlendFactor::One, BlendFactor::SourceAlpha),
            (BlendFactor::SourceColor, BlendFactor::InverseSourceColor),
            (BlendFactor::DestinationAlpha, BlendFactor::Zero),
            (BlendFactor::SourceAlphaSaturate, BlendFactor::One),
        ] {
            assert_eq!(
                BlendInterpretation::of(source, destination),
                BlendInterpretation::Unsupported,
                "{source:?}/{destination:?} is not expressible"
            );
            assert_eq!(
                BlendInterpretation::of(source, destination).alpha_mode(),
                AlphaMode::Blend
            );
        }
    }

    /// The pair the converter publishes for an additive glow card, read exactly as the loader hands
    /// it to the material handler: both factors are named, and the pair is additive.
    #[test]
    fn a_published_additive_pair_is_an_additive_material() {
        let extension: serde_json::Value = serde_json::from_str(
            r#"{"shaderFamily":"Glow","shaderFlags1":0,"shaderFlags2":0,"premultipliedAlpha":false,
                "screenDoorAlphaFade":false,"textureSlots":[],
                "blendSource":"SRC_ALPHA","blendDestination":"ONE"}"#,
        )
        .unwrap();
        let (source, destination) =
            published_blend_pair(Some(&extension)).expect("the converter publishes both factors");
        assert_eq!((source, destination), ("SRC_ALPHA", "ONE"));
        let pair = BlendFactor::from_name(source)
            .zip(BlendFactor::from_name(destination))
            .expect("both names are ones this engine knows");
        assert_eq!(
            BlendInterpretation::of(pair.0, pair.1).alpha_mode(),
            AlphaMode::Add
        );

        // `ONE`/`ONE` is the other additive spelling, and a zero/second factor is multiplicative.
        let one_one: serde_json::Value =
            serde_json::from_str(r#"{"blendSource":"ONE","blendDestination":"ONE"}"#).unwrap();
        let multiply: serde_json::Value =
            serde_json::from_str(r#"{"blendSource":"DEST_COLOR","blendDestination":"ZERO"}"#)
                .unwrap();
        let mode = |value: &serde_json::Value| {
            let (source, destination) = published_blend_pair(Some(value)).unwrap();
            let pair = BlendFactor::from_name(source)
                .zip(BlendFactor::from_name(destination))
                .unwrap();
            BlendInterpretation::of(pair.0, pair.1).alpha_mode()
        };
        assert_eq!(mode(&one_one), AlphaMode::Add);
        assert_eq!(mode(&multiply), AlphaMode::Multiply);

        // A material with no extension, one with a single field, and one whose factor name this
        // engine does not know all publish no pair this engine acts on.
        assert_eq!(published_blend_pair(None), None);
        assert_eq!(
            published_blend_pair(Some(
                &serde_json::from_str(r#"{"blendSource":"SRC_ALPHA"}"#).unwrap()
            )),
            None
        );
        let unknown =
            serde_json::from_str(r#"{"blendSource":"SRC_ALPHA","blendDestination":"ONE_HALF"}"#)
                .unwrap();
        let (source, destination) = published_blend_pair(Some(&unknown)).unwrap();
        assert_eq!(destination, "ONE_HALF");
        assert_eq!(
            BlendFactor::from_name(destination),
            None,
            "an unknown name is reported, not guessed at"
        );
        assert!(BlendFactor::from_name(source).is_some());
    }

    /// A minimal binary glTF - a header and one JSON chunk, no binary payload - that the parser the
    /// glTF loader itself uses can read: the material JSON goes in the document's `materials`.
    fn glb(materials: &str) -> Vec<u8> {
        let json = format!(r#"{{"asset":{{"version":"2.0"}},"materials":[{materials}]}}"#);
        let mut chunk = json.into_bytes();
        while chunk.len() % 4 != 0 {
            chunk.push(b' ');
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"glTF");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(20 + chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"JSON");
        bytes.extend_from_slice(&chunk);
        bytes
    }

    #[test]
    fn material_manifest_validity_controls_payload_readers_and_animation_installation() {
        use crate::extensions::DataSetExtensions;
        use crate::material_animation::{
            AnimatedPaletteMaterials, MaterialAnimationPlugin, MaterialAnimationRegistry,
        };
        use shared::extensions::DataExtensions;

        let bytes = glb(r#"{"alphaMode":"BLEND","extensions":{
            "OPEN_SKYRIM_material":{"shaderFamily":"effect","shaderFlags1":3221225592,
                "textureSlots":[{"semantic":"greyscale","texture":0}]},
            "MUDCRAB_material":{"blendSource":"SRC_ALPHA","blendDestination":"ONE",
                "falloffStartAngle":0.2,"falloffStopAngle":0.1,
                "falloffStartOpacity":1.0,"falloffStopOpacity":0.0,"softFalloffDepth":64.0},
            "OPEN_SKYRIM_material_animation":{"channels":[{
                "variable":"vOffset","interpolation":"LINEAR","times":[0.0,1.0],
                "values":[0.0,1.0],"loop":"cycle","start":0.0,"stop":1.0}]}
        }}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&bytes).unwrap();
        let material = document.document.materials().next().unwrap();
        let loaded = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            base_color_texture: Some(Handle::<Image>::default()),
            ..default()
        };
        let textures = [Some(Handle::<Image>::default())];
        for (entry, expected) in [
            (None, false),
            (Some((1, "old-core")), false),
            (Some((2, "core")), false),
            (Some((1, "core")), true),
        ] {
            let entries: Vec<_> = entry
                .into_iter()
                .map(|(version, built_from)| {
                    serde_json::json!({"id":"mudcrab.material","version":version,
                    "sha256":"unused","built_from":built_from})
                })
                .collect();
            let data = DataExtensions::from_manifest_json(
                &serde_json::json!({"extensions":entries}).to_string(),
                Some("core".into()),
            )
            .unwrap();
            let mut app = App::new();
            app.init_resource::<GltfExtensionHandlers>();
            app.insert_resource(DataSetExtensions::new(data));
            let enabled = material_extensions_enabled(&app);
            assert_eq!(enabled, expected);
            assert_eq!(
                skyrim_alpha_mode(&material, enabled),
                expected.then_some(AlphaMode::Add)
            );
            assert_eq!(
                material_animation_of(&material, enabled).is_some(),
                expected
            );
            let palette = effect_palette_of(&material, &loaded, &textures, enabled)
                .expect("core palette still loads without the material extension");
            assert_eq!(palette.settings.falloff.is_some(), expected);
            assert_eq!(palette.settings.soft_falloff_depth.is_some(), expected);
            assert_eq!(
                skyrim_material(&material, &loaded, enabled).is_some(),
                expected
            );

            install_material_animation(&mut app);
            register_skyrim_material_handler(&mut app);
            assert_eq!(
                app.world()
                    .resource::<GltfExtensionHandlers>()
                    .0
                    .read_blocking()
                    .len(),
                1,
                "the core material handler remains installed"
            );
            assert_eq!(app.is_plugin_added::<MaterialAnimationPlugin>(), expected);
            assert_eq!(
                app.world().contains_resource::<AnimatedPaletteMaterials>(),
                expected
            );
            assert_eq!(
                app.world().contains_resource::<MaterialAnimationRegistry>(),
                expected
            );
        }
        assert!(
            !material_extensions_enabled(&App::new()),
            "no dataset cannot authorize payloads"
        );
    }

    /// The whole reading path, not a hand-built `Value`: the extension as it sits in a `.glb` the
    /// converter wrote, parsed by the same glTF parser the loader uses. This is what proves the
    /// unknown extension survives parsing at all - `gltf::Material::extension_value` reads it out of
    /// the parser's flattened `others` map - and that a glow card comes out additive.
    #[test]
    fn a_glb_material_publishing_an_additive_pair_reads_as_additive() {
        let additive = glb(r#"{"name":"GlowAddMesh","alphaMode":"BLEND",
                "extensions":{"OPEN_SKYRIM_material":{"shaderFamily":"Glow","shaderFlags1":0,
                "shaderFlags2":0,"premultipliedAlpha":false,"screenDoorAlphaFade":false,
                "textureSlots":[]},
                "MUDCRAB_material":{"blendSource":"SRC_ALPHA","blendDestination":"ONE"}}}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&additive).expect("a valid .glb");
        let material = document.document.materials().next().expect("one material");
        assert_eq!(
            format!("{:?}", material.alpha_mode()),
            "Blend",
            "the converter publishes these materials as glTF `BLEND`, which Bevy loads as `Blend`"
        );
        assert_eq!(skyrim_alpha_mode(&material, true), Some(AlphaMode::Add));

        // The pair glTF already renders stays `Blend`, and a material with no extension at all is
        // left alone.
        let straight = glb(r#"{"name":"Glass","alphaMode":"BLEND",
                "extensions":{"MUDCRAB_material":{"blendSource":"SRC_ALPHA",
                "blendDestination":"INV_SRC_ALPHA"}}}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&straight).unwrap();
        let material = document.document.materials().next().unwrap();
        assert_eq!(skyrim_alpha_mode(&material, true), Some(AlphaMode::Blend));

        let plain = glb(r#"{"name":"Wall","alphaMode":"OPAQUE"}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&plain).unwrap();
        let material = document.document.materials().next().unwrap();
        assert_eq!(skyrim_alpha_mode(&material, true), None);

        // The blend keys are read from `MUDCRAB_material` only: a mesh from main's converter has
        // none, and a pair in the core extension is not looked at.
        let core_only = glb(r#"{"name":"CoreOnly","alphaMode":"BLEND",
                "extensions":{"OPEN_SKYRIM_material":{"blendSource":"SRC_ALPHA",
                "blendDestination":"ONE"}}}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&core_only).unwrap();
        let material = document.document.materials().next().unwrap();
        assert_eq!(skyrim_alpha_mode(&material, true), None);
    }

    /// A converted glow has to leave this handler at the engine's scale. The caps of Blackreach's
    /// glowing mushrooms are the case the fix is for: `emissiveFactor` `[0.212, 0.992, 1.0]` with an
    /// emissive strength of `2.0` and a `MASK` mode, so they publish no blend pair at all - and the
    /// handler used to return early on exactly those materials, leaving the glow at a magnitude of
    /// 2.0 against an ambient of 650 (`docs/research/visual-gaps-spec.md`, gap 1).
    #[test]
    fn a_masked_glow_is_scaled_even_without_a_blend_pair() {
        let cap = glb(r#"{"name":"BlackreachGiantMushroom01:3","alphaMode":"MASK",
                "emissiveFactor":[0.212,0.992,1.0],
                "extensions":{"KHR_materials_emissive_strength":{"emissiveStrength":2.0}}}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&cap).unwrap();
        let gltf_material = document.document.materials().next().unwrap();
        assert_eq!(
            skyrim_alpha_mode(&gltf_material, true),
            None,
            "the cap publishes no pair, which is why it used to be skipped"
        );
        // What Bevy's own handler published for it: `emissiveFactor` times the emissive strength.
        let loaded = StandardMaterial {
            alpha_mode: AlphaMode::Mask(0.5),
            emissive: LinearRgba::new(0.424, 1.984, 2.0, 1.0),
            ..default()
        };
        let published =
            skyrim_material(&gltf_material, &loaded, true).expect("the emissive is rescaled");
        assert_eq!(
            published.alpha_mode,
            AlphaMode::Mask(0.5),
            "and its mode is left exactly as glTF's `alphaMode` made it"
        );
        // At the engine's scale, then capped so its brightest channel stays below the tonemapper's
        // white: the cap's cyan keeps its proportions instead of burning out.
        let expected = capped_glow(exposed_emissive(loaded.emissive));
        assert_eq!(
            skyrim_material(&gltf_material, &loaded, false)
                .unwrap()
                .emissive,
            expected,
            "unusable extension payloads must not disable core emissive handling"
        );
        assert_eq!(
            published.emissive, expected,
            "the glow is published at the engine's scale, capped"
        );
        assert!((published.emissive.blue - GLOW_PEAK_CEILING).abs() < 1e-3);
        assert!(
            (published.emissive.red / published.emissive.blue - 0.424 / 2.0).abs() < 1e-4,
            "and the colour is kept"
        );
        assert_eq!(
            published.emissive.alpha, loaded.emissive.alpha,
            "and the alpha is the surface's coverage, not the glow's brightness: `LinearRgba * f32` \
             would have scaled it too"
        );
        assert!(
            crate::app::emissive_is_visible(published.emissive),
            "which is the point: a converted glow has to come out of this handler at the magnitude \
             the engine's lighting works in, got {:?}",
            published.emissive
        );

        // A torch's additive glow card keeps its pair *and* takes the scale: both changes come out
        // of the one handler.
        let card = glb(
            r#"{"name":"GlowAddMesh","alphaMode":"BLEND","emissiveFactor":[1.0,1.0,1.0],
                "extensions":{"MUDCRAB_material":{"blendSource":"SRC_ALPHA",
                "blendDestination":"ONE"},
                "KHR_materials_emissive_strength":{"emissiveStrength":3.0}}}"#,
        );
        let document = bevy::gltf::gltf::Gltf::from_slice(&card).unwrap();
        let gltf_material = document.document.materials().next().unwrap();
        let loaded = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            emissive: LinearRgba::new(3.0, 3.0, 3.0, 1.0),
            ..default()
        };
        let published = skyrim_material(&gltf_material, &loaded, true).unwrap();
        assert_eq!(published.alpha_mode, AlphaMode::Add);
        assert_eq!(
            published.emissive,
            capped_glow(exposed_emissive(loaded.emissive))
        );
    }

    /// The other class of emissive, which this handler must leave alone: Skyrim's own-emit *surface*
    /// materials. `landscape/ice/icepilel03.glb`'s `IcePileL03` and every snow-covered tree publish
    /// `emissiveFactor [1, 1, 1]` with the glow slot holding the model's own diffuse texture and
    /// **no emissive strength**; scaling those by [`EMISSIVE_EXPOSURE`] is what turned the Alftand
    /// ravine's ice white and clipped 45 % of a daylight frame, which is why the gate exists. The
    /// values here are the real ones, read out of the material JSON of the converted
    /// `landscape/ice/icepilel03.glb`.
    #[test]
    fn an_own_emit_surface_material_keeps_the_emissive_skyrim_gave_it() {
        let own_emit = glb(r#"{"name":"IcePileL03:0","alphaMode":"OPAQUE",
                "emissiveFactor":[1.0,1.0,1.0],
                "extensions":{"OPEN_SKYRIM_material":{"shaderFamily":"lighting",
                "shaderFlags1":2185233153,"shaderFlags2":50331681,"textureSlots":[]}}}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&own_emit).unwrap();
        let gltf_material = document.document.materials().next().unwrap();
        assert!(
            gltf_material.emissive_strength().is_none(),
            "the ice publishes no emissive strength: its multiple is 1"
        );
        // What Bevy published for it: `emissiveFactor` alone.
        let loaded = StandardMaterial {
            emissive: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
            ..default()
        };
        assert!(
            skyrim_material(&gltf_material, &loaded, true).is_none(),
            "an own-emit surface material is not this handler's to change"
        );

        // The strength is the whole difference: the same emissive with a multiple of 2 is a glow.
        let glow = glb(
            r#"{"name":"IcePileL03:0","alphaMode":"OPAQUE","emissiveFactor":[1.0,1.0,1.0],
                "extensions":{"KHR_materials_emissive_strength":{"emissiveStrength":2.0}}}"#,
        );
        let document = bevy::gltf::gltf::Gltf::from_slice(&glow).unwrap();
        let gltf_material = document.document.materials().next().unwrap();
        assert!(is_deliberate_glow(&gltf_material));
        // Bevy's own handler has already folded the strength into the emissive it published.
        let loaded = StandardMaterial {
            emissive: LinearRgba::new(2.0, 2.0, 2.0, 1.0),
            ..default()
        };
        let published = skyrim_material(&gltf_material, &loaded, true).unwrap();
        assert_eq!(
            published.emissive,
            capped_glow(exposed_emissive(loaded.emissive))
        );
        assert!(published.emissive.green > loaded.emissive.green);
    }

    /// A strength of exactly 1 is not a glow either: the converter writes the extension only above
    /// 1 (`crates/converter/src/material.rs`, `publish_emissive`), and a record whose multiple is 1
    /// emits its own texture at 1:1 like the own-emit class does.
    #[test]
    fn an_emissive_strength_of_one_is_not_a_deliberate_glow() {
        for strength in ["1.0", "0.5"] {
            let material_json = format!(
                r#"{{"name":"Faint","alphaMode":"OPAQUE","emissiveFactor":[1.0,1.0,1.0],
                    "extensions":{{"KHR_materials_emissive_strength":{{"emissiveStrength":{strength}}}}}}}"#
            );
            let document = bevy::gltf::gltf::Gltf::from_slice(&glb(&material_json)).unwrap();
            let gltf_material = document.document.materials().next().unwrap();
            assert!(
                !is_deliberate_glow(&gltf_material),
                "a strength of {strength} does not emit more than the texture it comes from"
            );
        }
    }

    /// The handler runs for every streamed material, so it must not republish the ones it has
    /// nothing to say about: an ordinary surface with no pair and no emissive, and a material whose
    /// pair asks for the alpha mode glTF already gave it.
    #[test]
    fn a_material_with_nothing_to_change_is_not_republished() {
        let plain = glb(r#"{"name":"StoneWall","alphaMode":"OPAQUE"}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&plain).unwrap();
        let gltf_material = document.document.materials().next().unwrap();
        assert!(
            skyrim_material(&gltf_material, &StandardMaterial::default(), true).is_none(),
            "an ordinary surface is left alone"
        );

        let straight = glb(r#"{"name":"Glass","alphaMode":"BLEND",
                "extensions":{"MUDCRAB_material":{"blendSource":"SRC_ALPHA",
                "blendDestination":"INV_SRC_ALPHA"}}}"#);
        let document = bevy::gltf::gltf::Gltf::from_slice(&straight).unwrap();
        let gltf_material = document.document.materials().next().unwrap();
        assert!(
            skyrim_material(
                &gltf_material,
                &StandardMaterial {
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                },
                true
            )
            .is_none(),
            "straight alpha-over is what glTF's `BLEND` already renders"
        );
    }

    /// A cell whose four quadrants carry exactly the layers given, in quadrant order.
    fn layer_fixture(cell_id: u32, quadrants: [Vec<TerrainLayerSnapshot>; 4]) -> TerrainSnapshot {
        TerrainSnapshot {
            cell_id,
            width: 33,
            height: 33,
            heights: vec![0.0; 33 * 33],
            normals: [0, 0, 127].repeat(33 * 33),
            vertex_colors: vec![255; 33 * 33 * 3],
            layers: quadrants.into_iter().flatten().collect(),
            water_height: None,
            water_type_form_id: None,
        }
    }

    fn base_layer(quadrant: u8, texture_form_id: u32) -> TerrainLayerSnapshot {
        TerrainLayerSnapshot {
            texture_form_id,
            quadrant,
            layer: 0,
            is_base: true,
            weights: Vec::new(),
        }
    }

    fn overlay_layer(
        quadrant: u8,
        layer: u16,
        texture_form_id: u32,
        weights: Vec<(u16, f32)>,
    ) -> TerrainLayerSnapshot {
        TerrainLayerSnapshot {
            texture_form_id,
            quadrant,
            layer,
            is_base: false,
            weights,
        }
    }

    /// The white squares in the visual review: a quadrant whose `LAND` carries no `BTXT` samples
    /// Bevy's 1x1 white fallback. It takes the base of a neighbouring quadrant of the same cell
    /// instead - the first that has one, in quadrant order - while a quadrant with its own base
    /// keeps it.
    #[test]
    fn quadrant_without_a_base_texture_borrows_a_neighbouring_quadrants_base() {
        let terrain = layer_fixture(
            0x0001_2345,
            [
                vec![base_layer(0, 3)],
                vec![base_layer(1, 0), overlay_layer(1, 1, 9, vec![(0, 1.0)])],
                vec![base_layer(2, 5)],
                vec![base_layer(3, 7)],
            ],
        );
        assert_eq!(quadrant_base_texture_form_id(&terrain, 0).unwrap(), 3);
        assert_eq!(
            quadrant_base_texture_form_id(&terrain, 1).unwrap(),
            3,
            "quadrant 0's base, quadrant 1's first neighbour in quadrant order, not its own overlay"
        );
        assert_eq!(quadrant_base_texture_form_id(&terrain, 2).unwrap(), 5);
        assert_eq!(quadrant_base_texture_form_id(&terrain, 3).unwrap(), 7);

        // Quadrant order, not proximity: quadrant 0 takes quadrant 1's base before quadrant 2's.
        let ordered = layer_fixture(
            0x0001_2345,
            [
                vec![base_layer(0, 0)],
                vec![base_layer(1, 11)],
                vec![base_layer(2, 13)],
                vec![base_layer(3, 17)],
            ],
        );
        assert_eq!(quadrant_base_texture_form_id(&ordered, 0).unwrap(), 11);

        // A neighbouring quadrant whose own base is missing is skipped rather than borrowed from
        // empty: quadrant 2's base is the first textured one after it.
        let skipped = layer_fixture(
            0x0001_2345,
            [
                vec![base_layer(0, 0)],
                vec![base_layer(1, 0)],
                vec![base_layer(2, 0)],
                vec![base_layer(3, 17)],
            ],
        );
        assert_eq!(quadrant_base_texture_form_id(&skipped, 0).unwrap(), 17);
    }

    /// With no textured neighbour the quadrant still avoids the white fallback: it samples the
    /// overlay it paints most of itself with, and a quadrant with nothing to borrow at all keeps
    /// the fallback rather than inventing a texture.
    #[test]
    fn quadrant_without_a_base_texture_or_textured_neighbour_uses_its_strongest_overlay() {
        let terrain = layer_fixture(
            0x0001_2345,
            [
                vec![
                    base_layer(0, 0),
                    overlay_layer(0, 1, 9, vec![(0, 0.25)]),
                    overlay_layer(0, 2, 11, vec![(0, 0.5), (1, 0.5)]),
                    overlay_layer(0, 3, 13, Vec::new()),
                ],
                vec![base_layer(1, 0)],
                vec![base_layer(2, 0)],
                vec![base_layer(3, 0)],
            ],
        );
        assert_eq!(
            quadrant_base_texture_form_id(&terrain, 0).unwrap(),
            11,
            "the overlay covering the most of the quadrant"
        );
        assert_eq!(
            quadrant_base_texture_form_id(&terrain, 1).unwrap(),
            0,
            "an untextured neighbour base is not borrowed, and quadrant 1 has no overlay"
        );

        // An overlay with no `VTXT` opacities covers nothing, so a stronger one wins even when its
        // own texture sits in a later slot.
        let weights_only = layer_fixture(
            0x0001_2345,
            [
                vec![
                    base_layer(0, 0),
                    overlay_layer(0, 1, 9, Vec::new()),
                    overlay_layer(0, 2, 11, vec![(0, 0.1)]),
                ],
                vec![base_layer(1, 0)],
                vec![base_layer(2, 0)],
                vec![base_layer(3, 0)],
            ],
        );
        assert_eq!(quadrant_base_texture_form_id(&weights_only, 0).unwrap(), 11);
    }

    /// A catalogue answering every landscape texture form id the fixtures use, one texture set per
    /// id so each id is its own image path.
    fn texture_catalogue(path: &std::path::Path, form_ids: &[u32]) -> AssetCatalog {
        catalogue_with_normal_maps(path, form_ids, &[])
    }

    /// The same catalogue, with a normal map beside the diffuse of every id in `normal_ids`, so a
    /// quadrant can be shown to bind the normal map of the id whose diffuse it samples.
    fn catalogue_with_normal_maps(
        path: &std::path::Path,
        form_ids: &[u32],
        normal_ids: &[u32],
    ) -> AssetCatalog {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT,normal_path TEXT);
                 CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER);
                 CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT);",
            )
            .unwrap();
        for form_id in form_ids {
            connection
                .execute(
                    "INSERT INTO landscape_textures VALUES(?1,?1)",
                    rusqlite::params![form_id],
                )
                .unwrap();
            let normal_path = normal_ids
                .contains(form_id)
                .then(|| format!("textures/land/{form_id}_n.dds"));
            connection
                .execute(
                    "INSERT INTO texture_sets VALUES(?1,?2,?3)",
                    rusqlite::params![form_id, format!("textures/land/{form_id}.dds"), normal_path],
                )
                .unwrap();
        }
        drop(connection);
        AssetCatalog::open(path).unwrap()
    }

    /// The material path, not just the choice: the quadrant the reviewer saw as a white square
    /// binds the borrowed texture in its base slot, and a quadrant with its own base still binds
    /// that instead.
    #[test]
    fn borrowed_base_texture_reaches_the_quadrant_material() {
        let terrain = layer_fixture(
            0x0001_2345,
            [
                vec![base_layer(0, 0)],
                vec![base_layer(1, 0)],
                vec![base_layer(2, 5)],
                vec![base_layer(3, 7)],
            ],
        );
        let directory = tempfile::tempdir().unwrap();
        let catalog = texture_catalogue(&directory.path().join("catalogue.db"), &[5, 7]);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Image>();
        let asset_server = app.world().resource::<AssetServer>().clone();

        let borrowed = TerrainExtension::from_quadrant(&terrain, 1, &catalog, &asset_server)
            .unwrap()
            .0;
        let sibling = TerrainExtension::from_quadrant(&terrain, 2, &catalog, &asset_server)
            .unwrap()
            .0;
        let own = TerrainExtension::from_quadrant(&terrain, 3, &catalog, &asset_server)
            .unwrap()
            .0;
        assert_eq!(
            borrowed.layer_0, sibling.layer_0,
            "quadrant 1 samples quadrant 2's base texture"
        );
        assert!(
            borrowed.layer_0.is_some(),
            "the base slot must not fall back to Bevy's 1x1 white image"
        );
        assert_ne!(
            borrowed.layer_0, own.layer_0,
            "a quadrant with a base of its own keeps it"
        );
    }

    /// A quadrant that borrows a neighbour's base binds that base's normal map too, so the borrowed
    /// layer's surface detail matches the diffuse it is drawn with.
    #[test]
    fn a_borrowed_base_binds_the_borrowed_layers_normal_map() {
        let terrain = layer_fixture(
            0x0001_2345,
            [
                vec![base_layer(0, 0)],
                vec![base_layer(1, 0)],
                vec![base_layer(2, 5)],
                vec![base_layer(3, 7)],
            ],
        );
        let directory = tempfile::tempdir().unwrap();
        let catalog =
            catalogue_with_normal_maps(&directory.path().join("catalogue.db"), &[5, 7], &[5, 7]);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Image>();
        let asset_server = app.world().resource::<AssetServer>().clone();

        let borrowed = TerrainExtension::from_quadrant(&terrain, 1, &catalog, &asset_server)
            .unwrap()
            .0;
        let sibling = TerrainExtension::from_quadrant(&terrain, 2, &catalog, &asset_server)
            .unwrap()
            .0;
        assert!(borrowed.has_normal_maps(), "a borrowed base has one too");
        assert_eq!(
            borrowed.normal_0, sibling.normal_0,
            "quadrant 1 samples the normal map of the base it borrows from quadrant 2"
        );
        assert!(borrowed.normal_0.is_some());
        assert_eq!(borrowed.normal_1, None, "the unused slots stay unbound");
    }

    /// A layer whose form id has no normal map leaves its flag 0, so the shader never samples a
    /// stand-in for it: the maps that are there are blended alone.
    #[test]
    fn a_quadrant_whose_layers_have_no_normal_maps_leaves_the_flags_clear() {
        let terrain = layer_fixture(
            0x0001_2345,
            [
                vec![base_layer(0, 5)],
                vec![base_layer(1, 7)],
                vec![base_layer(2, 5)],
                vec![base_layer(3, 7)],
            ],
        );
        let directory = tempfile::tempdir().unwrap();
        let catalog = texture_catalogue(&directory.path().join("catalogue.db"), &[5, 7]);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Image>();
        let asset_server = app.world().resource::<AssetServer>().clone();

        let extension = TerrainExtension::from_quadrant(&terrain, 0, &catalog, &asset_server)
            .unwrap()
            .0;
        assert!(!extension.has_normal_maps());
        assert_eq!(extension.settings.normal_layers_0, Vec4::ZERO);
        assert_eq!(extension.settings.normal_layers_1, Vec4::ZERO);
        assert_eq!(extension.normal_0, None);
    }

    #[test]
    fn terrain_sampler_repeats_with_linear_mips_and_16x_anisotropy() {
        let sampler = terrain_layer_sampler();
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        assert_eq!(sampler.mag_filter, bevy::image::ImageFilterMode::Linear);
        assert_eq!(sampler.min_filter, bevy::image::ImageFilterMode::Linear);
        assert_eq!(sampler.mipmap_filter, bevy::image::ImageFilterMode::Linear);
        assert_eq!(sampler.anisotropy_clamp, 16);
    }

    #[test]
    fn landscape_uvs_repeat_twelve_times_per_quadrant_without_scaling_weights() {
        let terrain = terrain_fixture_with_overlays(0x12345, &[vec![(0, 1.0)]]);
        for quadrant in 0..4 {
            let mesh = crate::streaming::build_terrain_quadrant_mesh(&terrain, quadrant).unwrap();
            let VertexAttributeValues::Float32x2(uvs) =
                mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
            else {
                panic!("UVs missing")
            };
            let grids = crate::streaming::quadrant_overlay_weights(&terrain, quadrant).unwrap();
            let settings = TerrainSettings::for_quadrant(quadrant, 2, &grids);
            let tiling = settings.tiling_and_layer_count.xy();
            let origin = Vec2::from(uvs[0]);
            assert_eq!(
                (Vec2::from(uvs[16]) - origin) * tiling,
                Vec2::new(12.0, 0.0)
            );
            assert_eq!(
                (Vec2::from(uvs[272]) - origin) * tiling,
                Vec2::new(0.0, 12.0)
            );
            // Blend coordinates still address the original 17x17 LAND samples.
            let last = (Vec2::from(uvs[288]) * 2.0 - settings.quadrant_origin.xy()) * 16.0;
            assert_eq!(last, Vec2::splat(16.0));
        }
    }

    /// A cell whose four quadrants each carry a base layer plus the given overlays, in the order the
    /// `VTXT` lists are given, so a weight field built from it can be read back against the lists
    /// that produced it.
    fn terrain_fixture_with_overlays(
        cell_id: u32,
        overlays: &[Vec<(u16, f32)>],
    ) -> TerrainSnapshot {
        let mut layers = Vec::new();
        for quadrant in 0..4u8 {
            layers.push(TerrainLayerSnapshot {
                texture_form_id: 1,
                quadrant,
                layer: 0,
                is_base: true,
                weights: Vec::new(),
            });
            for (slot, weights) in overlays.iter().enumerate() {
                let layer = u16::try_from(slot + 1).expect("fixture overlay count fits a u16");
                layers.push(TerrainLayerSnapshot {
                    texture_form_id: u32::from(layer) + 6,
                    quadrant,
                    layer,
                    is_base: false,
                    weights: weights.clone(),
                });
            }
        }
        TerrainSnapshot {
            cell_id,
            width: 33,
            height: 33,
            heights: vec![0.0; 33 * 33],
            normals: [0, 0, 127].repeat(33 * 33),
            vertex_colors: vec![255; 33 * 33 * 3],
            layers,
            water_height: None,
            water_type_form_id: None,
        }
    }
}

use crate::{
    color_pipeline::{DEFAULT_SCENE_EV100, SceneColorPipeline},
    config::WaterReflectionPreset,
    profiling::ProfilingState,
    view_policy::{
        SecondaryViewTarget, ViewDemand, ViewPolicy, ViewPolicySystems, ViewRequest, ViewShadows,
    },
    world::{cache::TerrainSnapshot, database::AssetCatalog},
};
use bevy::{
    app::{HierarchyPropagatePlugin, PropagateOver, PropagateSet},
    asset::embedded_asset,
    camera::{
        Exposure, RenderTarget,
        primitives::{Aabb, Frustum},
        visibility::{RenderLayers, VisibilitySystems},
    },
    core_pipeline::{mip_generation::experimental::depth::ViewDepthPyramid, prepass::DepthPrepass},
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
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;

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
        ));
        app.add_plugins((
            MaterialPlugin::<TerrainMaterial>::default(),
            MaterialPlugin::<WaterMaterial>::default(),
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
            prepare_water_reflection.in_set(ViewPolicySystems::Prepare),
        );

        // `--reflection-cascades N` installs a limit only when explicitly requested.
        // The default, `all`, and 4 keep every cascade and leave the limiting system out.
        add_configured_water_reflection_cascade_limit(app);

        let bridge = RendererProofBridge::default();
        app.insert_resource(bridge.clone());
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

/// The sample grid of one terrain quadrant: `17x17` opacities on the same `128`-unit spacing as the
/// quadrant's mesh columns, which is what LAND's `VTXT` entries index.
pub(crate) const QUADRANT_WEIGHT_SAMPLES: usize = 17;

/// Overlay layers a quadrant can carry beside its base: the runtime's cap, since
/// [`crate::streaming::quadrant_layers`] rejects a quadrant with more than six layers in total.
pub(crate) const OVERLAY_WEIGHT_SLOTS: usize = 5;

/// `Vec4`s per overlay in the weight field: one [`QUADRANT_WEIGHT_SAMPLES`]-square grid packed four
/// samples to a `Vec4` and rounded up to whole `Vec4`s, so every overlay starts on a 16-byte
/// boundary. The shader declares the same number (`WEIGHT_GRID_WORDS` in `terrain.wgsl`).
pub(crate) const OVERLAY_WEIGHT_WORDS: usize =
    (QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES).div_ceil(4);

/// The whole field one material carries: [`OVERLAY_WEIGHT_SLOTS`] overlays of
/// [`OVERLAY_WEIGHT_WORDS`] words each. The shader declares the same number as the length of its
/// `weights` array.
pub(crate) const WEIGHT_FIELD_WORDS: usize = OVERLAY_WEIGHT_SLOTS * OVERLAY_WEIGHT_WORDS;

pub type Layer = usize;

/// The layer of the world: terrain, and (here) the placed objects too.
pub const WORLD_LAYER: Layer = 0;

/// The full-detail object layer, separate from reflected world geometry.
pub const PLACED_OBJECT_LAYER: Layer = 5;

/// Full-detail stock objects stay separate from the reflected LOD world. Layers 2 through 4
/// belong to the portal destination, animated door layer, and unshadowed reflection replica.
/// Seamless cells instead receive their explicit role stamp from the portal's layer resolver.

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
        ..ImageSamplerDescriptor::linear()
    }
}

/// Packs a quadrant's overlay weight grids into the field the material uniform carries: sample `s`
/// of overlay `o` lands in component `s % 4` of word `o * OVERLAY_WEIGHT_WORDS + s / 4`, which is
/// where `grid_weight` in `terrain.wgsl` reads it. Grids beyond [`OVERLAY_WEIGHT_SLOTS`] and
/// samples beyond one grid are dropped, so a malformed grid cannot write outside the field.
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

    /// The settings of one quadrant of `terrain`: the tiling the shader repeats every layer by, the
    /// quadrant's origin inside the cell, and its overlay weight field. The shader interpolates the
    /// field itself, so a weight reaches the fragment stage exactly as LAND records it instead of
    /// through a vertex attribute Bevy re-normalizes.
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
            normal_layers_0: Vec4::ZERO,
            normal_layers_1: Vec4::ZERO,
            weight_source: if overlay_weights.is_empty() {
                Vec4::ZERO
            } else {
                Vec4::X
            },
            weights: weight_field(overlay_weights),
        }
    }

    /// The settings of a material that carries no weight field, so the shader reads the weights the
    /// mesh packs into its vertex attributes instead. Only the stand-in terrain a synthetic scene
    /// draws without a LAND snapshot - the streaming and benchmark fixtures - uses it.
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
            normal_layers_0: Vec4::ZERO,
            normal_layers_1: Vec4::ZERO,
            weight_source: Vec4::ZERO,
            weights: [Vec4::ZERO; WEIGHT_FIELD_WORDS],
        }
    }
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
        let mut textures: [Option<Handle<Image>>; 6] = std::array::from_fn(|_| None);
        let mut normals: [Option<Handle<Image>>; 6] = std::array::from_fn(|_| None);
        let layers = crate::streaming::quadrant_layers(terrain, quadrant)?;
        let mut handles = Vec::with_capacity(layers.len());
        let mut normal_handles = Vec::new();
        for ((target, normal), layer) in textures.iter_mut().zip(normals.iter_mut()).zip(&layers) {
            if layer.texture_form_id == 0 {
                // A preserved unresolved layer keeps its slot and weights with no image bound.
                continue;
            }
            if let Some(path) = catalog.landscape_normal(layer.texture_form_id) {
                // A normal map holds directions, not colours: it must not be decoded as sRGB.
                let handle = asset_server
                    .load_builder()
                    .with_settings(|settings: &mut ImageLoaderSettings| {
                        settings.sampler = ImageSampler::Descriptor(terrain_layer_sampler());
                        settings.is_srgb = false;
                    })
                    .load(path.to_owned());
                *normal = Some(handle.clone());
                normal_handles.push(handle);
            }
            let path = catalog
                .landscape_diffuse(layer.texture_form_id)
                .ok_or_else(|| {
                    format!(
                        "LAND {:08X} quadrant {quadrant} texture {:08X} has no diffuse image",
                        terrain.cell_id, layer.texture_form_id
                    )
                })?;
            let handle = asset_server
                .load_builder()
                .with_settings(|settings: &mut ImageLoaderSettings| {
                    settings.sampler = ImageSampler::Descriptor(terrain_layer_sampler());
                })
                .load(path.to_owned());
            *target = Some(handle.clone());
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

    /// The material of one quadrant of a synthetic `terrain`, drawn with textures a fixture already
    /// holds in memory. It carries the same settings a streamed quadrant does, so a fixture with
    /// overlay layers shows the weight field the shader interpolates rather than only the packed
    /// vertex attributes it falls back on.
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
    pub(crate) fn reads_weight_field(&self) -> bool {
        self.settings.weight_source.x > 0.5
    }

    /// Unbinds every layer's normal map, so the quadrant is lit by its geometric normal as a
    /// quadrant without normal maps is. Used when a normal map fails to load: a normal map is
    /// optional detail, and losing one must not hide the terrain its diffuse layers can draw.
    pub(crate) fn drop_normal_maps(&mut self) {
        let none: [Option<Handle<Image>>; 6] = Default::default();
        self.settings.set_normal_layers(&none);
        [
            self.normal_0,
            self.normal_1,
            self.normal_2,
            self.normal_3,
            self.normal_4,
            self.normal_5,
        ] = none;
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
}

/// Apply live water controls without rewriting materials merely to advance their clock.
fn apply_water_settings(
    settings: Option<Res<crate::settings::RenderingSettings>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let Some(settings) = settings else { return };
    let wanted_wave = Vec4::new(
        settings.wave_scale,
        settings.wave_speed,
        if settings.water_waves {
            settings.wave_strength
        } else {
            0.0
        },
        0.0,
    );
    let wanted_reflection = if settings.water_reflections {
        settings.water_reflection_strength
    } else {
        0.0
    };
    let changed: Vec<_> = materials
        .iter()
        .filter_map(|(id, material)| {
            let water = &material.extension;
            let flow = f32::from(u8::from(
                settings.water_flow_normals && water.flow_normal.is_some(),
            ));
            (water.settings.wave_scale_speed_strength != wanted_wave
                || water.settings.flow_direction.w != flow
                || water.settings.fresnel_reflectivity.z != wanted_reflection)
                .then_some((id, flow))
        })
        .collect();
    for (id, flow) in changed {
        if let Some(mut material) = materials.get_mut(id) {
            let water = &mut material.extension;
            water.settings.wave_scale_speed_strength = wanted_wave;
            water.settings.flow_direction.w = flow;
            water.settings.fresnel_reflectivity.z = wanted_reflection;
        }
    }
}

#[derive(Resource, Clone)]
pub struct WaterReflectionTexture(pub Handle<Image>);

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
        let Some(seen_by) = water_drawn_by(layers, visibility, &main_layers) else {
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
) -> Option<WaterSeenBy> {
    if visibility.is_some_and(|visibility| !visibility.get()) {
        return None;
    }
    let layers = layers.cloned().unwrap_or_default();
    if layers.intersects(main_layers) {
        Some(WaterSeenBy::Main)
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

#[cfg(test)]
mod tests {
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

    use crate::world::components::{StreamingCamera, WaterSurface};

    use bevy::app::Propagate;

    use bevy::camera::{primitives::CascadesFrusta, visibility::ViewVisibility};

    use bevy::image::ImageFilterMode;

    use bevy::light::update_directional_light_frusta;

    use bevy::mesh::VertexAttributeValues;

    use bevy::reflect::{PartialReflect, ReflectRef};

    const MAIN_CAMERA_FAR: f32 = 1000.0;

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

    const CELL_WATER_HALF_EXTENTS: Vec3 = Vec3::new(2048.0, 0.0, 2048.0);

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

    fn main_camera_frustum(main: &Transform) -> Frustum {
        let projection = main_camera_projection();
        let global = GlobalTransform::from(*main);
        let clip_from_world = projection.get_clip_from_view() * global.to_matrix().inverse();
        Frustum(ViewFrustum::from_clip_from_world(&clip_from_world))
    }

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

    fn without_whitespace(source: &str) -> String {
        source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    }

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

    #[test]
    fn the_main_view_renders_every_layer() {
        let main_view = RenderLayers::from_layers(&MAIN_VIEW_LAYERS);
        assert!(main_view.intersects(&RenderLayers::layer(WORLD_LAYER)));
        assert!(main_view.intersects(&RenderLayers::layer(WATER_LAYER)));
        assert!(main_view.intersects(&PLACED_OBJECT_RENDER_LAYERS));
    }

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

    #[test]
    fn animated_water_advances_the_shader_phase() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Assets<WaterMaterial>>()
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

    #[test]
    fn dropping_normal_maps_unbinds_them_and_clears_their_flags() {
        let mut extension = TerrainExtension::default();
        let normals: [Option<Handle<Image>>; 6] =
            std::array::from_fn(|index| (index % 2 == 0).then(Handle::default));
        extension.settings.set_normal_layers(&normals);
        [extension.normal_0, extension.normal_2, extension.normal_4] =
            [normals[0].clone(), normals[2].clone(), normals[4].clone()];
        assert!(extension.has_normal_maps());

        extension.drop_normal_maps();

        assert!(!extension.has_normal_maps());
        assert!(
            [
                &extension.normal_0,
                &extension.normal_1,
                &extension.normal_2,
                &extension.normal_3,
                &extension.normal_4,
                &extension.normal_5,
            ]
            .iter()
            .all(|normal| normal.is_none())
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

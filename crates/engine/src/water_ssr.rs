//! Water Plus: the water reflects by a screen-space ray march (`--water-reflection ssr`) instead of
//! the mirrored reflection camera (`--water-reflection planar`, the default).
//!
//! # Why this and not Bevy's `ScreenSpaceReflections`
//!
//! Bevy's SSR is deferred-only, and the water is a forward, alpha-blended material
//! (`research-1126`, section 1). The march therefore lives in `shaders/water.wgsl`, and what it
//! needs from the frame is two things the water does not get on its own:
//!
//! * **the opaque depth** - the main and portal cameras both carry `DepthPrepass`, and the
//!   shader reads it through `prepass_utils::prepass_depth`;
//! * **the opaque colour** - a blended material has no copy of the frame behind it, so this module
//!   makes one: [`water_ssr_copy`], a `Core3d` system that copies the view's colour target after
//!   the opaque pass and before the transparent pass (the slot Bevy's own transmission copy uses,
//!   `bevy_pbr-0.19.0/src/transmission/mod.rs`). The water is `Transparent3d`, so it is drawn
//!   after the copy and sees the opaque scene without itself in it.
//!
//! # Where the copy lives
//!
//! In the view's own `ViewTransmissionTexture`, which is binding 24 and 25 of the view bind group.
//! That is why the water material gets no new binding: a material's bind group is shared by every
//! view that draws it, and the copy is per view (a second camera, a render-to-texture view, has its
//! own depth and colour).
//! A view with no glass has no transmission texture of its own, so this module puts one there:
//! [`prepare_water_ssr_textures`] replaces it with a texture one row taller than the target. The
//! copy fills the top rows; the extra row holds the sky colours the march falls back to
//! ([`sky_ramp`]) and a sentinel the shader checks, because a view that is not marked can still
//! carry Bevy's own transmission texture and the shader must not read a ramp out of that.
//! Bevy's transmissive pass copies into whatever texture the view carries, so glass keeps working
//! (it overwrites the opaque copy with its own steps, which the water then also reflects).
//!
//! # What only `ssr` pays for
//!
//! Everything here is behind [`WaterSsrSettings::enabled`]. Under `planar` the plugin registers no
//! system, no extraction and no shader definition, so the `Core3d` schedule, the render schedule and
//! the water pipeline are what they were; `water_ssr_plugin_installs_nothing_under_planar` pins it.
//! A view is marked ([`WaterSsrView`]) only while a water plane it draws is in its frustum, so an
//! interior with no water never copies.
//!
//! # Portability
//!
//! This module is written to move to another branch unchanged. It works for any `Camera3d` that
//! has a `DepthPrepass` and draws a [`WaterSurface`], and it knows nothing of the portal, of view
//! policies or of the planar reflection camera: what a branch has to add to join it to its own
//! views (here, switching the planar camera's demand off and handing each camera's sky to
//! [`WaterSsrSky`]) lives in `crate::render::portal_water_ssr_hook`. On the shader side the march
//! is `water_ssr_reflect` in `shaders/water.wgsl`, which returns a colour and a confidence and
//! leaves what fills a miss to its caller.
//!
//! # The step counts
//!
//! `--water-ssr-steps` reaches the shader as shader definitions
//! (`WATER_SSR_LINEAR_STEPS`, `WATER_SSR_BISECTION_STEPS`), not uniforms, so both loops have
//! constant bounds. The settings are read at pipeline specialization, where the material extension
//! has no access to the world, through one process-wide cell written when the plugin is built
//! ([`shader_settings`]). The configuration never changes during a run.

use crate::{
    config::{EngineConfig, WaterReflectionMode, WaterSsrSteps},
    sky::SkyPalette,
    world::components::WaterSurface,
};
use bevy::{
    camera::{
        ClearColorConfig,
        primitives::{Aabb, Frustum},
        visibility::{RenderLayers, VisibilitySystems},
    },
    core_pipeline::{Core3d, Core3dSystems, core_3d::main_opaque_pass_3d, prepass::DepthPrepass},
    image::ToExtents,
    pbr::{ViewTransmissionTexture, main_transmissive_pass_3d},
    prelude::*,
    render::{
        GpuResourceAppExt, Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            Extent3d, FilterMode, Origin3d, Sampler, SamplerDescriptor, TexelCopyBufferLayout,
            TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
            TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        texture::TextureCache,
        view::{ExtractedView, ViewTarget},
    },
    shader::ShaderDefVal,
    transform::TransformSystems,
};
use std::sync::atomic::{AtomicU32, Ordering};

/// Texels in the sky ramp: the sky colour at [`SKY_RAMP_TEXELS`] elevations between the horizon and
/// the zenith. `shaders/water.wgsl` has the same number, `WATER_SSR_RAMP_TEXELS`.
pub const SKY_RAMP_TEXELS: usize = 32;

/// The texel after the ramp. The shader reads it first and uses the ramp only when it finds this
/// value, so a view whose transmission texture is Bevy's own (a glass view this module did not
/// mark) is never read as a ramp. Every component is exact in half precision.
pub const RAMP_SENTINEL: [f32; 4] = [1.0, 0.5, 0.25, 0.125];

/// The ramp row's width in texels: the ramp and the sentinel.
const RAMP_ROW_TEXELS: usize = SKY_RAMP_TEXELS + 1;

/// What `--water-reflection` and `--water-ssr-steps` ask of this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterSsrSettings {
    /// `--water-reflection ssr`.
    pub enabled: bool,
    /// `--water-ssr-steps`.
    pub steps: WaterSsrSteps,
}

impl WaterSsrSettings {
    /// `planar`: nothing is installed.
    pub const OFF: Self = Self {
        enabled: false,
        steps: WaterSsrSteps::DEFAULT,
    };

    /// The settings a run's configuration asks for.
    pub fn from_config(config: &EngineConfig) -> Self {
        Self {
            enabled: config.water_reflection_mode == WaterReflectionMode::Ssr,
            steps: config.water_ssr_steps,
        }
    }

    /// Packs the settings into the cell [`shader_settings`] reads: bit 31 for enabled, then the
    /// bisection and linear counts in a byte and a half-word each.
    const fn pack(self) -> u32 {
        (self.enabled as u32) << 31 | self.steps.bisection << 16 | self.steps.linear
    }

    const fn unpack(bits: u32) -> Self {
        Self {
            enabled: bits >> 31 == 1,
            steps: WaterSsrSteps {
                linear: bits & 0xffff,
                bisection: (bits >> 16) & 0x7fff,
            },
        }
    }
}

/// The settings the water pipeline is specialized with. A material extension's `specialize` has no
/// world to ask, and the settings are fixed for a run, so [`WaterSsrPlugin`] writes them here when
/// it is built.
static SHADER_SETTINGS: AtomicU32 = AtomicU32::new(WaterSsrSettings::OFF.pack());

/// The settings the plugin was built with ([`WaterSsrSettings::OFF`] before it is).
pub fn shader_settings() -> WaterSsrSettings {
    WaterSsrSettings::unpack(SHADER_SETTINGS.load(Ordering::Relaxed))
}

/// The shader definitions `settings` gives `water.wgsl`: none under `planar`, so the shader is the
/// one the engine has always compiled.
pub fn shader_defs_for(settings: WaterSsrSettings) -> Vec<ShaderDefVal> {
    if !settings.enabled {
        return Vec::new();
    }
    vec![
        ShaderDefVal::Bool("WATER_SSR".into(), true),
        ShaderDefVal::UInt("WATER_SSR_LINEAR_STEPS".into(), settings.steps.linear),
        ShaderDefVal::UInt("WATER_SSR_BISECTION_STEPS".into(), settings.steps.bisection),
    ]
}

/// Adds the run's water shader definitions to a pipeline's fragment stage
/// (`WaterExtension::specialize`).
pub fn push_shader_defs(defs: &mut Vec<ShaderDefVal>) {
    defs.extend(shader_defs_for(shader_settings()));
}

/// The systems of this module that a host orders its own against.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum WaterSsrSystems {
    /// Marks the cameras that draw water ([`WaterSsrView`]). Runs in `PostUpdate`, after transform
    /// and visibility propagation and the camera frusta.
    Mark,
}

/// The sky a camera's water falls back to when a reflection ray misses, as the weather's palette
/// the camera's dome is drawn with. A host that draws a sky puts it on each camera; a camera
/// without one (or with `palette: None`, a space with no sky) falls back to its clear colour.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct WaterSsrSky {
    pub palette: Option<SkyPalette>,
}

/// Marks a camera that draws water this frame, in the main world; its render-world copy is what
/// makes [`prepare_water_ssr_textures`] give the view a texture to copy the opaque colour into.
/// Carries the sky colours the march falls back to ([`WaterSsrSky`]), so each view falls back to
/// its own sky.
#[derive(Component, Clone, Debug, PartialEq, ExtractComponent)]
pub struct WaterSsrView {
    /// The sky colour by elevation, in the linear space the frame is lit in ([`sky_ramp`]).
    pub sky_ramp: [Vec4; SKY_RAMP_TEXELS],
}

/// The texture a marked view copies its opaque colour into. A render-world component, inserted by
/// [`prepare_water_ssr_textures`] beside the [`ViewTransmissionTexture`] it replaced.
#[derive(Component, Clone)]
pub struct WaterSsrCopy {
    /// The target's size plus the ramp row.
    pub texture: Texture,
}

/// The sampler of every view's copy.
#[derive(Resource)]
struct WaterSsrSampler(Sampler);

impl FromWorld for WaterSsrSampler {
    fn from_world(world: &mut World) -> Self {
        Self(
            world
                .resource::<RenderDevice>()
                .create_sampler(&SamplerDescriptor {
                    label: Some("water_ssr_sampler"),
                    mag_filter: FilterMode::Linear,
                    min_filter: FilterMode::Linear,
                    ..default()
                }),
        )
    }
}

/// `--water-reflection ssr`. See the module documentation; under `planar` it adds nothing.
pub struct WaterSsrPlugin;

impl Plugin for WaterSsrPlugin {
    fn build(&self, app: &mut App) {
        let settings = app
            .world()
            .get_resource::<EngineConfig>()
            .map_or(WaterSsrSettings::OFF, WaterSsrSettings::from_config);
        SHADER_SETTINGS.store(settings.pack(), Ordering::Relaxed);
        if !settings.enabled {
            return;
        }
        app.add_systems(
            PostUpdate,
            mark_water_ssr_views
                .in_set(WaterSsrSystems::Mark)
                .after(TransformSystems::Propagate)
                .after(VisibilitySystems::VisibilityPropagate)
                .after(VisibilitySystems::UpdateFrusta),
        );
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(
                Render,
                prepare_water_ssr_textures.in_set(RenderSystems::PrepareResourcesFlush),
            )
            .add_systems(
                Core3d,
                water_ssr_copy
                    .after(main_opaque_pass_3d)
                    .before(main_transmissive_pass_3d)
                    .in_set(Core3dSystems::MainPass),
            );
        app.add_plugins(ExtractComponentPlugin::<WaterSsrView>::default());
    }

    fn finish(&self, app: &mut App) {
        if !shader_settings().enabled {
            return;
        }
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_gpu_resource::<WaterSsrSampler>();
        }
    }
}

/// Marks the cameras that draw a water plane this frame, and clears the mark from those that do
/// not.
///
/// A camera draws a water plane when the plane is visible, shares a render layer with the camera
/// (a plane or a camera with no layers is on layer 0, like every entity) and its bounds, grown by
/// [`FRUSTUM_MARGIN`], reach the camera's frustum. Only active `Camera3d`s with a `DepthPrepass`
/// qualify: the march reads that depth. A plane whose bounds are not known yet counts as in view.
/// An interior with no water marks nothing and copies nothing.
#[allow(clippy::type_complexity)]
fn mark_water_ssr_views(
    mut commands: Commands,
    cameras: Query<
        (
            Entity,
            &Camera,
            Option<&Frustum>,
            Option<&RenderLayers>,
            Option<&WaterSsrSky>,
            Option<&WaterSsrView>,
        ),
        (With<Camera3d>, With<DepthPrepass>),
    >,
    water: Query<
        (
            &GlobalTransform,
            Option<&Aabb>,
            Option<&RenderLayers>,
            Option<&InheritedVisibility>,
        ),
        With<WaterSurface>,
    >,
    clear_color: Option<Res<ClearColor>>,
) {
    let global_clear = clear_color.map_or(LinearRgba::BLACK, |clear| clear.0.to_linear());
    let margin = Vec3::splat(FRUSTUM_MARGIN);
    for (camera_entity, camera, frustum, camera_layers, sky, current) in &cameras {
        let sees_water = camera.is_active && {
            let camera_layers = camera_layers.cloned().unwrap_or_default();
            water.iter().any(|(transform, bounds, layers, visibility)| {
                if visibility.is_some_and(|visibility| !visibility.get()) {
                    return false;
                }
                if !layers
                    .cloned()
                    .unwrap_or_default()
                    .intersects(&camera_layers)
                {
                    return false;
                }
                let (Some(frustum), Some(bounds)) = (frustum, bounds) else {
                    return true;
                };
                let grown = Aabb::from_min_max(
                    Vec3::from(bounds.min()) - margin,
                    Vec3::from(bounds.max()) + margin,
                );
                frustum.intersects_obb(&grown, &transform.affine(), true, true)
            })
        };
        let clear = match &camera.clear_color {
            ClearColorConfig::Custom(color) => color.to_linear(),
            ClearColorConfig::Default => global_clear,
            ClearColorConfig::None => LinearRgba::BLACK,
        };
        sync_mark(
            &mut commands,
            camera_entity,
            current,
            sees_water.then(|| WaterSsrView {
                sky_ramp: sky_ramp(sky.and_then(|sky| sky.palette.as_ref()), clear),
            }),
        );
    }
}

/// Slack added to a water plane's bounds before the frustum test, in world units (4096 to a cell),
/// so a turn or a move between two frames cannot bring water in before the view has a copy.
const FRUSTUM_MARGIN: f32 = 512.0;

/// Writes or removes a camera's mark, touching the component only when it changes: an extraction
/// of an unchanged mark is cheap, a rewrite every frame is not.
fn sync_mark(
    commands: &mut Commands,
    entity: Entity,
    current: Option<&WaterSsrView>,
    wanted: Option<WaterSsrView>,
) {
    match (current, wanted) {
        (Some(current), Some(wanted)) if *current == wanted => {}
        (_, Some(wanted)) => {
            commands.entity(entity).insert(wanted);
        }
        (Some(_), None) => {
            commands.entity(entity).remove::<WaterSsrView>();
        }
        (None, None) => {}
    }
}

/// The elevation in degrees of ramp texel `index`: the texels crowd towards the horizon, where the
/// dome fades from the clear colour to the sky over its lowest two degrees (`crate::sky`'s
/// `DOME_MASKS`) and a uniform ramp would step over it. The shader inverts it with a square root.
pub fn ramp_elevation_degrees(index: usize) -> f32 {
    let t = index as f32 / (SKY_RAMP_TEXELS - 1) as f32;
    90.0 * t * t
}

/// The colour a camera sees in the sky at each ramp elevation: the dome as `shaders/sky.wgsl`
/// draws it (the weather's colours mixed in sRGB by `SkyPalette::dome_colour`, converted to linear
/// and scaled by the palette's brightness) blended by the dome's alpha over the camera's clear
/// colour. A camera with no sky (an interior, `None`) shows its clear colour all the way up.
///
/// The CPU function is the source of truth for the dome; the shader only interpolates it, so the
/// water's fallback and the dome cannot drift apart.
pub fn sky_ramp(palette: Option<&SkyPalette>, clear: LinearRgba) -> [Vec4; SKY_RAMP_TEXELS] {
    std::array::from_fn(|index| {
        let clear = Vec3::new(clear.red, clear.green, clear.blue);
        let Some(palette) = palette else {
            return clear.extend(1.0);
        };
        let dome = palette.dome_colour(ramp_elevation_degrees(index));
        let linear = LinearRgba::from(Srgba::new(dome.red, dome.green, dome.blue, 1.0));
        let sky = Vec3::new(linear.red, linear.green, linear.blue) * palette.brightness;
        (sky * dome.alpha + clear * (1.0 - dome.alpha)).extend(1.0)
    })
}

/// Gives every marked view the texture it copies into, and takes it back from a view that is no
/// longer marked.
///
/// Runs after Bevy's own transmission textures are prepared (`PrepareResources`), so the texture
/// inserted here is the one the view bind group binds. The texture is [`TextureFormat::Rgba16Float`]
/// like the HDR target it copies; a view in another format is left alone, and its water has no
/// reflection (the shader finds no sentinel), which is also what a camera nothing marked gets.
#[allow(clippy::too_many_arguments)]
fn prepare_water_ssr_textures(
    mut commands: Commands,
    mut texture_cache: ResMut<TextureCache>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    sampler: Res<WaterSsrSampler>,
    marked: Query<(Entity, &ExtractedCamera, &ExtractedView, &WaterSsrView)>,
    unmarked: Query<
        (Entity, &WaterSsrCopy, Option<&ViewTransmissionTexture>),
        Without<WaterSsrView>,
    >,
) {
    for (entity, copy, transmission) in &unmarked {
        let mut view = commands.entity(entity);
        view.remove::<WaterSsrCopy>();
        // Only our own texture goes: a glass view's is Bevy's, written every frame it is needed.
        if transmission.is_some_and(|transmission| transmission.texture.id() == copy.texture.id()) {
            view.remove::<ViewTransmissionTexture>();
        }
    }
    for (entity, camera, view, mark) in &marked {
        let Some(size) = camera.physical_target_size else {
            continue;
        };
        if view.target_format != TextureFormat::Rgba16Float || size.x < RAMP_ROW_TEXELS as u32 {
            bevy::log::warn_once!(
                "--water-reflection ssr: a view of {}x{} in {:?} cannot carry the sky ramp, so its \
                 water has no reflection",
                size.x,
                size.y,
                view.target_format
            );
            continue;
        }
        let cached = texture_cache.get(
            &render_device,
            TextureDescriptor {
                label: Some("water_ssr_opaque_copy"),
                // One row more than the target: the sky ramp.
                size: Extent3d {
                    width: size.x,
                    height: size.y + 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            },
        );
        let row = ramp_row_bytes(&mark.sky_ramp);
        render_queue.write_texture(
            TexelCopyTextureInfo {
                texture: &cached.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: size.y,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            &row,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row.len() as u32),
                rows_per_image: None,
            },
            Extent3d {
                width: RAMP_ROW_TEXELS as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        commands.entity(entity).insert((
            WaterSsrCopy {
                texture: cached.texture.clone(),
            },
            ViewTransmissionTexture {
                texture: cached.texture,
                view: cached.default_view,
                sampler: sampler.0.clone(),
            },
        ));
    }
}

/// The ramp and its sentinel as the bytes of one `Rgba16Float` row.
fn ramp_row_bytes(ramp: &[Vec4; SKY_RAMP_TEXELS]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(RAMP_ROW_TEXELS * 8);
    for texel in ramp.iter().copied().chain([Vec4::from(RAMP_SENTINEL)]) {
        for component in texel.to_array() {
            bytes.extend_from_slice(&f32_to_f16_bits(component).to_le_bytes());
        }
    }
    bytes
}

/// An `f32` as IEEE half-precision bits, rounded to nearest even; values beyond half's range
/// become infinity and values under its smallest subnormal become zero.
pub fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x007f_ffff;
    if exponent == 0xff {
        // Infinity, or a NaN that stays one.
        return sign | 0x7c00 | if mantissa != 0 { 0x0200 } else { 0 };
    }
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 0x1f {
        return sign | 0x7c00;
    }
    if half_exponent <= 0 {
        if half_exponent < -10 {
            return sign;
        }
        // A half subnormal: the implicit bit joins the mantissa and it shifts into place.
        let mantissa = mantissa | 0x0080_0000;
        let shift = (14 - half_exponent) as u32;
        let kept = mantissa >> shift;
        let remainder = mantissa & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        let rounded = if remainder > halfway || (remainder == halfway && kept & 1 == 1) {
            kept + 1
        } else {
            kept
        };
        return sign | rounded as u16;
    }
    let mut result = ((half_exponent as u32) << 10) | (mantissa >> 13);
    let remainder = mantissa & 0x1fff;
    // A carry out of the mantissa lands in the exponent, which is the right answer.
    if remainder > 0x1000 || (remainder == 0x1000 && result & 1 == 1) {
        result += 1;
    }
    sign | result as u16
}

/// Copies the view's colour target into its [`WaterSsrCopy`], after the opaque pass and before the
/// transparent pass, and times the copy as `render/water_ssr_copy/elapsed_gpu`.
///
/// The `ViewQuery` skips the system for a view that has no [`WaterSsrCopy`], so only a view that
/// draws water pays for it. One copy per marked view per frame; the copy is of the same texture
/// region Bevy's own transmission copy takes.
fn water_ssr_copy(
    view: ViewQuery<(&ExtractedCamera, &ViewTarget, &WaterSsrCopy)>,
    mut ctx: RenderContext,
) {
    let (camera, target, copy) = view.into_inner();
    let Some(size) = camera.physical_target_size else {
        return;
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "water_ssr_copy");
    ctx.command_encoder().copy_texture_to_texture(
        target.main_texture().as_image_copy(),
        copy.texture.as_image_copy(),
        size.to_extents(),
    );
    span.end(ctx.command_encoder());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::SubApp;

    fn ssr_config() -> EngineConfig {
        EngineConfig {
            water_reflection_mode: WaterReflectionMode::Ssr,
            ..default()
        }
    }

    // --- half precision ---------------------------------------------------------------------

    #[test]
    fn half_precision_conversion_matches_ieee_values() {
        let cases: [(f32, u16); 14] = [
            (0.0, 0x0000),
            (-0.0, 0x8000),
            (1.0, 0x3c00),
            (-2.0, 0xc000),
            (0.5, 0x3800),
            (0.25, 0x3400),
            (0.125, 0x3000),
            (65504.0, 0x7bff),
            (65520.0, 0x7c00),
            (f32::INFINITY, 0x7c00),
            (6.103_515_6e-5, 0x0400),
            (5.960_464_5e-8, 0x0001),
            (1.0e-9, 0x0000),
            (0.1, 0x2e66),
        ];
        for (value, bits) in cases {
            assert_eq!(f32_to_f16_bits(value), bits, "{value}");
        }
        let nan = f32_to_f16_bits(f32::NAN);
        assert_eq!(nan & 0x7c00, 0x7c00);
        assert_ne!(nan & 0x03ff, 0, "a NaN stays a NaN");
    }

    #[test]
    fn half_precision_rounds_to_nearest_even() {
        // 1 + 2^-11 is halfway between 1.0 and the next half; ties go to the even mantissa.
        assert_eq!(f32_to_f16_bits(1.0 + 2.0f32.powi(-11)), 0x3c00);
        // 1 + 3 * 2^-11 is halfway between 0x3c01 and 0x3c02: the even one is 0x3c02.
        assert_eq!(f32_to_f16_bits(1.0 + 3.0 * 2.0f32.powi(-11)), 0x3c02);
        // Past halfway rounds up.
        assert_eq!(f32_to_f16_bits(1.0 + 2.0f32.powi(-11) * 1.01), 0x3c01);
    }

    // --- the ramp ---------------------------------------------------------------------------

    #[test]
    fn the_ramp_row_ends_in_the_sentinel_the_shader_looks_for() {
        let ramp = sky_ramp(Some(&SkyPalette::SKYRIM_CLEAR_DAY), LinearRgba::BLACK);
        let bytes = ramp_row_bytes(&ramp);
        assert_eq!(bytes.len(), (SKY_RAMP_TEXELS + 1) * 8);
        let sentinel = &bytes[SKY_RAMP_TEXELS * 8..];
        let halves: Vec<u16> = sentinel
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(halves, [0x3c00, 0x3800, 0x3400, 0x3000]);
        assert_eq!(
            RAMP_SENTINEL.map(f32_to_f16_bits),
            [0x3c00, 0x3800, 0x3400, 0x3000]
        );
    }

    #[test]
    fn the_ramp_starts_at_the_clear_colour_and_ends_at_the_zenith() {
        let clear = LinearRgba::rgb(0.2, 0.3, 0.4);
        let palette = SkyPalette::SKYRIM_CLEAR_DAY;
        let ramp = sky_ramp(Some(&palette), clear);
        // The dome is transparent at the horizon, so the camera sees what it cleared to.
        assert!((ramp[0].truncate() - Vec3::new(0.2, 0.3, 0.4)).length() < 1.0e-6);
        // At the zenith the dome is opaque and is the Sky-Upper colour.
        let upper = LinearRgba::from(palette.upper.to_srgba());
        let top = ramp[SKY_RAMP_TEXELS - 1];
        assert!((top.x - upper.red).abs() < 1.0e-5, "{top:?} {upper:?}");
        assert!((top.y - upper.green).abs() < 1.0e-5);
        assert!((top.z - upper.blue).abs() < 1.0e-5);
        assert_eq!(top.w, 1.0);
    }

    #[test]
    fn the_ramp_applies_the_palettes_brightness() {
        let bright = SkyPalette::SKYRIM_CLEAR_DAY;
        let dim = SkyPalette {
            brightness: 0.5,
            ..bright
        };
        let top = SKY_RAMP_TEXELS - 1;
        let full = sky_ramp(Some(&bright), LinearRgba::BLACK)[top];
        let half = sky_ramp(Some(&dim), LinearRgba::BLACK)[top];
        assert!((half.truncate() - full.truncate() * 0.5).length() < 1.0e-6);
    }

    #[test]
    fn an_interior_with_no_sky_shows_its_clear_colour_all_the_way_up() {
        let clear = LinearRgba::rgb(0.01, 0.02, 0.03);
        for texel in sky_ramp(None, clear) {
            assert_eq!(texel, Vec4::new(0.01, 0.02, 0.03, 1.0));
        }
    }

    #[test]
    fn the_ramp_crowds_its_texels_towards_the_horizon() {
        assert_eq!(ramp_elevation_degrees(0), 0.0);
        assert_eq!(ramp_elevation_degrees(SKY_RAMP_TEXELS - 1), 90.0);
        // The dome's fade from the clear colour takes two degrees; several texels are inside it.
        assert!(
            (1..SKY_RAMP_TEXELS)
                .take_while(|&index| ramp_elevation_degrees(index) < 2.1)
                .count()
                >= 3
        );
        // The shader's inverse: texel position = sqrt(elevation / 90) * (texels - 1).
        for index in 0..SKY_RAMP_TEXELS {
            let position =
                (ramp_elevation_degrees(index) / 90.0).sqrt() * (SKY_RAMP_TEXELS - 1) as f32;
            assert!((position - index as f32).abs() < 1.0e-4, "{index}");
        }
    }

    // --- the settings and the shader definitions --------------------------------------------

    #[test]
    fn settings_survive_the_cell_they_travel_through() {
        for linear in [1, 16, 128] {
            for bisection in [0, 6, 32] {
                for enabled in [false, true] {
                    let settings = WaterSsrSettings {
                        enabled,
                        steps: WaterSsrSteps { linear, bisection },
                    };
                    assert_eq!(WaterSsrSettings::unpack(settings.pack()), settings);
                }
            }
        }
        assert_eq!(WaterSsrSettings::OFF.pack() >> 31, 0);
    }

    #[test]
    fn planar_gives_the_shader_no_definitions() {
        let planar = WaterSsrSettings::from_config(&EngineConfig::default());
        assert!(!planar.enabled);
        assert!(
            shader_defs_for(planar).is_empty(),
            "the shader stays as it was"
        );
    }

    #[test]
    fn ssr_gives_the_shader_its_step_counts() {
        let config = EngineConfig {
            water_reflection_mode: WaterReflectionMode::Ssr,
            water_ssr_steps: WaterSsrSteps {
                linear: 24,
                bisection: 4,
            },
            ..default()
        };
        let defs = shader_defs_for(WaterSsrSettings::from_config(&config));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_SSR".into(), true)));
        assert!(defs.contains(&ShaderDefVal::UInt("WATER_SSR_LINEAR_STEPS".into(), 24)));
        assert!(defs.contains(&ShaderDefVal::UInt("WATER_SSR_BISECTION_STEPS".into(), 4)));
        let default_defs = shader_defs_for(WaterSsrSettings::from_config(&ssr_config()));
        assert!(default_defs.contains(&ShaderDefVal::UInt("WATER_SSR_LINEAR_STEPS".into(), 16)));
        assert!(default_defs.contains(&ShaderDefVal::UInt("WATER_SSR_BISECTION_STEPS".into(), 6)));
    }

    // --- what the plugin installs -----------------------------------------------------------

    /// An app with a stand-in render sub-app that has the two schedules the plugin adds to.
    fn app_with_render_stub(config: EngineConfig) -> App {
        let mut app = App::new();
        app.insert_resource(config);
        let mut render = SubApp::new();
        render.add_schedule(bevy::render::Render::base_schedule());
        render.add_schedule(Core3d::base_schedule());
        app.insert_sub_app(RenderApp, render);
        app.add_plugins(WaterSsrPlugin);
        app
    }

    /// The identities of the systems a schedule of this world holds (a schedule nothing was added
    /// to does not exist, and holds none).
    fn scheduled_systems(
        world: &mut World,
        label: impl bevy::ecs::schedule::ScheduleLabel + Clone,
    ) -> Vec<std::any::TypeId> {
        if !world.resource::<Schedules>().contains(label.clone()) {
            return Vec::new();
        }
        world.schedule_scope(label, |world, schedule| {
            schedule.initialize(world).unwrap();
            schedule
                .systems()
                .unwrap()
                .map(|(_, system)| bevy::ecs::system::System::system_type(&**system))
                .collect()
        })
    }

    /// What `IntoSystem` makes of a system function, to compare with what a schedule holds.
    fn identity<M>(system: impl IntoSystem<(), (), M>) -> std::any::TypeId {
        bevy::ecs::system::System::system_type(&IntoSystem::into_system(system))
    }

    fn render_world(app: &mut App) -> &mut World {
        app.get_sub_app_mut(RenderApp).unwrap().world_mut()
    }

    #[test]
    fn water_ssr_plugin_installs_nothing_under_planar() {
        let mut app = app_with_render_stub(EngineConfig::default());
        let core3d = scheduled_systems(render_world(&mut app), Core3d);
        assert!(
            core3d.is_empty(),
            "the copy is not scheduled under planar, and Core3d is as it was"
        );
        let render = scheduled_systems(render_world(&mut app), bevy::render::Render);
        assert!(render.is_empty(), "no texture is prepared under planar");
        // Nothing is registered in the main world either: no marking system.
        let post_update = scheduled_systems(app.world_mut(), PostUpdate);
        assert!(post_update.is_empty());
        assert!(
            render_world(&mut app)
                .get_resource::<WaterSsrSampler>()
                .is_none()
        );
    }

    #[test]
    fn water_ssr_plugin_schedules_the_copy_and_the_texture_under_ssr() {
        let mut app = app_with_render_stub(ssr_config());
        let core3d = scheduled_systems(render_world(&mut app), Core3d);
        assert_eq!(core3d, [identity(water_ssr_copy)], "the copy is scheduled");
        let render = scheduled_systems(render_world(&mut app), bevy::render::Render);
        assert_eq!(render, [identity(prepare_water_ssr_textures)]);
        let post_update = scheduled_systems(app.world_mut(), PostUpdate);
        assert_eq!(post_update, [identity(mark_water_ssr_views)]);
    }

    // --- marking the views that draw water --------------------------------------------------

    fn marking_app(config: EngineConfig) -> App {
        let mut app = App::new();
        app.insert_resource(config);
        app.add_plugins(WaterSsrPlugin);
        app
    }

    /// A `Camera3d` with a depth prepass at (0, 120, 0) looking down -Z, with the frustum Bevy's
    /// `update_frusta` would give it.
    fn spawn_observer(app: &mut App, sky: Option<WaterSsrSky>) -> Entity {
        let pose = Transform::from_xyz(0.0, 120.0, 0.0);
        let global = GlobalTransform::from(pose);
        let projection = Projection::Perspective(PerspectiveProjection {
            far: 40_000.0,
            ..default()
        });
        let frustum = projection.compute_frustum(&global);
        let mut camera = app.world_mut().spawn((
            Camera3d::default(),
            DepthPrepass,
            pose,
            global,
            projection,
            frustum,
        ));
        if let Some(sky) = sky {
            camera.insert(sky);
        }
        camera.id()
    }

    fn spawn_water(app: &mut App, position: Vec3, layers: RenderLayers) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(position),
                GlobalTransform::from_translation(position),
                Aabb::from_min_max(
                    Vec3::new(-2048.0, 0.0, -2048.0),
                    Vec3::new(2048.0, 0.0, 2048.0),
                ),
                layers,
                InheritedVisibility::VISIBLE,
                WaterSurface,
            ))
            .id()
    }

    fn marked(app: &App, camera: Entity) -> bool {
        app.world().get::<WaterSsrView>(camera).is_some()
    }

    #[test]
    fn a_camera_with_water_in_view_is_marked_and_one_without_is_not() {
        let mut app = marking_app(ssr_config());
        let camera = spawn_observer(&mut app, None);
        app.update();
        assert!(
            !marked(&app, camera),
            "an interior with no water copies nothing"
        );

        let water = spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, -800.0),
            RenderLayers::layer(0),
        );
        app.update();
        assert!(marked(&app, camera));

        app.world_mut().entity_mut(water).despawn();
        app.update();
        assert!(!marked(&app, camera), "the mark goes with the water");
    }

    #[test]
    fn water_behind_the_camera_does_not_mark_it() {
        let mut app = marking_app(ssr_config());
        let camera = spawn_observer(&mut app, None);
        spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, 20_000.0),
            RenderLayers::layer(0),
        );
        app.update();
        assert!(!marked(&app, camera));
    }

    #[test]
    fn hidden_water_does_not_mark_the_camera() {
        let mut app = marking_app(ssr_config());
        let camera = spawn_observer(&mut app, None);
        let water = spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, -800.0),
            RenderLayers::layer(0),
        );
        app.world_mut()
            .entity_mut(water)
            .insert(InheritedVisibility::HIDDEN);
        app.update();
        assert!(!marked(&app, camera));
    }

    #[test]
    fn planar_marks_nothing_even_with_water_in_view() {
        let mut app = marking_app(EngineConfig::default());
        let camera = spawn_observer(&mut app, None);
        spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, -800.0),
            RenderLayers::layer(0),
        );
        for _ in 0..3 {
            app.update();
        }
        assert!(
            !marked(&app, camera),
            "no marking system exists under planar"
        );
    }

    #[test]
    fn the_mark_carries_the_cameras_own_sky() {
        let mut app = marking_app(ssr_config());
        let palette = SkyPalette {
            brightness: 0.25,
            ..SkyPalette::SKYRIM_CLEAR_DAY
        };
        let camera = spawn_observer(
            &mut app,
            Some(WaterSsrSky {
                palette: Some(palette),
            }),
        );
        spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, -800.0),
            RenderLayers::layer(0),
        );
        app.update();
        let mark = app.world().get::<WaterSsrView>(camera).unwrap();
        // The default camera clears to the (absent) clear colour resource: black.
        assert_eq!(mark.sky_ramp, sky_ramp(Some(&palette), LinearRgba::BLACK));
    }

    #[test]
    fn a_camera_clearing_to_a_custom_colour_falls_back_to_it() {
        let mut app = marking_app(ssr_config());
        let camera = spawn_observer(&mut app, None);
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Camera>()
            .unwrap()
            .clear_color = ClearColorConfig::Custom(Color::linear_rgb(0.5, 0.25, 0.125));
        spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, -800.0),
            RenderLayers::layer(0),
        );
        app.update();
        let mark = app.world().get::<WaterSsrView>(camera).unwrap();
        assert_eq!(mark.sky_ramp[3], Vec4::new(0.5, 0.25, 0.125, 1.0));
    }

    #[test]
    fn a_second_view_is_marked_by_the_water_on_its_own_layers_while_it_is_active() {
        const OTHER_LAYER: usize = 5;
        let mut app = marking_app(ssr_config());
        let first = spawn_observer(&mut app, None);
        let second = spawn_observer(&mut app, None);
        {
            let mut second = app.world_mut().entity_mut(second);
            second.insert(RenderLayers::layer(OTHER_LAYER));
            second.get_mut::<Camera>().unwrap().is_active = false;
        }
        spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, -800.0),
            RenderLayers::layer(OTHER_LAYER),
        );
        app.update();
        assert!(!marked(&app, first), "the first view draws no such layer");
        assert!(!marked(&app, second), "the second view is switched off");

        app.world_mut()
            .entity_mut(second)
            .get_mut::<Camera>()
            .unwrap()
            .is_active = true;
        app.update();
        assert!(
            marked(&app, second),
            "the second view's water is its own view's"
        );
        assert!(!marked(&app, first));

        app.world_mut()
            .entity_mut(second)
            .get_mut::<Camera>()
            .unwrap()
            .is_active = false;
        app.update();
        assert!(!marked(&app, second));
    }

    #[test]
    fn a_camera_without_a_depth_prepass_is_never_marked() {
        let mut app = marking_app(ssr_config());
        let camera = spawn_observer(&mut app, None);
        app.world_mut().entity_mut(camera).remove::<DepthPrepass>();
        spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, -800.0),
            RenderLayers::layer(0),
        );
        app.update();
        assert!(!marked(&app, camera), "the march reads the prepass depth");
    }

    #[test]
    fn water_this_module_has_never_met_a_bounds_for_still_counts_as_in_view() {
        let mut app = marking_app(ssr_config());
        let camera = spawn_observer(&mut app, None);
        let water = spawn_water(
            &mut app,
            Vec3::new(0.0, 40.0, 20_000.0),
            RenderLayers::layer(0),
        );
        app.world_mut().entity_mut(water).remove::<Aabb>();
        app.update();
        assert!(marked(&app, camera));
    }

    // --- the shader ---------------------------------------------------------------------------

    mod shader {
        use naga_oil::compose::{
            ComposableModuleDescriptor, Composer, NagaModuleDescriptor, ShaderDefValue,
            ShaderLanguage, ShaderType,
        };
        use std::collections::HashMap;

        // Stand-ins for the Bevy modules `water.wgsl` imports. Each keeps the real module's
        // path, item names and signatures (bevy_pbr 0.19.0), and only the bodies are reduced; the
        // composition proves the water shader's own code parses, type-checks and validates with
        // and without the screen-space reflection, for any step counts.
        const PBR_TYPES: &str = "
#define_import_path bevy_pbr::pbr_types
struct StandardMaterial { base_color: vec4<f32>, flags: u32 }
struct PbrInput { material: StandardMaterial, N: vec3<f32>, V: vec3<f32> }
";
        const FORWARD_IO: &str = "
#define_import_path bevy_pbr::forward_io
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
}
struct FragmentOutput { @location(0) color: vec4<f32> }
";
        const VIEW_BINDINGS: &str = "
#define_import_path bevy_pbr::mesh_view_bindings
struct View {
    clip_from_world: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    world_from_view: mat4x4<f32>,
    clip_from_view: mat4x4<f32>,
    world_position: vec3<f32>,
    viewport: vec4<f32>,
}
struct Globals { time: f32, delta_time: f32, frame_count: u32 }
@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(11) var<uniform> globals: Globals;
@group(0) @binding(24) var view_transmission_texture: texture_2d<f32>;
@group(0) @binding(25) var view_transmission_sampler: sampler;
#ifdef DEPTH_PREPASS
@group(0) @binding(20) var depth_prepass_texture: texture_depth_2d;
#endif
";
        const VIEW_TRANSFORMATIONS: &str = "
#define_import_path bevy_pbr::view_transformations
#import bevy_pbr::mesh_view_bindings as view_bindings
fn ndc_to_uv(ndc: vec2<f32>) -> vec2<f32> { return ndc * vec2(0.5, -0.5) + vec2(0.5); }
fn perspective_camera_near() -> f32 { return view_bindings::view.clip_from_view[3][2]; }
fn position_ndc_to_world(ndc_pos: vec3<f32>) -> vec3<f32> {
    let world_pos = view_bindings::view.world_from_clip * vec4(ndc_pos, 1.0);
    return world_pos.xyz / world_pos.w;
}
";
        const PREPASS_UTILS: &str = "
#define_import_path bevy_pbr::prepass_utils
#import bevy_pbr::mesh_view_bindings as view_bindings
#ifdef DEPTH_PREPASS
fn prepass_depth(frag_coord: vec4<f32>, sample_index: u32) -> f32 {
    return textureLoad(view_bindings::depth_prepass_texture, vec2<i32>(frag_coord.xy), 0);
}
#endif
";
        const PBR_FRAGMENT: &str = "
#define_import_path bevy_pbr::pbr_fragment
#import bevy_pbr::{pbr_types, forward_io::VertexOutput}
fn pbr_input_from_standard_material(in: VertexOutput, is_front: bool) -> pbr_types::PbrInput {
    var pbr_input: pbr_types::PbrInput;
    pbr_input.V = vec3<f32>(0.0, 1.0, 0.0);
    pbr_input.N = in.world_normal;
    return pbr_input;
}
";
        const PBR_FUNCTIONS: &str = "
#define_import_path bevy_pbr::pbr_functions
#import bevy_pbr::pbr_types
fn alpha_discard(material: pbr_types::StandardMaterial, output_color: vec4<f32>) -> vec4<f32> {
    return output_color;
}
fn apply_pbr_lighting(in: pbr_types::PbrInput) -> vec4<f32> { return in.material.base_color; }
fn main_pass_post_lighting_processing(
    pbr_input: pbr_types::PbrInput,
    input_color: vec4<f32>,
) -> vec4<f32> {
    return input_color;
}
";
        const WATER: &str = include_str!("shaders/water.wgsl");

        fn defs(pairs: &[(&str, ShaderDefValue)]) -> HashMap<String, ShaderDefValue> {
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), *value))
                .collect()
        }

        /// Composes `water.wgsl` against the stubs with these definitions, or says why it failed.
        fn compose(shader_defs: HashMap<String, ShaderDefValue>) -> Result<(), String> {
            let mut composer = Composer::default();
            for (path, source) in [
                ("pbr_types.wgsl", PBR_TYPES),
                ("forward_io.wgsl", FORWARD_IO),
                ("mesh_view_bindings.wgsl", VIEW_BINDINGS),
                ("view_transformations.wgsl", VIEW_TRANSFORMATIONS),
                ("prepass_utils.wgsl", PREPASS_UTILS),
                ("pbr_fragment.wgsl", PBR_FRAGMENT),
                ("pbr_functions.wgsl", PBR_FUNCTIONS),
            ] {
                let added = composer
                    .add_composable_module(ComposableModuleDescriptor {
                        source,
                        file_path: path,
                        language: ShaderLanguage::Wgsl,
                        shader_defs: shader_defs.clone(),
                        ..Default::default()
                    })
                    .map(|_| ());
                if let Err(error) = added {
                    return Err(format!("stub {path}: {}", error.emit_to_string(&composer)));
                }
            }
            let composed = composer
                .make_naga_module(NagaModuleDescriptor {
                    source: WATER,
                    file_path: "water.wgsl",
                    shader_type: ShaderType::Wgsl,
                    shader_defs,
                    ..Default::default()
                })
                .map(|_| ());
            composed.map_err(|error| error.emit_to_string(&composer))
        }

        fn ssr(
            linear: u32,
            bisection: u32,
            depth_prepass: bool,
        ) -> HashMap<String, ShaderDefValue> {
            let mut pairs = vec![
                ("MATERIAL_BIND_GROUP", ShaderDefValue::UInt(2)),
                ("WATER_SSR", ShaderDefValue::Bool(true)),
                ("WATER_SSR_LINEAR_STEPS", ShaderDefValue::UInt(linear)),
                ("WATER_SSR_BISECTION_STEPS", ShaderDefValue::UInt(bisection)),
            ];
            if depth_prepass {
                pairs.push(("DEPTH_PREPASS", ShaderDefValue::Bool(true)));
            }
            defs(&pairs)
        }

        #[test]
        fn the_planar_shader_composes_as_it_always_did() {
            compose(defs(&[("MATERIAL_BIND_GROUP", ShaderDefValue::UInt(2))])).unwrap();
            // The planar pipeline of a view with a depth prepass is the same shader.
            compose(defs(&[
                ("MATERIAL_BIND_GROUP", ShaderDefValue::UInt(2)),
                ("DEPTH_PREPASS", ShaderDefValue::Bool(true)),
            ]))
            .unwrap();
        }

        #[test]
        fn the_ssr_shader_composes_with_the_march() {
            compose(ssr(16, 6, true)).unwrap();
        }

        #[test]
        fn the_ssr_shader_composes_without_a_depth_prepass_as_the_sky_alone() {
            compose(ssr(16, 6, false)).unwrap();
        }

        #[test]
        fn the_ssr_shader_composes_at_the_extremes_of_the_step_counts() {
            for (linear, bisection) in [(1, 0), (2, 1), (128, 32)] {
                compose(ssr(linear, bisection, true))
                    .unwrap_or_else(|error| panic!("{linear},{bisection}: {error}"));
            }
        }

        #[test]
        fn the_checker_does_catch_a_broken_shader() {
            // Guards the test itself: a step count that is not a number must not compose.
            let mut broken = ssr(16, 6, true);
            broken.insert("WATER_SSR_LINEAR_STEPS".into(), ShaderDefValue::Bool(true));
            assert!(compose(broken).is_err());
        }
    }
}

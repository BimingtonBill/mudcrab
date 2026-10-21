//! Renders a secondary view without the sun's own shadow cascades, using stock Bevy.
//!
//! # Why a route is needed
//!
//! Bevy builds and renders a separate set of directional shadow cascades for *every* active camera
//! (`bevy_light::cascade`, `bevy_pbr::render::light`), so an active water reflection camera adds
//! four more cascade passes over the same sun. The shadow settings belong to the light and to the
//! meshes, not to the camera, and neither Bevy 0.19 nor 0.20 has a per-camera switch for
//! directional cascades. Bevy's open PR #24625 (per-camera point and spot shadow maps) does not
//! cover cascades.
//!
//! # The route
//!
//! What a light lights and which meshes it shadows are both decided by render-layer intersection,
//! so the route spends one layer, [`UNSHADOWED_REFLECTION_LAYER`]:
//!
//! | Entity | Layers |
//! |---|---|
//! | Main camera | main view layers, never the replica layer |
//! | Reflected world meshes (terrain, sky) | world layer **and** the replica layer |
//! | Source sun | light layers, shadows as configured |
//! | [`UnshadowedSun`] copy | the replica layer only, shadows off |
//! | A view with [`ViewShadows::On`] | its shadowed layers (the world layer) |
//! | A view with [`ViewShadows::Off`] | its unshadowed layers (the replica layer) |
//!
//! A view is on exactly one of the two routes, never both, so it never sees the shadowed sun and
//! the copy together. Geometry is not duplicated; only layer membership is widened. The copy is
//! kept in step with the source sun every frame by [`sync_unshadowed_suns`], and it exists only
//! while some routed view asks for shadows off and would otherwise draw that sun.
//!
//! Limits of the stock route: it removes the reflection's GPU cascade passes, not all CPU cascade
//! work (Bevy still builds cascade matrices for every active camera), and only the sun is
//! replicated. Point and spot lights need no copy: they cast no cascades, so they carry the replica
//! layer themselves (`POINT_LIGHT_LAYERS` in `render.rs`) and light both routes.
//!
//! # Swapping it out
//!
//! This whole module is the boundary. When Bevy gains a per-view directional shadow switch, a
//! single system that applies [`ViewPolicy::shadows`] to that switch replaces [`ShadowRoutePlugin`],
//! and the layer plumbing in `render.rs` (`reflectable_world_layers`) goes with it.

use crate::{
    render::UNSHADOWED_REFLECTION_LAYER,
    view_policy::{ViewPolicy, ViewShadows},
};
use bevy::{
    camera::visibility::{RenderLayers, VisibilitySystems},
    ecs::query::QueryData,
    light::{CascadeShadowConfig, SunDisk, VolumetricLight},
    prelude::*,
    reflect::PartialReflect,
    transform::TransformSystems,
};

/// Marks a view whose [`ViewPolicy::shadows`] is realised by switching its [`RenderLayers`]
/// between two sets: the one that sees the shadowed sun, and the one that sees the copy.
///
/// The two sets must be disjoint from each other's sun, which [`ShadowRoutePlugin`] cannot check
/// for an arbitrary caller; the water reflection builds them in `render.rs` and a test holds them.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ViewShadowRoute {
    /// The layers the view draws while its policy says [`ViewShadows::On`].
    pub shadowed: RenderLayers,
    /// The layers the view draws while its policy says [`ViewShadows::Off`].
    pub unshadowed: RenderLayers,
}

impl ViewShadowRoute {
    /// The layers a view with `shadows` draws.
    pub fn layers_for(&self, shadows: ViewShadows) -> &RenderLayers {
        match shadows {
            ViewShadows::On => &self.shadowed,
            ViewShadows::Off => &self.unshadowed,
        }
    }
}

/// An unshadowed copy of a source sun, on [`UNSHADOWED_REFLECTION_LAYER`] only.
///
/// It is never a source itself: [`sync_unshadowed_suns`] only reads lights without this marker, so
/// copies cannot be copied.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnshadowedSun {
    /// The light this one follows.
    pub source: Entity,
}

/// Registers the route: view layers follow their policy, and the sun copy follows its source.
pub struct ShadowRoutePlugin;

impl Plugin for ShadowRoutePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (route_view_shadows, sync_unshadowed_suns)
                .before(TransformSystems::Propagate)
                .before(VisibilitySystems::CheckVisibility),
        );
    }
}

/// Points each routed view at the layers its policy's shadow choice selects.
fn route_view_shadows(mut views: Query<(&ViewPolicy, &ViewShadowRoute, &mut RenderLayers)>) {
    for (policy, route, mut layers) in &mut views {
        let wanted = route.layers_for(policy.shadows);
        if *layers != *wanted {
            *layers = wanted.clone();
        }
    }
}

/// Keeps one [`UnshadowedSun`] per source sun that a shadowless view needs, and the copy's
/// direction, colour, illuminance, cascade configuration, optional markers and enabled state equal
/// to its source's every frame.
///
/// A source is copied while some routed view whose policy says [`ViewShadows::Off`] would draw it
/// on its shadowed route, that is, while the source's layers intersect that view's `shadowed`
/// layers. A source that already has its shadow maps off is copied too, on purpose: the shadowless
/// view draws only the replica layer, so without a copy it would lose its only sun.
///
/// The copy is the source's [`DirectionalLight`] with every shadow switch cleared, the source's
/// [`CascadeShadowConfig`], and the same [`VolumetricLight`] and [`SunDisk`] the source has. A
/// source that is a root entity is read through its `Transform` rather than its `GlobalTransform`,
/// because the sun is moved in `Update` and propagation has not run yet; a source below a parent
/// has only its propagated pose to give. The copy's visibility mirrors the source's, which is how a
/// sun is switched off in Bevy.
fn sync_unshadowed_suns(
    mut commands: Commands,
    routes: Query<(&ViewPolicy, &ViewShadowRoute)>,
    sources: Query<SunSource, Without<UnshadowedSun>>,
    mut copies: Query<(Entity, &UnshadowedSun, SunCopy)>,
) {
    let shadowless_views: Vec<&RenderLayers> = routes
        .iter()
        .filter(|(policy, _)| policy.shadows == ViewShadows::Off)
        .map(|(_, route)| &route.shadowed)
        .collect();
    let needed = |layers: Option<&RenderLayers>| {
        let layers = layers.unwrap_or(&DEFAULT_LAYERS);
        shadowless_views.iter().any(|view| layers.intersects(view))
    };

    let mut followed = Vec::new();
    for (copy, unshadowed, mut components) in &mut copies {
        let Some(source) = sources
            .get(unshadowed.source)
            .ok()
            .filter(|source| needed(source.layers))
        else {
            commands.entity(copy).despawn();
            continue;
        };
        followed.push(unshadowed.source);
        let desired = unshadowed_copy_of(source.light);
        if !components
            .light
            .reflect_partial_eq(&desired)
            .unwrap_or(false)
        {
            *components.light = desired;
        }
        if !components
            .cascades
            .reflect_partial_eq(source.cascades)
            .unwrap_or(false)
        {
            *components.cascades = source.cascades.clone();
        }
        components.transform.set_if_neq(source_pose(
            source.transform,
            source.global,
            source.parent,
        ));
        components.visibility.set_if_neq(*source.visibility);
        if components.volumetric != source.volumetric.is_some() {
            if source.volumetric.is_some() {
                commands.entity(copy).insert(VolumetricLight);
            } else {
                commands.entity(copy).remove::<VolumetricLight>();
            }
        }
        match (components.disk.as_deref(), source.disk) {
            (None, Some(disk)) => {
                commands.entity(copy).insert(disk.clone());
            }
            (Some(_), None) => {
                commands.entity(copy).remove::<SunDisk>();
            }
            (Some(have), Some(disk)) if !same_disk(have, disk) => {
                commands.entity(copy).insert(disk.clone());
            }
            _ => {}
        }
    }
    for source in &sources {
        if followed.contains(&source.entity) || !needed(source.layers) {
            continue;
        }
        let mut copy = commands.spawn((
            Name::new("Unshadowed sun (shadowless views)"),
            UnshadowedSun {
                source: source.entity,
            },
            unshadowed_copy_of(source.light),
            source.cascades.clone(),
            RenderLayers::layer(UNSHADOWED_REFLECTION_LAYER),
            source_pose(source.transform, source.global, source.parent),
            *source.visibility,
        ));
        if source.volumetric.is_some() {
            copy.insert(VolumetricLight);
        }
        if let Some(disk) = source.disk {
            copy.insert(disk.clone());
        }
    }
}

/// The layers an entity without a [`RenderLayers`] component is on: Bevy's default, layer 0.
static DEFAULT_LAYERS: RenderLayers = RenderLayers::layer(0);

/// What [`sync_unshadowed_suns`] reads off a source sun.
#[derive(QueryData)]
struct SunSource {
    entity: Entity,
    light: &'static DirectionalLight,
    cascades: &'static CascadeShadowConfig,
    transform: &'static Transform,
    global: &'static GlobalTransform,
    parent: Option<&'static ChildOf>,
    visibility: &'static Visibility,
    layers: Option<&'static RenderLayers>,
    volumetric: Option<&'static VolumetricLight>,
    disk: Option<&'static SunDisk>,
}

/// What [`sync_unshadowed_suns`] writes on a copy.
#[derive(QueryData)]
#[query_data(mutable)]
struct SunCopy {
    light: &'static mut DirectionalLight,
    cascades: &'static mut CascadeShadowConfig,
    transform: &'static mut Transform,
    visibility: &'static mut Visibility,
    volumetric: Has<VolumetricLight>,
    disk: Option<&'static mut SunDisk>,
}

/// `SunDisk` has no equality of its own.
fn same_disk(left: &SunDisk, right: &SunDisk) -> bool {
    left.angular_size == right.angular_size && left.intensity == right.intensity
}

/// The source light with every shadow switch cleared.
fn unshadowed_copy_of(source: &DirectionalLight) -> DirectionalLight {
    DirectionalLight {
        shadow_maps_enabled: false,
        contact_shadows_enabled: false,
        ..*source
    }
}

/// A root light's own `Transform`, which is this frame's; otherwise the pose propagation last gave
/// it, which is the best a parented light can offer before propagation runs.
fn source_pose(
    transform: &Transform,
    global: &GlobalTransform,
    parent: Option<&ChildOf>,
) -> Transform {
    if parent.is_none() {
        *transform
    } else {
        global.compute_transform()
    }
}

#[cfg(test)]
mod tests {
    // An unrelated content layer outside both public reflection routes.
    const UNROUTED_TEST_LAYER: usize = 31;
    use super::*;
    use crate::render::{
        LIGHT_LAYERS, MAIN_VIEW_LAYERS, PLACED_OBJECT_LAYER, REFLECTION_VIEW_LAYERS, WATER_LAYER,
        WORLD_LAYER, reflectable_world_layers,
    };
    use bevy::light::CascadeShadowConfigBuilder;

    fn shadowed_layers() -> RenderLayers {
        RenderLayers::from_layers(REFLECTION_VIEW_LAYERS)
    }

    fn unshadowed_layers() -> RenderLayers {
        RenderLayers::layer(UNSHADOWED_REFLECTION_LAYER)
    }

    fn route() -> ViewShadowRoute {
        ViewShadowRoute {
            shadowed: shadowed_layers(),
            unshadowed: unshadowed_layers(),
        }
    }

    fn policy(shadows: ViewShadows) -> ViewPolicy {
        ViewPolicy {
            shadows,
            ..ViewPolicy::baseline()
        }
    }

    struct RouteHarness {
        app: App,
        view: Entity,
        sun: Entity,
    }

    impl RouteHarness {
        fn new(shadows: ViewShadows) -> Self {
            let mut app = App::new();
            app.add_plugins(ShadowRoutePlugin);
            let sun = app
                .world_mut()
                .spawn((
                    DirectionalLight {
                        illuminance: 12_000.0,
                        color: Color::srgb(1.0, 0.9, 0.8),
                        shadow_maps_enabled: true,
                        contact_shadows_enabled: true,
                        ..default()
                    },
                    RenderLayers::from_layers(&LIGHT_LAYERS),
                    Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
                ))
                .id();
            let view = app
                .world_mut()
                .spawn((
                    policy(shadows),
                    route(),
                    RenderLayers::from_layers(REFLECTION_VIEW_LAYERS),
                ))
                .id();
            Self { app, view, sun }
        }

        fn set_shadows(&mut self, shadows: ViewShadows) {
            self.app
                .world_mut()
                .entity_mut(self.view)
                .insert(policy(shadows));
        }

        fn view_layers(&self) -> RenderLayers {
            self.app
                .world()
                .entity(self.view)
                .get::<RenderLayers>()
                .cloned()
                .expect("the view keeps its layers")
        }

        fn copies(&mut self) -> Vec<(Entity, DirectionalLight, Transform, Visibility)> {
            let mut query = self.app.world_mut().query::<(
                Entity,
                &UnshadowedSun,
                &DirectionalLight,
                &Transform,
                &Visibility,
            )>();
            query
                .iter(self.app.world())
                .map(|(entity, _, light, transform, visibility)| {
                    (entity, *light, *transform, *visibility)
                })
                .collect()
        }

        fn directional_lights(&mut self) -> usize {
            self.app
                .world_mut()
                .query::<&DirectionalLight>()
                .iter(self.app.world())
                .count()
        }

        fn only_copy(&mut self) -> (DirectionalLight, Transform, Visibility) {
            let copies = self.copies();
            assert_eq!(copies.len(), 1, "one source sun has one copy");
            let (_, light, transform, visibility) = copies[0];
            (light, transform, visibility)
        }
    }

    #[test]
    fn the_replica_layer_is_in_no_main_view_and_no_source_light() {
        let replica = unshadowed_layers();
        assert!(!RenderLayers::from_layers(&MAIN_VIEW_LAYERS).intersects(&replica));
        assert!(!RenderLayers::from_layers(&LIGHT_LAYERS).intersects(&replica));
        assert!(
            RenderLayers::from_layers(&crate::render::POINT_LIGHT_LAYERS).intersects(&replica),
            "a point light must still light the shadowless route"
        );
        for layer in [WORLD_LAYER, WATER_LAYER, PLACED_OBJECT_LAYER] {
            assert_ne!(layer, UNSHADOWED_REFLECTION_LAYER);
        }
    }

    #[test]
    fn a_shadows_off_view_sees_the_copy_and_never_the_source_sun() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        harness.app.update();
        let layers = harness.view_layers();
        let source = RenderLayers::from_layers(&LIGHT_LAYERS);
        assert!(
            !layers.intersects(&source),
            "the shadowed sun must not light a shadowless view"
        );
        assert!(layers.intersects(&unshadowed_layers()));
        let copy = harness.copies();
        assert_eq!(copy.len(), 1);
        let copy_layers = harness
            .app
            .world()
            .entity(copy[0].0)
            .get::<RenderLayers>()
            .unwrap();
        assert!(layers.intersects(copy_layers));
        assert!(
            !harness
                .app
                .world()
                .entity(harness.sun)
                .get::<RenderLayers>()
                .unwrap()
                .intersects(copy_layers),
            "the source sun and its copy never share a layer"
        );
    }

    #[test]
    fn a_shadows_on_view_sees_the_source_sun_and_never_the_copy() {
        let mut harness = RouteHarness::new(ViewShadows::On);
        harness.app.update();
        let layers = harness.view_layers();
        assert!(layers.intersects(&RenderLayers::from_layers(&LIGHT_LAYERS)));
        assert!(!layers.intersects(&unshadowed_layers()));
        assert!(
            harness.copies().is_empty(),
            "no view wants the copy, so none exists"
        );
    }

    #[test]
    fn a_view_is_on_exactly_one_route_whatever_its_policy_says() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        for shadows in [
            ViewShadows::Off,
            ViewShadows::On,
            ViewShadows::Off,
            ViewShadows::On,
        ] {
            harness.set_shadows(shadows);
            harness.app.update();
            let layers = harness.view_layers();
            let on_source = layers.intersects(&RenderLayers::from_layers(&LIGHT_LAYERS));
            let on_copy = layers.intersects(&unshadowed_layers());
            assert_ne!(on_source, on_copy, "{shadows:?} is on one route, not both");
            assert_eq!(on_copy, shadows == ViewShadows::Off);
        }
    }

    #[test]
    fn reflected_world_meshes_are_visible_in_both_routes() {
        let world = reflectable_world_layers();
        assert!(world.intersects(&shadowed_layers()));
        assert!(world.intersects(&unshadowed_layers()));
        // The source sun still shadows them, so they are in its layers too.
        assert!(world.intersects(&RenderLayers::from_layers(&LIGHT_LAYERS)));
    }

    /// Placed objects keep their own stock layer. Neither reflection route admits those objects,
    /// the water surface, or the portal's destination space.
    #[test]
    fn water_and_the_portal_destination_stay_out_of_both_routes() {
        let destination = RenderLayers::layer(UNROUTED_TEST_LAYER);
        for layers in [shadowed_layers(), unshadowed_layers()] {
            assert!(!layers.intersects(&destination));
            assert!(!layers.intersects(&RenderLayers::layer(WATER_LAYER)));
            assert!(!layers.intersects(&RenderLayers::layer(PLACED_OBJECT_LAYER)));
        }
        let world = reflectable_world_layers();
        assert!(!world.intersects(&RenderLayers::layer(WATER_LAYER)));
        assert!(!world.intersects(&destination));
        assert_ne!(PLACED_OBJECT_LAYER, WORLD_LAYER);
    }

    #[test]
    fn the_copy_has_the_sources_light_with_every_shadow_switch_off() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        harness.app.update();
        let (light, transform, visibility) = harness.only_copy();
        let source = *harness
            .app
            .world()
            .entity(harness.sun)
            .get::<DirectionalLight>()
            .unwrap();
        assert!(source.shadow_maps_enabled, "the source keeps its shadows");
        assert!(!light.shadow_maps_enabled);
        assert!(!light.contact_shadows_enabled);
        assert_eq!(light.illuminance, source.illuminance);
        assert_eq!(light.color, source.color);
        assert_eq!(
            transform.rotation,
            Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)
        );
        assert_eq!(visibility, Visibility::Inherited);
    }

    #[test]
    fn source_sun_changes_reach_the_copy_the_same_frame() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        harness.app.update();

        let turned = Quat::from_euler(EulerRot::XYZ, -0.3, 1.1, 0.0);
        let sun = harness.sun;
        {
            let mut entity = harness.app.world_mut().entity_mut(sun);
            entity.get_mut::<Transform>().unwrap().rotation = turned;
            let mut light = entity.get_mut::<DirectionalLight>().unwrap();
            light.illuminance = 3_000.0;
            light.color = Color::srgb(0.2, 0.4, 1.0);
            light.shadow_maps_enabled = false;
            entity.insert(Visibility::Hidden);
        }
        harness.app.update();

        let (light, transform, visibility) = harness.only_copy();
        assert_eq!(transform.rotation, turned);
        assert_eq!(light.illuminance, 3_000.0);
        assert_eq!(light.color, Color::srgb(0.2, 0.4, 1.0));
        assert!(!light.shadow_maps_enabled);
        assert_eq!(
            visibility,
            Visibility::Hidden,
            "a hidden sun hides its copy"
        );
    }

    #[test]
    fn the_copy_is_never_copied() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        for _ in 0..5 {
            harness.app.update();
        }
        assert_eq!(
            harness.directional_lights(),
            2,
            "one source and one copy, however many frames run"
        );
        assert_eq!(harness.copies().len(), 1);
    }

    #[test]
    fn the_copy_goes_away_with_its_source_or_its_demand() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        harness.app.update();
        assert_eq!(harness.copies().len(), 1);

        harness.set_shadows(ViewShadows::On);
        harness.app.update();
        harness.app.update();
        assert!(harness.copies().is_empty(), "no routed view wants it");

        harness.set_shadows(ViewShadows::Off);
        harness.app.update();
        harness.app.update();
        assert_eq!(harness.copies().len(), 1, "wanted again, so it returns");

        let sun = harness.sun;
        harness.app.world_mut().entity_mut(sun).despawn();
        harness.app.update();
        harness.app.update();
        assert!(harness.copies().is_empty(), "no source, no copy");
        assert_eq!(harness.directional_lights(), 0);
    }

    #[test]
    fn each_source_sun_gets_its_own_copy() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        harness
            .app
            .world_mut()
            .spawn((DirectionalLight::default(), Transform::default()));
        harness.app.update();
        harness.app.update();
        assert_eq!(harness.copies().len(), 2);
        assert_eq!(harness.directional_lights(), 4);
    }

    #[test]
    fn a_source_with_shadows_off_is_still_copied_so_the_view_keeps_its_sun() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        let sun = harness.sun;
        harness
            .app
            .world_mut()
            .entity_mut(sun)
            .get_mut::<DirectionalLight>()
            .unwrap()
            .shadow_maps_enabled = false;
        harness.app.update();
        harness.app.update();
        assert_eq!(harness.copies().len(), 1);
    }

    #[test]
    fn a_source_no_shadowless_view_would_draw_gets_no_copy() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        let sun = harness.sun;
        harness
            .app
            .world_mut()
            .entity_mut(sun)
            // The doorway's sun is such a source: it lights the portal's destination layer only.
            .insert(RenderLayers::layer(UNROUTED_TEST_LAYER));
        harness.app.update();
        harness.app.update();
        assert!(harness.copies().is_empty());
    }

    #[test]
    fn the_copy_mirrors_the_sources_cascades_volumetric_marker_and_sun_disk() {
        let mut harness = RouteHarness::new(ViewShadows::Off);
        let sun = harness.sun;
        let cascades = CascadeShadowConfigBuilder {
            num_cascades: 2,
            maximum_distance: 1234.0,
            ..default()
        }
        .build();
        harness.app.world_mut().entity_mut(sun).insert((
            cascades.clone(),
            VolumetricLight,
            SunDisk::EARTH,
        ));
        harness.app.update();
        harness.app.update();

        let copy = harness.copies()[0].0;
        let entity = harness.app.world().entity(copy);
        assert_eq!(
            entity.get::<CascadeShadowConfig>().unwrap().bounds,
            cascades.bounds
        );
        assert!(entity.contains::<VolumetricLight>());
        assert_eq!(
            entity.get::<SunDisk>().unwrap().angular_size,
            SunDisk::EARTH.angular_size
        );

        harness
            .app
            .world_mut()
            .entity_mut(sun)
            .remove::<(VolumetricLight, SunDisk)>();
        harness.app.update();
        harness.app.update();
        let entity = harness.app.world().entity(copy);
        assert!(!entity.contains::<VolumetricLight>());
        assert!(!entity.contains::<SunDisk>());
    }
}

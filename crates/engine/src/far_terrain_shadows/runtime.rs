//! Root draft: explicit startup option only. No installation occurs off.
//! Ready publications describe compact CPU uploads, never per-view drawable facts.
use super::{
    gpu::UploadLimits,
    runtime_owner::{Publication, SourceOwner, WorldState},
};
use crate::cluster::{package::ValidatedPackage, terrain::ClusterTerrain};
use bevy::{
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, RenderApp,
        camera::{ExtractedCamera, extract_cameras},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        renderer::RenderDevice,
        view::ExtractedView,
    },
};
use std::{collections::BTreeSet, sync::Arc};

pub struct FarTerrainShadowsPlugin;

#[derive(Resource, Default)]
struct OwnerState(Option<SourceOwner>);

#[derive(Resource, Default)]
struct WorldDemand(BTreeSet<u32>);

#[derive(Resource)]
struct UploadPolicy(UploadLimits);

#[derive(Resource, Default, Clone, ExtractResource)]
pub(crate) struct Published(pub(crate) Option<Arc<Publication>>);

impl Plugin for FarTerrainShadowsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnerState>()
            .init_resource::<WorldDemand>()
            .init_resource::<Published>()
            .add_systems(PostUpdate, sync_owner)
            .add_plugins(ExtractResourcePlugin::<Published>::default());
        super::render::install(app);
        let render = app
            .get_sub_app_mut(RenderApp)
            .expect("explicit terrain shadows require the render app");
        render.add_systems(
            ExtractSchedule,
            extract_world_demand
                .after(crate::cluster::render::SourceExtraction)
                .after(extract_cameras),
        );
    }

    fn finish(&self, app: &mut App) {
        let limits = app.world().resource::<RenderDevice>().limits();
        // Preliminary packet/upload bounds only. The material-specialized
        // descriptor (including selection binding 10) is checked independently.
        app.insert_resource(UploadPolicy(UploadLimits {
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_buffer_size: limits.max_buffer_size,
            max_storage_buffers_per_shader_stage: limits.max_storage_buffers_per_shader_stage,
            max_uniform_buffers_per_shader_stage: limits.max_uniform_buffers_per_shader_stage,
            max_uniform_buffer_binding_size: limits.max_uniform_buffer_binding_size,
            max_bind_groups: limits.max_bind_groups,
            existing_fragment_storage: 0,
            existing_fragment_uniforms: 0,
        }));
    }
}

fn sync_owner(
    package: Option<Res<ValidatedPackage>>,
    terrain: Option<Res<ClusterTerrain>>,
    policy: Res<UploadPolicy>,
    demand: Res<WorldDemand>,
    mut state: ResMut<OwnerState>,
    mut published: ResMut<Published>,
) {
    let (Some(package), Some(terrain)) = (package, terrain) else {
        if state.0.is_some() {
            state.0 = None;
        }
        if published.0.is_some() {
            published.0 = None;
        }
        return;
    };
    let heights = terrain.canonical_height_snapshot();
    if state
        .0
        .as_ref()
        .is_none_or(|owner| !owner.matches_source(&package.0, &heights))
    {
        state.0 = Some(SourceOwner::new(package.0.clone(), heights, policy.0));
        info!("preparing certified canonical terrain shadows in the background");
    }
    let owner = state.0.as_mut().expect("owner established above");
    for &world in &demand.0 {
        owner.request_world(world);
    }
    owner.poll();
    let next = owner.publication();
    if published
        .0
        .as_ref()
        .is_some_and(|old| Arc::ptr_eq(old, &next))
    {
        return;
    }
    let same_source = published
        .0
        .as_ref()
        .is_some_and(|old| old.source.same_allocation(&next.source));
    for (&world, status) in &next.worlds {
        let previous = same_source
            .then(|| published.0.as_ref().and_then(|old| old.worlds.get(&world)))
            .flatten();
        match status {
            WorldState::Pending => {}
            WorldState::Ready(upload) => {
                if !matches!(previous, Some(WorldState::Ready(old)) if Arc::ptr_eq(old, upload)) {
                    info!(
                        world,
                        revision = next.revision,
                        bytes = upload.upload().words.len() * 4,
                        "canonical terrain shadow CPU upload ready; per-view GPU proof remains separate"
                    );
                }
            }
            WorldState::Unavailable(reason) => {
                if !matches!(previous, Some(WorldState::Unavailable(old)) if old == reason) {
                    warn!(world, %reason, "canonical terrain shadow source unavailable");
                }
            }
        }
    }
    published.0 = Some(next);
}

fn extract_world_demand(
    domains: crate::cluster::render::ViewDomains,
    views: Query<&ExtractedView, With<ExtractedCamera>>,
    mut main: ResMut<MainWorld>,
) {
    // Demand is only prefetching. Render authorization separately rechecks the
    // current source, exact retained view and selected sun in the same frame.
    let worlds: BTreeSet<_> = views
        .iter()
        .filter_map(|view| domains.world(view.retained_view_entity).flatten())
        .collect();
    let mut demand = main.resource_mut::<WorldDemand>();
    if demand.0 != worlds {
        demand.0 = worlds;
    }
}

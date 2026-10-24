//! Filter owned render bodies in a private template before WorldAssetRoot can spawn them.

use super::{OwnershipCounters, OwnershipDeferred, OwnershipFacts, OwnershipReference};
use crate::cluster::{
    flat_source::OriginalStockScene,
    source_identity::{SourcePrimitive, SourcePrimitiveMesh},
};
use bevy::{asset::AssetId, prelude::*, world_serialization::WorldAssetRoot};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Nonrender source data for strict bounds/material checks and physics.
/// The embedded values are not ECS render components and are never extracted.
#[derive(Component, Clone, Reflect)]
#[reflect(Component)]
pub(crate) struct OwnedPrimitiveSource {
    pub(crate) mesh: Mesh3d,
    pub(crate) material: Option<MeshMaterial3d<StandardMaterial>>,
}

#[derive(Resource, Default)]
struct Templates(HashMap<(AssetId<WorldAsset>, Vec<u32>), Handle<WorldAsset>>);

pub(super) fn install(app: &mut App) {
    app.register_type::<OwnedPrimitiveSource>()
        .register_type::<SourcePrimitiveMesh>()
        .init_resource::<Templates>()
        .add_message::<AssetEvent<WorldAsset>>()
        .add_systems(Update, source_changes);
}

/// Clone all gameplay/source data, granting render bodies only to the fixed
/// certified refusal set. New source ordinals never acquire stock permission.
pub(crate) fn clone_with_stock_ordinals(
    source: &WorldAsset,
    stock: &BTreeSet<u32>,
    registry: &AppTypeRegistry,
) -> Result<WorldAsset, String> {
    let mut scene = source
        .clone_with(registry)
        .map_err(|error| format!("cannot clone owned source template: {error}"))?;
    let mut remove = Vec::new();
    let mut seen = HashSet::new();
    for node in scene.world.iter_entities() {
        if !node.contains::<Mesh3d>() {
            continue;
        }
        let tag = node
            .get::<SourcePrimitive>()
            .ok_or("owned source mesh is missing occurrence identity")?;
        if !tag.matches_default_scene || !seen.insert(tag.ordinal) {
            return Err("owned source occurrence identity is invalid or duplicated".to_owned());
        }
        if !stock.contains(&tag.ordinal) {
            remove.push(node.id());
        }
    }
    if !stock.iter().all(|ordinal| seen.contains(ordinal)) {
        return Err("owned source lost a permanently refused ordinal".to_owned());
    }
    for entity in remove {
        let mut node = scene.world.entity_mut(entity);
        let mesh = node
            .take::<Mesh3d>()
            .ok_or("owned source mesh disappeared")?;
        let material = node.take::<MeshMaterial3d<StandardMaterial>>();
        node.insert((
            SourcePrimitiveMesh(mesh.0.clone()),
            OwnedPrimitiveSource { mesh, material },
        ))
        .remove::<(
            MeshMaterial3d<crate::nif_depth::NifDepthMaterial>,
            MeshMaterial3d<crate::snow::SnowMaterial>,
            MeshMaterial3d<crate::effect_palette::EffectPaletteMaterial>,
            bevy::light::NotShadowCaster,
            bevy::light::NotShadowReceiver,
            bevy::camera::visibility::VisibilityClass,
            bevy::camera::visibility::ViewVisibility,
            bevy::camera::primitives::Aabb,
        )>();
    }
    Ok(scene)
}

/// A source/stock-allowlist pair shares one filtered asset across its placements.
/// An empty allowlist retains gameplay/source nodes with no render bodies.
/// The source asset remains complete and keeps the original dependency lifetime.
pub(crate) fn filtered_scene(
    world: &mut World,
    source: &Handle<WorldAsset>,
    stock: &BTreeSet<u32>,
) -> Result<Handle<WorldAsset>, String> {
    world.init_resource::<Templates>();
    let key = (source.id(), stock.iter().copied().collect::<Vec<_>>());
    if let Some(scene) = world.resource::<Templates>().0.get(&key) {
        return Ok(scene.clone());
    }
    let registry = world.resource::<AppTypeRegistry>().clone();
    // Partial promotion also uses this helper in apps without ownership systems.
    registry.write().register::<OwnedPrimitiveSource>();
    registry.write().register::<SourcePrimitiveMesh>();
    let scene = {
        let assets = world.resource::<Assets<WorldAsset>>();
        let original = assets
            .get(source)
            .ok_or("owned source scene is unavailable")?;
        clone_with_stock_ordinals(original, stock, &registry)?
    };
    let scene = world.resource_mut::<Assets<WorldAsset>>().add(scene);
    world
        .resource_mut::<Templates>()
        .0
        .insert(key, scene.clone());
    Ok(scene)
}

/// Keep the ordinary readiness failure path, but never give a raw source to the
/// spawner. A later AssetServer retry cannot instantiate an owned render body.
pub(crate) fn fail_closed(world: &mut World, root: Entity, source: Handle<WorldAsset>) {
    let empty = world
        .resource_mut::<Assets<WorldAsset>>()
        .add(WorldAsset::new(World::new()));
    world
        .entity_mut(root)
        .insert((WorldAssetRoot(empty), OriginalStockScene(source)))
        .remove::<(crate::streaming::PendingModel, OwnershipDeferred)>();
}

pub(crate) fn arm(world: &mut World, root: Entity, source: Handle<WorldAsset>) {
    let reference = world
        .get::<OwnershipReference>(root)
        .map(|reference| reference.0);
    let selection = reference.and_then(|reference| {
        world.get_resource::<OwnershipFacts>().map(|facts| {
            let mut stock = facts
                .certificate
                .as_ref()
                .and_then(|package| package.placed_reference(reference))
                .and_then(|placed| placed.stock_primitives.clone())
                .unwrap_or_default();
            stock.extend(facts.permanent_refused_ordinals(reference));
            (
                facts.can_spawn(reference),
                facts.references.contains_key(&reference),
                stock,
                facts.owned_ordinals(reference).len() as u64,
            )
        })
    });
    let Some((decided, mapped, stock, owned_count)) = selection else {
        world
            .entity_mut(root)
            .insert(WorldAssetRoot(source))
            .remove::<crate::streaming::PendingModel>();
        return;
    };
    if !decided {
        // Failed source loads can arrive before a terminal qualification. Leave
        // failure accounting to streaming, with no raw source capable of spawning.
        fail_closed(world, root, source);
        return;
    }
    if !mapped {
        // Wholly convert-refused and out-of-scope references keep the ordinary
        // loader. A mapped reference always uses its fixed stock allowlist,
        // including when every mapped record was initially material-refused.
        world
            .entity_mut(root)
            .insert(WorldAssetRoot(source))
            .remove::<(crate::streaming::PendingModel, OwnershipDeferred)>();
        return;
    }
    match filtered_scene(world, &source, &stock) {
        Ok(scene) => {
            let filtered = scene != source;
            let mut node = world.entity_mut(root);
            node.insert(WorldAssetRoot(scene))
                .remove::<(crate::streaming::PendingModel, OwnershipDeferred)>();
            if filtered {
                node.insert(OriginalStockScene(source));
                if let Some(mut counters) = world.get_resource_mut::<OwnershipCounters>() {
                    counters.owned_bodies_removed =
                        counters.owned_bodies_removed.saturating_add(owned_count);
                }
            }
        }
        Err(reason) => {
            error!(?root, %reason, "owned source instancing failed; no stock body is permitted");
            fail_closed(world, root, source);
        }
    }
}

fn source_changes(mut events: MessageReader<AssetEvent<WorldAsset>>, mut commands: Commands) {
    let changed: HashSet<_> = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(*id),
            _ => None,
        })
        .collect();
    if changed.is_empty() {
        return;
    }
    commands.queue(move |world: &mut World| {
        let refresh: Vec<_> = world.resource::<Templates>().0.iter()
            .filter(|((source, _), _)| changed.contains(source))
            .map(|((source, stock), scene)| (*source, stock.iter().copied().collect::<BTreeSet<_>>(), scene.clone()))
            .collect();
        let registry = world.resource::<AppTypeRegistry>().clone();
        for (source, stock, handle) in refresh {
            let result = world.resource::<Assets<WorldAsset>>().get(source)
                .ok_or_else(|| "owned source scene was removed".to_owned())
                .and_then(|scene| clone_with_stock_ordinals(scene, &stock, &registry));
            let scene = result.unwrap_or_else(|reason| {
                error!(%reason, ?source, "edited owned source refused; render bodies remain absent");
                WorldAsset::new(World::new())
            });
            if let Err(error) = world.resource_mut::<Assets<WorldAsset>>().insert(handle.id(), scene) {
                error!(%error, "cannot refresh filtered owned source");
            }
        }
    });
}

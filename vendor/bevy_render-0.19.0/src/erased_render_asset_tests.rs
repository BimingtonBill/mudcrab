use super::*;
use bevy_ecs::{system::RunSystemOnce, world::World};
use bevy_shader::Shader;

#[test]
fn prepared_asset_deltas_are_exact_and_drained_without_a_population_walk() {
    let mut assets = ErasedRenderAssets::<u32>::default();
    let used = AssetId::<Shader>::default();
    let unrelated = AssetId::<Shader>::invalid();
    assets.insert(used, 1);
    let first_revision = assets.asset_revision(used);
    assert_eq!(assets.take_changed_assets(), vec![used.untyped()]);
    assets.insert(unrelated, 2);
    assert_eq!(assets.asset_revision(used), first_revision);
    assert_eq!(assets.take_changed_assets(), vec![unrelated.untyped()]);
    assert!(assets.take_changed_assets().is_empty());
    assets.get(used);
    assert!(assets.take_changed_assets().is_empty());
    assets.get_mut(used);
    assert_eq!(assets.take_changed_assets(), vec![used.untyped()]);
    assets.remove(used);
    assert_eq!(assets.asset_revision(used), None);
    assert_eq!(assets.take_changed_assets(), vec![used.untyped()]);
}

#[test]
fn erased_assets_revision_tracks_insert_replace_and_remove() {
    let mut assets = ErasedRenderAssets::<u32>::default();
    let id = AssetId::<Shader>::default();
    assert_eq!(assets.revision(), 0);
    assert_eq!(assets.insert(id, 1), None);
    assert_eq!(assets.revision(), 1);
    assert_eq!(assets.insert(id, 2), Some(1));
    assert_eq!(assets.revision(), 2);
    assert_eq!(assets.remove(id), Some(2));
    assert_eq!(assets.revision(), 3);
    assert_eq!(assets.remove(id), None);
    assert_eq!(assets.revision(), 3);
}

#[test]
fn erased_assets_revision_tracks_mutable_access_but_not_reads_or_misses() {
    let mut assets = ErasedRenderAssets::<u32>::default();
    let id = AssetId::<Shader>::default();
    assets.insert(id, 1);
    assert_eq!(assets.get(id), Some(&1));
    assert_eq!(assets.iter().count(), 1);
    assert_eq!(assets.get_mut(AssetId::<Shader>::invalid()), None);
    assert_eq!(assets.revision(), 1);
    *assets.get_mut(id).unwrap() = 2;
    assert_eq!(assets.revision(), 2);
    for (_, value) in assets.iter_mut() {
        *value += 1;
    }
    assert_eq!(assets.get(id), Some(&3));
    assert_eq!(assets.revision(), 3);
}

struct RecoveryAsset;

impl ErasedRenderAsset for RecoveryAsset {
    type SourceAsset = Shader;
    type ErasedAsset = u32;
    type Param = ();

    fn prepare_asset(
        _: Shader,
        _: AssetId<Shader>,
        _: &mut SystemParamItem<Self::Param>,
    ) -> Result<u32, PrepareAssetError<Shader>> {
        Ok(1)
    }
}

#[test]
fn erased_assets_revision_tracks_device_recovery_removals_only() {
    let mut assets = ErasedRenderAssets::<u32>::default();
    assets.insert(AssetId::<Shader>::default(), 1);
    assets.insert(AssetId::<bevy_image::Image>::default(), 2);
    let mut world = World::new();
    world.insert_resource(assets);
    world.init_resource::<PrepareNextFrameAssets<RecoveryAsset>>();
    world
        .run_system_once(collect_erased_render_assets_to_reextract::<RecoveryAsset>)
        .unwrap();
    let assets = world.resource::<ErasedRenderAssets<u32>>();
    assert_eq!(assets.revision(), 3);
    assert_eq!(assets.get(AssetId::<Shader>::default()), None);
    assert_eq!(
        assets.get(AssetId::<bevy_image::Image>::default()),
        Some(&2)
    );
    world
        .run_system_once(collect_erased_render_assets_to_reextract::<RecoveryAsset>)
        .unwrap();
    assert_eq!(world.resource::<ErasedRenderAssets<u32>>().revision(), 3);
}

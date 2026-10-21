//! Actual GLB loader/WorldAsset-spawner evidence for public animation source ownership.
//!
//! Historical test names retain traceability. No cluster geometry tag, path cache, elapsed frame
//! count or collected Gltf root certifies animation readiness. The completed scene inventory does.
use super::*;
use crate::cluster::source_identity::{
    SourceStaticModel,
    actual_loader_tests::{loader_app_with, wait_for_loaded},
};
use bevy::asset::AssetId;
use std::{
    fs,
    time::{Duration, Instant},
};
const PATH: &str = "invented-static.glb";
fn static_glb() -> Vec<u8> {
    let mut binary = Vec::new();
    for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        binary.extend(value.to_le_bytes());
    }
    let document = serde_json::json!({
        "asset":{"version":"2.0"},
        "buffers":[{"byteLength":binary.len()}],
        "bufferViews":[{"buffer":0,"byteLength":binary.len()}],
        "accessors":[{
            "bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
            "min":[0,0,0],"max":[1,1,0]
        }],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
        "nodes":[{"mesh":0}],
        "scenes":[{"nodes":[0]}],"scene":0
    });
    let mut json = serde_json::to_vec(&document).unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = Vec::new();
    bytes.extend(b"glTF");
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend((12_u32 + 8 + json.len() as u32 + 8 + binary.len() as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    bytes.extend((binary.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(binary);
    bytes
}

fn empty_glb() -> Vec<u8> {
    let mut json = serde_json::to_vec(&serde_json::json!({
        "asset": {"version": "2.0"},
        "nodes": [],
        "scenes": [{"nodes": []}],
        "scene": 0
    }))
    .unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = Vec::new();
    bytes.extend(b"glTF");
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend((12_u32 + 8 + json.len() as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    bytes
}

struct Loaded {
    app: App,
    // App/assets drop before tempfile cleanup, including on assertion failure.
    _directory: tempfile::TempDir,
    source: Handle<WorldAsset>,
    path: &'static str,
    meshes: Vec<AssetId<Mesh>>,
}

fn loaded_bytes(path: &'static str, bytes: Vec<u8>) -> Loaded {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("OWNER.txt"),
        "Private animation actual-loader fixture\n",
    )
    .unwrap();
    fs::write(directory.path().join(path), bytes).unwrap();
    // Register the real extension BEFORE the real loader starts; injecting an inventory after
    // loading would erase precisely the evidence these regressions must retain.
    let mut app = loader_app_with(directory.path(), |app| {
        app.init_asset::<AnimationClip>()
            .init_asset::<AnimationGraph>()
            .add_plugins(ModelAnimationPlugin);
    });
    let root: Handle<Gltf> = app.world().resource::<AssetServer>().load(path);
    wait_for_loaded(&mut app, &root);
    let source = app
        .world()
        .resource::<Assets<Gltf>>()
        .get(&root)
        .unwrap()
        .scenes[0]
        .clone();
    let meshes = {
        let scenes = app.world().resource::<Assets<WorldAsset>>();
        let template = scenes.get(&source).unwrap();
        let mut inventories = template.world.try_query::<&SourceClips>().unwrap();
        assert_eq!(
            inventories.iter(&template.world).count(),
            1,
            "actual loader completed one inventory"
        );
        assert!(
            inventories
                .iter(&template.world)
                .next()
                .unwrap()
                .0
                .is_empty()
        );
        // A scene without meshes never registers `Mesh3d` in its world.
        template
            .world
            .try_query::<(Entity, &Mesh3d)>()
            .map(|mut query| {
                query
                    .iter(&template.world)
                    .map(|(entity, mesh)| {
                        assert_eq!(
                            template.world.get::<SourceStaticModel>(entity),
                            Some(&SourceStaticModel(true))
                        );
                        mesh.0.id()
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    drop(root);
    until(&mut app, |app| {
        app.world()
            .resource::<AssetServer>()
            .get_handle::<Gltf>(path)
            .is_none()
    });
    Loaded {
        app,
        _directory: directory,
        source,
        path,
        meshes,
    }
}
fn loaded() -> Loaded {
    loaded_bytes(PATH, static_glb())
}
fn until(app: &mut App, condition: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.update();
        if condition(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "actual loader/spawner observation timed out"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn reference(loaded: &mut Loaded, id: u32) -> Entity {
    let entity = loaded
        .app
        .world_mut()
        .spawn((
            MeshHandle(loaded.path.into()),
            FormId(id),
            WorldAssetRoot(loaded.source.clone()),
        ))
        .id();
    until(&mut loaded.app, |app| {
        app.world().get::<SourceClipBinding>(entity).is_some()
    });
    entity
}
fn static_answer(app: &App, reference: Entity) -> Entity {
    let binding = app
        .world()
        .get::<SourceClipBinding>(reference)
        .expect("completed live scene inventory");
    assert!(binding.graph.is_none() && binding.idle.is_none() && binding.open.is_none());
    assert!(binding.players.is_empty());
    assert!(
        app.world()
            .get::<SourceClips>(binding.source)
            .unwrap()
            .0
            .is_empty()
    );
    binding.source
}
fn live_meshes(app: &mut App, reference: Entity) -> Vec<(Entity, AssetId<Mesh>)> {
    let world = app.world();
    let mut pending = vec![reference];
    let mut result = Vec::new();
    while let Some(entity) = pending.pop() {
        if let Some(mesh) = world.get::<Mesh3d>(entity) {
            result.push((entity, mesh.0.id()));
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    result
}
fn no_root(loaded: &Loaded) {
    assert!(
        loaded
            .app
            .world()
            .resource::<AssetServer>()
            .get_handle::<Gltf>(loaded.path)
            .is_none()
    );
}

#[test]
fn cached_no_clip_for_a_tagged_source_does_not_acquire_source_proof_ownership() {
    let mut loaded = loaded();
    let first = reference(&mut loaded, 1);
    let second = reference(&mut loaded, 2);
    let first_source = static_answer(&loaded.app, first);
    let second_source = static_answer(&loaded.app, second);
    assert_ne!(
        first_source, second_source,
        "each reference records its own actual instance, never a path-cache proof"
    );
    loaded
        .app
        .world_mut()
        .entity_mut(first_source)
        .remove::<SourceClips>();
    loaded.app.update();
    assert!(loaded.app.world().get::<SourceClipBinding>(first).is_none());
    assert_eq!(static_answer(&loaded.app, second), second_source);
    no_root(&loaded);
}

#[test]
fn actual_static_scene_resolves_immediately_without_root_reload_or_scene_mesh_replacement() {
    let mut loaded = loaded();
    assert!(
        !loaded.meshes.is_empty(),
        "fixture contains real loader meshes"
    );
    let first = reference(&mut loaded, 1);
    let second = reference(&mut loaded, 2);
    let sources = [
        static_answer(&loaded.app, first),
        static_answer(&loaded.app, second),
    ];
    let meshes = live_meshes(&mut loaded.app, first);
    assert!(!meshes.is_empty());
    loaded
        .app
        .world_mut()
        .resource_mut::<Messages<AssetEvent<WorldAsset>>>()
        .drain()
        .for_each(drop);
    loaded
        .app
        .world_mut()
        .resource_mut::<Messages<AssetEvent<Mesh>>>()
        .drain()
        .for_each(drop);
    for _ in 0..244 {
        loaded.app.update();
        no_root(&loaded);
        assert_eq!(static_answer(&loaded.app, first), sources[0]);
        assert_eq!(static_answer(&loaded.app, second), sources[1]);
        assert_eq!(
            live_meshes(&mut loaded.app, first),
            meshes,
            "same live mesh entities and handles"
        );
        for mesh in &loaded.meshes {
            assert!(
                loaded
                    .app
                    .world()
                    .resource::<Assets<Mesh>>()
                    .contains(*mesh)
            );
        }
        assert!(
            loaded
                .app
                .world_mut()
                .resource_mut::<Messages<AssetEvent<WorldAsset>>>()
                .drain()
                .all(|e| !matches!(
                    e,
                    AssetEvent::Added { .. }
                        | AssetEvent::Modified { .. }
                        | AssetEvent::Removed { .. }
                ))
        );
        assert!(
            loaded
                .app
                .world_mut()
                .resource_mut::<Messages<AssetEvent<Mesh>>>()
                .drain()
                .all(|e| !matches!(
                    e,
                    AssetEvent::Added { .. }
                        | AssetEvent::Modified { .. }
                        | AssetEvent::Removed { .. }
                ))
        );
    }
}

#[test]
fn empty_untagged_and_explicitly_nonstatic_source_scenes_never_certify_no_animation() {
    // Geometry eligibility is deliberately no longer animation proof. Keep the three historical
    // counterexamples, but remove the completed inventory: none may manufacture its replacement.
    for variant in 0..3 {
        let mut loaded = loaded();
        let reference = reference(&mut loaded, 1);
        let source = static_answer(&loaded.app, reference);
        loaded
            .app
            .world_mut()
            .entity_mut(source)
            .remove::<SourceClips>();
        let meshes = live_meshes(&mut loaded.app, reference);
        for (entity, _) in meshes {
            match variant {
                0 => {
                    loaded.app.world_mut().entity_mut(entity).remove::<Mesh3d>();
                }
                1 => {
                    loaded
                        .app
                        .world_mut()
                        .entity_mut(entity)
                        .remove::<SourceStaticModel>();
                }
                _ => {
                    loaded
                        .app
                        .world_mut()
                        .entity_mut(entity)
                        .insert(SourceStaticModel(false));
                }
            }
        }
        for _ in 0..244 {
            loaded.app.update();
        }
        assert!(
            loaded
                .app
                .world()
                .get::<SourceClipBinding>(reference)
                .is_none()
        );
        no_root(&loaded);
    }
}

#[test]
fn actual_source_modification_evicts_no_clip_and_reclaims_its_existing_reference() {
    let mut loaded = loaded();
    let entity = reference(&mut loaded, 1);
    let old_source = static_answer(&loaded.app, entity);
    let scene_root: Entity;
    {
        let mut scenes = loaded.app.world_mut().resource_mut::<Assets<WorldAsset>>();
        let mut template = scenes.get_mut(&loaded.source).unwrap();
        let ids: Vec<_> = template
            .world
            .query_filtered::<Entity, With<SourceClips>>()
            .iter(&template.world)
            .collect();
        assert_eq!(ids.len(), 1, "the loader marks exactly one scene root");
        for id in &ids {
            template.world.entity_mut(*id).remove::<SourceClips>();
        }
        scene_root = ids[0];
    } // Real Assets mutation emits Modified; the real spawner replaces the live instance.
    until(&mut loaded.app, |app| {
        app.world().get_entity(old_source).is_err()
            && app.world().get::<SourceClipBinding>(entity).is_none()
    });
    // The world-asset spawner debounces `Modified` events that arrive within a few frames of the
    // previous one (it assumes they are not a real change), so the second edit waits them out.
    for _ in 0..4 {
        loaded.app.update();
    }
    {
        let mut scenes = loaded.app.world_mut().resource_mut::<Assets<WorldAsset>>();
        let mut template = scenes.get_mut(&loaded.source).unwrap();
        template
            .world
            .entity_mut(scene_root)
            .insert(SourceClips::default());
    }
    until(&mut loaded.app, |app| {
        app.world().get::<SourceClipBinding>(entity).is_some()
    });
    assert_ne!(static_answer(&loaded.app, entity), old_source);
    no_root(&loaded);
}

#[test]
fn actual_source_collection_evicts_no_clip_even_after_asset_server_path_disappears() {
    let mut loaded = loaded();
    let entity = reference(&mut loaded, 1);
    let source_instance = static_answer(&loaded.app, entity);
    let source_id = loaded.source.id();
    // A live instance owns its inventory even after its template/root is collected. End its
    // lifetime explicitly; the resolver must release only this reference's old binding.
    loaded
        .app
        .world_mut()
        .entity_mut(entity)
        .remove::<WorldAssetRoot>();
    loaded.app.world_mut().entity_mut(source_instance).despawn();
    let source = std::mem::take(&mut loaded.source);
    drop(source);
    until(&mut loaded.app, |app| {
        app.world()
            .resource::<AssetServer>()
            .get_path(source_id)
            .is_none()
            && !app
                .world()
                .resource::<Assets<WorldAsset>>()
                .contains(source_id)
    });
    assert!(
        loaded
            .app
            .world()
            .get::<SourceClipBinding>(entity)
            .is_none()
    );
    no_root(&loaded);
}

#[test]
fn empty_scene_root_derived_no_clip_survives_source_edits_without_repeated_root_loads() {
    let mut loaded = loaded_bytes("invented-empty-trigger.glb", empty_glb());
    assert!(loaded.meshes.is_empty());
    let references = [reference(&mut loaded, 1), reference(&mut loaded, 2)];
    let old = references.map(|e| static_answer(&loaded.app, e));
    {
        let mut scenes = loaded.app.world_mut().resource_mut::<Assets<WorldAsset>>();
        let mut template = scenes.get_mut(&loaded.source).unwrap();
        let temporary = template.world.spawn_empty().id();
        template.world.despawn(temporary);
    }
    until(&mut loaded.app, |app| {
        references.iter().zip(old).all(|(reference, old)| {
            app.world()
                .get::<SourceClipBinding>(*reference)
                .is_some_and(|b| b.source != old)
        })
    });
    for _ in 0..244 {
        loaded.app.update();
        for reference in references {
            static_answer(&loaded.app, reference);
        }
        no_root(&loaded);
    }
}

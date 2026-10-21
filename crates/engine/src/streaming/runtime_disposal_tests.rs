use super::*;

#[test]
fn shared_material_image_keeps_readiness_exact_but_does_not_pin_disposed_generated_bytes() {
    for image_path in ["textures/shared.ktx2", "Textures/Shared.ktx2"] {
        let (mut app, directory) = catalog_dependency_fixture();
        // Supply the case alias explicitly on case-sensitive test filesystems.
        let file = directory.path().join(image_path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, []).unwrap();
        app.world_mut().resource_mut::<StreamingRuntime>().catalog = replacement_catalog();
        let shared: Handle<Image> = app.world().resource::<AssetServer>().load(image_path);
        wait_for_catalog_asset(&mut app, &shared);
        // Keep the canonical identity live as well: discovery must not overwrite
        // the material's exact alias ID with the canonical path's loader IDs.
        let canonical: Handle<Image> = app
            .world()
            .resource::<AssetServer>()
            .load("textures/shared.ktx2");
        wait_for_catalog_asset(&mut app, &canonical);
        let (a, mesh_a, material_a) = replacement_surface(&mut app);
        let resource_a = bind_replacement(
            &mut app,
            1,
            CellDetail::Terrain,
            a,
            &["textures/shared.ktx2"],
        );
        let (b, _mesh_b, material_b) = replacement_surface(&mut app);
        let resource_b =
            bind_replacement(&mut app, 2, CellDetail::Full, b, &["textures/shared.ktx2"]);
        for material in [&material_a, &material_b] {
            app.world_mut()
                .resource_mut::<Assets<TerrainMaterial>>()
                .get_mut(material.id())
                .unwrap()
                .base
                .base_color_texture = Some(shared.clone());
        }
        let bridge = app.world().resource::<StreamingPreparationBridge>().clone();
        // Declare GPU observation before the first ownership pass. A headless
        // pass intentionally completes CPU-only preparation immediately.
        bridge.publish_fixture(1, vec![], false);
        observe_at(&mut app, 1);
        let runtime = app.world().resource::<StreamingRuntime>();
        let texture = runtime.resource_key("textures/shared.ktx2", None);
        for resource in [&resource_a, &resource_b, &texture] {
            assert!(
                runtime.watches[resource]
                    .demand
                    .images
                    .contains(&shared.id())
            );
        }
        let mut incomplete = watched_demands(&app);
        for demand in &mut incomplete {
            if demand.key == runtime.watches[&resource_a].demand.key
                || demand.key == runtime.watches[&resource_b].demand.key
            {
                demand.images.clear();
            }
        }
        bridge.publish_fixture(2, incomplete, true);
        observe_at(&mut app, 2);
        let runtime = app.world().resource::<StreamingRuntime>();
        assert!(!runtime.prepared_resources.contains(&resource_a));
        assert!(!runtime.prepared_resources.contains(&resource_b));
        assert!(runtime.ledger.totals().transient_bytes > 0);
        let preparation_demands = watched_demands(&app);
        bridge.publish_fixture(3, preparation_demands.clone(), true);
        observe_at(&mut app, 3);
        let runtime = app.world().resource::<StreamingRuntime>();
        assert!(runtime.prepared_resources.contains(&resource_a));
        assert!(runtime.prepared_resources.contains(&resource_b));
        assert_eq!(runtime.ledger.totals().transient_bytes, 0);
        let before = runtime.ledger.totals().resident_bytes;

        app.world_mut().despawn(a);
        app.world_mut()
            .resource_mut::<Assets<Mesh>>()
            .remove(mesh_a.id());
        app.world_mut()
            .resource_mut::<Assets<TerrainMaterial>>()
            .remove(material_a.id());
        observe_at(&mut app, 4);
        // Exact absence still waits for the two-frame disposal interval.
        let disposed = app.world().resource::<StreamingRuntime>().watches[&resource_a]
            .demand
            .clone();
        bridge.publish_absent_fixture(5, vec![disposed.clone()]);
        observe_at(&mut app, 5);
        assert!(
            app.world()
                .resource::<StreamingRuntime>()
                .ledger
                .contains_resource(&resource_a)
        );
        // An old observation with preparation IDs cannot certify disposal IDs.
        bridge.publish_absent_fixture(6, preparation_demands);
        observe_at(&mut app, 6);
        assert!(
            app.world()
                .resource::<StreamingRuntime>()
                .ledger
                .contains_resource(&resource_a)
        );
        assert!(!disposed.images.contains(&shared.id()));
        assert_eq!(disposed.meshes, vec![mesh_a.id()]);
        assert_eq!(
            disposed.materials,
            vec![bevy::asset::UntypedAssetId::from(material_a.id())]
        );
        bridge.publish_fixture(7, vec![disposed.clone()], true);
        observe_at(&mut app, 7);
        assert!(
            app.world()
                .resource::<StreamingRuntime>()
                .ledger
                .contains_resource(&resource_a)
        );
        // Only A's allocations are observed absent. B/shared are never reported
        // absent, and the shared CPU image remains bound to B's material.
        bridge.publish_absent_fixture(8, vec![disposed]);
        observe_at(&mut app, 8);
        let runtime = app.world().resource::<StreamingRuntime>();
        assert!(!runtime.ledger.contains_resource(&resource_a));
        assert!(runtime.ledger.resource_has_owners(&resource_b));
        assert!(runtime.ledger.resource_has_owners(&texture));
        assert!(
            runtime
                .ledger
                .owner_resources(&runtime.cells[&b].owner)
                .unwrap()
                .contains(&texture)
        );
        assert_eq!(runtime.ledger.totals().resident_bytes, before - (MIB + 120));
        assert_eq!(runtime.ledger.totals().resident_bytes, MIB + 420 + 512);
        assert_eq!(runtime.ledger.totals().orphan_bytes(), Some(0));
        assert!(
            app.world()
                .resource::<Assets<Image>>()
                .contains(shared.id())
        );
        assert_eq!(
            app.world()
                .resource::<Assets<TerrainMaterial>>()
                .get(material_b.id())
                .unwrap()
                .base
                .base_color_texture
                .as_ref()
                .unwrap()
                .id(),
            shared.id()
        );
    }
}

#[test]
fn private_generated_image_retains_cpu_and_gpu_disposal_proof() {
    let (mut app, _directory) = fixture(true);
    app.world_mut().resource_mut::<StreamingRuntime>().catalog = replacement_catalog();
    let (root, mesh, material) = replacement_surface(&mut app);
    let image = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::default());
    app.world_mut()
        .resource_mut::<Assets<TerrainMaterial>>()
        .get_mut(material.id())
        .unwrap()
        .base
        .base_color_texture = Some(image.clone());
    let resource = bind_replacement(&mut app, 1, CellDetail::Terrain, root, &[]);
    observe_at(&mut app, 1);
    let bridge = app.world().resource::<StreamingPreparationBridge>().clone();
    bridge.publish_fixture(2, watched_demands(&app), true);
    observe_at(&mut app, 2);
    app.world_mut().despawn(root);
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .remove(mesh.id());
    app.world_mut()
        .resource_mut::<Assets<TerrainMaterial>>()
        .remove(material.id());
    observe_at(&mut app, 3);
    let disposed = app.world().resource::<StreamingRuntime>().watches[&resource]
        .demand
        .clone();
    assert_eq!(disposed.images, vec![image.id()]);
    bridge.publish_absent_fixture(4, vec![disposed.clone()]);
    observe_at(&mut app, 5);
    assert!(
        app.world()
            .resource::<StreamingRuntime>()
            .ledger
            .contains_resource(&resource)
    );
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .remove(image.id());
    bridge.publish_fixture(6, vec![disposed.clone()], true);
    observe_at(&mut app, 6);
    assert!(
        app.world()
            .resource::<StreamingRuntime>()
            .ledger
            .contains_resource(&resource)
    );
    bridge.publish_absent_fixture(7, vec![disposed]);
    observe_at(&mut app, 7);
    assert!(
        !app.world()
            .resource::<StreamingRuntime>()
            .ledger
            .contains_resource(&resource)
    );
}

#[test]
fn another_owners_texture_claim_cannot_remove_an_unclaimed_image_from_disposal() {
    let (mut app, _directory) = catalog_dependency_fixture();
    app.world_mut().resource_mut::<StreamingRuntime>().catalog = replacement_catalog();
    let shared: Handle<Image> = app
        .world()
        .resource::<AssetServer>()
        .load("textures/shared.ktx2");
    wait_for_catalog_asset(&mut app, &shared);
    let (a, mesh_a, material_a) = replacement_surface(&mut app);
    let resource_a = bind_replacement(&mut app, 1, CellDetail::Terrain, a, &[]);
    let (b, _mesh_b, _material_b) = replacement_surface(&mut app);
    bind_replacement(&mut app, 2, CellDetail::Full, b, &["textures/shared.ktx2"]);
    app.world_mut()
        .resource_mut::<Assets<TerrainMaterial>>()
        .get_mut(material_a.id())
        .unwrap()
        .base
        .base_color_texture = Some(shared.clone());
    observe_at(&mut app, 1);
    app.world_mut().despawn(a);
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .remove(mesh_a.id());
    app.world_mut()
        .resource_mut::<Assets<TerrainMaterial>>()
        .remove(material_a.id());
    observe_at(&mut app, 2);
    let runtime = app.world().resource::<StreamingRuntime>();
    assert_eq!(
        runtime.watches[&resource_a].demand.images,
        vec![shared.id()]
    );
    assert!(!cpu_absent(app.world(), &runtime.watches[&resource_a]));
}

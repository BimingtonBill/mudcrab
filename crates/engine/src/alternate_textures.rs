//! Alternate textures of base models.
//!
//! A base record (a static, a tree, a door, ...) can name shapes of its model and give each a
//! texture set to draw instead of the one the mesh was authored with: `DirtCliffs01FieldGrass01`
//! draws the green `DirtCliffs01` slab with the field grass the terrain around it uses. The
//! converter keeps the list in `model_alternate_textures`; [`AssetCatalog`](crate::world::database::AssetCatalog)
//! reads it and the streamer puts it on each reference of that base. When the reference's
//! converted scene has been spawned, [`apply_alternate_textures`] loads the replacement maps of
//! the shapes the model has and, once every one is loaded, swaps them in. The reference's
//! readiness scan waits for that (see [`AlternateTextures`]), so the strict material validation
//! never sees a half-swapped model.
//!
//! The diffuse and normal maps are replaced, which is what the converted materials sample; the
//! shape keeps every other input (shader flags, alpha mode, specular and glow maps), and a texture
//! set that lacks a slot leaves the shape's own map in it. Shapes match on their name, which the
//! converter keeps as the glTF mesh name; the shape index is stored with the list but not needed
//! to find a shape.
use bevy::{
    gltf::GltfMeshName,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    world_serialization::WorldInstanceReady,
};
use std::{collections::HashMap, sync::Arc};

/// One shape's replacement textures, as converted asset paths relative to the asset root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternateTexture {
    pub shape_name: String,
    pub shape_index: u32,
    pub diffuse: Option<String>,
    pub normal: Option<String>,
}

/// The alternate textures of the base record a reference places. It stays on the reference until
/// they are applied (or have failed to load): the readiness scan treats a reference that still has
/// it as not ready.
#[derive(Component, Debug, Clone)]
pub struct AlternateTextures(pub Arc<[AlternateTexture]>);

/// Set on a reference once its model's scene is spawned, so its shapes exist to be retextured.
#[derive(Component)]
struct SceneSpawned;

/// Replacement images already requested, and the materials already built from them, so every
/// reference of a base shares one material per shape, as the scene's own materials are shared.
#[derive(Resource, Default)]
pub struct AlternateMaterials {
    images: HashMap<(String, bool), Handle<Image>>,
    built: HashMap<AlternateMaterialKey, Handle<StandardMaterial>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AlternateMaterialKey {
    original: AssetId<StandardMaterial>,
    diffuse: Option<String>,
    normal: Option<String>,
}

/// Adds the systems that apply a reference's alternate textures once its scene is spawned.
pub struct AlternateTexturesPlugin;

impl Plugin for AlternateTexturesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AlternateMaterials>()
            .add_observer(mark_scene_spawned)
            .add_systems(Update, apply_alternate_textures);
    }
}

fn mark_scene_spawned(
    ready: On<WorldInstanceReady>,
    references: Query<(), With<AlternateTextures>>,
    mut commands: Commands,
) {
    if references.contains(ready.entity) {
        commands.entity(ready.entity).insert(SceneSpawned);
    }
}

/// The replacement for the shape called `mesh_name`, matching names the way the converter keeps
/// them: a NIF's names are not case sensitive.
fn replacement_for<'list>(
    list: &'list [AlternateTexture],
    mesh_name: &str,
) -> Option<&'list AlternateTexture> {
    list.iter()
        .find(|alternate| alternate.shape_name.eq_ignore_ascii_case(mesh_name))
}

/// `original` with the replacement's maps in place of its own. A slot the replacement leaves empty
/// keeps the original's map. `image` yields the loaded handle of a path (`true` for a colour map).
fn replaced_material(
    original: &StandardMaterial,
    replacement: &AlternateTexture,
    mut image: impl FnMut(&str, bool) -> Handle<Image>,
) -> StandardMaterial {
    let mut material = original.clone();
    if let Some(path) = &replacement.diffuse {
        material.base_color_texture = Some(image(path, true));
    }
    if let Some(path) = &replacement.normal {
        material.normal_map_texture = Some(image(path, false));
    }
    material
}

/// Models repeat their textures across a shape, so the replacements must too; the converted glTF
/// images carry the same address modes.
fn model_texture_sampler() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: 16,
        ..ImageSamplerDescriptor::linear()
    }
}

/// The handle of a replacement image, requesting it the first time it is asked for.
fn requested_image(
    images: &mut HashMap<(String, bool), Handle<Image>>,
    asset_server: &AssetServer,
    path: &str,
    srgb: bool,
) -> Handle<Image> {
    images
        .entry((path.to_owned(), srgb))
        .or_insert_with(|| {
            asset_server
                .load_builder()
                .with_settings(move |settings: &mut ImageLoaderSettings| {
                    settings.sampler = ImageSampler::Descriptor(model_texture_sampler());
                    if !srgb {
                        // A normal map holds directions, not colours.
                        settings.is_srgb = false;
                    }
                })
                .load(path.to_owned())
        })
        .clone()
}

/// What retexturing one reference's shapes needs: which shapes get which replacement.
struct Retexture {
    shape: Entity,
    original: Handle<StandardMaterial>,
    replacement: AlternateTexture,
    mesh_name: String,
}

/// Gives the shapes of a spawned model the alternate textures of the reference that placed it,
/// once their images are loaded. A reference whose images failed to load keeps the model's own
/// textures and says so in the log.
#[allow(clippy::too_many_arguments)]
fn apply_alternate_textures(
    references: Query<(Entity, &AlternateTextures), With<SceneSpawned>>,
    children: Query<&Children>,
    shapes: Query<(&GltfMeshName, &MeshMaterial3d<StandardMaterial>)>,
    asset_server: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<AlternateMaterials>,
    mut commands: Commands,
) {
    for (root, alternates) in &references {
        let retextures: Vec<Retexture> = children
            .iter_descendants(root)
            .filter_map(|shape| {
                let (mesh_name, material) = shapes.get(shape).ok()?;
                let replacement = replacement_for(&alternates.0, &mesh_name.0)?;
                Some(Retexture {
                    shape,
                    original: material.0.clone(),
                    replacement: replacement.clone(),
                    mesh_name: mesh_name.0.clone(),
                })
            })
            .collect();
        // Request every image the shapes need, then wait until all of them are loaded.
        let wanted: Vec<Handle<Image>> = retextures
            .iter()
            .flat_map(|retexture| {
                [
                    (retexture.replacement.diffuse.as_deref(), true),
                    (retexture.replacement.normal.as_deref(), false),
                ]
            })
            .filter_map(|(path, srgb)| {
                path.map(|path| requested_image(&mut state.images, &asset_server, path, srgb))
            })
            .collect();
        if wanted
            .iter()
            .any(|handle| asset_server.load_state(handle.id()).is_failed())
        {
            error!(
                reference = ?root,
                "alternate textures failed to load; the model keeps the textures it was authored with"
            );
            commands
                .entity(root)
                .remove::<(AlternateTextures, SceneSpawned)>();
            continue;
        }
        if !wanted.iter().all(|handle| images.contains(handle.id())) {
            continue;
        }
        for retexture in retextures {
            let key = AlternateMaterialKey {
                original: retexture.original.id(),
                diffuse: retexture.replacement.diffuse.clone(),
                normal: retexture.replacement.normal.clone(),
            };
            let handle = match state.built.get(&key) {
                Some(handle) => handle.clone(),
                None => {
                    let Some(original) = materials.get(&retexture.original) else {
                        // The scene is ready, so its materials are in the store; a missing one
                        // means the model did not load as converted, which the readiness scan
                        // reports.
                        warn!(
                            "alternate textures: material of shape {} is not loaded",
                            retexture.mesh_name
                        );
                        continue;
                    };
                    let replaced =
                        replaced_material(original, &retexture.replacement, |path, srgb| {
                            requested_image(&mut state.images, &asset_server, path, srgb)
                        });
                    let handle = materials.add(replaced);
                    state.built.insert(key, handle.clone());
                    handle
                }
            };
            commands
                .entity(retexture.shape)
                .insert(MeshMaterial3d(handle));
        }
        commands
            .entity(root)
            .remove::<(AlternateTextures, SceneSpawned)>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{asset::uuid_handle, ecs::system::RunSystemOnce};

    fn alternate(name: &str, diffuse: Option<&str>, normal: Option<&str>) -> AlternateTexture {
        AlternateTexture {
            shape_name: name.to_owned(),
            shape_index: 0,
            diffuse: diffuse.map(str::to_owned),
            normal: normal.map(str::to_owned),
        }
    }

    #[test]
    fn a_shape_matches_its_replacement_by_name_without_regard_to_case() {
        let list = [
            alternate("DirtCliffs01:1", Some("landscape/fieldgrass01.ktx2"), None),
            alternate("Skirt", None, None),
        ];
        assert_eq!(
            replacement_for(&list, "dirtcliffs01:1").map(|found| found.shape_name.as_str()),
            Some("DirtCliffs01:1")
        );
        assert_eq!(replacement_for(&list, "DirtCliffs01:0"), None);
        assert_eq!(replacement_for(&[], "Skirt"), None);
    }

    #[test]
    fn a_replacement_swaps_only_the_slots_it_has() {
        let diffuse = uuid_handle!("00000000-0000-0000-0000-000000000001");
        let normal = uuid_handle!("00000000-0000-0000-0000-000000000002");
        let original = StandardMaterial {
            base_color_texture: Some(diffuse),
            normal_map_texture: Some(normal.clone()),
            perceptual_roughness: 0.25,
            ..default()
        };
        let replacement = alternate("Slab", Some("landscape/fieldgrass01.ktx2"), None);
        let mut asked = Vec::new();
        let replaced = replaced_material(&original, &replacement, |path, srgb| {
            asked.push((path.to_owned(), srgb));
            uuid_handle!("00000000-0000-0000-0000-000000000003")
        });
        assert_eq!(
            asked,
            vec![("landscape/fieldgrass01.ktx2".to_owned(), true)]
        );
        assert_eq!(
            replaced.base_color_texture.as_ref().map(Handle::id),
            Some(uuid_handle!("00000000-0000-0000-0000-000000000003").id())
        );
        assert_eq!(replaced.normal_map_texture, Some(normal));
        assert_eq!(replaced.perceptual_roughness, 0.25);

        let both = alternate("Slab", Some("a.ktx2"), Some("a_n.ktx2"));
        let mut asked = Vec::new();
        replaced_material(&original, &both, |path, srgb| {
            asked.push((path.to_owned(), srgb));
            Handle::default()
        });
        assert_eq!(
            asked,
            vec![("a.ktx2".to_owned(), true), ("a_n.ktx2".to_owned(), false)]
        );
    }

    /// An app with the asset stores the system reads, the replacement image already in memory, and
    /// two references of one base each with a named shape and an untouched one.
    fn app_with_two_references() -> (App, Handle<StandardMaterial>, [[Entity; 3]; 2]) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_resource::<AlternateMaterials>();
        let original = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        app.world_mut()
            .resource_mut::<AlternateMaterials>()
            .images
            .insert(("landscape/fieldgrass01.ktx2".to_owned(), true), image);
        let list: Arc<[AlternateTexture]> = Arc::from(vec![alternate(
            "Slab:1",
            Some("landscape/fieldgrass01.ktx2"),
            None,
        )]);
        let mut spawn_reference = || {
            let world = app.world_mut();
            let grass = world
                .spawn((
                    GltfMeshName("Slab:1".to_owned()),
                    MeshMaterial3d(original.clone()),
                ))
                .id();
            let rock = world
                .spawn((
                    GltfMeshName("Slab:0".to_owned()),
                    MeshMaterial3d(original.clone()),
                ))
                .id();
            let root = world
                .spawn((AlternateTextures(list.clone()), SceneSpawned))
                .add_children(&[grass, rock])
                .id();
            [root, grass, rock]
        };
        let first = spawn_reference();
        let second = spawn_reference();
        (app, original, [first, second])
    }

    #[test]
    fn a_reference_swaps_only_the_named_shape_and_shares_the_material_with_the_next() {
        let (mut app, original, [first, second]) = app_with_two_references();
        app.world_mut()
            .run_system_once(apply_alternate_textures)
            .unwrap();
        let material = |app: &App, entity: Entity| {
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(entity)
                .unwrap()
                .0
                .clone()
        };
        assert_eq!(
            material(&app, first[2]),
            original,
            "the other shape keeps its material"
        );
        assert_ne!(material(&app, first[1]), original);
        assert_eq!(
            material(&app, first[1]),
            material(&app, second[1]),
            "two references of a base share one built material"
        );
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        let built = materials.get(&material(&app, first[1])).unwrap();
        assert!(built.base_color_texture.is_some());
        for root in [first[0], second[0]] {
            assert!(
                app.world().get::<AlternateTextures>(root).is_none(),
                "applied references are ready for the readiness scan"
            );
        }
    }

    #[test]
    fn a_reference_waits_while_its_replacement_image_is_still_loading() {
        let (mut app, original, [first, _]) = app_with_two_references();
        // Forget the loaded image: the system now asks the asset server for a file that is not
        // there, which is neither loaded nor yet failed on the first frame.
        app.world_mut()
            .resource_mut::<AlternateMaterials>()
            .images
            .clear();
        app.world_mut()
            .run_system_once(apply_alternate_textures)
            .unwrap();
        let shape = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(first[1])
            .unwrap();
        assert_eq!(
            shape.0, original,
            "nothing is swapped before the image is in"
        );
        assert!(
            app.world().get::<AlternateTextures>(first[0]).is_some(),
            "the reference stays not ready"
        );
    }
}

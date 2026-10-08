//! The sky dome each camera draws, coloured from the space that camera is showing.
//!
//! `crate::sky` is upstream main's dome: the measured weight table of `meshes/sky/atmosphere.nif`,
//! the [`SkyMaterial`] and the shader that draw it, and a plugin - `SkyPlugin` - this branch does
//! **not** add, because it writes every marked camera's clear colour and distance fog from one
//! global palette, which would fight [`crate::atmosphere`] and [`crate::portal`]'s per-space
//! atmosphere every frame (see `local/merge/port-prep-sky.md`). What is ported here is the dome
//! itself, as presentation only: nothing in this module writes a clear colour, fog, ambient light
//! or sun.
//!
//! The port is a module of its own rather than a rewrite of `sky.rs` so that `sky.rs` stays
//! byte-identical to main's and the next catch-up with main has nothing to resolve in it.
//!
//! One dome per camera, on that camera's own render layers. The main camera draws layers 0 and 1
//! and the doorway's camera draws only [`crate::portal::DESTINATION_LAYER`], so each dome is drawn
//! in exactly one view; a single dome on both would hang the main view's sky in the doorway. Each
//! dome's colours come from its own camera's space ([`CameraSky`]), which is what lets a doorway
//! open off a lit street into a cave and show the cave's own murk.

use crate::sky::{SkyCamera, SkyDome, SkyMaterial, SkyPalette, SkyUniform, dome_mesh};
use bevy::{
    asset::embedded_asset,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    light::NotShadowCaster,
    pbr::MaterialPlugin,
    prelude::*,
    transform::TransformSystems,
};

/// The colours one camera's sky is drawn with, written by whichever system knows the space that
/// camera shows: [`crate::atmosphere`]'s `update_atmosphere` for the main camera, from
/// [`crate::streaming::ActiveCell`], and [`crate::portal`]'s `update_destination_atmosphere` for
/// the doorway's, from the door's destination.
///
/// `None` is a space with no sky to draw - an interior, a space whose row clears `has_sky`, or an
/// underground worldspace the database does not know - and hides that camera's dome. A camera with
/// no `CameraSky` at all is hidden too, and deliberately: a run whose atmosphere never wrote one has
/// no space to colour a dome from, so a room the engine has no record of shows no sky rather than
/// the engine's default one.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CameraSky {
    pub palette: Option<SkyPalette>,
}

/// The sky dome, for the cameras marked with [`SkyCamera`](crate::sky::SkyCamera).
///
/// Added by `crate::app::run`; it is one plugin because the dome is one thing - the mesh, its
/// material and the three systems that keep a dome on its camera and in its camera's colours.
pub struct SkyDomePlugin;

impl Plugin for SkyDomePlugin {
    fn build(&self, app: &mut App) {
        // Where `SkyMaterial` looks for its shader (`crate::sky`'s `SKY_SHADER`). Embedded from
        // this file, not that one, because `SkyPlugin` is never added - the macro's path is
        // derived from the asset's path and the crate's `src/` (`bevy_asset`'s
        // `_embedded_asset_path`), and both files sit in `src/`, so the two produce one path.
        embedded_asset!(app, "shaders/sky.wgsl");
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .add_systems(
                PostUpdate,
                (spawn_sky_domes, follow_sky_cameras, colour_sky_domes)
                    .chain()
                    // The follow writes a `Transform`, and the propagation at the end of
                    // `PostUpdate` is what turns it into the `GlobalTransform` this frame extracts.
                    .before(TransformSystems::Propagate),
            );
    }
}

/// One dome per [`SkyCamera`], on that camera's own render layers plus the reflection replica layer
/// when those layers are the world's ([`crate::render::reflectable_layers`]): a sky seen through
/// layer 0 is reflected on both shadow routes, and a doorway's sky (on
/// [`crate::portal::DESTINATION_LAYER`]) is reflected by neither.
///
/// The dome starts hidden and repainted by [`colour_sky_domes`] further down the same chain: on the
/// first frame in an interior there is no space written to the camera yet, and a bright dome for
/// that one frame would be a worse guess than none.
fn spawn_sky_domes(
    mut commands: Commands,
    cameras: Query<(Entity, &Transform, Option<&RenderLayers>), Added<SkyCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    for (camera, transform, layers) in &cameras {
        commands.spawn((
            Name::new("Sky dome"),
            SkyDome { camera },
            Mesh3d(meshes.add(dome_mesh())),
            MeshMaterial3d(materials.add(SkyMaterial::new(&SkyPalette::SKYRIM_CLEAR_DAY))),
            Transform::from_translation(transform.translation),
            Visibility::Hidden,
            // The camera's own layers, so this dome is drawn by the camera it follows and by no
            // other view. A camera without any renders layer 0, which is `RenderLayers`' default.
            // A dome of a world camera carries the replica layer beside them
            // (`crate::render::reflectable_layers`): the shadowed reflection route draws the sky
            // through the world layer, so the shadowless one has to draw it too, and the portal
            // camera's dome (the destination's sky) is left on `DESTINATION_LAYER` alone.
            crate::render::reflectable_layers(&layers.cloned().unwrap_or_default()),
            NotShadowCaster,
            // The dome always surrounds its camera.
            NoFrustumCulling,
        ));
    }
}

/// Keeps each dome centred on its camera's position; the dome never rotates. The cameras this
/// follows are root entities, so their `Transform` is their world position, and a dome left behind
/// by a despawned camera goes with it.
fn follow_sky_cameras(
    mut commands: Commands,
    cameras: Query<&Transform, (With<SkyCamera>, Without<SkyDome>)>,
    mut domes: Query<(Entity, &SkyDome, &mut Transform)>,
) {
    for (entity, dome, mut transform) in &mut domes {
        match cameras.get(dome.camera) {
            Ok(camera) => {
                if transform.translation != camera.translation {
                    transform.translation = camera.translation;
                }
            }
            Err(_) => commands.entity(entity).despawn(),
        }
    }
}

/// Paints every dome with its own camera's [`CameraSky`], and hides it where that space has no sky.
///
/// The material is compared before it is written rather than rebuilt every frame: `Assets::get_mut`
/// marks the material changed, and a changed material marks the mesh changed for the next
/// extraction. Comparing is what makes this system self-healing as well - a dome that missed a
/// write is repainted on the next frame - which is the reason `crate::portal`'s destination sun
/// compares rather than tracking changes too.
fn colour_sky_domes(
    cameras: Query<&CameraSky>,
    mut domes: Query<(&SkyDome, &MeshMaterial3d<SkyMaterial>, &mut Visibility)>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    for (dome, material, mut visibility) in &mut domes {
        let Some(palette) = cameras.get(dome.camera).ok().and_then(|sky| sky.palette) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let wanted = SkyUniform::new(&palette);
        if let Some(mut sky) = materials.get_mut(&material.0)
            && sky.sky != wanted
        {
            sky.sky = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portal::DESTINATION_LAYER;

    /// The layers `app::setup_world` gives the main camera.
    const MAIN_LAYERS: [usize; 2] = [0, 1];

    /// The plugin's systems, without `SkyDomePlugin` itself: the same shape `crate::sky`'s own
    /// tests build, so a test here needs no asset server, no material plugin and no GPU.
    fn dome_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<SkyMaterial>>()
            .add_systems(
                Update,
                (spawn_sky_domes, follow_sky_cameras, colour_sky_domes).chain(),
            );
        app
    }

    /// A camera with the markers and layers of one of the engine's two sky cameras, and the space
    /// its dome is coloured from.
    fn spawn_camera(app: &mut App, layers: RenderLayers, palette: Option<SkyPalette>) -> Entity {
        app.world_mut()
            .spawn((
                Camera::default(),
                Transform::from_xyz(100.0, 250.0, -40.0),
                SkyCamera,
                CameraSky { palette },
                layers,
            ))
            .id()
    }

    /// Every dome in the world, as its entity, its camera and its layers.
    fn domes(app: &mut App) -> Vec<(Entity, SkyDome, RenderLayers, Visibility)> {
        let mut query = app
            .world_mut()
            .query::<(Entity, &SkyDome, &RenderLayers, &Visibility)>();
        let mut found: Vec<_> = query
            .iter(app.world())
            .map(|(entity, dome, layers, visibility)| (entity, *dome, layers.clone(), *visibility))
            .collect();
        found.sort_by_key(|(entity, ..)| *entity);
        found
    }

    /// The dome of one camera.
    fn dome_of(app: &mut App, camera: Entity) -> (Entity, SkyDome, RenderLayers, Visibility) {
        let found: Vec<_> = domes(app)
            .into_iter()
            .filter(|(_, dome, ..)| dome.camera == camera)
            .collect();
        assert_eq!(found.len(), 1, "one dome per sky camera");
        found[0].clone()
    }

    /// The uniform the dome's material is drawn with.
    fn drawn_colours(app: &App, dome: Entity) -> SkyUniform {
        let handle = app
            .world()
            .get::<MeshMaterial3d<SkyMaterial>>(dome)
            .expect("the dome carries a material")
            .0
            .clone();
        app.world()
            .resource::<Assets<SkyMaterial>>()
            .get(&handle)
            .expect("the material is in the world")
            .sky
            .clone()
    }

    /// A palette that is not the engine's default, so a dome taking the wrong camera's colours
    /// cannot pass by accident.
    fn destination_palette() -> SkyPalette {
        SkyPalette {
            upper: Color::srgb_u8(0, 169, 183),
            lower: Color::srgb_u8(0, 0, 0),
            horizon: Color::srgb_u8(14, 156, 156),
            fog_far: Color::srgb_u8(14, 156, 156),
            brightness: 1.0,
        }
    }

    /// The two cameras this branch has - the main camera on layers 0 and 1 and the doorway's on
    /// `DESTINATION_LAYER` - each get a dome of their own, on disjoint layers. One entity on both
    /// layers would be the main view's sky hanging in the doorway. The world camera's dome also
    /// carries the reflection replica layer (impl-1094), and the doorway's does not: the sky hangs
    /// in the water reflection exactly as the rest of the world does, and the destination's sky
    /// stays inside the doorway.
    #[test]
    fn each_sky_camera_gets_one_dome_on_its_own_layers() {
        let mut app = dome_app();
        let main = spawn_camera(
            &mut app,
            RenderLayers::from_layers(&MAIN_LAYERS),
            Some(SkyPalette::SKYRIM_CLEAR_DAY),
        );
        let portal = spawn_camera(
            &mut app,
            RenderLayers::layer(DESTINATION_LAYER),
            Some(destination_palette()),
        );
        app.update();

        let found = domes(&mut app);
        assert_eq!(found.len(), 2, "one dome per sky camera");
        let (_, main_dome, main_layers, _) = dome_of(&mut app, main);
        let (_, portal_dome, portal_layers, _) = dome_of(&mut app, portal);
        assert_eq!(main_dome.camera, main);
        assert_eq!(portal_dome.camera, portal);
        assert_eq!(
            main_layers,
            crate::render::reflectable_layers(&RenderLayers::from_layers(&MAIN_LAYERS)),
            "the world camera's dome is reflected on either shadow route"
        );
        assert_eq!(portal_layers, RenderLayers::layer(DESTINATION_LAYER));
        assert!(
            !main_layers.intersects(&portal_layers),
            "a dome may not be drawn by both cameras: {main_layers:?} against {portal_layers:?}"
        );

        // A camera without the marker gets nothing, and the dome's transform starts on its camera.
        app.world_mut()
            .spawn((Camera::default(), Transform::default()));
        app.update();
        assert_eq!(domes(&mut app).len(), 2);
        let (main_dome_entity, ..) = dome_of(&mut app, main);
        assert_eq!(
            app.world()
                .get::<Transform>(main_dome_entity)
                .unwrap()
                .translation,
            Vec3::new(100.0, 250.0, -40.0)
        );
    }

    /// A camera whose space has no sky - an interior, or a space the atmosphere has not written -
    /// has its dome hidden, and gets it back when its space has one again.
    #[test]
    fn a_camera_in_a_space_without_a_sky_has_its_dome_hidden() {
        let mut app = dome_app();
        let camera = spawn_camera(&mut app, RenderLayers::default(), None);
        app.update();
        let (dome, ..) = dome_of(&mut app, camera);
        assert_eq!(
            app.world().get::<Visibility>(dome).unwrap(),
            &Visibility::Hidden,
            "no sky in this space"
        );

        // And a camera nothing has told anything about is hidden as well.
        let unwritten = spawn_camera(&mut app, RenderLayers::default(), None);
        app.world_mut().entity_mut(unwritten).remove::<CameraSky>();
        app.update();
        let (dome, ..) = dome_of(&mut app, unwritten);
        assert_eq!(
            app.world().get::<Visibility>(dome).unwrap(),
            &Visibility::Hidden,
            "a camera with no space written is not shown the engine's default sky"
        );

        app.world_mut().entity_mut(camera).insert(CameraSky {
            palette: Some(SkyPalette::SKYRIM_CLEAR_DAY),
        });
        app.update();
        let (dome, ..) = dome_of(&mut app, camera);
        assert_eq!(
            app.world().get::<Visibility>(dome).unwrap(),
            &Visibility::Inherited
        );
    }

    /// The whole point of a dome per camera: the doorway's dome wears the *destination's* colours
    /// while the main camera's wears the space the player stands in, in the same frame.
    #[test]
    fn each_dome_takes_the_colours_of_its_own_cameras_space() {
        let mut app = dome_app();
        let main = spawn_camera(
            &mut app,
            RenderLayers::from_layers(&MAIN_LAYERS),
            Some(SkyPalette::SKYRIM_CLEAR_DAY),
        );
        let portal = spawn_camera(
            &mut app,
            RenderLayers::layer(DESTINATION_LAYER),
            Some(destination_palette()),
        );
        app.update();

        let (main_dome, ..) = dome_of(&mut app, main);
        let (portal_dome, ..) = dome_of(&mut app, portal);
        let main_colours = drawn_colours(&app, main_dome);
        let portal_colours = drawn_colours(&app, portal_dome);
        assert_eq!(main_colours, SkyUniform::new(&SkyPalette::SKYRIM_CLEAR_DAY));
        assert_eq!(portal_colours, SkyUniform::new(&destination_palette()));
        assert_ne!(
            main_colours, portal_colours,
            "the doorway is not drawn with the space around it"
        );

        // A repaint follows the camera's space, and only when it changes: the material is the same
        // asset before and after a frame that writes the same palette again.
        let handle = |app: &App, dome: Entity| {
            app.world()
                .get::<MeshMaterial3d<SkyMaterial>>(dome)
                .unwrap()
                .0
                .clone()
        };
        let before = handle(&app, portal_dome);
        app.update();
        assert_eq!(handle(&app, portal_dome), before);
        assert_eq!(drawn_colours(&app, portal_dome), portal_colours);

        app.world_mut().entity_mut(portal).insert(CameraSky {
            palette: Some(SkyPalette::SKYRIM_CLEAR_DAY),
        });
        app.update();
        assert_eq!(
            drawn_colours(&app, portal_dome),
            SkyUniform::new(&SkyPalette::SKYRIM_CLEAR_DAY),
            "the doorway's dome follows the destination the portal is drawing"
        );
    }

    /// The dome is presentation: nothing in this plugin may write the clear colour, the distance
    /// fog, the ambient light or a sun, which `crate::atmosphere` and `crate::portal` own. A second
    /// writer of any of them is the fight this port exists to avoid.
    #[test]
    fn no_dome_system_writes_the_clear_colour_the_fog_the_ambient_or_the_sun() {
        let mut app = dome_app();
        app.insert_resource(ClearColor(Color::srgb(0.1, 0.2, 0.3)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.4, 0.5, 0.6),
                brightness: 123.0,
                ..default()
            });
        let clear = Color::srgb(0.7, 0.1, 0.1);
        let camera = app
            .world_mut()
            .spawn((
                Camera {
                    clear_color: ClearColorConfig::Custom(clear),
                    ..default()
                },
                Transform::default(),
                SkyCamera,
                CameraSky {
                    palette: Some(destination_palette()),
                },
                RenderLayers::from_layers(&MAIN_LAYERS),
            ))
            .id();
        // The fog, the ambient and the sun a camera of this engine may already carry, written by
        // the systems that own them.
        let fog = DistanceFog {
            color: Color::srgb(0.2, 0.3, 0.4),
            falloff: bevy::pbr::FogFalloff::Linear {
                start: 10.0,
                end: 20.0,
            },
            ..default()
        };
        app.world_mut().entity_mut(camera).insert((
            fog.clone(),
            AmbientLight {
                color: Color::srgb(0.5, 0.5, 0.5),
                brightness: 456.0,
                ..default()
            },
        ));
        let sun = app
            .world_mut()
            .spawn(DirectionalLight {
                color: Color::srgb(0.9, 0.8, 0.7),
                illuminance: 7_000.0,
                ..default()
            })
            .id();

        for _ in 0..3 {
            app.update();
        }

        let entity = app.world().entity(camera);
        match entity.get::<Camera>().unwrap().clear_color {
            ClearColorConfig::Custom(colour) => assert_eq!(
                colour, clear,
                "the clear colour belongs to the space's atmosphere"
            ),
            other => panic!("the dome rewrote the clear colour to {other:?}"),
        }
        assert_eq!(
            app.world().resource::<ClearColor>().0,
            Color::srgb(0.1, 0.2, 0.3)
        );
        let written = entity.get::<DistanceFog>().expect("the fog is still there");
        assert_eq!(written.color, fog.color);
        match (&written.falloff, &fog.falloff) {
            (
                bevy::pbr::FogFalloff::Linear { start, end },
                bevy::pbr::FogFalloff::Linear {
                    start: want_start,
                    end: want_end,
                },
            ) => assert_eq!((start, end), (want_start, want_end)),
            other => panic!("the dome rewrote the falloff: {other:?}"),
        }
        let ambient = entity
            .get::<AmbientLight>()
            .expect("the camera's ambient is still there");
        assert_eq!(
            (ambient.color, ambient.brightness),
            (Color::srgb(0.5, 0.5, 0.5), 456.0)
        );
        let global = app.world().resource::<GlobalAmbientLight>();
        assert_eq!(
            (global.color, global.brightness),
            (Color::srgb(0.4, 0.5, 0.6), 123.0)
        );
        let mut lights = app.world_mut().query::<&DirectionalLight>();
        let found: Vec<_> = lights.iter(app.world()).cloned().collect();
        assert_eq!(found.len(), 1, "no dome system spawns a sun");
        assert_eq!(found[0].color, Color::srgb(0.9, 0.8, 0.7));
        assert_eq!(found[0].illuminance, 7_000.0);
        let _ = sun;
    }

    /// The dome is a child of nothing: it follows its camera and goes when the camera does.
    #[test]
    fn a_dome_follows_its_camera_and_leaves_with_it() {
        let mut app = dome_app();
        let camera = spawn_camera(
            &mut app,
            RenderLayers::from_layers(&MAIN_LAYERS),
            Some(SkyPalette::SKYRIM_CLEAR_DAY),
        );
        app.update();
        let (dome, _, _, _) = dome_of(&mut app, camera);

        {
            let mut transform = app.world_mut().get_mut::<Transform>(camera).unwrap();
            transform.translation = Vec3::new(-8_000.0, 1_200.0, 4_096.0);
            transform.rotation = Quat::from_rotation_y(2.0);
        }
        app.update();
        let transform = *app.world().get::<Transform>(dome).unwrap();
        assert_eq!(transform.translation, Vec3::new(-8_000.0, 1_200.0, 4_096.0));
        assert_eq!(transform.rotation, Quat::IDENTITY, "the dome never turns");

        app.world_mut().despawn(camera);
        app.update();
        assert!(domes(&mut app).is_empty());
    }

    /// The plugin embeds the dome's shader where `SkyMaterial` looks for it. Both are strings, and
    /// a mismatch is a dome that is drawn with a missing shader - invisible at runtime and silent
    /// in every other test here.
    #[test]
    fn the_dome_shader_is_embedded_where_the_material_looks_for_it() {
        use bevy::{asset::io::embedded::EmbeddedAssetRegistry, shader::ShaderRef};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), SkyDomePlugin));
        let ShaderRef::Path(asked) = SkyMaterial::fragment_shader() else {
            panic!("the dome's shader is a path");
        };
        let registry = app.world().resource::<EmbeddedAssetRegistry>();
        assert!(
            registry.remove_asset(asked.path()).is_some(),
            "nothing is embedded at {asked:?}, which is where the material asks for it"
        );
    }
}

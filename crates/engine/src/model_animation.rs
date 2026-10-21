//! Authored model clips owned by their loaded scene, without a second glTF load.
use crate::{
    doors::LoadDoor,
    world::components::{FormId, MeshHandle},
};
use bevy::{
    animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex},
    asset::LoadContext,
    gltf::{
        GltfLoaderSettings,
        extensions::{ErasedGltfExtensionHandler, GltfExtensionHandler, GltfExtensionHandlers},
    },
    prelude::*,
};

/// Document inventory copied with each scene instance. Empty is proof of no named clips;
/// absence means unresolved. Scene reload replaces this component and its instance entity.
#[derive(Component, Reflect, Clone, Default, Debug)]
#[reflect(Component)]
pub struct SourceClips(pub Vec<(String, Handle<AnimationClip>)>);

#[derive(Default, Clone)]
struct ClipInventory {
    clips: SourceClips,
    complete: bool,
}
impl GltfExtensionHandler for ClipInventory {
    fn dyn_clone(&self) -> Box<dyn ErasedGltfExtensionHandler> {
        Box::new(self.clone())
    }
    fn on_root(
        &mut self,
        _context: &mut LoadContext<'_>,
        gltf: &bevy::gltf::gltf::Gltf,
        settings: &GltfLoaderSettings,
    ) {
        self.clips.0.clear();
        self.complete = settings.load_animations || gltf.animations().next().is_none();
        // An intentionally disabled animation loader is not proof of a static source.
        if !settings.load_animations && gltf.animations().next().is_some() {
            warn!("authored model animations disabled in loader settings");
        }
    }
    fn on_animations_collected(
        &mut self,
        _context: &mut LoadContext<'_>,
        _animations: &[Handle<AnimationClip>],
        named: &bevy::platform::collections::HashMap<Box<str>, Handle<AnimationClip>>,
        _roots: &bevy::platform::collections::HashSet<usize>,
    ) {
        self.clips.0 = named
            .iter()
            .map(|(name, clip)| (name.to_string(), clip.clone()))
            .collect();
    }
    fn on_scene_completed(
        &mut self,
        _context: &mut LoadContext<'_>,
        _scene: &bevy::gltf::gltf::Scene<'_>,
        root: Entity,
        world: &mut World,
    ) {
        if self.complete {
            world.entity_mut(root).insert(self.clips.clone());
        }
    }
}

/// Install ordinary playback only where the public capsule owns the live world.
pub fn add_ordinary_animation_plugins(app: &mut App) {
    if app
        .world()
        .resource::<crate::config::EngineConfig>()
        .interactive_world_physics()
    {
        if !app.is_plugin_added::<ModelAnimationPlugin>() {
            app.add_plugins(ModelAnimationPlugin);
        }
        if !app.is_plugin_added::<crate::door_animation::DoorAnimationPlugin>() {
            app.add_plugins(crate::door_animation::DoorAnimationPlugin);
        }
    }
}

pub struct ModelAnimationPlugin;
impl Plugin for ModelAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SourceClips>();
        app.world_mut()
            .resource_mut::<GltfExtensionHandlers>()
            .0
            .write_blocking()
            .push(Box::<ClipInventory>::default());
        app.add_systems(Update, (resolve_source_clips, play_idle_clips).chain());
    }
}

/// A resolved source, including a real static answer. Retains the graph while its scene lives.
#[derive(Component, Clone, Debug)]
pub struct SourceClipBinding {
    pub source: Entity,
    pub players: Vec<Entity>,
    pub graph: Option<Handle<AnimationGraph>>,
    pub open: Option<AnimationNodeIndex>,
    pub close: Option<AnimationNodeIndex>,
    pub idle: Option<(AnimationNodeIndex, f32)>,
}
#[derive(Component)]
struct IdleStarted;
#[derive(Component, Debug)]
pub struct InvalidSourceClip;

#[expect(
    clippy::type_complexity,
    reason = "One source resolver serves authored doors and ambient references."
)]
pub(crate) fn resolve_source_clips(
    mut commands: Commands,
    references: Query<(Entity, Option<&LoadDoor>, Option<&SourceClipBinding>), With<MeshHandle>>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    inventories: Query<Ref<SourceClips>>,
    mut players: Query<(Option<&AnimationGraphHandle>, &mut AnimationPlayer)>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    invalid: Query<(), With<InvalidSourceClip>>,
) {
    for (reference, door, binding) in &references {
        if binding.is_some_and(|b| {
            inventories.get(b.source).is_ok_and(|i| !i.is_changed())
                && parents.iter_ancestors(b.source).any(|e| e == reference)
                && b.players.iter().all(|p| players.contains(*p))
        }) {
            continue;
        }
        if let Some(binding) = binding {
            // The authored binding owns playback only while its graph is installed.
            // Private adapters may have replaced it with a derived graph on this player.
            if let Some(graph) = &binding.graph {
                for player in &binding.players {
                    if let Ok((Some(installed), mut playback)) = players.get_mut(*player)
                        && installed.0 == *graph
                    {
                        playback.stop_all();
                        commands
                            .entity(*player)
                            .try_remove::<AnimationGraphHandle>();
                    }
                }
            }
            commands
                .entity(reference)
                .try_remove::<(SourceClipBinding, IdleStarted)>();
        }
        let Some((source, inventory)) = children
            .iter_descendants(reference)
            .find_map(|e| inventories.get(e).ok().map(|i| (e, i)))
        else {
            continue;
        };
        let named = |wanted: &str, ignore_case: bool| {
            inventory
                .0
                .iter()
                .find(|(name, _)| {
                    if ignore_case {
                        name.eq_ignore_ascii_case(wanted)
                    } else {
                        name == wanted
                    }
                })
                .map(|(_, h)| h.clone())
        };
        let selected = if door.is_some() {
            [named("Open", true), named("Close", true), None]
        } else {
            [None, None, named("Idle", false)]
        };
        let mut duration = 0.0;
        let mut ready = true;
        for handle in selected.iter().flatten() {
            let Some(clip) = clips.get(handle) else {
                ready = false;
                break;
            };
            if !clip.duration().is_finite() || clip.duration() <= 0.0 {
                if !invalid.contains(reference) {
                    error!(
                        ?reference,
                        "authored source clip has invalid duration; animation remains unresolved"
                    );
                    commands.entity(reference).try_insert(InvalidSourceClip);
                }
                ready = false;
                break;
            }
            duration = clip.duration();
        }
        if !ready {
            continue;
        }
        let animated = selected.iter().any(Option::is_some);
        let graph_players: Vec<_> = if animated {
            children
                .iter_descendants(source)
                .chain(std::iter::once(source))
                .filter(|e| players.contains(*e))
                .collect()
        } else {
            Vec::new()
        };
        if animated && graph_players.is_empty() {
            continue;
        }
        let mut nodes = [None; 3];
        let graph = animated.then(|| {
            let mut graph = AnimationGraph::new();
            for (slot, handle) in selected.into_iter().enumerate() {
                if let Some(handle) = handle {
                    nodes[slot] = Some(graph.add_clip(handle, 1.0, graph.root));
                }
            }
            graphs.add(graph)
        });
        if let Some(graph) = &graph {
            for player in &graph_players {
                commands
                    .entity(*player)
                    .try_insert(AnimationGraphHandle(graph.clone()));
            }
        }
        commands
            .entity(reference)
            .try_remove::<(IdleStarted, InvalidSourceClip)>();
        commands.entity(reference).try_insert(SourceClipBinding {
            source,
            players: graph_players,
            graph,
            open: nodes[0],
            close: nodes[1],
            idle: nodes[2].map(|n| (n, duration)),
        });
    }
}

fn play_idle_clips(
    mut commands: Commands,
    references: Query<
        (Entity, &FormId, &SourceClipBinding),
        (Without<LoadDoor>, Without<IdleStarted>),
    >,
    mut players: Query<&mut AnimationPlayer>,
) {
    for (entity, form, binding) in &references {
        if let Some((node, duration)) = binding.idle {
            for player in &binding.players {
                if let Ok(mut player) = players.get_mut(*player) {
                    player
                        .play(node)
                        .repeat()
                        .seek_to(start_offset(form.0, duration));
                }
            }
        }
        commands.entity(entity).try_insert(IdleStarted);
    }
}

fn start_offset(form_id: u32, seconds: f32) -> f32 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0.0;
    }
    let offset = (f64::from(form_id) % f64::from(seconds)) as f32;
    if offset.is_finite() && offset < seconds {
        offset
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_capsule_installs_playback_and_automated_cameras_do_not() {
        for automated in 0..3 {
            let mut config = crate::config::EngineConfig::default();
            if automated == 1 {
                config.benchmark_frames = Some(10);
            }
            if automated == 2 {
                config.auto_fly_speed = 100.0;
            }
            let mut app = App::new();
            app.init_resource::<GltfExtensionHandlers>()
                .insert_resource(config);
            add_ordinary_animation_plugins(&mut app);
            add_ordinary_animation_plugins(&mut app);
            assert_eq!(
                app.is_plugin_added::<ModelAnimationPlugin>(),
                automated == 0
            );
            assert_eq!(
                app.is_plugin_added::<crate::door_animation::DoorAnimationPlugin>(),
                automated == 0
            );
            assert!(
                !app.world()
                    .contains_resource::<crate::streaming::StreamingWorld>()
            );
        }
    }
    #[test]
    fn deterministic_ambient_phase_preserves_low_form_id_bits() {
        assert_eq!(start_offset(u32::MAX, 2.0), 1.0);
        assert_eq!(start_offset(17, 2.0), 1.0);
        assert_eq!(start_offset(17, 0.0), 0.0);
    }
}

//! Ordinary engine interaction prompt. Private HUD adapters consume the same DoorTarget.
use crate::doors::{DoorTarget, DoorTargetSystems};
use bevy::prelude::*;
#[derive(Component)]
pub struct DoorPrompt;
pub struct DoorPromptPlugin;
pub fn add_door_prompt(app: &mut App) {
    if !app.is_plugin_added::<DoorPromptPlugin>() {
        app.add_plugins(DoorPromptPlugin);
    }
}
impl Plugin for DoorPromptPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorTarget>()
            .add_systems(Startup, spawn_door_prompt)
            .add_systems(
                PostUpdate,
                sync_door_prompt
                    .after(DoorTargetSystems)
                    .before(bevy::ui::UiSystems::Prepare),
            );
    }
}
fn spawn_door_prompt(mut commands: Commands) {
    commands.spawn((
        DoorPrompt,
        Text::new(""),
        TextFont::from_font_size(18.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(55.0),
            left: Val::Percent(35.0),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
    ));
}
pub fn sync_door_prompt(
    target: Res<DoorTarget>,
    mut prompt: Query<(&mut Text, &mut Node), With<DoorPrompt>>,
) {
    for (mut text, mut node) in &mut prompt {
        match &target.0 {
            Some(target) => {
                if text.as_str() != target.prompt {
                    **text = target.prompt.clone();
                }
                if node.display != Display::Flex {
                    node.display = Display::Flex;
                }
            }
            None => {
                if node.display != Display::None {
                    node.display = Display::None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doors::TargetedDoor;
    #[test]
    fn shared_prompt_tracks_hidden_visible_hidden_without_private_hud() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        add_door_prompt(&mut app);
        add_door_prompt(&mut app);
        app.update();
        let prompt = app
            .world_mut()
            .query_filtered::<Entity, With<DoorPrompt>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(prompt).unwrap().display,
            Display::None
        );
        let door = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<DoorTarget>().0 = Some(TargetedDoor {
            door,
            prompt: crate::doors::open_prompt("Inn"),
        });
        app.update();
        assert_eq!(
            app.world().get::<Node>(prompt).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Text>(prompt).unwrap().as_str(),
            "E  Open  Inn"
        );
        app.world_mut().resource_mut::<DoorTarget>().0 = None;
        app.update();
        assert_eq!(
            app.world().get::<Node>(prompt).unwrap().display,
            Display::None
        );
    }
}

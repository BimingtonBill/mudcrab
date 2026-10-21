//! The demo HUD's one shared style, and the top-left notices panel every HUD piece drops a
//! short-lived message into.
//!
//! Before this the demo's HUD was three unrelated looks: `player.rs`'s bare white door prompt and
//! a help line that faded after 25 seconds and never came back (never once mentioning `F12`),
//! `field_notes.rs`'s own green "Saved" box, and `demo_tour.rs`'s cream objective line counting
//! down doors in the corner - which is what the user meant by "there's orange text at the top and
//! white text at the bottom, weird" (2026-09-25). This module borrows the pose tool's own look
//! (`crate::pose_capture::setup_start_shot_hud`'s panel: cream text over a dark translucent panel,
//! a fixed margin from the window's edge) as the one style every piece below now uses, so the
//! controls panel, the door prompt, the note box and the notices read as one thing.
//!
//! What lives here is the shared style and the notices panel; the controls panel and the door
//! prompt stay in `crate::player` (they read the player's own state), and the note box stays in
//! `crate::field_notes` (it reads the capture run) - each spawns its panel with
//! [`panel_node`]/[`text`]/[`centered_row`] and shows or hides it with [`hidden_for_this_run`].

use crate::config::EngineConfig;
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};

/// The HUD's text colour: warm cream, the pose tool's own - never Bevy's default white, which is
/// what the user was reading as two unrelated things at the top and the bottom of the screen.
pub const TEXT_COLOR: Color = Color::srgb(0.95, 0.92, 0.8);
/// A HUD panel's background: near-black, translucent enough that the world still reads through it.
pub const PANEL_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
/// The margin every HUD panel keeps from the edge of the window - the pose tool's own 12 px.
pub const MARGIN: Val = Val::Px(12.0);
/// The padding inside a HUD panel, around its text.
pub const PADDING: Val = Val::Px(8.0);
/// The HUD's usual text size.
pub const FONT_SIZE: f32 = 16.0;
/// How long a notice - "Saved 03 (...)", or the name of a place just arrived at - stays up before
/// it fades: about the time it takes to read one short line (the user's own number, "~3 s").
pub const NOTICE_SECONDS: f32 = 3.0;

/// A `(Text, TextFont, TextColor)` bundle in the HUD's style, at `size`.
pub fn text(value: impl Into<String>, size: f32) -> (Text, TextFont, TextColor) {
    (
        Text::new(value.into()),
        TextFont::from_font_size(size),
        TextColor(TEXT_COLOR),
    )
}

/// A dark translucent panel, padded and absolutely positioned, `display` at spawn - the shape
/// every HUD panel below wraps its own text in. The caller sets whichever of `top`/`bottom`/
/// `left`/`right` anchors it.
pub fn panel_node(display: Display) -> (Node, BackgroundColor) {
    (
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::all(PADDING),
            display,
            ..default()
        },
        BackgroundColor(PANEL_COLOR),
    )
}

/// An invisible full-width row anchored at `top`, that centres whatever panel is spawned as its
/// child - the door prompt and the note box both want to sit in the middle of the screen rather
/// than hug a corner, and a panel cannot centre itself without knowing its own width.
pub fn centered_row(top: Val, display: Display) -> Node {
    Node {
        position_type: PositionType::Absolute,
        top,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        display,
        ..default()
    }
}

/// Whether the HUD stays off in this run: an automated run whose screenshots must stay clean
/// (`--demo-tour` or `--shots`) that has not asked to keep the window on screen (`--show-window`),
/// the same rule [`EngineConfig::window_offscreen`] opens the window by. Kept here so every HUD
/// piece hides on it rather than each re-deriving it, and so `--show-window` is also what puts the
/// HUD back for a look at it (`docs/demo/README.md`).
pub fn hidden_for_this_run(config: &EngineConfig) -> bool {
    config.window_offscreen()
}

// ---------------------------------------------------------------------------------------------
// The shared notices panel
// ---------------------------------------------------------------------------------------------

/// A short-lived message shown in the HUD's shared top-left notices panel - an `F12` "Saved" line
/// from `crate::field_notes`, or the name of a place just arrived at - so the two do not each
/// claim their own corner. The newest notice replaces whatever was on screen.
#[derive(Resource, Default)]
pub struct Notices {
    current: Option<(String, f32)>,
}

impl Notices {
    /// Shows `message` for [`NOTICE_SECONDS`], replacing whatever notice was already up.
    pub fn show(&mut self, message: impl Into<String>) {
        self.current = Some((message.into(), NOTICE_SECONDS));
    }

    /// Counts the current notice down by `delta_secs`; clears it once its time is up. Called once
    /// a frame ([`fade_notices`]).
    pub fn fade(&mut self, delta_secs: f32) {
        if let Some((_, remaining)) = self.current.as_mut() {
            *remaining -= delta_secs;
            if *remaining <= 0.0 {
                self.current = None;
            }
        }
    }

    /// The notice on screen now, if any.
    pub fn text(&self) -> Option<&str> {
        self.current.as_ref().map(|(text, _)| text.as_str())
    }
}

/// The notices panel's own entity, top-left; [`sync_notices`] fills and hides it.
#[derive(Component)]
pub struct NoticesPanel;

/// Spawns the (initially empty and hidden) notices panel. Added to `Startup` alongside
/// `crate::field_notes::setup_hud`, which is the run's own unconditional HUD plugin - the notices
/// panel is not the field notes' alone, but there is no other unconditional place to hang it.
pub fn spawn_notices_panel(mut commands: Commands) {
    let (mut node, background) = panel_node(Display::None);
    node.top = MARGIN;
    node.left = MARGIN;
    commands.spawn((
        NoticesPanel,
        node,
        background,
        text(String::new(), FONT_SIZE),
    ));
}

/// Counts down the notice on screen now.
pub fn fade_notices(time: Res<Time>, mut notices: ResMut<Notices>) {
    notices.fade(time.delta_secs());
}

/// Shows or hides the notices panel for the notice on screen now, suppressed like the rest of the
/// HUD in a run whose screenshots must stay clean.
pub fn sync_notices(
    config: Res<EngineConfig>,
    notices: Res<Notices>,
    mut panel: Query<(&mut Text, &mut Node), With<NoticesPanel>>,
) {
    let Ok((mut text, mut node)) = panel.single_mut() else {
        return;
    };
    match notices.text() {
        Some(message) if !hidden_for_this_run(&config) => {
            if text.as_str() != message {
                **text = message.to_owned();
            }
            node.display = Display::Flex;
        }
        _ => node.display = Display::None,
    }
}

// ---------------------------------------------------------------------------------------------
// The door prompt
// ---------------------------------------------------------------------------------------------

/// Where the door prompt sits: enough below the middle of the screen that it does not sit over
/// where a player naturally looks (the door itself, and whatever is behind it), and close enough
/// to be read without looking away.
pub const DOOR_PROMPT_TOP: Val = Val::Percent(58.0);
/// The door prompt's text size.
pub const DOOR_PROMPT_FONT_SIZE: f32 = 20.0;

// Public HUD owns spawning, synchronization and the component identity.
pub use crate::hud::{DoorPrompt, DoorPromptPlugin, add_door_prompt, sync_door_prompt};

/// Private styling wraps the shared prompt after its Startup spawn. Only the parent row owns
/// capture suppression; the public synchronizer alone controls the prompt's Display and Text.
pub fn style_door_prompt(
    mut commands: Commands,
    config: Res<EngineConfig>,
    prompts: Query<(Entity, &Node), With<DoorPrompt>>,
) {
    for (prompt, old_node) in &prompts {
        let display = if hidden_for_this_run(&config) {
            Display::None
        } else {
            Display::Flex
        };
        let row = commands.spawn(centered_row(DOOR_PROMPT_TOP, display)).id();
        let (mut node, background) = panel_node(old_node.display);
        node.display = old_node.display;
        commands.entity(prompt).insert((
            node,
            background,
            text(String::new(), DOOR_PROMPT_FONT_SIZE),
            ChildOf(row),
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// The frame-rate counter
// ---------------------------------------------------------------------------------------------

/// How often the frame-rate counter's text changes, in seconds: fast enough to follow a stutter,
/// slow enough to read.
pub const FPS_REFRESH_SECONDS: f32 = 0.25;

/// The frame-rate counter's own entity, bottom-right (the user asked for one, 2026-09-26).
#[derive(Component)]
pub struct FpsPanel;

/// Spawns the frame-rate counter, bottom-right (the graphics panel opens top-right), hidden in a run whose screenshots must stay clean.
pub fn spawn_fps_panel(mut commands: Commands, config: Res<EngineConfig>) {
    let display = if hidden_for_this_run(&config) {
        Display::None
    } else {
        Display::Flex
    };
    let (mut node, background) = panel_node(display);
    node.bottom = MARGIN;
    node.right = MARGIN;
    commands.spawn((FpsPanel, node, background, text("-- fps", FONT_SIZE)));
}

/// The counter's line: frames a second and the frame time, from Bevy's smoothed frame-time
/// diagnostic.
pub fn fps_line(fps: f64, frame_ms: f64) -> String {
    format!("{fps:.0} fps  {frame_ms:.1} ms")
}

/// Refreshes the frame-rate counter every [`FPS_REFRESH_SECONDS`].
pub fn update_fps_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mut since: Local<f32>,
    mut panel: Query<&mut Text, With<FpsPanel>>,
) {
    *since += time.delta_secs();
    if *since < FPS_REFRESH_SECONDS {
        return;
    }
    *since = 0.0;
    let smoothed = |path| diagnostics.get(path).and_then(|d| d.smoothed());
    let (Some(fps), Some(frame_ms)) = (
        smoothed(&FrameTimeDiagnosticsPlugin::FPS),
        smoothed(&FrameTimeDiagnosticsPlugin::FRAME_TIME),
    ) else {
        return;
    };
    if let Ok(mut text) = panel.single_mut() {
        **text = fps_line(fps, frame_ms);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_fps_line_reads_frames_a_second_and_milliseconds() {
        assert_eq!(super::fps_line(143.6, 6.964), "144 fps  7.0 ms");
    }

    use super::*;
    use crate::doors::{DoorTarget, TargetedDoor};

    /// The prompt is a pure view of `DoorTarget`: the line when a door is targeted, hidden when not,
    /// in any door mode and movement mode.
    #[test]
    fn the_door_prompt_shows_exactly_the_targeted_doors_line() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<DoorTarget>()
            .add_systems(Update, sync_door_prompt);
        let prompt = app
            .world_mut()
            .spawn((DoorPrompt, Text::new(""), Node::default()))
            .id();
        let door = app.world_mut().spawn_empty().id();
        app.update();
        assert_eq!(
            app.world().get::<Node>(prompt).unwrap().display,
            Display::None
        );

        app.world_mut().resource_mut::<DoorTarget>().0 = Some(TargetedDoor {
            door,
            prompt: crate::doors::open_prompt("Sleeping Giant Inn"),
        });
        app.update();
        assert_eq!(
            app.world().get::<Node>(prompt).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Text>(prompt).unwrap().as_str(),
            "E  Open  Sleeping Giant Inn"
        );

        app.world_mut().resource_mut::<DoorTarget>().0 = None;
        app.update();
        assert_eq!(
            app.world().get::<Node>(prompt).unwrap().display,
            Display::None
        );
    }

    #[test]
    fn private_capture_style_wraps_one_public_prompt_without_competing_display_policy() {
        for hidden in [false, true] {
            let mut config = EngineConfig::default();
            config.portal.demo_tour = Some("unused-capture-fixture".into());
            config.portal.show_window = !hidden;
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(config)
                .add_systems(PostStartup, style_door_prompt);
            add_door_prompt(&mut app);
            crate::hud::add_door_prompt(&mut app);
            let door = app.world_mut().spawn_empty().id();
            app.world_mut().resource_mut::<DoorTarget>().0 = Some(TargetedDoor {
                door,
                prompt: crate::doors::open_prompt("Invented inn"),
            });
            app.update();
            let prompt = app
                .world_mut()
                .query_filtered::<Entity, With<crate::hud::DoorPrompt>>()
                .single(app.world())
                .unwrap();
            assert_eq!(
                app.world().get::<Node>(prompt).unwrap().display,
                Display::Flex
            );
            let row = app.world().get::<ChildOf>(prompt).unwrap().parent();
            assert_eq!(
                app.world().get::<Node>(row).unwrap().display,
                if hidden { Display::None } else { Display::Flex }
            );
            assert_eq!(
                app.world().get::<Text>(prompt).unwrap().as_str(),
                "E  Open  Invented inn"
            );
            app.world_mut().resource_mut::<DoorTarget>().0 = None;
            app.update();
            assert_eq!(
                app.world().get::<Node>(prompt).unwrap().display,
                Display::None
            );
        }
    }

    #[test]
    fn a_notice_fades_after_its_time_and_the_newest_replaces_the_old() {
        let mut notices = Notices::default();
        assert_eq!(
            notices.text(),
            None,
            "nothing shown until something calls show"
        );
        notices.show("Saved 01");
        assert_eq!(notices.text(), Some("Saved 01"));
        notices.fade(NOTICE_SECONDS - 0.1);
        assert_eq!(notices.text(), Some("Saved 01"), "not yet its time");
        notices.show("Riverwood");
        assert_eq!(
            notices.text(),
            Some("Riverwood"),
            "the newest notice replaces the old one, whether or not the old one had faded"
        );
        notices.fade(NOTICE_SECONDS + 1.0);
        assert_eq!(notices.text(), None, "faded for good");
    }
}

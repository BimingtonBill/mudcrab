//! The eye adjusts on crossing (`docs/design/door-layer.md`, "The eye adjusts on crossing", note
//! 05): the user asked that the resolution pop of a doorway drawn below full resolution "should
//! imitate the real human eye adjusting to the new space". Two halves:
//!
//! 1. **Before the crossing** ([`PortalApproach`], [`track_portal_approach`]): the portal's render
//!    target rises from the `portal_scale` graphics setting toward full resolution as the player
//!    nears the doorway, so the image they step into is close to full detail.
//!    `crate::portal`'s `resize_portal_target` sizes the target at [`PortalApproach::scale`].
//! 2. **After the crossing** ([`EyeFocus`]): the new space comes into focus over
//!    [`FOCUS_SECONDS`], a soft focus as strong as the resolution the doorway was last drawn at was
//!    low, and nil at an effective scale of 1. Auto exposure already imitates the eye's brightness
//!    adaptation and is left alone.
//!
//! # The soft focus is Bevy's depth of field, made uniform
//!
//! The blur is Bevy 0.19's own `DepthOfField` in its Gaussian mode on the main camera, present
//! only while the focus clears (`bevy_post_process-0.19.0/src/dof/mod.rs`; two separable
//! fullscreen passes after bloom and before tonemapping, `depth_of_field.after(bloom)
//! .before(tonemapping)`). Its circle of confusion is
//! `clamp(scale * |depth - focus| / (depth * (focus - f)) * height, 0, max_coc)`
//! (`dof/dof.wgsl`, `calculate_circle_of_confusion`) with `scale = f² / (sensor_height *
//! aperture_f_stops)` (`dof/mod.rs`, `extract_depth_of_field_settings`). A focus nearer than the
//! near plane and an aperture of [`FOCUS_APERTURE_F_STOPS`] make the unclamped value millions of
//! pixels for every depth, so every pixel is blurred by exactly `max_coc`: a uniform blur whose one
//! knob, `max_circle_of_confusion_diameter`, is the envelope. The Gaussian kernel's σ is a quarter
//! of it (`bevy_post_process-0.19.0/src/gaussian_blur.wgsl`), and its weights change smoothly with
//! σ, so the blur clears without steps.
//!
//! Below [`FOCUS_MIN_COC`] the component is removed: the kernel is invisible there (σ = 1/8 px), and
//! at a diameter under about 0.3 px its weights underflow to 0/0 in `gaussian_blur.wgsl`.
//!
//! The first use of a pipeline compiles it, in the background, and until then the pass draws
//! nothing; a focus that appeared a few frames into its own clearing would read as an effect. So
//! the component is also on the camera for the first [`WARM_FRAMES`] frames of the run at
//! [`FOCUS_MIN_COC`], which compiles the two pipelines while the world is loading.

use crate::{
    doors::DoorCrossed, graphics_settings::GraphicsSettings, portal::PortalState,
    world::components::StreamingCamera,
};
use bevy::post_process::dof::{DepthOfField, DepthOfFieldMode};
use bevy::prelude::*;

/// Creation units in a metre (a Skyrim unit is 1.43 cm), as `crate::app` uses.
const UNITS_PER_METRE: f32 = 70.0;

/// Nearer than this to the doorway, the approach is complete: 1.5 m, one stride and a half. From
/// here the doorway fills most of the screen, and at a walk (`crate::player::WALK_SPEED`, 2.1 m/s)
/// the crossing is under a second away.
pub const APPROACH_NEAR: f32 = 1.5 * UNITS_PER_METRE;

/// Farther than this from the doorway, the portal draws at the setting: 6 m. The rise from here to
/// [`APPROACH_NEAR`] takes about two seconds at a walk and one at a run (5 m/s), slow enough that
/// no single step of it is noticed, and it starts where the doorway is still a small part of the
/// screen, so the extra texels cost little while they are few.
pub const APPROACH_FAR: f32 = 6.0 * UNITS_PER_METRE;

/// The share of the gap between the setting and full resolution the approach closes: the image
/// stepped into is "close to full detail" (the design note), and the eye's focus clears the rest.
///
/// Not all of it: at [`APPROACH_NEAR`] the doorway fills the screen, and a portal at full
/// resolution there is a second full-screen view in the costliest frames of the whole approach,
/// which is the cost the setting is there to cut. At 0.8 a setting of 0.5 arrives at 0.9 (81 % of
/// the full view's texels) and 0.1 at 0.82.
pub const APPROACH_SHARE: f32 = 0.8;

/// The approach moves the target size in this many steps from the setting to its end, so walking
/// up to a doorway reallocates it a handful of times rather than every frame
/// ([`quantised_closeness`]).
pub const APPROACH_STEPS: f32 = 8.0;

/// How long the soft focus takes to clear, in seconds: about half a second, the eye's own time to
/// re-accommodate after a change of scene.
pub const FOCUS_SECONDS: f32 = 0.5;

/// The focus's blur diameter, in screen pixels, per unit of the doorway image's upscale
/// (`1 / scale - 1`): a doorway drawn at 0.9 of full resolution starts at 0.67 px, one at 0.5 at
/// 6 px. A bilinear upscale by `1 / scale` is about that much softer than the full-resolution view
/// once it is past the first half pixel; the eye starts where the image was and sharpens it.
pub const FOCUS_COC_PER_UPSCALE: f32 = 6.0;

/// The strongest the focus starts: 12 px, so a doorway at the lowest scale (0.1, a 9x upscale)
/// still clears as a soft focus and not as a smear.
pub const FOCUS_MAX_COC: f32 = 12.0;

/// Below this blur diameter, in pixels, the focus is over and the pass is removed (see the module
/// notes).
pub const FOCUS_MIN_COC: f32 = 0.5;

/// Frames at the start of a run the depth-of-field pass runs, invisibly, so its pipelines exist by
/// the first crossing (see the module notes).
pub const WARM_FRAMES: u32 = 120;

/// Nearer than the camera's near plane (Bevy's default, 0.1): every rendered depth is behind it.
const FOCUS_DISTANCE: f32 = 0.05;

/// An aperture that makes every unclamped circle of confusion enormous (see the module notes).
const FOCUS_APERTURE_F_STOPS: f32 = 1.0e-6;

/// How close the player is to the open doorway, from 0 (at or beyond [`APPROACH_FAR`]) to 1 (at or
/// within [`APPROACH_NEAR`]), eased at both ends (smoothstep) so the rise neither starts nor
/// finishes abruptly.
pub fn approach_closeness(distance: f32) -> f32 {
    let t = ((APPROACH_FAR - distance) / (APPROACH_FAR - APPROACH_NEAR)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The closeness the target is sized at: `raw` rounded to one of [`APPROACH_STEPS`] steps, but only
/// once it is more than three quarters of a step from `previous`, so a player standing still on a
/// step's boundary does not resize the target back and forth. The two ends are exact.
pub fn quantised_closeness(previous: f32, raw: f32) -> f32 {
    let raw = raw.clamp(0.0, 1.0);
    if raw == 0.0 || raw == 1.0 {
        return raw;
    }
    if (raw - previous).abs() <= 0.75 / APPROACH_STEPS {
        return previous;
    }
    (raw * APPROACH_STEPS).round() / APPROACH_STEPS
}

/// The portal's effective scale at `setting` (held to 0.1..=1, as the setting is) and `closeness`:
/// the setting, moved [`APPROACH_SHARE`] of the way to 1 as the closeness reaches 1. 1 at a setting
/// of 1.
pub fn approach_scale(setting: f32, closeness: f32) -> f32 {
    let setting = setting.clamp(0.1, 1.0);
    setting + (1.0 - setting) * APPROACH_SHARE * closeness.clamp(0.0, 1.0)
}

/// The distance from `point` to the doorway rectangle the portal quad stands in (`quad`: centred
/// on the doorway, a unit plane facing +Z scaled to the opening's width and height).
pub fn distance_to_doorway(quad: &Transform, point: Vec3) -> f32 {
    let local = quad.rotation.inverse() * (point - quad.translation);
    let half = (quad.scale.truncate().abs() * 0.5).extend(0.0);
    (local - local.clamp(-half, half)).length()
}

/// The blur diameter, in pixels, a focus starts at after a crossing through a doorway drawn at
/// `scale`: [`FOCUS_COC_PER_UPSCALE`] per unit of upscale, at most [`FOCUS_MAX_COC`], and 0 at a
/// scale of 1 or below [`FOCUS_MIN_COC`].
pub fn focus_strength(scale: f32) -> f32 {
    let scale = scale.clamp(0.1, 1.0);
    let coc = (FOCUS_COC_PER_UPSCALE * (1.0 / scale - 1.0)).min(FOCUS_MAX_COC);
    if coc < FOCUS_MIN_COC { 0.0 } else { coc }
}

/// The focus's blur diameter `elapsed` seconds after it started at `strength`: clearing over
/// [`FOCUS_SECONDS`] with an ease-out (`(1 - t)²`: quickest at first, settling gently, the way an
/// eye's accommodation does), and exactly 0 from then on.
pub fn focus_envelope(strength: f32, elapsed: f32) -> f32 {
    let t = (elapsed / FOCUS_SECONDS).clamp(0.0, 1.0);
    strength * (1.0 - t) * (1.0 - t)
}

/// The portal's approach this frame: how close the player is to the open doorway and the scale the
/// target is sized at. Written by [`track_portal_approach`] after `crate::portal` has placed this
/// frame's doorway, read by its `resize_portal_target` and by the focus after a crossing.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PortalApproach {
    /// The quantised closeness ([`quantised_closeness`]); 0 with no doorway open.
    pub closeness: f32,
    /// The effective portal scale: [`approach_scale`] of the setting and the closeness.
    pub scale: f32,
    /// Whether the portal drew through a doorway this frame.
    pub doorway: bool,
}

impl Default for PortalApproach {
    fn default() -> Self {
        Self {
            closeness: 0.0,
            scale: 1.0,
            doorway: false,
        }
    }
}

/// Measures the player's approach to the doorway the portal draws through this frame. Registered by
/// `crate::portal::PortalPlugin` after `update_portal` and before `resize_portal_target`.
pub(crate) fn track_portal_approach(
    state: Option<Res<PortalState>>,
    camera: Query<&Transform, With<StreamingCamera>>,
    settings: Option<Res<GraphicsSettings>>,
    mut approach: ResMut<PortalApproach>,
) {
    let setting = settings.map_or(1.0, |settings| settings.portal_scale);
    let doorway = state.and_then(|state| state.open_doorway());
    let raw = match (doorway, camera.single()) {
        (Some(doorway), Ok(camera)) => {
            approach_closeness(distance_to_doorway(&doorway.quad, camera.translation))
        }
        _ => 0.0,
    };
    let closeness = quantised_closeness(approach.closeness, raw);
    let next = PortalApproach {
        closeness,
        scale: approach_scale(setting, closeness),
        doorway: doorway.is_some(),
    };
    if *approach != next {
        *approach = next;
    }
}

/// The soft focus after a crossing, and the pipeline warm-up at the start of a run.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct EyeFocus {
    /// The blur diameter the current focus started at, in pixels; 0 with no focus under way.
    pub strength: f32,
    /// Seconds since it started.
    pub elapsed: f32,
    /// Frames of the warm-up run so far ([`WARM_FRAMES`]).
    pub warmed: u32,
}

impl EyeFocus {
    /// The blur diameter this frame, or `None` when the pass should be off.
    fn coc(&self) -> Option<f32> {
        let focus = focus_envelope(self.strength, self.elapsed);
        if focus >= FOCUS_MIN_COC {
            Some(focus)
        } else if self.warmed < WARM_FRAMES {
            Some(FOCUS_MIN_COC)
        } else {
            None
        }
    }
}

/// A crossing starts the focus, as strong as the doorway it went through was last drawn low
/// ([`focus_strength`] of [`PortalApproach::scale`], which still holds the frame before the
/// crossing: this runs before the portal re-measures). Nothing for a crossing no doorway was drawn
/// for (a crossing on the spot shows no low-resolution image), or with the `eye-focus` setting off.
pub fn start_eye_focus(
    mut crossed: MessageReader<DoorCrossed>,
    approach: Option<Res<PortalApproach>>,
    settings: Option<Res<GraphicsSettings>>,
    mut focus: ResMut<EyeFocus>,
) {
    if crossed.read().count() == 0 {
        return;
    }
    let enabled = settings.is_none_or(|settings| settings.eye_focus);
    let strength = approach
        .filter(|approach| approach.doorway)
        .map_or(0.0, |approach| focus_strength(approach.scale));
    if enabled && strength > 0.0 {
        debug!(
            strength,
            "eye: the new space comes into focus after the crossing"
        );
        focus.strength = strength;
        focus.elapsed = 0.0;
    }
}

/// Clears the focus over [`FOCUS_SECONDS`] on the main camera, adding Bevy's depth of field while
/// it is visible (or warming up) and removing it after.
pub fn drive_eye_focus(
    mut commands: Commands,
    time: Res<Time>,
    mut focus: ResMut<EyeFocus>,
    mut camera: Query<(Entity, Option<&mut DepthOfField>), With<StreamingCamera>>,
) {
    let Ok((entity, depth_of_field)) = camera.single_mut() else {
        return;
    };
    let coc = focus.coc();
    match (coc, depth_of_field) {
        (Some(coc), Some(mut depth_of_field)) => {
            if depth_of_field.max_circle_of_confusion_diameter != coc {
                depth_of_field.max_circle_of_confusion_diameter = coc;
            }
        }
        (Some(coc), None) => {
            commands.entity(entity).insert(uniform_blur(coc));
        }
        (None, Some(_)) => {
            commands.entity(entity).remove::<DepthOfField>();
        }
        (None, None) => {}
    }
    // Counted from the first frame the camera exists, so the warm-up covers the pipelines' compile.
    focus.warmed = focus.warmed.saturating_add(1).min(WARM_FRAMES);
    if focus.strength > 0.0 {
        focus.elapsed += time.delta_secs();
        if focus.elapsed >= FOCUS_SECONDS {
            focus.strength = 0.0;
            focus.elapsed = 0.0;
        }
    }
}

/// Bevy's Gaussian depth of field, blurring every pixel by `coc` pixels (see the module notes).
fn uniform_blur(coc: f32) -> DepthOfField {
    DepthOfField {
        mode: DepthOfFieldMode::Gaussian,
        focal_distance: FOCUS_DISTANCE,
        aperture_f_stops: FOCUS_APERTURE_F_STOPS,
        max_circle_of_confusion_diameter: coc,
        // Finite, so the sky's infinite depth does not make `|depth - focus| / depth` NaN.
        max_depth: 1.0e6,
        ..default()
    }
}

/// The focus after a crossing. `crate::portal::PortalPlugin` adds it, with [`PortalApproach`] and
/// [`track_portal_approach`] in its own frame.
pub struct EyeAdjustPlugin;

impl Plugin for EyeAdjustPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PortalApproach>()
            .init_resource::<EyeFocus>()
            .add_systems(
                Update,
                (start_eye_focus, drive_eye_focus)
                    .chain()
                    // The crossing's message is written in `DoorTransition`; the approach it reads
                    // is re-measured in `PortalFrame`, so this sits between the two.
                    .after(crate::transition::DoorTransition)
                    .before(crate::portal::PortalFrame),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn the_approach_rises_from_the_setting_far_away_to_near_full_at_the_doorway() {
        assert_eq!(approach_closeness(APPROACH_FAR), 0.0);
        assert_eq!(approach_closeness(APPROACH_FAR * 3.0), 0.0);
        assert_eq!(approach_closeness(APPROACH_NEAR), 1.0);
        assert_eq!(approach_closeness(0.0), 1.0);
        let mid = approach_closeness((APPROACH_FAR + APPROACH_NEAR) * 0.5);
        assert!((mid - 0.5).abs() < 1e-6, "{mid}");
        // Eased: the first and last tenths of the way move it less than a tenth.
        let span = APPROACH_FAR - APPROACH_NEAR;
        assert!(approach_closeness(APPROACH_FAR - span * 0.1) < 0.05);
        assert!(approach_closeness(APPROACH_NEAR + span * 0.1) > 0.95);
        let mut last = 0.0;
        for step in 0..=100 {
            let closeness = approach_closeness(APPROACH_FAR - span * step as f32 / 100.0);
            assert!(closeness >= last, "monotonic");
            last = closeness;
        }

        assert_eq!(approach_scale(0.5, 0.0), 0.5);
        assert!((approach_scale(0.5, 1.0) - 0.9).abs() < 1e-6);
        assert!((approach_scale(0.1, 1.0) - 0.82).abs() < 1e-6);
        assert_eq!(approach_scale(1.0, 0.0), 1.0);
        assert_eq!(approach_scale(1.0, 1.0), 1.0, "nothing to rise to at 1");
        assert_eq!(approach_scale(0.0, 0.0), 0.1, "held to the setting's range");
    }

    #[test]
    fn the_approach_resizes_in_steps_and_does_not_thrash_on_a_boundary() {
        let step = 1.0 / APPROACH_STEPS;
        // Ends are exact.
        assert_eq!(quantised_closeness(0.5, 0.0), 0.0);
        assert_eq!(quantised_closeness(0.5, 1.0), 1.0);
        // A raw value dithering around a step boundary keeps the size it has.
        let boundary = 2.5 * step;
        let first = quantised_closeness(0.0, boundary + 0.01);
        assert_eq!(first, 3.0 * step);
        for jitter in [-0.02, 0.02, -0.03, 0.01] {
            assert_eq!(quantised_closeness(first, boundary + jitter), first);
        }
        // A walk from far to near takes each step once: at most APPROACH_STEPS distinct sizes.
        let mut sizes = vec![0.0f32];
        let mut closeness = 0.0;
        for frame in 0..=600 {
            let distance = APPROACH_FAR - (APPROACH_FAR - APPROACH_NEAR) * frame as f32 / 600.0;
            closeness = quantised_closeness(closeness, approach_closeness(distance));
            if sizes.last() != Some(&closeness) {
                sizes.push(closeness);
            }
        }
        assert_eq!(*sizes.last().unwrap(), 1.0);
        assert!(sizes.len() <= APPROACH_STEPS as usize + 1, "{sizes:?}");
    }

    #[test]
    fn the_distance_is_to_the_doorway_rectangle() {
        let quad = Transform::from_xyz(10.0, 100.0, 0.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::new(80.0, 200.0, 1.0));
        // Straight in front (the quad faces +X after the turn): the perpendicular distance.
        assert!((distance_to_doorway(&quad, Vec3::new(60.0, 120.0, 0.0)) - 50.0).abs() < 1e-3);
        // In the plane, inside the opening: zero.
        assert!(distance_to_doorway(&quad, Vec3::new(10.0, 50.0, 30.0)) < 1e-3);
        // Beside the opening, in its plane: the distance to its edge.
        assert!((distance_to_doorway(&quad, Vec3::new(10.0, 100.0, 70.0)) - 30.0).abs() < 1e-3);
    }

    #[test]
    fn the_focus_is_nil_at_full_scale_and_clears_in_its_time() {
        assert_eq!(focus_strength(1.0), 0.0, "nil at scale 1");
        assert_eq!(focus_strength(0.95), 0.0, "below the visible minimum");
        assert!((focus_strength(0.5) - 6.0).abs() < 1e-5);
        assert_eq!(focus_strength(0.1), FOCUS_MAX_COC);
        assert!(focus_strength(0.9) > focus_strength(0.95));
        assert!(focus_strength(0.5) > focus_strength(0.9));

        let strength = focus_strength(0.5);
        assert_eq!(focus_envelope(strength, 0.0), strength);
        assert_eq!(focus_envelope(strength, FOCUS_SECONDS), 0.0);
        assert_eq!(focus_envelope(strength, FOCUS_SECONDS * 2.0), 0.0);
        // Eased out: a quarter of the time takes off almost half of it.
        let quarter = focus_envelope(strength, FOCUS_SECONDS * 0.25);
        assert!((quarter / strength - 0.5625).abs() < 1e-5);
        let mut last = strength;
        for step in 1..=50 {
            let focus = focus_envelope(strength, FOCUS_SECONDS * step as f32 / 50.0);
            assert!(focus <= last);
            last = focus;
        }
        assert_eq!(focus_envelope(0.0, 0.1), 0.0);
    }

    /// An app with the focus systems, a main camera, a tenth of a second a frame, and the warm-up
    /// already done.
    fn focus_app(settings: Option<GraphicsSettings>) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .add_message::<DoorCrossed>()
            .add_plugins(EyeAdjustPlugin);
        if let Some(settings) = settings {
            app.insert_resource(settings);
        }
        let camera = app.world_mut().spawn(StreamingCamera).id();
        app.world_mut().resource_mut::<EyeFocus>().warmed = WARM_FRAMES;
        app.update();
        (app, camera)
    }

    fn cross(app: &mut App, scale: f32, doorway: bool) {
        *app.world_mut().resource_mut::<PortalApproach>() = PortalApproach {
            closeness: 1.0,
            scale,
            doorway,
        };
        app.world_mut().write_message(DoorCrossed {
            from_ref_id: 1,
            label: "Riverwood".to_owned(),
        });
    }

    fn blur(app: &App, camera: Entity) -> Option<f32> {
        app.world()
            .get::<DepthOfField>(camera)
            .map(|dof| dof.max_circle_of_confusion_diameter)
    }

    #[test]
    fn a_crossing_through_a_low_resolution_doorway_focuses_and_clears() {
        let (mut app, camera) = focus_app(None);
        assert_eq!(blur(&app, camera), None);
        cross(&mut app, 0.5, true);
        app.update();
        let first = blur(&app, camera).expect("the focus starts on the crossing frame");
        assert!((first - 6.0).abs() < 1e-4, "{first}");
        let dof = *app.world().get::<DepthOfField>(camera).unwrap();
        assert_eq!(dof.mode, DepthOfFieldMode::Gaussian);
        let mut last = first;
        let mut frames = 1;
        loop {
            app.update();
            frames += 1;
            assert!(frames < 20);
            let Some(coc) = blur(&app, camera) else {
                break;
            };
            assert!(coc < last, "clears every frame");
            last = coc;
        }
        // 0.1 s a frame: gone by the end of FOCUS_SECONDS.
        assert!(
            frames as f32 * 0.1 <= FOCUS_SECONDS + 0.1 + 1e-4,
            "{frames}"
        );
    }

    #[test]
    fn no_focus_at_full_scale_without_a_doorway_or_with_the_setting_off() {
        let (mut app, camera) = focus_app(None);
        cross(&mut app, 1.0, true);
        app.update();
        assert_eq!(blur(&app, camera), None, "nil at scale 1");
        cross(&mut app, 0.5, false);
        app.update();
        assert_eq!(blur(&app, camera), None, "no doorway was drawn");

        let mut off = GraphicsSettings::current();
        off.eye_focus = false;
        let (mut app, camera) = focus_app(Some(off));
        cross(&mut app, 0.5, true);
        app.update();
        assert_eq!(blur(&app, camera), None, "eye-focus off");
    }

    #[test]
    fn the_pass_warms_up_invisibly_at_the_start_of_a_run() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<DoorCrossed>()
            .add_plugins(EyeAdjustPlugin);
        let camera = app.world_mut().spawn(StreamingCamera).id();
        app.update();
        app.update();
        assert_eq!(blur(&app, camera), Some(FOCUS_MIN_COC));
        for _ in 0..WARM_FRAMES {
            app.update();
        }
        assert_eq!(blur(&app, camera), None, "removed after the warm-up");
    }
}

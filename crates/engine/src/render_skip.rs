//! The one way a camera draws nothing for a frame while staying [`Camera::is_active`]: its render
//! schedule is taken out of the frame.
//!
//! Two views pace themselves and skip frames between renders - the portal camera under the
//! graphics setting `portal_every` ([`crate::portal`]'s `cadence` module) and a managed view under
//! a [`crate::view_policy::ViewPolicy`] - and both skip the same way, through [`SkippedViews`].
//!
//! # Why not `Camera::is_active`
//!
//! The obvious skip - `is_active = false` on the frames between - tears the view down in the render
//! world every time, and the next rendered frame rebuilds it from nothing, which costs more than a
//! skipped frame saves:
//!
//! * `bevy_render-0.19.0/src/camera.rs` `extract_cameras` removes `ExtractedCamera`,
//!   `ExtractedView` and `RenderVisibleEntities` from an inactive camera's render entity, so the
//!   next active frame starts from an empty visible set and every visible mesh is "added";
//! * `bevy_core_pipeline-0.19.0/src/core_3d/mod.rs` `extract_core_3d_camera_phases` skips an
//!   inactive camera and then `retain`s only live views, dropping the view's retained
//!   opaque/alpha-mask/transparent bins;
//! * `bevy_pbr-0.19.0/src/material.rs` `specialize_material_meshes` (and `prepass/mod.rs`'s twin)
//!   `retain` their per-view `SpecializedMaterialPipelineCache` to the live views, so every mesh of
//!   the destination is specialized and queued again the frame the camera comes back.
//!
//! That teardown is what the off-screen and occluded-doorway skips already pay, once per
//! transition; paid every other frame it is the cost of a newly opened doorway on every frame the
//! view does render. Measured on the water preset's cadence (2026-10-09, portal-heavy x4
//! interleaved, `local/review/portal-b3-window-2026-10-09/`): switching the reflection camera off
//! on the skipped frames made the render thread 5.8 ms *slower* (specialize_and_views +1.37,
//! queue +0.68, prepare +0.73, p95 +7.6 ms) while the graph saved only 0.85 ms.
//!
//! # The skip: the camera's render schedule, not its extraction
//!
//! The camera stays active, so extraction, specialization, queuing and the per-view prepare keep
//! their retained state and do only their usual incremental work. What is skipped is the camera's
//! render schedule: `bevy_core_pipeline-0.19.0/src/schedule.rs` `camera_driver` runs one `Core3d`
//! schedule per entry of [`SortedCameras`], so a skipped camera's entry is taken out of that list
//! on a skipped frame ([`drop_skipped_views`], after `sort_cameras` and `prepare_lights` -
//! `bevy_pbr-0.19.0/src/render/light.rs` iterates `SortedCameras` - and before `render_system`).
//! Everything the camera's `Core3d` schedule encodes goes with it: the GPU mesh preprocessing and
//! indirect-parameter dispatches for its view, the shadow passes of its cascades
//! (`per_view_shadow_pass`, in `Core3d` in `bevy_pbr-0.19.0/src/lib.rs`), its depth prepass, main
//! passes and the post-processing into its target. The target image is a persistent asset, so it
//! keeps the last frame's contents; the view's cached textures are still requested by the prepare
//! systems every frame, so the texture cache does not drop them.
//!
//! Not saved: extraction, specialization, queuing, sorting, batching and the per-view prepare of
//! the skipped view still run each frame.
//!
//! # A view nothing demands is switched off instead
//!
//! [`Camera::is_active`] is still the answer for a view the policy says is not rendered at all - no
//! demand, or an invalid policy - and pays the teardown above, once, on the transition. A skip
//! through this module is for a frame between renders of a view that *is* demanded.
//!
//! # The list
//!
//! [`SkippedViews`] collects this frame's skipped cameras in the main world; the systems that pace
//! a view register there ([`crate::view_policy::apply_view_policies`],
//! [`crate::portal`]'s `register_portal_skip`). [`clear_skipped_views`] empties it in `First`, so a
//! skip never outlives its frame, and [`extract_skipped_views`] resolves each camera to its render
//! entity for the render world, where [`drop_skipped_views`] takes their entries out of
//! [`SortedCameras`].

use bevy::{
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems, camera::SortedCameras,
        renderer::render_system, sync_world::RenderEntity,
    },
};

/// The cameras whose render schedule is skipped this frame, by main-world entity.
///
/// Written by the systems that pace a view (`crate::view_policy::apply_view_policies` for a managed
/// view, `crate::portal`'s `pace_portal_camera`'s companion for the portal camera), read once per
/// frame at extraction, and cleared in `First` - so a mark lives exactly one frame and a camera
/// that renders has no entry, whatever the previous frame held.
#[derive(Resource, Debug, Default)]
pub struct SkippedViews(Vec<Entity>);

impl SkippedViews {
    /// Marks `camera` as skipped this frame. Marking twice is the same as once.
    pub fn add(&mut self, camera: Entity) {
        if !self.0.contains(&camera) {
            self.0.push(camera);
        }
    }

    /// Whether `camera` is skipped this frame.
    pub fn contains(&self, camera: Entity) -> bool {
        self.0.contains(&camera)
    }

    /// Whether no camera is skipped this frame.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The cameras skipped this frame, for the extraction.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }

    /// Forgets every mark. `First` does this once a frame ([`clear_skipped_views`]).
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// Registers the shared skip: the resource a pacing system marks, and the three systems that carry
/// the marks to the render thread and take the cameras' schedules out of the frame.
///
/// Registered once, by the plugin that owns the renderer's plumbing
/// ([`crate::render::VercidiumRendererPlugin`]); a pacing plugin
/// ([`crate::view_policy::SecondaryViewPolicyPlugin`], [`crate::portal`]'s `PortalCadencePlugin`)
/// only [`init_resource`](World::init_resource)s [`SkippedViews`] so its own tests can run without
/// the renderer.
pub struct RenderSkipPlugin;

impl Plugin for RenderSkipPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkippedViews>()
            .add_systems(First, clear_skipped_views);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<SkippedRenderViews>()
            .add_systems(ExtractSchedule, extract_skipped_views)
            .add_systems(
                Render,
                drop_skipped_views
                    .in_set(RenderSystems::Render)
                    .before(render_system),
            );
    }
}

/// Empties the frame's list, before any pacing system decides. A mark from the previous frame must
/// not survive into a frame the camera renders: the only thing that keeps a skipped camera's
/// schedule from running is this frame's entry.
fn clear_skipped_views(mut skipped: ResMut<SkippedViews>) {
    skipped.clear();
}

/// The render entities of this frame's skipped cameras.
///
/// The main-world list holds main entities (what the pacing systems know), and the render-side
/// [`SortedCameras`] holds render entities; this is the one place the two are matched, once per
/// frame at extraction.
#[derive(Resource, Default)]
struct SkippedRenderViews(Vec<Entity>);

fn extract_skipped_views(
    mut skipped: ResMut<SkippedRenderViews>,
    views: Extract<Res<SkippedViews>>,
    render_entities: Extract<Query<&RenderEntity>>,
) {
    skipped.0.clear();
    skipped.0.extend(views.iter().filter_map(|camera| {
        // A camera with no render entity (never synced to the render world) or one that is already
        // gone has no schedule to skip.
        render_entities
            .get(camera)
            .ok()
            .map(|render_entity| render_entity.id())
    }));
}

/// Takes each skipped camera's entry out of the list `camera_driver` runs render schedules for.
///
/// It runs in `RenderSystems::Render`, after `sort_cameras` and `prepare_lights` (which iterate
/// [`SortedCameras`]) and before `render_system`, which runs the `RenderGraph` schedule
/// `camera_driver` sits in.
fn drop_skipped_views(skipped: Res<SkippedRenderViews>, mut sorted: ResMut<SortedCameras>) {
    if skipped.0.is_empty() {
        return;
    }
    sorted
        .0
        .retain(|camera| !skipped.0.contains(&camera.entity));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{camera::CameraOutputMode, render::camera::SortedCamera};

    fn camera(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    fn sorted(entity: Entity, order: isize) -> SortedCamera {
        SortedCamera {
            entity,
            order,
            target: None,
            hdr: false,
            output_mode: CameraOutputMode::Skip,
        }
    }

    #[test]
    fn the_list_marks_a_camera_once_and_clears() {
        let mut skipped = SkippedViews::default();
        assert!(skipped.is_empty());
        skipped.add(camera(1));
        skipped.add(camera(1));
        skipped.add(camera(2));
        assert!(skipped.contains(camera(1)));
        assert!(skipped.contains(camera(2)));
        assert!(!skipped.contains(camera(3)));
        assert!(!skipped.is_empty());
        assert_eq!(skipped.iter().collect::<Vec<_>>(), [camera(1), camera(2)]);
        skipped.clear();
        assert!(skipped.is_empty());
        assert!(!skipped.contains(camera(1)));
    }

    /// The drop keeps every other camera and removes exactly the skipped ones, in their order.
    #[test]
    fn only_the_skipped_cameras_leave_the_sorted_list() {
        let mut app = App::new();
        app.insert_resource(SortedCameras(vec![
            sorted(camera(1), -1),
            sorted(camera(2), 0),
            sorted(camera(3), 1),
        ]))
        .insert_resource(SkippedRenderViews(vec![camera(2)]))
        .add_systems(Update, drop_skipped_views);
        app.update();
        let sorted = app.world().resource::<SortedCameras>();
        assert_eq!(
            sorted
                .0
                .iter()
                .map(|camera| camera.entity)
                .collect::<Vec<_>>(),
            [camera(1), camera(3)]
        );
    }

    #[test]
    fn an_empty_list_leaves_the_sorted_cameras_untouched() {
        let mut app = App::new();
        app.insert_resource(SortedCameras(vec![sorted(camera(1), 0)]))
            .insert_resource(SkippedRenderViews::default())
            .add_systems(Update, drop_skipped_views);
        app.update();
        assert_eq!(app.world().resource::<SortedCameras>().0.len(), 1);
    }

    /// A mark lasts exactly the frame it was made in: `First` empties the list, so the frame after
    /// a skip has the camera back in the sorted list even though nothing unmarked it by hand.
    #[test]
    fn a_skip_does_not_outlive_its_frame() {
        #[derive(Resource)]
        struct SkipFor(u32);

        fn pace(skip_for: Option<Res<SkipFor>>, mut skipped: ResMut<SkippedViews>) {
            if let Some(skip_for) = skip_for {
                skipped.add(camera(skip_for.0));
            }
        }

        let mut app = App::new();
        app.add_plugins(RenderSkipPlugin)
            .insert_resource(SkipFor(1))
            .add_systems(Update, pace);
        app.update();
        assert!(app.world().resource::<SkippedViews>().contains(camera(1)));
        // The next frame marks nothing: the previous frame's mark is gone.
        app.world_mut().remove_resource::<SkipFor>();
        app.update();
        assert!(
            app.world().resource::<SkippedViews>().is_empty(),
            "a mark must not carry into the next frame"
        );
    }
}

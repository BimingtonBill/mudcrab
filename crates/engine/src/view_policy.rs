//! Bounds what a secondary camera costs, and refills it the frame the world jumps.
//!
//! A camera carrying a [`ViewPolicy`] is *managed*. [`apply_view_policies`] is the only writer of
//! its [`Camera::is_active`], it keeps the camera render target's image handle for the camera's
//! whole life (so water samples the last frame the view rendered rather than a cleared texture),
//! and it resizes that image only when the policy's `target_size` changes. The camera renders on
//! the frames the policy admits it and is switched off on the others.
//!
//! Nothing here knows what a view shows. The view's own adapter - the water reflection in
//! `render.rs` today, a portal view later - runs in [`ViewPolicySystems::Prepare`] and fills the
//! camera's [`ViewDemand`] with what it wants this frame: the consumer's pose, the view's pose and
//! projection, and a revision of the values its own domain depends on. [`apply_view_policies`] then
//! compares that demand with the frame the view was last scheduled on and decides.
//!
//! Both run in `PostUpdate` before transform propagation and camera updates, so a frame that moves
//! or rebases a consumer is reflected in the same frame rather than the next one. Adapters must
//! read the world through [`TransformHelper`](bevy::transform::helper::TransformHelper) for the
//! same reason: physics and the streaming fixture write `Transform` in `Update`, and the
//! `GlobalTransform` on those entities is a frame behind until propagation runs.
//!
//! # Decision rules
//!
//! Each frame, for each managed camera:
//!
//! 1. An invalid policy leaves the camera off.
//! 2. The accumulated [`DirtyReasons`] gain any cause the policy can see by comparing values: a
//!    changed render origin or [`CameraSpace`], a message naming the view, a changed projection or
//!    domain revision, a consumer that moved past the policy's jump thresholds since the pose the
//!    view was *last scheduled* with, demand that has just reopened, or the staleness deadline.
//! 3. Without a demand the camera is switched off and the image and the dirty reasons are kept. An
//!    unseen view is not refreshed just because its image has aged.
//! 4. With a demand, a dirty view, a view that has never been scheduled, or one past
//!    `max_staleness` is admitted now. Otherwise it is admitted when its [`ViewUpdateRate`] is due:
//!    every frame, every other *admission* rather than on a global parity, or not at all on demand.
//! 5. An admitted view gets the demanded pose and projection, and the frame's values are recorded
//!    as the new scheduled state. Every other demanded view is switched off for this frame.

use crate::{sky::CameraSpace, streaming::RenderOrigin};
use bevy::{
    camera::CameraUpdateSystems, prelude::*, render::render_resource::Extent3d,
    transform::TransformSystems,
};
use std::time::Duration;

/// The largest target side a [`ViewPolicy`] may ask for.
///
/// The bound only exists so a mistyped policy is refused rather than handed to the renderer:
/// 8192 is inside the guaranteed minimum `max_texture_dimension_2d` of every backend this engine
/// targets.
pub const MAX_TARGET_DIMENSION: u32 = 8192;

/// The reflection target as it has always been: the size behaviour is kept at until a preset is
/// chosen for the water view.
pub const BASELINE_TARGET_SIZE: UVec2 = UVec2::new(1024, 576);

/// The water preset target (an unverified starting value): a quarter of the pixels, 16:9 like the
/// default window so the view's projection keeps the observer's aspect while costing less.
pub const WATER_TARGET_SIZE: UVec2 = UVec2::new(512, 288);

/// The water preset's admission priority. Priority ranks optional work against other optional
/// work; it is not [`Camera::order`], which stays the texture dependency order.
pub const WATER_PRIORITY: i16 = -100;

/// How long a demanded view may go unrendered before the deadline forces a refresh.
pub const DEFAULT_MAX_STALENESS: Duration = Duration::from_millis(100);

/// How far a consumer may move from the pose its view was last scheduled with before the view is
/// stale.
pub const DEFAULT_JUMP_DISTANCE: f32 = 256.0;

/// How far a consumer may turn from the pose its view was last scheduled with before the view is
/// stale: ten degrees.
pub const DEFAULT_JUMP_ANGLE_RADIANS: f32 = 10.0 * (std::f32::consts::PI / 180.0);

/// How often a managed view renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewUpdateRate {
    /// Every frame it is demanded.
    #[default]
    EveryFrame,
    /// Every other *admission*: a view skipped for one frame is due on the next, however the skips
    /// line up with any global frame parity.
    EveryOtherFrame,
    /// Only when the view is dirty or past `max_staleness`.
    OnDemand,
}

/// Whether a managed view renders the sun's directional shadow cascades of its own.
///
/// A view's shadows are not shared with the main view's: Bevy builds and renders a separate set of
/// cascades for every active camera, so a secondary view that keeps them on pays for a second
/// shadow pass over the same sun. The policy only *states* the choice; the route that realises it
/// is [`crate::shadow_route`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewShadows {
    /// The view is lit by the shadowed sun, as every view is by default.
    #[default]
    On,
    /// The view is lit by an unshadowed copy of the sun and renders no cascades of its own.
    Off,
}

/// What a managed view is allowed to cost.
///
/// The preset a view is given decides everything about its cost; see [`ViewPolicy::baseline`] and
/// [`ViewPolicy::water`]. The default is [`ViewPolicy::baseline`], so a camera that carries a
/// policy without choosing one keeps the behaviour it had before the policy existed.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ViewPolicy {
    /// The size of the view's image target. A change resizes it (losing its contents) and forces a
    /// render.
    pub target_size: UVec2,
    pub update_rate: ViewUpdateRate,
    /// Ranks this view against other optional views. Not [`Camera::order`].
    pub priority: i16,
    /// The age at which a demanded view is refreshed whatever its update rate says. Zero means
    /// every demanded frame; [`Duration::MAX`] leaves the update rate as the only admission.
    pub max_staleness: Duration,
    /// How far the consumer may move from the view's last scheduled pose before the view is forced.
    pub jump_distance: f32,
    /// How far the consumer may turn from the view's last scheduled pose before the view is forced.
    pub jump_angle_radians: f32,
    /// Whether the view renders the sun's shadow cascades of its own.
    pub shadows: ViewShadows,
}

impl ViewPolicy {
    /// Today's behaviour for a view that has not been given a preset: full size, every frame.
    pub fn baseline() -> Self {
        Self {
            target_size: BASELINE_TARGET_SIZE,
            update_rate: ViewUpdateRate::EveryFrame,
            priority: 0,
            max_staleness: DEFAULT_MAX_STALENESS,
            jump_distance: DEFAULT_JUMP_DISTANCE,
            jump_angle_radians: DEFAULT_JUMP_ANGLE_RADIANS,
            shadows: ViewShadows::On,
        }
    }

    /// Starting settings for the water reflection: a quarter of the pixels,
    /// every other frame, low priority, no shadow cascades of its own, with a hundred milliseconds
    /// and 256 units or ten degrees of staleness allowed before the view is forced.
    ///
    /// **What it draws, it draws whole** (impl-1094): `shadows: Off` moves the view to the
    /// shadowless route ([`crate::shadow_route`]), which draws the same content the shadowed route
    /// does through [`crate::render::UNSHADOWED_REFLECTION_LAYER`] - every producer of world content
    /// widens its layers with [`crate::render::reflectable_layers`], the portal's cell stamping
    /// included, so a preset that drew a thinner scene would be a shortcut rather than a saving.
    ///
    /// These are unverified quality settings, not measured optima. They exist to be compared with
    /// [`ViewPolicy::baseline`] on the same scene.
    pub fn water() -> Self {
        Self {
            target_size: WATER_TARGET_SIZE,
            update_rate: ViewUpdateRate::EveryOtherFrame,
            priority: WATER_PRIORITY,
            shadows: ViewShadows::Off,
            ..Self::baseline()
        }
    }

    /// A [`Duration::ZERO`] staleness is legal and means "refresh every demanded frame"; a
    /// [`Duration::MAX`] one means "only when something changed". A negative or non-finite jump
    /// threshold would silently make every pose a jump, so it is refused instead.
    pub fn validate(&self) -> Result<(), ViewPolicyError> {
        if self.target_size.x == 0 || self.target_size.y == 0 {
            return Err(ViewPolicyError::EmptyTargetSize);
        }
        if self.target_size.x > MAX_TARGET_DIMENSION || self.target_size.y > MAX_TARGET_DIMENSION {
            return Err(ViewPolicyError::TargetSizeTooLarge(self.target_size));
        }
        if !self.jump_distance.is_finite() || self.jump_distance < 0.0 {
            return Err(ViewPolicyError::InvalidJumpDistance(self.jump_distance));
        }
        if !self.jump_angle_radians.is_finite() || self.jump_angle_radians < 0.0 {
            return Err(ViewPolicyError::InvalidJumpAngle(self.jump_angle_radians));
        }
        Ok(())
    }
}

impl Default for ViewPolicy {
    fn default() -> Self {
        Self::baseline()
    }
}

/// Why a [`ViewPolicy`] was refused.
///
/// An invalid policy is reported, not corrected: the view stays off and the error is recorded, so a
/// mistyped preset is visible rather than silently rounded to something renderable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewPolicyError {
    EmptyTargetSize,
    TargetSizeTooLarge(UVec2),
    InvalidJumpDistance(f32),
    InvalidJumpAngle(f32),
}

impl std::fmt::Display for ViewPolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTargetSize => {
                write!(formatter, "a view policy target size must be positive")
            }
            Self::TargetSizeTooLarge(size) => write!(
                formatter,
                "a view policy target size must be at most {MAX_TARGET_DIMENSION} on a side, not {size}"
            ),
            Self::InvalidJumpDistance(distance) => write!(
                formatter,
                "a view policy jump distance must be finite and nonnegative, not {distance}"
            ),
            Self::InvalidJumpAngle(angle) => write!(
                formatter,
                "a view policy jump angle must be finite and nonnegative, not {angle} radians"
            ),
        }
    }
}

/// One frame's request for a managed view, filled by the view's adapter.
///
/// No `PartialEq`: [`Projection`] has none, so pieces are compared as the policy needs them -
/// [`projection_changed`] for the projection and [`pose_jumped`] for the observer's pose.
#[derive(Debug, Clone)]
pub struct ViewRequest {
    /// The consumer's current pose. Its growth away from the pose the view was last scheduled with
    /// is what [`ViewPolicy::jump_distance`] and [`ViewPolicy::jump_angle_radians`] measure.
    pub observer_pose: GlobalTransform,
    /// The pose the view's camera should render from this frame.
    pub pose: Transform,
    /// The projection it should render with, or `None` to leave the camera's projection alone.
    pub projection: Option<Projection>,
    /// A revision of the values the adapter's own domain depends on, opaque to the policy: any
    /// change to it forces the view to render again. The water adapter derives it from the mirror
    /// plane's height, so a surface rising under an otherwise unmoved observer still refreshes.
    pub domain_revision: u64,
}

/// What a managed view is needed for this frame.
///
/// The adapter replaces this every frame: a request while a consumer can draw the view, `None` when
/// nothing needs it - no water in view, or no observer to mirror. The policy then switches the
/// camera off and keeps the image and any pending invalidation.
#[derive(Component, Debug, Clone, Default)]
pub struct ViewDemand {
    pub request: Option<ViewRequest>,
}

/// What a managed view renders into: the image the policy keeps alive and resizes.
///
/// `image` is the handle the view's materials sample and it never changes for the life of the
/// camera, so a view that skips a frame leaves the previous frame's texels on screen. `allocated`
/// records the size the policy last set that image to, for diagnostics; the image asset itself is
/// what the policy compares a policy's `target_size` against.
#[derive(Component, Debug, Clone)]
pub struct SecondaryViewTarget {
    pub image: Handle<Image>,
    pub allocated: UVec2,
}

/// Why a managed view must render on the next frame it is demanded.
///
/// The reasons accumulate while a view is not demanded and are cleared only by a scheduled frame,
/// so a view that is hidden when the world jumps is refreshed the frame it is needed again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirtyReasons {
    /// It has never been scheduled.
    pub never_scheduled: bool,
    /// An [`InvalidateSecondaryViews`] message named it.
    pub invalidated: bool,
    /// The render origin moved.
    pub origin: bool,
    /// The camera space changed between exterior and interior.
    pub space: bool,
    /// The adapter's domain revision changed.
    pub domain: bool,
    /// The demanded projection differs from the one it was last scheduled with.
    pub projection: bool,
    /// The consumer moved or turned past the policy's jump thresholds.
    pub pose_jump: bool,
    /// Nothing needed the view, and something does again.
    pub demand_reopened: bool,
    /// It went unrendered past the policy's maximum staleness.
    pub stale: bool,
    /// Its image was resized, so its texels are cropped rather than fresh.
    pub target_resized: bool,
    /// Its policy was invalid and is valid again, so what it holds was made under other rules.
    pub revalidated: bool,
}

impl DirtyReasons {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// What a managed view was last scheduled with.
///
/// Everything here is compared, never recomputed: the jump thresholds measure the consumer against
/// the pose of the frame the view actually rendered, so a run of small moves cannot each stay
/// inside the threshold and never force a refresh.
#[derive(Debug, Clone)]
pub struct ScheduledView {
    /// The policy's own frame count when the view was scheduled.
    pub frame: u64,
    /// The elapsed time when the view was scheduled.
    pub time: Duration,
    /// The consumer's pose when the view was scheduled.
    pub observer_pose: GlobalTransform,
    /// The projection the view was scheduled with.
    pub projection: Option<Projection>,
    /// The render origin when the view was scheduled.
    pub origin: Option<IVec2>,
    /// The camera space when the view was scheduled.
    pub space: Option<CameraSpace>,
    /// The adapter's domain revision when the view was scheduled.
    pub domain_revision: u64,
}

/// What a managed view cost and saved, counted per view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewCounters {
    /// Frames the view was demanded.
    pub requested: u64,
    /// Frames the view was admitted and rendered.
    pub rendered: u64,
    /// Demanded frames it was not admitted on.
    pub skipped: u64,
    /// Admissions a dirty reason asked for, rather than the update rate alone.
    pub forced: u64,
}

/// What a view's policy remembers between frames.
#[derive(Component, Debug, Clone, Default)]
pub struct ViewPolicyState {
    /// Frames this policy has run, its own count rather than any global frame number.
    pub frames: u64,
    /// The last frame the view was scheduled, and with what.
    pub last_scheduled: Option<ScheduledView>,
    /// Reasons the view must render, accumulated since it was last scheduled.
    pub dirty: DirtyReasons,
    /// The reasons that forced the last scheduled frame, kept for diagnostics.
    pub last_forced: DirtyReasons,
    /// Frames demanded, rendered, skipped and forced.
    pub counters: ViewCounters,
    /// Whether the view was demanded on the previous frame.
    pub demand_open: bool,
    /// The error the policy last failed [`ViewPolicy::validate`] with, or `None` while it is valid.
    pub invalid: Option<ViewPolicyError>,
    /// Whether the missing-[`SecondaryViewTarget`] warning has been logged for this view.
    pub warned_missing_target: bool,
}

/// A discontinuity the policy cannot see in the values it compares.
///
/// The reasons only exist so a caller can say *what* jumped - the effect is the same for all of
/// them: any view the message names renders on the next frame it is demanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewInvalidationReason {
    /// A consumer was moved somewhere new in one step: a teleport, a door landing, a debug jump.
    Teleport,
    /// The render origin was rebased.
    RenderOriginRebase,
    /// The active space changed, as when the player crosses a door or an interior boundary.
    SpaceChange,
}

/// Tells the secondary views that the world changed in a way the policy cannot compare.
///
/// # Callers
///
/// Send this **after the destination values are committed**: after the consumer's transform and
/// [`CameraSpace`] have been written, in the same frame. A caller that sends it first, or from a
/// step that runs after `PostUpdate`'s view policies, has its views refreshed on the following
/// frame instead. Door crossings, teleport scripts and map travel must all send it; a pose jump
/// the policy can measure by itself is the exception, not the rule.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidateSecondaryViews {
    pub reason: ViewInvalidationReason,
    /// The managed camera to invalidate, or `None` for every managed view.
    pub target: Option<Entity>,
}

/// Where the view-policy systems run, relative to each other.
///
/// Both sets are ordered before `PostUpdate`'s transform propagation and camera updates: a view
/// admitted this frame has its pose propagated and its frustum built from that pose before the
/// frame is extracted, which is what makes a teleport land in the same frame.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewPolicySystems {
    /// Adapters fill their camera's [`ViewDemand`] here.
    Prepare,
    /// [`apply_view_policies`] decides here, and is the only writer of `Camera::is_active`.
    Apply,
}

/// Registers the policy: the invalidation message, and the ordered systems it runs.
pub struct SecondaryViewPolicyPlugin;

impl Plugin for SecondaryViewPolicyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<InvalidateSecondaryViews>()
            .configure_sets(
                PostUpdate,
                (ViewPolicySystems::Prepare, ViewPolicySystems::Apply)
                    .chain()
                    .before(TransformSystems::Propagate)
                    .before(CameraUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                apply_view_policies.in_set(ViewPolicySystems::Apply),
            );
    }
}

/// Decides which managed views render this frame, and is their only writer of
/// [`Camera::is_active`].
///
/// See the module documentation for the rules. Everything it compares comes from the view's own
/// [`ViewDemand`] or from the engine-wide values a view can be invalidated by; it never reads a
/// view's contents.
#[allow(clippy::type_complexity)]
pub fn apply_view_policies(
    time: Res<Time>,
    origin: Option<Res<RenderOrigin>>,
    space: Option<Res<CameraSpace>>,
    mut invalidations: MessageReader<InvalidateSecondaryViews>,
    mut images: ResMut<Assets<Image>>,
    mut views: Query<(
        Entity,
        &ViewPolicy,
        &mut ViewPolicyState,
        Option<&ViewDemand>,
        &mut Camera,
        &mut Transform,
        &mut Projection,
        Option<&mut SecondaryViewTarget>,
    )>,
) {
    let origin = origin.map(|origin| origin.0);
    let space = space.map(|space| *space);
    // Read once, and only here: a message naming no managed view is dropped rather than left for a
    // frame in which that entity happens to exist.
    let invalidations: Vec<InvalidateSecondaryViews> = invalidations.read().copied().collect();

    for (entity, policy, mut state, demand, mut camera, mut transform, mut projection, target) in
        &mut views
    {
        state.frames = state.frames.wrapping_add(1);
        let frame = state.frames;

        if let Err(error) = policy.validate() {
            if state.invalid != Some(error) {
                warn!(
                    view = ?entity,
                    "a view policy is invalid and its view stays off: {error}"
                );
                state.invalid = Some(error);
            }
            set_active(&mut camera, false);
            // Demand seen during an invalid period still counts, so demand that reappears once the
            // policy is valid again reopens the view and forces a refresh.
            state.demand_open = demand.is_some_and(|demand| demand.request.is_some());
            continue;
        }
        let revalidated = state.invalid.take().is_some();

        let request = demand.and_then(|demand| demand.request.as_ref());
        let demanded = request.is_some();
        let mut dirty = state.dirty;
        dirty.revalidated |= revalidated;

        // Causes the policy can see whether or not the view is demanded, so that a view hidden
        // when the world jumps is still refreshed the frame it is needed again.
        if state.last_scheduled.is_none() {
            dirty.never_scheduled = true;
        } else if let Some(last) = &state.last_scheduled {
            dirty.origin |= last.origin != origin;
            dirty.space |= last.space != space;
        }
        if invalidations.iter().any(|invalidation| {
            invalidation.target.is_none() || invalidation.target == Some(entity)
        }) {
            dirty.invalidated = true;
        }

        if let Some(request) = request {
            if let Some(last) = &state.last_scheduled {
                dirty.domain |= last.domain_revision != request.domain_revision;
                dirty.projection |=
                    projection_changed(last.projection.as_ref(), request.projection.as_ref());
                dirty.pose_jump |= pose_jumped(
                    &last.observer_pose,
                    &request.observer_pose,
                    policy.jump_distance,
                    policy.jump_angle_radians,
                );
                dirty.stale |= time.elapsed().saturating_sub(last.time) >= policy.max_staleness;
            }
            dirty.demand_reopened |= !state.demand_open;
        }

        if target.is_some() {
            state.warned_missing_target = false;
        } else if !state.warned_missing_target {
            warn!(
                view = ?entity,
                "a managed view has no SecondaryViewTarget, so its target is never resized"
            );
            state.warned_missing_target = true;
        }
        if let Some(mut target) = target
            && images
                .get(&target.image)
                .is_some_and(|image| image.size() != policy.target_size)
        {
            // A resize is a new allocation whose contents are cropped rather than scaled, so it
            // counts as an invalidation on top of whatever asked for this policy's size.
            if let Some(mut image) = images.get_mut(&target.image) {
                image.resize(Extent3d {
                    width: policy.target_size.x,
                    height: policy.target_size.y,
                    depth_or_array_layers: 1,
                });
            }
            target.allocated = policy.target_size;
            dirty.target_resized = true;
        }

        let due = match policy.update_rate {
            ViewUpdateRate::EveryFrame => true,
            ViewUpdateRate::EveryOtherFrame => match &state.last_scheduled {
                Some(last) => frame.saturating_sub(last.frame) >= 2,
                None => true,
            },
            ViewUpdateRate::OnDemand => false,
        };
        let forced = dirty.any();
        let admitted = demanded && (forced || due);

        state.counters.requested += u64::from(demanded);
        if admitted {
            state.counters.rendered += 1;
            state.counters.forced += u64::from(forced);
        } else if demanded {
            state.counters.skipped += 1;
        }

        match request {
            Some(request) if admitted => {
                if *transform != request.pose {
                    *transform = request.pose;
                }
                // Writing the projection marks it changed, so only write when the parts the policy
                // carries differ: the camera's own `far` and aspect stay its own.
                if let Some(requested) = &request.projection {
                    apply_projection(&mut projection, requested);
                }
                set_active(&mut camera, true);
                state.last_forced = dirty;
                state.last_scheduled = Some(ScheduledView {
                    frame,
                    time: time.elapsed(),
                    observer_pose: request.observer_pose,
                    projection: request.projection.clone(),
                    origin,
                    space,
                    domain_revision: request.domain_revision,
                });
                state.dirty = DirtyReasons::default();
            }
            _ => {
                set_active(&mut camera, false);
                state.dirty = dirty;
            }
        }
        state.demand_open = demanded;
    }
}

/// Writes `Camera::is_active` only when it changes, so a view that stays on or off does not mark
/// its camera changed every frame.
fn set_active(camera: &mut Mut<Camera>, active: bool) {
    if camera.is_active != active {
        camera.is_active = active;
    }
}

/// Whether two projections differ in what a managed view takes from its observer.
///
/// For a perspective projection that is the field of view and the near plane. The aspect belongs
/// to the view's own target (Bevy recomputes it from the target), and `far` bounds what the view
/// culls and draws, so neither is carried over and a difference in either alone is not a change.
/// Other projections are compared by their clip-from-view matrix, with the camera's own far
/// substituted into the request so a view that keeps its far does not read as changed every frame. `None` means the adapter asked
/// for no projection at all, so only a change between `Some` and `None` counts.
fn projection_changed(current: Option<&Projection>, requested: Option<&Projection>) -> bool {
    match (current, requested) {
        (Some(Projection::Perspective(current)), Some(Projection::Perspective(requested))) => {
            current.fov != requested.fov || current.near != requested.near
        }
        // The camera keeps its own far, so the request is compared as it would be once written.
        (Some(current), Some(requested)) => {
            current.get_clip_from_view() != with_far(requested, current.far()).get_clip_from_view()
        }
        (current, requested) => current.is_some() != requested.is_some(),
    }
}

/// `projection` with its far replaced by `far`; a custom projection has none and is returned as is.
fn with_far(projection: &Projection, far: f32) -> Projection {
    let mut projection = projection.clone();
    match &mut projection {
        Projection::Perspective(projection) => projection.far = far,
        Projection::Orthographic(projection) => projection.far = far,
        Projection::Custom(_) => {}
    }
    projection
}

/// Gives `camera` the observer's projection without its `far` or aspect, writing only on a change.
///
/// A perspective projection takes the field of view and near plane. Any other kind is replaced
/// whole with the camera's far kept, except [`Projection::Custom`], which has no far to keep and is
/// taken as requested.
///
/// Carrying the observer's `far` over would make the secondary view cull and draw as far as the
/// observer does, which is exactly the cost a [`ViewPolicy`] exists to bound.
fn apply_projection(camera: &mut Mut<Projection>, requested: &Projection) {
    if !projection_changed(Some(camera.as_ref()), Some(requested)) {
        return;
    }
    match (&mut **camera, requested) {
        (Projection::Perspective(current), Projection::Perspective(requested)) => {
            current.fov = requested.fov;
            current.near = requested.near;
        }
        (current, requested) => {
            *current = with_far(requested, current.far());
        }
    }
}

/// Whether the consumer moved past the policy's jump thresholds since the view was last scheduled.
///
/// The comparison is against the last *scheduled* pose rather than the previous frame's, so several
/// small moves add up; and the angle is between orientations, so a turn of any axis counts.
fn pose_jumped(
    scheduled: &GlobalTransform,
    current: &GlobalTransform,
    distance: f32,
    angle_radians: f32,
) -> bool {
    scheduled.translation().distance(current.translation()) > distance
        || scheduled.rotation().angle_between(current.rotation()) > angle_radians
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{camera::RenderTarget, render::render_resource::TextureFormat};

    /// A managed view driven by hand: the policy, its state, and an image target, with no adapter
    /// and no renderer, so a frame sequence is nothing but the values written between frames.
    struct ViewHarness {
        app: App,
        camera: Entity,
        image: Handle<Image>,
    }

    impl ViewHarness {
        fn new(policy: ViewPolicy) -> Self {
            let mut app = App::new();
            app.init_resource::<Assets<Image>>()
                .init_resource::<Time>()
                .insert_resource(RenderOrigin(IVec2::ZERO))
                .insert_resource(CameraSpace::Exterior)
                .add_plugins(SecondaryViewPolicyPlugin);
            let image =
                app.world_mut()
                    .resource_mut::<Assets<Image>>()
                    .add(Image::new_target_texture(
                        policy.target_size.x,
                        policy.target_size.y,
                        TextureFormat::Rgba16Float,
                        None,
                    ));
            let camera = app
                .world_mut()
                .spawn((
                    Camera::default(),
                    Transform::default(),
                    Projection::default(),
                    RenderTarget::Image(image.clone().into()),
                    policy,
                    ViewPolicyState::default(),
                    ViewDemand::default(),
                    SecondaryViewTarget {
                        image: image.clone(),
                        allocated: policy.target_size,
                    },
                ))
                .id();
            Self { app, camera, image }
        }

        /// Fills the camera's demand as an adapter would, for this frame.
        fn demand(&mut self, request: Option<ViewRequest>) {
            self.app
                .world_mut()
                .entity_mut(self.camera)
                .get_mut::<ViewDemand>()
                .expect("the harness camera is managed")
                .request = request;
        }

        fn set_policy(&mut self, policy: ViewPolicy) {
            self.app.world_mut().entity_mut(self.camera).insert(policy);
        }

        fn set_origin(&mut self, origin: IVec2) {
            self.app.world_mut().resource_mut::<RenderOrigin>().0 = origin;
        }

        fn set_space(&mut self, space: CameraSpace) {
            *self.app.world_mut().resource_mut::<CameraSpace>() = space;
        }

        fn invalidate(&mut self, reason: ViewInvalidationReason, target: Option<Entity>) {
            self.app
                .world_mut()
                .write_message(InvalidateSecondaryViews { reason, target });
        }

        /// Moves the harness clock, so staleness deadlines can be crossed without waiting.
        fn advance(&mut self, millis: u64) {
            self.app
                .world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(millis));
        }

        fn frame(&mut self) -> ViewFrame {
            self.app.update();
            let world = self.app.world();
            let camera = world.get::<Camera>(self.camera).unwrap();
            let state = world.get::<ViewPolicyState>(self.camera).unwrap();
            let target = world.get::<SecondaryViewTarget>(self.camera).unwrap();
            ViewFrame {
                active: camera.is_active,
                pose: *world.get::<Transform>(self.camera).unwrap(),
                projection: world.get::<Projection>(self.camera).unwrap().clone(),
                image_size: world
                    .resource::<Assets<Image>>()
                    .get(&self.image)
                    .expect("the target image lives as long as the view")
                    .size(),
                handle: target.image.clone(),
                allocated: target.allocated,
                dirty: state.dirty,
                last_forced: state.last_forced,
                counters: state.counters,
            }
        }
    }

    struct ViewFrame {
        active: bool,
        pose: Transform,
        projection: Projection,
        image_size: UVec2,
        handle: Handle<Image>,
        allocated: UVec2,
        dirty: DirtyReasons,
        last_forced: DirtyReasons,
        counters: ViewCounters,
    }

    /// A frame's request from a consumer standing at `observer` looking down -Z, mirrored to
    /// `pose`. The projection is whichever one the harness was given, so a policy test never has to
    /// think about it.
    fn request(observer: Transform, pose: Transform) -> ViewRequest {
        ViewRequest {
            observer_pose: GlobalTransform::from(observer),
            pose,
            projection: Some(Projection::default()),
            domain_revision: 0,
        }
    }

    fn still(observer: Transform) -> ViewRequest {
        request(
            observer,
            Transform::from_translation(Vec3::new(0.0, -50.0, 0.0)),
        )
    }

    #[test]
    fn a_every_frame_policy_renders_every_demanded_frame() {
        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        let mut rendered = 0;
        for _ in 0..3 {
            harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
            let frame = harness.frame();
            assert!(frame.active);
            assert_eq!(frame.counters.skipped, 0);
            rendered += 1;
            assert_eq!(frame.counters.rendered, rendered);
            assert_eq!(frame.counters.requested, rendered);
        }
    }

    #[test]
    fn an_every_other_frame_policy_skips_a_frame_between_renders() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::EveryOtherFrame;
        let mut harness = ViewHarness::new(policy);
        let demand = |harness: &mut ViewHarness| {
            harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        };

        demand(&mut harness);
        assert!(
            harness.frame().active,
            "the first demanded frame has no schedule yet"
        );
        demand(&mut harness);
        assert!(
            !harness.frame().active,
            "one frame after an admission is not due"
        );
        demand(&mut harness);
        assert!(
            harness.frame().active,
            "two frames after an admission is due"
        );
        demand(&mut harness);
        assert!(!harness.frame().active);
        // The demand is carried on to the fifth frame, which is two frames after the third
        // admission and so is due again.
        let counters = harness.frame().counters;
        assert_eq!(counters.requested, 5);
        assert_eq!(counters.rendered, 3);
        assert_eq!(counters.skipped, 2);
    }

    #[test]
    fn an_on_demand_policy_renders_only_when_something_changes() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let mut last = None;
        for _ in 0..5 {
            harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
            // A minute of wall clock between frames: with no deadline, age alone never admits one.
            harness.advance(60_000);
            last = Some(harness.frame());
        }
        let counters = last.unwrap().counters;
        assert_eq!(counters.requested, 5);
        assert_eq!(counters.rendered, 1, "only the first frame had no schedule");
        assert_eq!(counters.skipped, 4);
        assert_eq!(counters.forced, 1);
    }

    #[test]
    fn a_staleness_deadline_forces_a_render_of_an_otherwise_idle_view() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::from_millis(100);
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        assert!(harness.frame().active);

        harness.advance(50);
        harness.demand(Some(still(observer)));
        assert!(!harness.frame().active, "half the deadline has not passed");

        harness.advance(60);
        harness.demand(Some(still(observer)));
        let frame = harness.frame();
        assert!(frame.active, "the deadline has passed");
        assert!(frame.last_forced.stale);
        assert!(
            !frame.dirty.any(),
            "a scheduled frame clears the accumulated reasons"
        );
    }

    #[test]
    fn a_view_without_demand_is_off_and_keeps_its_image() {
        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        let rendered = harness.frame();
        assert!(rendered.active);

        for _ in 0..3 {
            harness.demand(None);
            let frame = harness.frame();
            assert!(!frame.active);
            assert_eq!(frame.handle, rendered.handle);
            assert_eq!(frame.image_size, rendered.image_size);
        }
        assert_eq!(harness.frame().counters.requested, 1);
        assert_eq!(harness.frame().counters.rendered, 1);
    }

    #[test]
    fn reopening_a_demand_renders_at_once() {
        // On demand and with no deadline, so the reopening itself is the only thing that can admit
        // the frame: every other value in the request is identical to the admitted one.
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        assert!(
            harness.frame().active,
            "the first demanded frame has no schedule"
        );
        harness.demand(None);
        assert!(!harness.frame().active);

        harness.demand(Some(still(observer)));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.demand_reopened);
    }

    #[test]
    fn the_image_handle_never_changes_and_a_policy_change_resizes_it() {
        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        let first = harness.frame();
        assert_eq!(first.image_size, BASELINE_TARGET_SIZE);
        assert_eq!(first.allocated, BASELINE_TARGET_SIZE);
        // A baseline policy asks for the size the target already has, so the image is untouched.
        assert!(!first.last_forced.target_resized);

        for _ in 0..3 {
            harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
            let frame = harness.frame();
            assert_eq!(
                frame.handle, first.handle,
                "the handle is what materials hold"
            );
            assert_eq!(frame.image_size, BASELINE_TARGET_SIZE);
            assert!(!frame.last_forced.target_resized);
        }

        harness.set_policy(ViewPolicy::water());
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        let resized = harness.frame();
        assert!(resized.active);
        assert!(
            resized.last_forced.target_resized,
            "a resize forces a render"
        );
        assert_eq!(resized.image_size, WATER_TARGET_SIZE);
        assert_eq!(resized.allocated, WATER_TARGET_SIZE);
        assert_eq!(
            resized.handle, first.handle,
            "resizing keeps the same handle"
        );

        // The next admission, two frames on, has nothing left to resize.
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        assert!(
            !harness.frame().active,
            "the water preset skips a frame between renders"
        );
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        let settled = harness.frame();
        assert!(settled.active);
        assert_eq!(settled.image_size, WATER_TARGET_SIZE);
        assert!(!settled.last_forced.target_resized);
    }

    #[test]
    fn a_teleport_invalidation_renders_on_a_would_skip_frame() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::EveryOtherFrame;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        assert!(
            harness.frame().active,
            "the first demanded frame has no schedule"
        );

        // The teleport commits the pose and says so; the frame that follows the admission is one
        // the cadence would skip, and it renders anyway.
        harness.demand(Some(still(observer)));
        harness.invalidate(ViewInvalidationReason::Teleport, Some(harness.camera));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.invalidated);
    }

    #[test]
    fn an_invalidation_for_another_view_leaves_this_one_alone() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::EveryOtherFrame;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);
        let other_view = harness.app.world_mut().spawn_empty().id();

        harness.demand(Some(still(observer)));
        assert!(harness.frame().active);

        harness.demand(Some(still(observer)));
        harness.invalidate(ViewInvalidationReason::Teleport, Some(other_view));
        let frame = harness.frame();
        assert!(
            !frame.active,
            "the message named a different view, and the cadence skips this frame"
        );
        assert!(!frame.dirty.invalidated);
    }

    #[test]
    fn an_invalidation_without_a_target_names_every_view() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::EveryOtherFrame;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        assert!(harness.frame().active);

        harness.demand(Some(still(observer)));
        harness.invalidate(ViewInvalidationReason::RenderOriginRebase, None);
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.invalidated);
    }

    #[test]
    fn explicit_invalidation_renders_on_an_unchanged_pose() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        assert!(harness.frame().active);
        harness.demand(Some(still(observer)));
        assert!(!harness.frame().active);

        // A door can land a consumer in the same pose in a new space; nothing else in the demand
        // moved, so only the message can refresh the view.
        harness.demand(Some(still(observer)));
        harness.invalidate(ViewInvalidationReason::SpaceChange, None);
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.invalidated);
    }

    #[test]
    fn an_origin_change_while_hidden_stays_dirty_until_demand_returns() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        assert!(harness.frame().active);
        harness.demand(Some(still(observer)));
        assert!(!harness.frame().active);

        harness.demand(None);
        harness.set_origin(IVec2::new(3, -2));
        let hidden = harness.frame();
        assert!(!hidden.active);
        assert!(
            hidden.dirty.origin,
            "the change is remembered while nothing needs the view"
        );

        // Still hidden, and still dirty: nothing refreshes an unseen view.
        harness.demand(None);
        let still_hidden = harness.frame();
        assert!(!still_hidden.active);
        assert!(still_hidden.dirty.origin);

        harness.demand(Some(still(observer)));
        let reopened = harness.frame();
        assert!(reopened.active);
        assert!(reopened.last_forced.origin);
        assert!(!reopened.dirty.any());
    }

    #[test]
    fn a_space_change_renders_a_demanded_view() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        harness.frame();
        harness.demand(Some(still(observer)));
        assert!(!harness.frame().active);

        harness.set_space(CameraSpace::Interior);
        harness.demand(Some(still(observer)));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.space);
    }

    #[test]
    fn a_projection_change_renders_a_demanded_view() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        harness.frame();
        let narrower = Projection::Perspective(PerspectiveProjection {
            fov: 0.5,
            ..default()
        });
        harness.demand(Some(ViewRequest {
            projection: Some(narrower),
            ..still(observer)
        }));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.projection);
        assert_eq!(
            frame.projection.get_clip_from_view(),
            Projection::Perspective(PerspectiveProjection {
                fov: 0.5,
                ..default()
            })
            .get_clip_from_view(),
            "the admitted frame carries the demanded projection"
        );
    }

    #[test]
    fn a_domain_revision_change_renders_a_demanded_view() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);

        harness.demand(Some(still(observer)));
        harness.frame();
        // The water level rose: every observable value of the request but the revision is the same.
        harness.demand(Some(ViewRequest {
            domain_revision: 1,
            ..still(observer)
        }));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.domain);
    }

    #[test]
    fn an_angular_jump_forces_a_render() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);

        harness.demand(Some(still(Transform::IDENTITY)));
        harness.frame();
        // Ten degrees is the threshold; twenty is a jump even with no translation at all.
        let turned = Transform::from_rotation(Quat::from_rotation_y(20.0_f32.to_radians()));
        harness.demand(Some(still(turned)));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.pose_jump);
    }

    #[test]
    fn accumulated_small_moves_cross_the_jump_threshold() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        policy.jump_distance = 256.0;
        let mut harness = ViewHarness::new(policy);

        harness.demand(Some(still(Transform::IDENTITY)));
        harness.frame();

        // Each step is well inside the threshold, but they are measured against the pose of the
        // frame the view last rendered, so they add up.
        for step in 1..=5 {
            let walked = Transform::from_translation(Vec3::new(step as f32 * 64.0, 0.0, 0.0));
            harness.demand(Some(still(walked)));
            let frame = harness.frame();
            if step < 5 {
                assert!(!frame.active, "step {step} is still inside the threshold");
                assert!(!frame.last_forced.pose_jump);
            } else {
                assert!(
                    frame.active,
                    "the fifth step is past 256 units from the last render"
                );
                assert!(frame.last_forced.pose_jump);
            }
        }
    }

    #[test]
    fn a_turn_within_the_threshold_does_not_force_a_render() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);

        harness.demand(Some(still(Transform::IDENTITY)));
        harness.frame();
        let turned = Transform::from_rotation(Quat::from_rotation_y(5.0_f32.to_radians()));
        harness.demand(Some(still(turned)));
        let frame = harness.frame();
        assert!(
            !frame.active,
            "five degrees is inside the ten degree threshold"
        );
        assert!(!frame.last_forced.pose_jump);
    }

    #[test]
    fn an_admitted_frame_carries_the_demanded_pose() {
        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        let pose = Transform::from_xyz(1.0, -40.0, 2.0);
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        harness.frame();
        harness.demand(Some(request(Transform::from_xyz(0.0, 100.0, 0.0), pose)));
        // Every-frame admits anyway; the point is that the pose written is the demanded one.
        let frame = harness.frame();
        assert_eq!(frame.pose, pose);
    }

    #[test]
    fn an_invalid_policy_leaves_the_view_off() {
        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        assert!(harness.frame().active);

        let mut broken = ViewPolicy::baseline();
        broken.target_size = UVec2::new(0, 576);
        harness.set_policy(broken);
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        let frame = harness.frame();
        assert!(!frame.active);
        assert_eq!(
            harness
                .app
                .world()
                .get::<ViewPolicyState>(harness.camera)
                .unwrap()
                .invalid,
            Some(ViewPolicyError::EmptyTargetSize)
        );
    }

    #[test]
    fn the_observer_far_and_aspect_never_reach_the_view() {
        #[derive(Resource, Default)]
        struct Rewrites(Vec<bool>);

        fn record(projection: Query<Ref<Projection>, With<ViewPolicy>>, mut out: ResMut<Rewrites>) {
            out.0.push(projection.single().unwrap().is_changed());
        }

        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        harness
            .app
            .init_resource::<Rewrites>()
            .add_systems(PostUpdate, record.after(ViewPolicySystems::Apply));
        let own_far = match harness
            .app
            .world()
            .get::<Projection>(harness.camera)
            .unwrap()
        {
            Projection::Perspective(projection) => projection.far,
            _ => panic!("the harness camera is perspective"),
        };
        let observer_projection = Projection::Perspective(PerspectiveProjection {
            fov: 0.7,
            aspect_ratio: 2.5,
            far: 90_000.0,
            ..default()
        });
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);
        let demand = ViewRequest {
            projection: Some(observer_projection),
            ..still(observer)
        };
        for _ in 0..3 {
            harness.demand(Some(demand.clone()));
            harness.frame();
        }
        let Projection::Perspective(applied) = harness
            .app
            .world()
            .get::<Projection>(harness.camera)
            .unwrap()
            .clone()
        else {
            panic!("still perspective");
        };
        assert_eq!(applied.far, own_far, "the view keeps its own far");
        assert_eq!(applied.fov, 0.7, "the observer's field of view is applied");
        assert_ne!(
            applied.aspect_ratio, 2.5,
            "the aspect stays the target's own"
        );
        let rewrites = &harness.app.world().resource::<Rewrites>().0;
        assert!(
            !rewrites[1] && !rewrites[2],
            "an unchanged request does not rewrite the projection: {rewrites:?}"
        );
    }

    #[test]
    fn demand_held_through_an_invalid_period_forces_a_refresh_on_return() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);
        harness.demand(Some(still(observer)));
        harness.frame();

        let mut broken = policy;
        broken.target_size = UVec2::new(0, 576);
        harness.set_policy(broken);
        harness.demand(Some(still(observer)));
        assert!(!harness.frame().active);

        harness.set_policy(policy);
        harness.demand(Some(still(observer)));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.revalidated);
    }

    #[test]
    fn an_orthographic_request_is_written_once() {
        #[derive(Resource, Default)]
        struct Rewrites(Vec<bool>);

        fn record(projection: Query<Ref<Projection>, With<ViewPolicy>>, mut out: ResMut<Rewrites>) {
            out.0.push(projection.single().unwrap().is_changed());
        }

        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        harness
            .app
            .init_resource::<Rewrites>()
            .add_systems(PostUpdate, record.after(ViewPolicySystems::Apply));
        let demand = ViewRequest {
            projection: Some(Projection::Orthographic(OrthographicProjection {
                far: 77_000.0,
                ..OrthographicProjection::default_3d()
            })),
            ..still(Transform::from_xyz(0.0, 100.0, 0.0))
        };
        for _ in 0..3 {
            harness.demand(Some(demand.clone()));
            harness.frame();
        }
        let rewrites = &harness.app.world().resource::<Rewrites>().0;
        assert!(
            !rewrites[1] && !rewrites[2],
            "an unchanged non-perspective request does not rewrite: {rewrites:?}"
        );
    }

    #[test]
    fn demand_that_reappears_after_an_invalid_period_forces_a_refresh() {
        let mut policy = ViewPolicy::baseline();
        policy.update_rate = ViewUpdateRate::OnDemand;
        policy.max_staleness = Duration::MAX;
        let mut harness = ViewHarness::new(policy);
        let observer = Transform::from_xyz(0.0, 100.0, 0.0);
        harness.demand(Some(still(observer)));
        harness.frame();
        harness.demand(Some(still(observer)));
        assert!(!harness.frame().active, "nothing forces an on-demand view");

        let mut broken = policy;
        broken.target_size = UVec2::new(0, 576);
        harness.set_policy(broken);
        harness.demand(None);
        assert!(!harness.frame().active);

        harness.set_policy(policy);
        harness.demand(Some(still(observer)));
        let frame = harness.frame();
        assert!(frame.active);
        assert!(frame.last_forced.demand_reopened);
    }

    #[test]
    fn a_managed_view_without_a_target_warns_once() {
        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        harness
            .app
            .world_mut()
            .entity_mut(harness.camera)
            .remove::<SecondaryViewTarget>();
        harness.demand(Some(still(Transform::from_xyz(0.0, 100.0, 0.0))));
        harness.app.update();
        let state = harness
            .app
            .world()
            .get::<ViewPolicyState>(harness.camera)
            .unwrap();
        assert!(state.warned_missing_target);
    }

    #[test]
    fn a_policy_without_a_demand_still_counts_its_frames() {
        let mut harness = ViewHarness::new(ViewPolicy::baseline());
        for _ in 0..3 {
            assert!(!harness.frame().active);
        }
        let state = harness
            .app
            .world()
            .get::<ViewPolicyState>(harness.camera)
            .unwrap();
        assert_eq!(state.frames, 3);
        assert!(state.dirty.never_scheduled);
    }
}

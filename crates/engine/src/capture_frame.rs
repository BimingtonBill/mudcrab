//! Receipts tied to screenshot entities, rather than callback arrival frames.
//!
//! Bevy 0.19 extracts each new Screenshot once and marks it Capturing in that same
//! extraction. A ticket is sampled in that extraction and finalized after rendering;
//! ScreenshotCaptured for the identical entity certifies completion of its readback.
//! Automated runners serialize primary-window requests and keep the pose frozen.
use crate::{
    color_pipeline::{
        SceneGradeMode, SceneGradeOverrides, SceneGradeSource, SceneVisualPreset,
        SkyrimCinematicPost,
    },
    shots::shot_camera_translation,
    streaming::{ActiveSpace, RenderOrigin, StreamingWorld},
    swap_bridge::{SwapInCameraView, SwapWaiting, pending_drawn},
    tone_mapping::SceneToneMapSource,
    world::components::StreamingCamera,
};
use bevy::{
    camera::{CompositingSpace, Exposure},
    core_pipeline::tonemapping::Tonemapping,
    diagnostic::FrameCount,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        mesh::RenderMesh,
        render_asset::{ExtractedAssets, RenderAssets, prepare_assets},
        texture::GpuImage,
        view::ColorGrading,
        view::screenshot::Screenshot,
    },
};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Component, Default)]
pub(crate) struct CaptureTicket {
    receipt: Arc<Mutex<Option<CaptureFrame>>>,
    /// The run's failure counters when this shot started settling, frozen into the request so the receipt
    /// reports only the failures of this shot: an earlier shot's failures are history, and one
    /// that happens after the request still shows up in the receipt.
    baseline: crate::shots::Failures,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CaptureFrame {
    pub main_frame: u32,
    pub render_frame: u64,
    pub camera_creation: [f32; 3],
    pub rotation_runtime_xyzw: [f32; 4],
    pub vertical_fov_radians: Option<f32>,
    pub render_origin: [i32; 2],
    pub pending_specializations: Option<usize>,
    pub work: crate::shots::SettleCounts,
    pub terrain_coverage: Vec<serde_json::Value>,
    pub terrain_gauges: std::collections::BTreeMap<String, f64>,
    pub grading: Option<CaptureGrade>,
}

/// Immutable grading evidence sampled with the rendered camera, before readback.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CaptureGrade {
    pub color_grade: String,
    pub visual_preset: String,
    pub preset_saturation: f32,
    pub resolved_grade: serde_json::Value,
    pub source: serde_json::Value,
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn capture_grade(
    source: Option<&SceneGradeSource>,
    tone_source: Option<&SceneToneMapSource>,
    mode: Option<&SceneGradeMode>,
    preset: Option<&SceneVisualPreset>,
    grading: Option<&ColorGrading>,
    post: Option<&SkyrimCinematicPost>,
    tonemapping: Option<&Tonemapping>,
    exposure: Option<&Exposure>,
    compositing: Option<&CompositingSpace>,
) -> Option<CaptureGrade> {
    capture_grade_with_overrides(
        source,
        tone_source,
        mode,
        preset,
        grading,
        post,
        tonemapping,
        exposure,
        compositing,
        SceneGradeOverrides::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn capture_grade_with_overrides(
    source: Option<&SceneGradeSource>,
    tone_source: Option<&SceneToneMapSource>,
    mode: Option<&SceneGradeMode>,
    preset: Option<&SceneVisualPreset>,
    grading: Option<&ColorGrading>,
    post: Option<&SkyrimCinematicPost>,
    tonemapping: Option<&Tonemapping>,
    exposure: Option<&Exposure>,
    compositing: Option<&CompositingSpace>,
    overrides: SceneGradeOverrides,
) -> Option<CaptureGrade> {
    let (source, mode, preset, grading, post) = (source?, *mode?, *preset?, grading?, post?);
    let fallback = SceneToneMapSource::default();
    let tone = tone_source.unwrap_or(&fallback);
    let resolved = tone
        .resolve_with_overrides(source, mode, preset, overrides)
        .ok()?;
    let (tonemapping, exposure, compositing) = (tonemapping?, exposure?, compositing?);
    // A source describes this frame only after its settings reached the camera.
    if grading.global.post_saturation != resolved.bevy_saturation
        || *post != resolved.post
        || *tonemapping != resolved.tonemapping
        || *compositing != CompositingSpace::Linear
    {
        return None;
    }
    let grade = resolved.grade;
    let mut evidence = tone.evidence_with_overrides(source, mode, preset, overrides);
    evidence["exposure_ev100"] = exposure.ev100.into();
    evidence["actual_camera"] = serde_json::json!({
        "tonemapping": format!("{tonemapping:?}"),
        "exposure_ev100": exposure.ev100,
        "compositing_space": format!("{compositing:?}"),
    });
    Some(CaptureGrade {
        color_grade: mode.name().into(),
        visual_preset: preset.name().into(),
        preset_saturation: preset.grade().saturation,
        resolved_grade: serde_json::json!({
            "saturation": grade.saturation,
            "brightness": grade.brightness,
            "contrast": grade.contrast,
            "tint_amount": grade.tint_amount,
            "tint_rgb": grade.tint_rgb,
            "bevy_saturation": grading.global.post_saturation,
            "post_saturation": post.saturation,
            "post_linear_domain": post.linear_domain,
            "post_enabled": post.enabled,
            "pivot": post.pivot,
            "tone_operator": post.tone_operator,
            "tone_coefficient": post.tone_coefficient,
            "sampled_luminance": post.sampled_luminance,
            "target_luminance": post.target_luminance,
        }),
        source: evidence,
    })
}

impl CaptureTicket {
    /// A ticket carrying the failure baseline of the shot that requested it.
    pub(crate) fn new(baseline: crate::shots::Failures) -> Self {
        Self {
            receipt: Arc::new(Mutex::new(None)),
            baseline,
        }
    }

    #[cfg(test)]
    pub(crate) fn baseline(&self) -> crate::shots::Failures {
        self.baseline
    }

    pub(crate) fn receipt(&self) -> Option<CaptureFrame> {
        self.receipt.lock().unwrap().clone()
    }

    #[cfg(test)]
    pub(crate) fn set_receipt(&self, frame: CaptureFrame) {
        *self.receipt.lock().unwrap() = Some(frame);
    }
}

pub(crate) struct CaptureFramePlugin;
impl Plugin for CaptureFramePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RetryReadiness>()
            .init_resource::<TransferReadiness>();
        let bridge = app.world().resource::<RetryReadiness>().clone();
        let transfers = app.world().resource::<TransferReadiness>().clone();
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(bridge)
                .insert_resource(transfers)
                .init_resource::<PendingRenderTransfers>()
                .init_resource::<ExtractedCaptures>()
                .add_systems(ExtractSchedule, extract_capture_frames)
                .add_systems(
                    Render,
                    (
                        observe_render_transfers
                            .before(prepare_assets::<RenderMesh>)
                            .before(prepare_assets::<GpuImage>),
                        publish_render_transfers
                            .after(RenderSystems::Queue)
                            .before(RenderSystems::Cleanup),
                        finalize_capture_frames.in_set(RenderSystems::Cleanup),
                    ),
                );
        }
    }
}

/// Updated after specialization/queue observation, and explicitly unavailable until observed.
#[derive(Resource, Clone, Default)]
pub(crate) struct RetryReadiness(pub Arc<Mutex<Option<usize>>>);

#[derive(Resource, Default)]
struct ExtractedCaptures {
    render_frame: u64,
    captures: Vec<(CaptureTicket, CaptureFrame)>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn extract_capture_frames(
    mut extracted: ResMut<ExtractedCaptures>,
    frame_inputs: Extract<(
        Res<FrameCount>,
        Res<RenderOrigin>,
        Query<
            (
                &Transform,
                &Projection,
                Option<&ColorGrading>,
                Option<&SkyrimCinematicPost>,
                Option<&Tonemapping>,
                Option<&Exposure>,
                Option<&CompositingSpace>,
            ),
            With<StreamingCamera>,
        >,
        Query<&CaptureTicket, With<Screenshot>>,
        Res<crate::streaming::StreamingMetrics>,
        Res<crate::render::RendererMetrics>,
        Res<crate::profiling::ProfilingState>,
        Res<crate::terrain_upload::TerrainMeshUploadReadiness>,
    )>,
    scene_inputs: Extract<(
        Query<Entity, With<bevy::window::PrimaryWindow>>,
        Query<(&crate::streaming::TerrainCoverage, &InheritedVisibility)>,
        Query<&crate::streaming::lod::terrain_batching::LodTerrainBatches>,
        Option<Res<SceneGradeSource>>,
        Option<Res<SceneGradeMode>>,
        Option<Res<SceneVisualPreset>>,
        Option<Res<SceneToneMapSource>>,
        Option<Res<SceneGradeOverrides>>,
    )>,
    view_inputs: Extract<(
        Res<crate::config::EngineConfig>,
        Option<Res<ActiveSpace>>,
        Option<Res<StreamingWorld>>,
        Query<Has<SwapInCameraView>, With<SwapWaiting>>,
    )>,
) {
    let (frame, origin, camera, tickets, streaming, renderer, profiler, uploads) = &*frame_inputs;
    let (config, active, world, waiting) = &*view_inputs;
    let (
        windows,
        surfaces,
        batches,
        grade_source,
        grade_mode,
        visual_preset,
        tone_source,
        grade_overrides,
    ) = &*scene_inputs;
    extracted.render_frame += 1;
    extracted.captures.clear();
    if windows.single().is_err() || tickets.iter().all(|ticket| ticket.receipt().is_some()) {
        return;
    }
    let Ok((transform, projection, grading, post, tonemapping, exposure, compositing)) =
        camera.single()
    else {
        return;
    };
    // Match app.rs's explicit modes without StreamingPlugin. A streaming fixture takes
    // precedence there, including when benchmark_only is also set. Missing resources never
    // select Capture scope for a run that should own a streamed world.
    let renderer_only = !config.streaming_fixture
        && (config.benchmark_only
            || config.material_fixture
            || config.terrain_water_fixture
            || config.transform_bounds_fixture
            || config.renderer_fixture
            || config.physics_fixture);
    let view = if renderer_only {
        None
    } else {
        let (Some(active), Some(world)) = (active.as_deref(), world.as_deref()) else {
            error!(
                active_cell_available = active.is_some(),
                streaming_world_available = world.is_some(),
                "cannot extract a streamed-world capture without current cell and world evidence"
            );
            return;
        };
        Some((active, world))
    };
    // Interior transforms are already absolute; switch_space retains the old exterior origin.
    // Outdoors and in renderer-only fixtures, preserve the pose driver's origin conversion.
    let absolute = if view.is_some_and(|(active, _)| active.interior.is_some()) {
        transform.translation
    } else {
        transform.translation - shot_camera_translation([0.0; 3], origin.0)
    };
    let pose = shared::coordinates::runtime_to_creation_vector(absolute.to_array());
    let view_counts = view.map(|(active, world)| {
        let key = if let Some(cell_id) = active.interior {
            crate::world::database::CellKey::Interior(cell_id)
        } else {
            let (grid_x, grid_y) = crate::config::grid_of(pose[0], pose[1]);
            crate::world::database::CellKey::Exterior {
                worldspace_id: active.worldspace_id.unwrap_or(config.worldspace_id),
                grid_x,
                grid_y,
            }
        };
        crate::shots::settle_counts(
            Some(key),
            0,
            pending_drawn(waiting),
            Some(world),
            Some(streaming),
        )
    });
    for ticket in tickets {
        if ticket.receipt().is_some() {
            continue;
        }
        let mut work = captured_work(
            ticket,
            streaming,
            renderer,
            profiler,
            uploads,
            renderer.final_path_active(),
        );
        if let Some(view_counts) = view_counts {
            work = work.with_view(view_counts);
        }
        let record = CaptureFrame {
            main_frame: frame.0,
            render_frame: extracted.render_frame,
            camera_creation: pose,
            rotation_runtime_xyzw: transform.rotation.to_array(),
            vertical_fov_radians: match projection {
                Projection::Perspective(p) => Some(p.fov),
                _ => None,
            },
            render_origin: origin.0.to_array(),
            pending_specializations: None,
            work,
            grading: capture_grade_with_overrides(
                grade_source.as_deref(),
                tone_source.as_deref(),
                grade_mode.as_deref(),
                visual_preset.as_deref(),
                grading,
                post,
                tonemapping,
                exposure,
                compositing,
                grade_overrides.as_deref().copied().unwrap_or_default(),
            ),
            terrain_gauges: profiler.terrain_gauges(),
            terrain_coverage: surfaces
                .iter()
                .filter(|(_, visible)| visible.get())
                .map(|(coverage, _)| serde_json::json!({"source": coverage.receipt()}))
                .chain(batches.iter().flat_map(|batch| batch.coverage_receipt()))
                .collect(),
        };
        extracted.captures.push((ticket.clone(), record));
    }
}

/// The work one captured receipt carries, read against the baseline frozen when its capture was
/// requested: a failure from an earlier shot is history and cannot reject this receipt, while a
/// failure after the request - including one between the request and this extraction - still can.
/// Everything pending is read as it stands, so work that is unavailable rather than absent leaves
/// the receipt without a claim of readiness.
fn captured_work(
    ticket: &CaptureTicket,
    streaming: &crate::streaming::StreamingMetrics,
    renderer: &crate::render::RendererMetrics,
    profiler: &crate::profiling::ProfilingState,
    uploads: &crate::terrain_upload::TerrainMeshUploadReadiness,
    renderer_ready: bool,
) -> crate::shots::SettleCounts {
    let mut work =
        crate::shots::SettleCounts::read(streaming, renderer, ticket.baseline, renderer_ready, 0);
    work.outstanding_terrain_uploads = uploads.outstanding();
    work.pending_batch_cpu = profiler
        .gauge("lod/pending_terrain_batch_chunks")
        .unwrap_or(0.0) as usize;
    work.pending_batch_initial = profiler
        .gauge("lod/pending_initial_terrain_upload_chunks")
        .unwrap_or(0.0) as usize;
    work.pending_batch_selection = profiler
        .gauge("lod/pending_terrain_selection_uploads")
        .unwrap_or(0.0) as usize;
    work
}

fn finalize_capture_frames(
    mut extracted: ResMut<ExtractedCaptures>,
    retries: Res<RetryReadiness>,
    transfers: Res<TransferReadiness>,
) {
    let pending = *retries.0.lock().unwrap();
    for (ticket, mut record) in extracted.captures.drain(..) {
        record.pending_specializations = pending;
        record.work.pending_specializations = pending;
        record.work.pending_render_transfers = *transfers.0.lock().unwrap();
        let mut receipt = ticket.receipt.lock().unwrap();
        // Never overwrite the render frame when asynchronous readback takes more frames.
        if receipt.is_none() {
            *receipt = Some(record);
        }
    }
}

pub(crate) fn validate_receipt(
    ticket: &CaptureTicket,
    requested: [f32; 3],
) -> Result<CaptureFrame, String> {
    let record = ticket.receipt().ok_or("no captured render-frame receipt")?;
    if record.camera_creation != requested {
        return Err("captured pose differs from requested pose".into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        shots::{Failures, SettleScope},
        streaming::{ActiveSpace, StreamingMetrics, StreamingWorld},
        swap_bridge::{SwapInCameraView, SwapWaiting},
        world::database::CellKey,
    };
    use bevy::{ecs::system::RunSystemOnce, render::MainWorld, window::PrimaryWindow};

    /// Run the production extraction against an actual MainWorld, without a GPU or readback.
    fn extraction_world(
        active: ActiveSpace,
        origin: IVec2,
        translation: Vec3,
        baseline: Failures,
    ) -> (World, CaptureTicket) {
        let ticket = CaptureTicket::new(baseline);
        let mut main = MainWorld::default();
        main.insert_resource(crate::config::EngineConfig::default());
        main.insert_resource(FrameCount(7));
        main.insert_resource(RenderOrigin(origin));
        main.insert_resource(active);
        main.insert_resource(StreamingWorld::default());
        main.insert_resource(StreamingMetrics::default());
        main.insert_resource(crate::render::RendererMetrics {
            gpu_preprocessing_active: true,
            gpu_culling_active: true,
            indirect_drawing_active: true,
            occlusion_culling_views: 1,
            hzb_views: 1,
            indirect_phase_buffers: 1,
            indirect_batch_sets: 1,
            proof_frames: 1,
            ..default()
        });
        main.insert_resource(crate::profiling::ProfilingState::default());
        main.insert_resource(crate::terrain_upload::TerrainMeshUploadReadiness::default());
        main.spawn(PrimaryWindow);
        main.spawn((
            StreamingCamera,
            Transform::from_translation(translation),
            Projection::Perspective(PerspectiveProjection::default()),
        ));
        main.spawn((Screenshot::primary_window(), ticket.clone()));

        let mut render = World::new();
        render.insert_resource(main);
        render.init_resource::<ExtractedCaptures>();
        render.insert_resource(RetryReadiness(Arc::new(Mutex::new(Some(0)))));
        render.insert_resource(TransferReadiness(Arc::new(Mutex::new(Some(0)))));
        let mut schedule = Schedule::new(ExtractSchedule);
        schedule.add_systems(extract_capture_frames);
        render.add_schedule(schedule);
        (render, ticket)
    }

    fn extracted_receipt(render: &mut World, ticket: &CaptureTicket) -> CaptureFrame {
        render.run_schedule(ExtractSchedule);
        render.run_system_once(finalize_capture_frames).unwrap();
        ticket
            .receipt()
            .expect("the extraction finalized a receipt")
    }

    #[test]
    fn renderer_only_fixtures_extract_capture_receipts_without_streaming_resources() {
        for fixture in [
            "material",
            "terrain_water",
            "transform_bounds",
            "renderer",
            "physics",
            "benchmark",
        ] {
            let (mut render, ticket) = extraction_world(
                ActiveSpace {
                    worldspace_id: Some(17),
                    interior: None,
                },
                IVec2::new(4, -10),
                Vec3::ZERO,
                Failures::default(),
            );
            {
                let mut main = render.resource_mut::<MainWorld>();
                main.insert_resource(crate::config::EngineConfig {
                    stdin_console: true,
                    material_fixture: fixture == "material",
                    terrain_water_fixture: fixture == "terrain_water",
                    transform_bounds_fixture: fixture == "transform_bounds",
                    renderer_fixture: fixture == "renderer",
                    physics_fixture: fixture == "physics",
                    benchmark_only: fixture == "benchmark",
                    ..default()
                });
                main.remove_resource::<ActiveSpace>();
                main.remove_resource::<StreamingWorld>();
            }

            let frame = extracted_receipt(&mut render, &ticket);
            assert_eq!(frame.work.scope, SettleScope::Capture, "{fixture}");
            assert!(
                frame.work.is_quiet(),
                "{fixture}: {}",
                frame.work.describe()
            );
            assert_eq!((frame.main_frame, frame.render_frame), (7, 1), "{fixture}");
            assert_eq!(frame.camera_creation, [16384.0, -40960.0, 0.0], "{fixture}");
            assert_eq!(frame.render_origin, [4, -10], "{fixture}");
        }
    }

    #[test]
    fn missing_streamed_world_state_never_downgrades_to_renderer_only_capture() {
        for mode in ["world", "streaming_benchmark", "streaming_fixture"] {
            for missing in ["cell", "world", "both"] {
                let (mut render, ticket) = extraction_world(
                    ActiveSpace {
                        worldspace_id: Some(17),
                        interior: Some(83),
                    },
                    IVec2::new(4, -10),
                    Vec3::ZERO,
                    Failures::default(),
                );
                {
                    let mut main = render.resource_mut::<MainWorld>();
                    main.insert_resource(crate::config::EngineConfig {
                        stdin_console: true,
                        benchmark_frames: (mode == "streaming_benchmark").then_some(120),
                        // app.rs gives streaming_fixture priority over benchmark_only.
                        streaming_fixture: mode == "streaming_fixture",
                        benchmark_only: mode == "streaming_fixture",
                        ..default()
                    });
                    if missing != "world" {
                        main.remove_resource::<ActiveSpace>();
                    }
                    if missing != "cell" {
                        main.remove_resource::<StreamingWorld>();
                    }
                }

                render.run_schedule(ExtractSchedule);
                render.run_system_once(finalize_capture_frames).unwrap();
                assert_eq!(
                    render.resource::<ExtractedCaptures>().render_frame,
                    1,
                    "{mode}/{missing}: extraction must handle absent state explicitly"
                );
                assert!(
                    ticket.receipt().is_none(),
                    "{mode}/{missing}: no receipt without world evidence"
                );
                assert!(render.resource::<ExtractedCaptures>().captures.is_empty());
            }
        }
    }

    #[test]
    fn interior_extraction_ignores_the_previous_exterior_origin() {
        let pose = [21750.0, -44530.0, 140.0];
        let (mut render, ticket) = extraction_world(
            ActiveSpace {
                worldspace_id: Some(17),
                interior: Some(83),
            },
            IVec2::new(4, -10),
            Vec3::from_array(shared::coordinates::creation_to_runtime_vector(pose)),
            Failures::default(),
        );
        render
            .resource_mut::<MainWorld>()
            .resource_mut::<StreamingWorld>()
            .set_resident_for_test(CellKey::Interior(83), Entity::PLACEHOLDER);

        let frame = extracted_receipt(&mut render, &ticket);
        assert_eq!(frame.camera_creation, pose);
        assert_eq!(frame.render_origin, [4, -10], "record the actual resource");
        assert_eq!(frame.work.scope, SettleScope::ViewAndCapture);
        assert!(frame.work.space_resident, "{}", frame.work.describe());
        assert!(frame.work.is_quiet(), "{}", frame.work.describe());
    }

    #[test]
    fn exterior_extraction_uses_the_captured_pose_and_active_worldspace() {
        let pose = [21750.0, -44530.0, 140.0];
        let origin = IVec2::new(4, -10);
        let (mut render, ticket) = extraction_world(
            ActiveSpace {
                worldspace_id: Some(17),
                interior: None,
            },
            origin,
            shot_camera_translation(pose, origin),
            Failures::default(),
        );
        render
            .resource_mut::<MainWorld>()
            .resource_mut::<StreamingWorld>()
            .set_resident_for_test(
                CellKey::Exterior {
                    worldspace_id: 17,
                    grid_x: 5,
                    grid_y: -11,
                },
                Entity::PLACEHOLDER,
            );

        let frame = extracted_receipt(&mut render, &ticket);
        assert_eq!(frame.camera_creation, pose);
        assert_eq!((frame.main_frame, frame.render_frame), (7, 1));
        assert!(frame.work.space_resident, "{}", frame.work.describe());
        assert!(frame.work.is_quiet(), "{}", frame.work.describe());
    }

    #[test]
    fn extraction_reads_the_new_active_cells_failure_after_the_request() {
        let (mut render, ticket) = extraction_world(
            ActiveSpace {
                worldspace_id: Some(17),
                interior: None,
            },
            IVec2::ZERO,
            Vec3::ZERO,
            Failures::default(),
        );
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.resource_mut::<StreamingWorld>().set_resident_for_test(
                CellKey::Exterior {
                    worldspace_id: 17,
                    grid_x: 0,
                    grid_y: 0,
                },
                Entity::PLACEHOLDER,
            );
            // A space switch after the request invalidates the old resident-cell observation.
            main.resource_mut::<ActiveSpace>().interior = Some(83);
            main.resource_mut::<StreamingWorld>()
                .set_failed_for_test(CellKey::Interior(83));
            main.resource_mut::<FrameCount>().0 = 9;
        }

        let frame = extracted_receipt(&mut render, &ticket);
        assert_eq!(frame.main_frame, 9);
        assert_eq!(frame.work.scope, SettleScope::ViewAndCapture);
        assert!(!frame.work.space_resident);
        assert!(frame.work.space_failed, "{}", frame.work.describe());
    }

    #[test]
    fn extraction_reads_camera_visible_swap_waiters_at_capture_time() {
        let (mut render, ticket) = extraction_world(
            ActiveSpace {
                worldspace_id: Some(17),
                interior: Some(83),
            },
            IVec2::ZERO,
            Vec3::ZERO,
            Failures::default(),
        );
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.resource_mut::<StreamingWorld>()
                .set_resident_for_test(CellKey::Interior(83), Entity::PLACEHOLDER);
            // Use the bridge's own markers; off-camera and non-waiting work is excluded.
            main.spawn((SwapWaiting, SwapInCameraView));
            main.spawn((SwapWaiting, SwapInCameraView));
            main.spawn(SwapWaiting);
            main.spawn(SwapInCameraView);
        }

        let frame = extracted_receipt(&mut render, &ticket);
        assert_eq!(frame.work.pending_drawn, 2);
        assert!(!frame.work.is_quiet(), "{}", frame.work.describe());
    }

    #[test]
    fn extraction_combines_view_evidence_with_the_frozen_failure_baseline() {
        let earlier = StreamingMetrics {
            failed_cells: 1,
            asset_load_failures: 2,
            failed_lod_chunks: 3,
            ..default()
        };
        let baseline = Failures::read(&earlier, &crate::render::RendererMetrics::default());
        let (mut render, ticket) = extraction_world(
            ActiveSpace {
                worldspace_id: Some(17),
                interior: Some(83),
            },
            IVec2::ZERO,
            Vec3::ZERO,
            baseline,
        );
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.resource_mut::<StreamingWorld>()
                .set_resident_for_test(CellKey::Interior(83), Entity::PLACEHOLDER);
            main.insert_resource(earlier);
            main.resource_mut::<StreamingMetrics>().asset_load_failures += 1;
        }

        let frame = extracted_receipt(&mut render, &ticket);
        assert_eq!(ticket.baseline(), baseline);
        assert!(frame.work.space_resident);
        assert_eq!(
            (frame.work.failed_cells, frame.work.failed_lod_work),
            (0, 0)
        );
        assert_eq!(frame.work.asset_load_failures, 1);
        assert!(!frame.work.is_quiet(), "{}", frame.work.describe());
    }

    #[test]
    fn incomplete_capture_has_no_receipt() {
        assert!(validate_receipt(&CaptureTicket::default(), [0.0; 3]).is_err());
    }

    #[test]
    fn tuned_camera_and_capture_evidence_share_effective_resolution() {
        let source = SceneGradeSource::default();
        let tone = SceneToneMapSource::default();
        let mode = SceneGradeMode::SnapshotSrgb;
        let preset = SceneVisualPreset::CoolGray;
        let overrides = SceneGradeOverrides {
            brightness: Some(1.74),
            tint_green: Some(0.67),
            pivot: Some(0.29),
            ..default()
        };
        let resolved = tone
            .resolve_with_overrides(&source, mode, preset, overrides)
            .unwrap();
        let mut grading = ColorGrading::default();
        grading.global.post_saturation = resolved.bevy_saturation;
        let exposure = Exposure {
            ev100: crate::color_pipeline::DEFAULT_SCENE_EV100,
        };
        let capture = capture_grade_with_overrides(
            Some(&source),
            Some(&tone),
            Some(&mode),
            Some(&preset),
            Some(&grading),
            Some(&resolved.post),
            Some(&resolved.tonemapping),
            Some(&exposure),
            Some(&CompositingSpace::Linear),
            overrides,
        )
        .expect("the same resolved grade labels the camera frame");
        assert_eq!(
            capture.resolved_grade["brightness"],
            serde_json::json!(1.74_f32)
        );
        assert_eq!(
            capture.resolved_grade["tint_rgb"][1],
            serde_json::json!(0.67_f32)
        );
        assert_eq!(capture.resolved_grade["pivot"], serde_json::json!(0.29_f32));
        assert_eq!(capture.source["pivot"], serde_json::json!(0.29_f32));
        assert_eq!(capture.source["default_pivot"], serde_json::json!(0.18_f32));
        assert_eq!(
            capture.source["grade_overrides"]["brightness"],
            serde_json::json!(1.74_f32)
        );
        assert_eq!(
            capture.source["effective_grade"]["brightness"],
            serde_json::json!(1.74_f32)
        );
        assert_eq!(
            capture.source["effective_stages"]["pivot"],
            serde_json::json!(0.29_f32)
        );
    }

    #[test]
    fn render_receipt_is_immutable_across_callback_delay() {
        let ticket = CaptureTicket::default();
        let source = SceneGradeSource::default();
        let mode = SceneGradeMode::SnapshotSrgb;
        let preset = SceneVisualPreset::CoolGray;
        let resolved = source.resolve(mode, preset).unwrap();
        let mut grading = ColorGrading::default();
        grading.global.post_saturation = resolved.bevy_saturation;
        let record = CaptureFrame {
            main_frame: 3,
            render_frame: 2,
            camera_creation: [1.0; 3],
            rotation_runtime_xyzw: [0.0; 4],
            vertical_fov_radians: None,
            render_origin: [0; 2],
            pending_specializations: Some(0),
            work: default(),
            terrain_gauges: default(),
            terrain_coverage: default(),
            grading: capture_grade(
                Some(&source),
                None,
                Some(&mode),
                Some(&preset),
                Some(&grading),
                Some(&resolved.post),
                Some(&Tonemapping::TonyMcMapface),
                Some(&Exposure { ev100: 9.7 }),
                Some(&CompositingSpace::Linear),
            ),
        };
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.insert_resource(ExtractedCaptures {
            render_frame: 2,
            captures: vec![(ticket.clone(), record.clone())],
        });
        world.insert_resource(RetryReadiness(Arc::new(Mutex::new(Some(0)))));
        world.insert_resource(TransferReadiness(Arc::new(Mutex::new(Some(0)))));
        world.run_system_once(finalize_capture_frames).unwrap();
        let mut later = record;
        later.main_frame = 99;
        later.render_frame = 98;
        later.grading = Some(CaptureGrade {
            color_grade: "record-srgb".into(),
            visual_preset: "vivid".into(),
            preset_saturation: 1.5,
            resolved_grade: serde_json::json!({"brightness": 1.135}),
            source: serde_json::json!({"source_kind": "authored-image-space-record"}),
        });
        world
            .resource_mut::<ExtractedCaptures>()
            .captures
            .push((ticket.clone(), later));
        world.run_system_once(finalize_capture_frames).unwrap();
        let captured = validate_receipt(&ticket, [1.0; 3]).unwrap();
        assert_eq!(captured.render_frame, 2);
        let captured_grade = captured.grading.unwrap();
        assert_eq!(captured_grade.color_grade, "snapshot-srgb");
        assert_eq!(captured_grade.visual_preset, "cool-gray");
        assert_eq!(
            captured_grade.resolved_grade["brightness"].as_f64(),
            Some(f64::from(1.38_f32))
        );
        assert_eq!(
            captured_grade.source["source_kind"],
            "approved-artistic-override"
        );
        assert!(validate_receipt(&ticket, [0.0; 3]).is_err());
    }

    /// A receipt is checked against the failures of the shot it belongs to: the run's history is
    /// not this capture's rejection, and a failure after the request still is.
    #[test]
    fn a_receipt_reports_the_failures_of_its_own_shot_only() {
        let earlier_failures = crate::streaming::StreamingMetrics {
            failed_cells: 1,
            asset_load_failures: 2,
            material_validation_failures: 1,
            failed_lod_queries: 3,
            failed_lod_chunks: 4,
            ..crate::streaming::StreamingMetrics::default()
        };
        let renderer = crate::render::RendererMetrics::default();
        let baseline = crate::shots::Failures::read(&earlier_failures, &renderer);
        let profiler = crate::profiling::ProfilingState::default();
        let uploads = crate::terrain_upload::TerrainMeshUploadReadiness::default();
        let observed = |work: &mut crate::shots::SettleCounts| {
            work.pending_specializations = Some(0);
            work.pending_render_transfers = Some(0);
        };

        // This shot added nothing, so its receipt is quiet despite the run's failures behind it.
        let ticket = CaptureTicket::new(baseline);
        let mut quiet = captured_work(
            &ticket,
            &earlier_failures,
            &renderer,
            &profiler,
            &uploads,
            true,
        );
        observed(&mut quiet);
        assert!(
            quiet.is_quiet(),
            "earlier failures poison this receipt: {}",
            quiet.describe()
        );

        // A failure between the request and this extraction is this shot's failure.
        let mut during = earlier_failures.clone();
        during.failed_lod_chunks += 1;
        during.failed_cells += 1;
        let mut fresh = captured_work(&ticket, &during, &renderer, &profiler, &uploads, true);
        observed(&mut fresh);
        assert_eq!(
            (fresh.failed_cells, fresh.failed_lod_work),
            (1, 1),
            "a failure after the request is counted"
        );
        assert!(!fresh.is_quiet(), "{}", fresh.describe());

        // Render-side work that has not been observed yet stays unavailable, not quiet.
        let unavailable = captured_work(
            &ticket,
            &earlier_failures,
            &renderer,
            &profiler,
            &uploads,
            true,
        );
        assert!(!unavailable.is_quiet(), "unobserved work is not quiet");

        // The renderer's final path has to be running for the frame to be a settled one.
        let mut not_ready = captured_work(
            &ticket,
            &earlier_failures,
            &renderer,
            &profiler,
            &uploads,
            false,
        );
        observed(&mut not_ready);
        assert!(!not_ready.is_quiet(), "an unready renderer is not settled");

        // Pending LOD work holds the receipt back whatever the failure baseline says.
        let mut pending_lod = earlier_failures.clone();
        pending_lod.pending_lod_queries = 1;
        let mut pending =
            captured_work(&ticket, &pending_lod, &renderer, &profiler, &uploads, true);
        observed(&mut pending);
        assert!(!pending.is_quiet(), "{}", pending.describe());
    }

    #[test]
    fn source_must_match_extracted_camera_before_it_can_label_a_capture() {
        let source = SceneGradeSource::default();
        let mode = SceneGradeMode::SnapshotSrgb;
        let preset = SceneVisualPreset::CoolGray;
        let old_camera = crate::color_pipeline::SceneColorPipeline::default();
        assert!(
            capture_grade(
                Some(&source),
                None,
                Some(&mode),
                Some(&preset),
                Some(&old_camera.grading),
                Some(&old_camera.cinematic_post),
                Some(&old_camera.tonemapping),
                Some(&old_camera.exposure),
                Some(&old_camera.compositing_space),
            )
            .is_none()
        );
    }

    #[test]
    fn tone_metadata_cannot_label_a_camera_with_a_different_mapper() {
        let source = SceneGradeSource::default();
        let mode = SceneGradeMode::SnapshotSrgb;
        let preset = SceneVisualPreset::CoolGray;
        let resolved = source.resolve(mode, preset).unwrap();
        let mut grading = ColorGrading::default();
        grading.global.post_saturation = resolved.bevy_saturation;
        assert!(
            capture_grade(
                Some(&source),
                None,
                Some(&mode),
                Some(&preset),
                Some(&grading),
                Some(&resolved.post),
                Some(&Tonemapping::None),
                Some(&Exposure { ev100: 9.7 }),
                Some(&CompositingSpace::Linear),
            )
            .is_none()
        );
    }
}

// Track asset generations before Bevy drains its extracted lists. Updated generations remove
// their old descriptor before preparation, so a retained descriptor cannot acknowledge old bytes.
#[derive(Resource, Clone, Default)]
pub(crate) struct TransferReadiness(pub Arc<Mutex<Option<usize>>>);

#[derive(Resource, Default)]
struct PendingRenderTransfers {
    meshes: std::collections::HashSet<bevy::asset::AssetId<Mesh>>,
    images: std::collections::HashSet<bevy::asset::AssetId<Image>>,
}

fn observe_render_transfers(
    mut pending: ResMut<PendingRenderTransfers>,
    meshes: Res<ExtractedAssets<RenderMesh>>,
    images: Res<ExtractedAssets<GpuImage>>,
) {
    for id in &meshes.removed {
        pending.meshes.remove(id);
    }
    for id in &images.removed {
        pending.images.remove(id);
    }
    pending
        .meshes
        .extend(meshes.extracted.iter().map(|(id, _)| *id));
    pending
        .images
        .extend(images.extracted.iter().map(|(id, _)| *id));
}

fn publish_render_transfers(
    mut pending: ResMut<PendingRenderTransfers>,
    meshes: Res<RenderAssets<RenderMesh>>,
    images: Res<RenderAssets<GpuImage>>,
    bridge: Res<TransferReadiness>,
) {
    pending.meshes.retain(|id| meshes.get(*id).is_none());
    pending.images.retain(|id| images.get(*id).is_none());
    *bridge.0.lock().unwrap() = Some(pending.meshes.len() + pending.images.len());
}

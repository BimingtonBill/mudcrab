use super::*;
use crate::render_phase::{
    admit_draw_function, admit_render_command_result, DrawError, RenderCommandResult,
};

#[test]
fn disabled_config_reads_only_the_directory() {
    let config = Config::from_env(|name| {
        assert_eq!(name, "MUDCRAB_RASTER_PARITY_DIR");
        None
    })
    .unwrap();
    assert!(config.directory.is_none());
    assert!(config.request.is_none());
    assert_eq!(config.min_frame, 0);
}

#[test]
fn enabled_config_rejects_malformed_frame_without_panicking() {
    for value in ["", "invalid", "-1", "4294967296"] {
        let result = Config::from_env(|name| match name {
            "MUDCRAB_RASTER_PARITY_DIR" => Some("audit".into()),
            "MUDCRAB_RASTER_PARITY_MIN_FRAME" => Some(value.into()),
            _ => panic!("Invalid config must fail before reading the request"),
        });
        assert!(result.unwrap_err().contains("MIN_FRAME"));
    }
}

#[test]
fn enabled_config_preserves_directory_request_and_frame_boundaries() {
    for (value, expected) in [(None, 0), (Some("0"), 0), (Some("4294967295"), u32::MAX)] {
        let config = Config::from_env(|name| match name {
            "MUDCRAB_RASTER_PARITY_DIR" => Some("audit".into()),
            "MUDCRAB_RASTER_PARITY_MIN_FRAME" => value.map(Into::into),
            "MUDCRAB_GPU_PARITY_REQUEST" => Some("request".into()),
            _ => panic!("Unexpected setting: {name}"),
        })
        .unwrap();
        assert_eq!(config.directory, Some(PathBuf::from("audit")));
        assert_eq!(config.request, Some(PathBuf::from("request")));
        assert_eq!(config.min_frame, expected);
    }
}

#[test]
fn empty_directory_fails_before_reading_other_settings() {
    let result = Config::from_env(|name| {
        assert_eq!(name, "MUDCRAB_RASTER_PARITY_DIR");
        Some("".into())
    });
    assert!(result.unwrap_err().contains("DIR"));
}

#[cfg(windows)]
#[test]
fn enabled_config_rejects_non_unicode_frame_without_panicking() {
    use std::os::windows::ffi::OsStringExt;

    let result = Config::from_env(|name| match name {
        "MUDCRAB_RASTER_PARITY_DIR" => Some("audit".into()),
        "MUDCRAB_RASTER_PARITY_MIN_FRAME" => Some(std::ffi::OsString::from_wide(&[0xD800])),
        _ => panic!("Invalid config must fail before reading the request"),
    });
    assert!(result.unwrap_err().contains("MIN_FRAME"));
}

#[test]
fn native_shadow_layout_admits_reads_and_refuses_render_storage_writes() {
    use crate::render_resource::{BindGroupLayoutDescriptor, BindGroupLayoutEntry};
    assert!(!read_only_render_layout(&[]));
    for stage in [ShaderStages::VERTEX, ShaderStages::FRAGMENT] {
        for read_only in [false, true] {
            let layout = BindGroupLayoutDescriptor::new(
                "shadow audit",
                &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: stage,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            );
            assert_eq!(read_only_render_layout(&[layout]), read_only);
        }
        for access in [
            StorageTextureAccess::WriteOnly,
            StorageTextureAccess::ReadWrite,
            StorageTextureAccess::Atomic,
            StorageTextureAccess::ReadOnly,
        ] {
            let layout = BindGroupLayoutDescriptor::new(
                "shadow audit",
                &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: stage,
                    ty: BindingType::StorageTexture {
                        access,
                        format: TextureFormat::R32Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                }],
            );
            assert_eq!(
                read_only_render_layout(&[layout]),
                access == StorageTextureAccess::ReadOnly
            );
        }
    }
}

#[test]
fn native_shadow_scope_preserves_failure_and_rejects_unknown_pipeline() {
    let (world, view, _) = draw_admission_world();
    let allowed = RenderPipelineId::new();
    let audit = NativeRenderAudit::enter(view, vec![allowed]);
    validate_raster_pipeline(Some(allowed));
    assert!(audit.failure().is_none());
    validate_raster_pipeline(Some(RenderPipelineId::new()));
    assert!(audit
        .failure()
        .unwrap()
        .contains("Unverified native render pipeline"));
    drop(audit);
    let audit = NativeRenderAudit::enter(view, Vec::new());
    assert_eq!(
        admit_render_command_result(
            &world,
            view,
            RenderCommandResult::Failure("failed shadow command")
        ),
        Err(DrawError::RenderCommandFailure("failed shadow command"))
    );
    assert_eq!(audit.failure().as_deref(), Some("failed shadow command"));
}

#[test]
fn native_shadow_scope_tracks_skip_missing_and_raw_pipeline_without_raster_lanes() {
    let (world, view, other) = draw_admission_world();
    let audit = NativeRenderAudit::enter(view, Vec::new());
    assert!(native_render_audit_active(view));
    assert!(!native_render_audit_active(other));
    assert_eq!(raster_parity_lane(view), None);
    assert_eq!(
        admit_render_command_result(&world, other, RenderCommandResult::Skip),
        Ok(())
    );
    assert!(audit.failure().is_none());
    assert_eq!(
        admit_render_command_result(&world, view, RenderCommandResult::Skip),
        Ok(())
    );
    assert!(audit.failure().unwrap().contains("skipped"));
    drop(audit);
    let audit = NativeRenderAudit::enter(view, Vec::new());
    assert!(admit_draw_function::<()>(&world, view, None).is_none());
    assert!(audit.failure().unwrap().contains("missing"));
    drop(audit);
    let audit = NativeRenderAudit::enter(view, Vec::new());
    validate_raster_pipeline(None);
    assert!(audit.failure().unwrap().contains("raw pass"));
    drop(audit);
    assert!(!native_render_audit_active(view));
    assert!(world.resource::<RasterParity>().lock().error.is_none());
}

#[test]
fn padding_is_excluded_but_every_native_byte_is_retained() {
    assert_eq!(
        unpack_rows(&[1, 2, 99, 99, 3, 4, 98, 98], 2, 4),
        [1, 2, 3, 4]
    );
}

#[test]
fn lane_is_scoped_to_view_and_restored() {
    let mut world = World::new();
    let view = world.spawn_empty().id();
    let other = world.spawn_empty().id();
    assert!(!raster_parity_forced_lane(view));
    {
        let _guard = LaneGuard::enter(view, RasterParityLane::Forced);
        assert!(raster_parity_forced_lane(view));
        assert!(!raster_parity_forced_lane(other));
    }
    assert!(!raster_parity_forced_lane(view));
}

#[test]
fn receipt_strings_escape_control_characters() {
    assert_eq!(json_string("a\"\\\n\u{1}"), "\"a\\\"\\\\\\n\\u0001\"");
}

fn draw_admission_world() -> (World, Entity, Entity) {
    let mut world = World::new();
    world.init_resource::<RasterParity>();
    let view = world.spawn_empty().id();
    let other = world.spawn_empty().id();
    (world, view, other)
}

#[test]
fn draw_admission_skip_fails_both_active_lanes_without_changing_draw_result() {
    for lane in [RasterParityLane::Retained, RasterParityLane::Forced] {
        let (world, view, _) = draw_admission_world();
        let _guard = LaneGuard::enter(view, lane);
        assert_eq!(
            admit_render_command_result(&world, view, RenderCommandResult::Skip),
            Ok(())
        );
        assert!(world.resource::<RasterParity>().lock().error.is_some());
    }
}

#[test]
fn draw_admission_skip_preserves_inactive_and_other_view_behavior() {
    let (world, view, other) = draw_admission_world();
    assert_eq!(
        admit_render_command_result(&world, view, RenderCommandResult::Skip),
        Ok(())
    );
    let _guard = LaneGuard::enter(other, RasterParityLane::Retained);
    assert_eq!(
        admit_render_command_result(&world, view, RenderCommandResult::Skip),
        Ok(())
    );
    assert!(world.resource::<RasterParity>().lock().error.is_none());
}

#[test]
fn draw_admission_success_including_intentional_once_suppression_is_accepted() {
    for lane in [RasterParityLane::Retained, RasterParityLane::Forced] {
        let (world, view, _) = draw_admission_world();
        let _guard = LaneGuard::enter(view, lane);
        assert_eq!(
            admit_render_command_result(&world, view, RenderCommandResult::Success),
            Ok(())
        );
        assert!(world.resource::<RasterParity>().lock().error.is_none());
    }
}

#[test]
fn draw_admission_command_failure_preserves_original_error() {
    let (world, view, _) = draw_admission_world();
    assert_eq!(
        admit_render_command_result(&world, view, RenderCommandResult::Failure("failure")),
        Err(DrawError::RenderCommandFailure("failure"))
    );
}

#[test]
fn draw_admission_missing_lookup_records_failure_in_both_active_lanes() {
    for lane in [RasterParityLane::Retained, RasterParityLane::Forced] {
        let (world, view, _) = draw_admission_world();
        let _guard = LaneGuard::enter(view, lane);
        assert!(admit_draw_function::<()>(&world, view, None).is_none());
        assert!(world.resource::<RasterParity>().lock().error.is_some());
    }
}

#[test]
fn missing_draw_without_audit_is_skipped() {
    let (world, view, _) = draw_admission_world();
    assert!(admit_draw_function::<()>(&world, view, None).is_none());
    assert!(world.resource::<RasterParity>().lock().error.is_none());
}

#[test]
fn draw_admission_missing_lookup_skips_without_polluting_other_view_audits() {
    let (world, view, other) = draw_admission_world();
    assert!(admit_draw_function::<()>(&world, view, None).is_none());
    let _guard = LaneGuard::enter(other, RasterParityLane::Retained);
    assert!(admit_draw_function::<()>(&world, view, None).is_none());
    assert!(world.resource::<RasterParity>().lock().error.is_none());
}

#[test]
fn draw_admission_present_lookup_preserves_draw_and_accepts_active_lanes() {
    for lane in [RasterParityLane::Retained, RasterParityLane::Forced] {
        let (world, view, _) = draw_admission_world();
        let _guard = LaneGuard::enter(view, lane);
        assert_eq!(admit_draw_function(&world, view, Some(7)), Some(7));
        assert!(world.resource::<RasterParity>().lock().error.is_none());
    }
}

#[test]
fn raw_and_unknown_pipeline_access_fail_only_the_active_audit() {
    let parity = RasterParity::default();
    let known = RenderPipelineId::new();
    let unknown = RenderPipelineId::new();
    {
        let _guard = PipelineGuard(PIPELINES.with(|slot| {
            slot.replace(Some(PipelineScope {
                allowed: vec![known],
                state: Arc::clone(&parity.state),
            }))
        }));
        validate_raster_pipeline(Some(known));
        assert!(parity.lock().error.is_none());
        validate_raster_pipeline(Some(unknown));
        assert!(parity.lock().error.is_some());
        parity.lock().error = None;
        validate_raster_pipeline(None);
        assert!(parity.lock().error.is_some());
    }
    parity.lock().error = None;
    validate_raster_pipeline(None);
    assert!(parity.lock().error.is_none());
}

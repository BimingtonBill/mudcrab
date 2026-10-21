//! Root-facing preparation of the canonical-surface visibility-field query.
//! No registration, scheduling, GPU execution or lighting mutation occurs here.

use crate::far_terrain_shadows::SurfaceTriangulation;
use crate::far_terrain_shadows::gpu::{GPU_VERSION, GpuUpload};
use crate::far_terrain_shadows::view::{
    MappingError, PackedView, ReceiverRelationship, ViewContext,
};

pub const WGSL_SOURCE: &str = include_str!("receiving.wgsl");
pub const ARITHMETIC_WGSL_SOURCE: &str = include_str!("arithmetic.wgsl");
/// Context word15=2 defines a canonical XY visibility-field receiver, distinct
/// from word15=1's assertion that an actual rendered receiver is exact contact.
pub const CANONICAL_FIELD_MODEL: u32 = 2;
pub const COPLANAR_REASON: u32 = 1 << 13;
pub const BAD_PACKET_REASON: u32 = 1 << 14;

/// CPU mirror of WGSL's exact exponent-bit scaling. Unsupported subnormal,
/// overflow or underflow is refused BEFORE executing floating arithmetic.
pub fn binary_scale(value: f32, shift: i32) -> Option<f32> {
    let bits = value.to_bits();
    if bits & 0x7fff_ffff == 0 {
        return Some(value);
    }
    let exponent = ((bits >> 23) & 255) as i32;
    let next = exponent.checked_add(shift)?;
    if exponent == 0 || exponent == 255 || !(1..=254).contains(&next) {
        return None;
    }
    Some(f32::from_bits((bits & 0x807f_ffff) | ((next as u32) << 23)))
}

/// CPU mirror of integer significand decomposition into cell delta and [0,32)
/// sample coordinate. Negative unrepresentable remainders return None, not a
/// rounded cell boundary. This is a numeric representation limit, not ray bias.
pub fn split_sample_coordinate(value: f32) -> Option<(i32, f32)> {
    let bits = value.to_bits();
    let absolute = bits & 0x7fff_ffff;
    if absolute == 0 {
        return Some((0, 0.0));
    }
    let raw = (absolute >> 23) & 255;
    if raw == 0 || raw > 155 {
        return None;
    }
    let exponent = raw as i32 - 127;
    let mut whole = 0u32;
    let mut remainder = f32::from_bits(absolute);
    if exponent >= 5 {
        let shift = (28 - exponent) as u32;
        let significand = (absolute & 0x007f_ffff) | 0x0080_0000;
        whole = significand >> shift;
        let fraction = significand & ((1 << shift) - 1);
        remainder = if fraction == 0 {
            0.0
        } else {
            binary_scale(fraction as f32, exponent - 23)?
        };
    }
    if bits & 0x8000_0000 == 0 {
        return Some((whole as i32, remainder));
    }
    if remainder == 0.0 {
        return Some((-(whole as i32), 0.0));
    }
    let rb = remainder.to_bits();
    let re = ((rb >> 23) & 255) as i32 - 127;
    if re < -19 {
        return None;
    }
    if re < 4 && (((rb & 0x007f_ffff) | 0x0080_0000) & ((1 << (4 - re)) - 1)) != 0 {
        return None;
    }
    Some((-(whole as i32) - 1, 32.0 - remainder))
}

/// CPU mirror of the WGSL footprint mapping with an explicit actual instance
/// cell origin. It does not certify the host's instance/world/snapshot association.
/// Rendered height is ignored by the canonical-surface shadow model.
pub fn map_renderer_footprint(
    renderer: [f32; 3],
    cell_size: f32,
    instance_origin: crate::far_terrain_shadows::CellCoord,
) -> Option<(crate::far_terrain_shadows::CellCoord, [f32; 2])> {
    let bits = cell_size.to_bits();
    let exponent = (bits >> 23) & 255;
    if bits & 0x807f_ffff != 0 || !(6..=253).contains(&exponent) {
        return None;
    }
    let shift = 132 - exponent as i32;
    let (dx, east) = split_sample_coordinate(binary_scale(renderer[0], shift)?)?;
    let (dy, north) = split_sample_coordinate(binary_scale(-renderer[2], shift)?)?;
    Some((
        crate::far_terrain_shadows::CellCoord {
            x: instance_origin.x.checked_add(dx)?,
            y: instance_origin.y.checked_add(dy)?,
        },
        [east, north],
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareError {
    SnapshotMismatch,
    UnsupportedPacket,
    CanonicalRecipeUnavailable,
    /// Current shader mapping requires a binary-power cell size, so sample
    /// coordinates use exact power-of-two scaling rather than approximate division.
    UnsupportedCoordinateScale,
    Mapping(MappingError),
}

/// Compare the FULL exact snapshot on the host before creating the view/instance
/// context. Root must also bind these resources in its matching publication epoch.
/// Expected view/sun IDs passed to WGSL are a guard, not a replacement for lifecycle
/// identity checks. Vertex/instance transforms must satisfy ViewContext's contract.
pub fn prepare_canonical_view(
    upload: &GpuUpload,
    context: &ViewContext,
) -> Result<PackedView, PrepareError> {
    if upload.identity != context.snapshot {
        return Err(PrepareError::SnapshotMismatch);
    }
    if upload.header[0] != GPU_VERSION || upload.header[1] != context.snapshot.worldspace {
        return Err(PrepareError::UnsupportedPacket);
    }
    if !matches!(context.snapshot.canonical, Some(recipe) if recipe.triangulation == SurfaceTriangulation::CanonicalBase)
    {
        return Err(PrepareError::CanonicalRecipeUnavailable);
    }
    let mut packed = context
        .pack_view(ReceiverRelationship::Unresolved)
        .map_err(PrepareError::Mapping)?;
    let bits = packed.words[3][2];
    let exponent = (bits >> 23) & 255;
    if bits & 0x807f_ffff != 0 || !(6..=253).contains(&exponent) {
        return Err(PrepareError::UnsupportedCoordinateScale);
    }
    packed.words[3][3] = CANONICAL_FIELD_MODEL;
    Ok(packed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::far_terrain_shadows::gpu::{Footprint, GpuUpload};
    use crate::far_terrain_shadows::*;

    fn context() -> ViewContext {
        ViewContext {
            snapshot: SnapshotIdentity {
                worldspace: 31,
                cell_size_bits: 4096.0f64.to_bits(),
                database: DatabaseIdentity {
                    schema_revision: 1,
                    content_hash: [1; 32],
                },
                core_cache: CoreCacheIdentity {
                    version: 1,
                    hash: [2; 32],
                },
                canonical: Some(CanonicalRecipeIdentity {
                    package: [3; 32],
                    catalog: [4; 32],
                    content_digest: [5; 32],
                    recipe_revision: 1,
                    triangulation: SurfaceTriangulation::CanonicalBase,
                }),
            },
            view_id: 11,
            view_generation: 4,
            sun_id: 12,
            sun_revision: 6,
            toward_sun: [1.0, 0.0, 0.125],
            view_origin: CellCoord { x: -20, y: 31 },
            instance_origin: CellCoord { x: -21, y: 32 },
            origin_revision: 7,
            cell_size: 4096.0,
        }
    }

    fn upload(view: &ViewContext) -> GpuUpload {
        let mut header = [0; 16];
        header[0] = GPU_VERSION;
        header[1] = view.snapshot.worldspace;
        GpuUpload {
            identity: view.snapshot,
            header,
            words: Vec::new(),
            footprint: Footprint::for_counts(0, 0).unwrap(),
        }
    }

    #[test]
    fn canonical_model_is_not_an_actual_rendered_contact_assertion() {
        let view = context();
        let packed = prepare_canonical_view(&upload(&view), &view).unwrap();
        assert_eq!(packed.words[3][3], 2);
        assert_eq!(packed.words[4][3], 0); // not a fixed-receiver probe
        assert_eq!(packed.words[0], [11, 0, 4, 0]);
        assert_eq!(packed.words[1], [12, 0, 6, 0]);
        assert_eq!(packed.words[3][0], (-21i32) as u32);
        assert_eq!(packed.words[3][1], 32);
    }

    #[test]
    fn full_snapshot_mismatch_and_unimplemented_scale_are_explicit() {
        let view = context();
        let data = upload(&view);
        let mut other = view;
        other.snapshot.core_cache.hash[0] ^= 1;
        assert_eq!(
            prepare_canonical_view(&data, &other),
            Err(PrepareError::SnapshotMismatch)
        );
        let mut other = view;
        other.cell_size = 3000.0;
        other.snapshot.cell_size_bits = 3000.0f64.to_bits();
        assert_eq!(
            prepare_canonical_view(&upload(&other), &other),
            Err(PrepareError::UnsupportedCoordinateScale)
        );
    }

    #[test]
    fn explicit_instance_origins_map_without_changing_the_shared_context() {
        let view = context();
        let packed = prepare_canonical_view(&upload(&view), &view).unwrap();
        let first = CellCoord { x: -21, y: 32 };
        let alternate = CellCoord { x: 400, y: -301 };
        let local = [-64.0, f32::NAN, -4160.0];
        assert_eq!(
            map_renderer_footprint(local, view.cell_size as f32, first),
            Some((CellCoord { x: -22, y: 33 }, [31.5, 0.5]))
        );
        assert_eq!(
            map_renderer_footprint(local, view.cell_size as f32, alternate),
            Some((CellCoord { x: 399, y: -300 }, [31.5, 0.5]))
        );
        // Same global footprint encoded relative to a different anchor.
        assert_eq!(
            map_renderer_footprint([4032.0, 99.0, -64.0], 4096.0, CellCoord { x: -22, y: 33 }),
            map_renderer_footprint(local, 4096.0, first)
        );
        assert_eq!(
            packed,
            prepare_canonical_view(&upload(&view), &view).unwrap()
        );
        assert_eq!(packed.words[3][0], first.x as u32);
        assert_eq!(packed.words[3][1], first.y as u32);
        assert_eq!(
            map_renderer_footprint([4096.0, 0.0, 0.0], 4096.0, CellCoord { x: i32::MAX, y: 0 }),
            None
        );
        assert_eq!(
            map_renderer_footprint([0.0, 0.0, 4096.0], 4096.0, CellCoord { x: 0, y: i32::MIN }),
            None
        );
    }

    #[test]
    fn power_scaling_never_flushes_a_nonzero_direction_or_footprint() {
        assert_eq!(binary_scale(f32::MIN_POSITIVE, -7), None);
        assert_eq!(binary_scale(f32::MAX, 1), None);
        assert_eq!(binary_scale(f32::from_bits(1), 7), None);
        assert_eq!(binary_scale(0.0, -7), Some(0.0));
        assert_eq!(binary_scale(0.000_001, -7), Some(0.000_001 / 128.0));
        assert_eq!(split_sample_coordinate(-f32::MIN_POSITIVE), None);
    }

    #[test]
    fn bitwise_split_matches_exact_binary_value_at_negative_and_far_offsets() {
        for value in [
            -1_048_576.5,
            -64.0,
            -32.25,
            -0.5,
            0.0,
            0.1,
            31.75,
            32.25,
            1_048_576.5,
        ] {
            let (cell, local) = split_sample_coordinate(value).unwrap();
            assert!((0.0..32.0).contains(&local));
            assert_eq!(f64::from(cell) * 32.0 + f64::from(local), f64::from(value));
        }
        assert_eq!(split_sample_coordinate(-0.1), None);
    }
}

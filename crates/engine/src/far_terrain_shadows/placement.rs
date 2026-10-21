//! Optional F1 metadata association only. Never certifies canonical contact,
//! package ownership, GPU publication epoch, visibility or a drawable pass.
//! Caller retains those proofs and supplies a FRESH forward record, not depth data.
use crate::{
    cluster::{
        render::{ClusterInstance, ClusterInstanceIdentity},
        terrain::TerrainViewBlock,
    },
    world::components::CELL_SIZE,
};
use bevy::math::{IVec2, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacementError {
    NotTerrain,
    BlockMismatch,
    WorldMismatch,
    TierMismatch,
    BatchMismatch,
    NonFiniteTransform,
    NonTranslationTransform,
    InexactAbsoluteTranslation,
    TranslationMismatch,
    NonFiniteOriginOffset,
    InexactOriginOffset,
    OriginOffsetMismatch,
    InexactRelativeTranslation,
    RoundedRebase,
    RecordLength,
    NotFreshActiveRecord,
}

fn exact_f32(value: f64, error: PlacementError) -> Result<f32, PlacementError> {
    let result = value as f32;
    if !value.is_finite() || !result.is_finite() || f64::from(result) != value {
        return Err(error);
    }
    Ok(result)
}

pub(crate) fn validate(
    block_index: u32,
    block: &TerrainViewBlock,
    instance: &ClusterInstance,
    origin: IVec2,
    actual_origin_offset: Vec3,
) -> Result<[u32; 3], PlacementError> {
    let ClusterInstanceIdentity::Terrain {
        block: actual_block,
        worldspace,
        tier,
        ..
    } = instance.identity
    else {
        return Err(PlacementError::NotTerrain);
    };
    if actual_block != block_index {
        return Err(PlacementError::BlockMismatch);
    }
    if worldspace != block.worldspace_id {
        return Err(PlacementError::WorldMismatch);
    }
    if tier != block.side_cells {
        return Err(PlacementError::TierMismatch);
    }
    if block.batch != Some(instance.batch) {
        return Err(PlacementError::BatchMismatch);
    }
    // Header-relative identity.anchor is intentionally NOT compared with the
    // absolute southwest cell anchor: their coordinate conventions differ.
    let matrix = instance.world_from_local;
    if !matrix.is_finite() {
        return Err(PlacementError::NonFiniteTransform);
    }
    // Numeric equality deliberately accepts either sign of zero, with no tolerance.
    if matrix.x_axis != Vec4::X
        || matrix.y_axis != Vec4::Y
        || matrix.z_axis != Vec4::Z
        || matrix.w_axis.w != 1.0
    {
        return Err(PlacementError::NonTranslationTransform);
    }
    let spacing = f64::from(CELL_SIZE);
    let absolute = Vec3::new(
        exact_f32(
            f64::from(block.anchor.x) * spacing,
            PlacementError::InexactAbsoluteTranslation,
        )?,
        0.0,
        exact_f32(
            -f64::from(block.anchor.y) * spacing,
            PlacementError::InexactAbsoluteTranslation,
        )?,
    );
    let translation = matrix.w_axis.truncate();
    if translation != absolute {
        return Err(PlacementError::TranslationMismatch);
    }
    if !actual_origin_offset.is_finite() {
        return Err(PlacementError::NonFiniteOriginOffset);
    }
    let offset = Vec3::new(
        exact_f32(
            -f64::from(origin.x) * spacing,
            PlacementError::InexactOriginOffset,
        )?,
        0.0,
        exact_f32(
            f64::from(origin.y) * spacing,
            PlacementError::InexactOriginOffset,
        )?,
    );
    if actual_origin_offset != offset {
        return Err(PlacementError::OriginOffsetMismatch);
    }
    // Subtract integer cells before scaling; never subtract large rounded world floats.
    let east = i64::from(block.anchor.x) - i64::from(origin.x);
    let north = i64::from(block.anchor.y) - i64::from(origin.y);
    let relative = Vec3::new(
        exact_f32(
            east as f64 * spacing,
            PlacementError::InexactRelativeTranslation,
        )?,
        0.0,
        exact_f32(
            -(north as f64) * spacing,
            PlacementError::InexactRelativeTranslation,
        )?,
    );
    if translation + actual_origin_offset != relative {
        return Err(PlacementError::RoundedRebase);
    }
    Ok([
        block.anchor.x as u32,
        block.anchor.y as u32,
        block.worldspace_id,
    ])
}

/// Stamp only a fresh, active 160-byte forward clip record. Failure is atomic:
/// every check precedes every write. Bytes outside state16..32 are untouched.
pub(crate) fn stamp(record: &mut [u8], metadata: [u32; 3]) -> Result<(), PlacementError> {
    if record.len() != 160 {
        return Err(PlacementError::RecordLength);
    }
    let active = u32::from_le_bytes(record[16..20].try_into().expect("checked four-byte state"));
    if active != 1.0_f32.to_bits() {
        return Err(PlacementError::NotFreshActiveRecord);
    }
    for (index, word) in [2.0_f32.to_bits(), metadata[0], metadata[1], metadata[2]]
        .into_iter()
        .enumerate()
    {
        let start = 16 + index * 4;
        record[start..start + 4].copy_from_slice(&word.to_le_bytes());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::reader::RecordKey;
    use bevy::math::Mat4;

    fn inputs(anchor: IVec2, origin: IVec2) -> (TerrainViewBlock, ClusterInstance, Vec3) {
        let block = TerrainViewBlock {
            worldspace_id: 0x70,
            anchor,
            side_cells: 4,
            cells: vec![anchor],
            batch: Some(7),
        };
        let instance = ClusterInstance {
            identity: ClusterInstanceIdentity::Terrain {
                block: 3,
                worldspace: block.worldspace_id,
                tier: block.side_cells,
                anchor: [-17, 23],
            }, // Deliberately distinct header-relative grid.
            mesh: 9,
            batch: 7,
            world_from_local: Mat4::from_translation(Vec3::new(
                anchor.x as f32 * CELL_SIZE,
                0.0,
                -(anchor.y as f32) * CELL_SIZE,
            )),
            source_worldspace: 0,
        };
        (
            block,
            instance,
            Vec3::new(
                -(origin.x as f32) * CELL_SIZE,
                0.0,
                origin.y as f32 * CELL_SIZE,
            ),
        )
    }

    #[test]
    fn exact_positive_negative_rebases_keep_raw_absolute_anchor_and_allow_signed_zero() {
        for (anchor, origin) in [
            (IVec2::new(4, -8), IVec2::new(1, -3)),
            (IVec2::new(-12, 16), IVec2::new(2, -5)),
        ] {
            let (block, mut instance, mut offset) = inputs(anchor, origin);
            instance.world_from_local.x_axis.y = -0.0;
            instance.world_from_local.z_axis.x = -0.0;
            instance.world_from_local.w_axis.y = -0.0;
            offset.y = -0.0;
            assert_eq!(
                validate(3, &block, &instance, origin, offset),
                Ok([anchor.x as u32, anchor.y as u32, 0x70])
            );
        }
    }

    #[test]
    fn wrong_identity_block_world_tier_and_missing_or_wrong_batch_refuse() {
        let origin = IVec2::ZERO;
        let (block, original, offset) = inputs(IVec2::new(4, 8), origin);
        for identity in [
            ClusterInstanceIdentity::Fixture,
            ClusterInstanceIdentity::Object(RecordKey {
                worldspace: 0x70,
                tier: 4,
                anchor: [4, 8],
                ordinal: 0,
            }),
        ] {
            let instance = ClusterInstance {
                identity,
                ..original
            };
            assert_eq!(
                validate(3, &block, &instance, origin, offset),
                Err(PlacementError::NotTerrain)
            );
        }
        for (field, expected) in [
            (0, PlacementError::BlockMismatch),
            (1, PlacementError::WorldMismatch),
            (2, PlacementError::TierMismatch),
        ] {
            let mut instance = original;
            if let ClusterInstanceIdentity::Terrain {
                block,
                worldspace,
                tier,
                ..
            } = &mut instance.identity
            {
                match field {
                    0 => *block += 1,
                    1 => *worldspace += 1,
                    _ => *tier += 1,
                }
            }
            assert_eq!(
                validate(3, &block, &instance, origin, offset),
                Err(expected)
            );
        }
        for batch in [None, Some(8)] {
            let mut block = block.clone();
            block.batch = batch;
            assert_eq!(
                validate(3, &block, &original, origin, offset),
                Err(PlacementError::BatchMismatch)
            );
        }
    }

    #[test]
    fn transforms_vertical_offset_and_unrepresentable_large_rebase_refuse() {
        let origin = IVec2::new(1, 2);
        let (block, original, offset) = inputs(IVec2::new(4, 8), origin);
        let mut matrices = vec![
            Mat4::from_scale(Vec3::splat(2.0)),
            Mat4::from_rotation_y(0.5),
        ];
        let mut shear = original.world_from_local;
        shear.x_axis.y = 0.25;
        matrices.push(shear);
        let mut perspective = original.world_from_local;
        perspective.z_axis.w = 0.25;
        matrices.push(perspective);
        for matrix in matrices {
            let instance = ClusterInstance {
                world_from_local: matrix,
                ..original
            };
            assert_eq!(
                validate(3, &block, &instance, origin, offset),
                Err(PlacementError::NonTranslationTransform)
            );
        }
        let mut changed = original;
        changed.world_from_local.w_axis.y = 1.0;
        assert_eq!(
            validate(3, &block, &changed, origin, offset),
            Err(PlacementError::TranslationMismatch)
        );
        changed = original;
        changed.world_from_local.w_axis.x = f32::NAN;
        assert_eq!(
            validate(3, &block, &changed, origin, offset),
            Err(PlacementError::NonFiniteTransform)
        );
        assert_eq!(
            validate(3, &block, &original, origin, offset + Vec3::Y),
            Err(PlacementError::OriginOffsetMismatch)
        );
        assert_eq!(
            validate(3, &block, &original, origin, Vec3::splat(f32::INFINITY)),
            Err(PlacementError::NonFiniteOriginOffset)
        );
        let (large, instance, offset) = inputs(IVec2::new(16_777_216, 0), IVec2::new(-1, 0));
        assert_eq!(
            validate(3, &large, &instance, IVec2::new(-1, 0), offset),
            Err(PlacementError::InexactRelativeTranslation)
        );
        let (large, instance, offset) = inputs(IVec2::new(16_777_217, 0), IVec2::ZERO);
        assert_eq!(
            validate(3, &large, &instance, IVec2::ZERO, offset),
            Err(PlacementError::InexactAbsoluteTranslation)
        );
        let (block, instance, offset) = inputs(IVec2::ZERO, IVec2::new(16_777_217, 0));
        assert_eq!(
            validate(3, &block, &instance, IVec2::new(16_777_217, 0), offset),
            Err(PlacementError::InexactOriginOffset)
        );
    }

    fn fresh_record() -> Vec<u8> {
        let mut bytes = vec![0xa5; 160];
        bytes[16..20].copy_from_slice(&1.0_f32.to_bits().to_le_bytes());
        bytes
    }

    #[test]
    fn stamping_is_raw_little_endian_and_changes_only_forward_state() {
        let metadata = [0x7fc0_0001, 0xff80_0001, 0xffff_ffff]; // NaN-shaped raw identities.
        let mut record = fresh_record();
        let before = record.clone();
        stamp(&mut record, metadata).unwrap();
        assert_eq!(&record[..16], &before[..16]);
        assert_eq!(&record[32..], &before[32..]);
        assert_eq!(&record[16..20], &2.0_f32.to_bits().to_le_bytes());
        assert_eq!(&record[20..24], &[1, 0, 0xc0, 0x7f]);
        assert_eq!(&record[24..28], &[1, 0, 0x80, 0xff]);
        assert_eq!(&record[28..32], &[0xff; 4]);
        let once = record.clone();
        assert_eq!(
            stamp(&mut record, metadata),
            Err(PlacementError::NotFreshActiveRecord)
        );
        assert_eq!(record, once);
    }

    #[test]
    fn inactive_malformed_and_already_stamped_records_are_unchanged_on_refusal() {
        for size in [0, 159, 161, 320] {
            let mut record = vec![0xa5; size];
            let before = record.clone();
            assert_eq!(
                stamp(&mut record, [1, 2, 3]),
                Err(PlacementError::RecordLength)
            );
            assert_eq!(record, before);
        }
        for state in [
            0.0_f32.to_bits(),
            2.0_f32.to_bits(),
            f32::NAN.to_bits(),
            (-1.0_f32).to_bits(),
        ] {
            let mut record = fresh_record();
            record[16..20].copy_from_slice(&state.to_le_bytes());
            let before = record.clone();
            assert_eq!(
                stamp(&mut record, [1, 2, 3]),
                Err(PlacementError::NotFreshActiveRecord)
            );
            assert_eq!(record, before);
        }
    }
}

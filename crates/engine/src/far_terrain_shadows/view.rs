//! Per-view coordinate contract for a future direct-sun terrain receiver.
//!
//! Renderer coordinates are `[east, up, -north]`; inventory coordinates are
//! `[east, north, up]`. Horizontal origins remain signed integer cells. Neither
//! mapping nor packing constructs a large absolute floating-point position.
//! This is an encoding contract, not a claim that GPU arithmetic proves a ray.

use super::{CellCoord, SnapshotIdentity, SurfaceTriangulation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingError {
    InvalidSpacing,
    SpacingIdentityMismatch,
    NonFinitePosition,
    InvalidSun,
    CellOverflow,
    /// A required operation/conversion would round; the receiver stays Unknown.
    PrecisionUnknown,
    /// Drawn/simplified receiver contact with the canonical shadow model is unproved.
    ReceiverRelationshipUnknown,
}

/// A reader/render adapter assertion, never inferred from a sampled height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverRelationship {
    /// Actual fragment lies on the selected, post-weld canonical base surface,
    /// with compatible coordinates. Only the t=0 contact may be excluded.
    ExactCanonicalContact,
    /// Includes an actual simplified fragment below or above that surface.
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewContext {
    pub snapshot: SnapshotIdentity,
    pub view_id: u64,
    pub view_generation: u64,
    pub sun_id: u64,
    pub sun_revision: u64,
    /// Creation `[east, north, up]`, toward the selected sun; not normalized.
    pub toward_sun: [f64; 3],
    pub view_origin: CellCoord,
    pub instance_origin: CellCoord,
    /// Changes whenever view/instance origin transforms are republished.
    pub origin_revision: u64,
    pub cell_size: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridReceiver {
    pub cell: CellCoord,
    /// Creation `[east, north, up]`; horizontal components in `[0, cell_size)`.
    /// Vertical component is world height, not a terrain-relative offset.
    pub local: [f64; 3],
}

/// Future WGSL `struct ViewWords { words: array<vec4<u32>, 25>, }` (400 bytes).
/// Serialize each u32 little-endian for upload; do not upload Rust struct memory.
/// Slots below are flattened word offsets; padding is zero:
/// 0..4 view id/generation; 4..8 sun id/revision; 8..12 origin revision,
/// view-origin x/y; 12..16 instance-origin x/y, spacing f32, relationship
/// (0=unresolved, 1=exact canonical contact);
/// 16..20 sun xyz f32, probe flag; 20..24 probe cell xy, local east/north f32;
/// 24..28 probe height f32, worldspace, spacing f64 low/high;
/// 28..32 database schema, core version, canonical-present, reserved;
/// 32..40 database digest; 40..48 core digest; 48..56 canonical package;
/// 56..64 canonical catalog; 64..72 canonical content; 72..76 canonical recipe,
/// triangulation tag (0=base,1=unsupported), unsupported payload, reserved.
/// 76..100 reserved zero. Full identities are compared by the host before binding;
/// GPU words retain them for diagnostics, not an abbreviated hash certificate.
/// Production uses [`ViewContext::pack_view`]: probe flag and words20..25 are zero.
/// Production receiver coordinates come from each actual fragment, never these
/// uniform probe slots. The relationship gates that fragment's shadow query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedView {
    pub words: [[u32; 4]; 25],
}

fn exact_f32(value: f64) -> Result<f32, MappingError> {
    let packed = value as f32;
    // Nonzero subnormals are representable but WGSL implementations may flush
    // them. They cannot carry this contract's unchanged-input assertion.
    if !value.is_finite()
        || !packed.is_finite()
        || (packed != 0.0 && !packed.is_normal())
        || f64::from(packed) != value
    {
        return Err(MappingError::PrecisionUnknown);
    }
    Ok(packed)
}

// Error-free subtraction; rejecting its residual avoids treating a rounded
// cell-local position as the original fragment. All intermediates are finite.
fn exact_sub(a: f64, b: f64) -> Result<f64, MappingError> {
    let result = a - b;
    let b_virtual = a - result;
    let a_virtual = result + b_virtual;
    let residual = (a - a_virtual) + (b_virtual - b);
    if !result.is_finite() || residual != 0.0 {
        return Err(MappingError::PrecisionUnknown);
    }
    Ok(result)
}

fn split_axis(origin: i32, offset: f64, spacing: f64) -> Result<(i32, f64), MappingError> {
    let quotient = offset / spacing;
    // A resulting i32 cell cannot differ from an i32 origin by more than this.
    if !quotient.is_finite() || quotient.floor().abs() > f64::from(u32::MAX) {
        return Err(MappingError::CellOverflow);
    }
    let mut delta = quotient.floor() as i64;
    for _ in 0..2 {
        let product = delta as f64 * spacing;
        if !product.is_finite() || (delta as f64).mul_add(spacing, -product) != 0.0 {
            return Err(MappingError::PrecisionUnknown);
        }
        let local = exact_sub(offset, product)?;
        // Rounded division can land on the adjacent integer. Resolve only by an
        // exact remainder; no epsilon, finite skip or guessed grid membership.
        if local < 0.0 {
            delta -= 1;
        } else if local >= spacing {
            delta += 1;
        } else {
            let cell = i64::from(origin)
                .checked_add(delta)
                .ok_or(MappingError::CellOverflow)?;
            return Ok((
                i32::try_from(cell).map_err(|_| MappingError::CellOverflow)?,
                local,
            ));
        }
    }
    Err(MappingError::PrecisionUnknown)
}

impl ViewContext {
    pub fn validate(&self) -> Result<(), MappingError> {
        if !self.cell_size.is_normal() || self.cell_size <= 0.0 {
            return Err(MappingError::InvalidSpacing);
        }
        if self.cell_size.to_bits() != self.snapshot.cell_size_bits {
            return Err(MappingError::SpacingIdentityMismatch);
        }
        if !self.toward_sun.iter().all(|x| x.is_finite()) || self.toward_sun == [0.0; 3] {
            return Err(MappingError::InvalidSun);
        }
        Ok(())
    }

    /// `renderer_local` is instance-local horizontally, world-height vertically.
    /// Vertical instance transforms must be applied by the caller without rounding
    /// before this contract can be certified. Rotated/scaled instances require a
    /// separately certified transform; cell origins alone do not describe them.
    pub fn map_receiver(
        &self,
        renderer_local: [f64; 3],
        relationship: ReceiverRelationship,
    ) -> Result<GridReceiver, MappingError> {
        self.validate()?;
        if relationship != ReceiverRelationship::ExactCanonicalContact {
            return Err(MappingError::ReceiverRelationshipUnknown);
        }
        if !renderer_local.iter().all(|x| x.is_finite()) {
            return Err(MappingError::NonFinitePosition);
        }
        let (x, east) = split_axis(self.instance_origin.x, renderer_local[0], self.cell_size)?;
        let (y, north) = split_axis(self.instance_origin.y, -renderer_local[2], self.cell_size)?;
        Ok(GridReceiver {
            cell: CellCoord { x, y },
            local: [east, north, renderer_local[1]],
        })
    }

    /// Production per-view/per-instance context. Unresolved relationships are
    /// encoded explicitly so the fragment path can return Unknown. No receiver
    /// position is supplied here: each production fragment supplies its own.
    /// A later GPU ray port still needs conservative arithmetic; exact inputs
    /// alone do not prove it.
    pub fn pack_view(
        &self,
        relationship: ReceiverRelationship,
    ) -> Result<PackedView, MappingError> {
        self.validate()?;
        let mut flat = [0u32; 100];
        fn u64_words(flat: &mut [u32], at: usize, value: u64) {
            flat[at] = value as u32;
            flat[at + 1] = (value >> 32) as u32;
        }
        fn digest(flat: &mut [u32], at: usize, value: [u8; 32]) {
            for (index, chunk) in value.as_chunks::<4>().0.iter().enumerate() {
                flat[at + index] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
        }
        u64_words(&mut flat, 0, self.view_id);
        u64_words(&mut flat, 2, self.view_generation);
        u64_words(&mut flat, 4, self.sun_id);
        u64_words(&mut flat, 6, self.sun_revision);
        u64_words(&mut flat, 8, self.origin_revision);
        flat[10] = self.view_origin.x as u32;
        flat[11] = self.view_origin.y as u32;
        flat[12] = self.instance_origin.x as u32;
        flat[13] = self.instance_origin.y as u32;
        flat[14] = exact_f32(self.cell_size)?.to_bits();
        let step = exact_f32(self.cell_size / super::CELL_INTERVALS as f64)?;
        if !step.is_normal() {
            return Err(MappingError::PrecisionUnknown);
        }
        flat[15] = u32::from(relationship == ReceiverRelationship::ExactCanonicalContact);
        for axis in 0..3 {
            flat[16 + axis] = exact_f32(self.toward_sun[axis])?.to_bits();
        }
        flat[25] = self.snapshot.worldspace;
        u64_words(&mut flat, 26, self.snapshot.cell_size_bits);
        flat[28] = self.snapshot.database.schema_revision;
        flat[29] = self.snapshot.core_cache.version;
        digest(&mut flat, 32, self.snapshot.database.content_hash);
        digest(&mut flat, 40, self.snapshot.core_cache.hash);
        if let Some(canonical) = self.snapshot.canonical {
            flat[30] = 1;
            digest(&mut flat, 48, canonical.package);
            digest(&mut flat, 56, canonical.catalog);
            digest(&mut flat, 64, canonical.content_digest);
            flat[72] = canonical.recipe_revision;
            if let SurfaceTriangulation::Unsupported(tag) = canonical.triangulation {
                flat[73] = 1;
                flat[74] = tag;
            }
        }
        let mut words = [[0; 4]; 25];
        for (out, chunk) in words.iter_mut().zip(flat.as_chunks::<4>().0.iter()) {
            out.copy_from_slice(chunk);
        }
        Ok(PackedView { words })
    }

    /// CPU reference/probe convenience, never a draw-wide receiver binding.
    /// Maps and gates contact before setting probe flag19=1 and words20..25.
    /// Offsets must survive f32 conversion exactly; no unrelated GridReceiver
    /// can be substituted. Production uses pack_view and per-fragment positions.
    pub fn pack_receiver(
        &self,
        renderer_local: [f64; 3],
        relationship: ReceiverRelationship,
    ) -> Result<PackedView, MappingError> {
        let receiver = self.map_receiver(renderer_local, relationship)?;
        let mut packed = self.pack_view(relationship)?;
        packed.words[4][3] = 1;
        packed.words[5][0] = receiver.cell.x as u32;
        packed.words[5][1] = receiver.cell.y as u32;
        packed.words[5][2] = exact_f32(receiver.local[0])?.to_bits();
        packed.words[5][3] = exact_f32(receiver.local[1])?.to_bits();
        packed.words[6][0] = exact_f32(receiver.local[2])?.to_bits();
        Ok(packed)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CoreCacheIdentity, DatabaseIdentity};
    use super::*;

    fn context() -> ViewContext {
        ViewContext {
            snapshot: SnapshotIdentity {
                worldspace: 7,
                cell_size_bits: 4096f64.to_bits(),
                database: DatabaseIdentity {
                    schema_revision: 1,
                    content_hash: [1; 32],
                },
                core_cache: CoreCacheIdentity {
                    version: 2,
                    hash: [2; 32],
                },
                canonical: None,
            },
            view_id: 1,
            view_generation: 2,
            sun_id: 3,
            sun_revision: 4,
            toward_sun: [1.0, 0.0, 1.0 / 1_048_576.0],
            view_origin: CellCoord { x: -100, y: 200 },
            instance_origin: CellCoord { x: 0, y: 0 },
            origin_revision: 5,
            cell_size: 4096.0,
        }
    }
    const CONTACT: ReceiverRelationship = ReceiverRelationship::ExactCanonicalContact;

    #[test]
    fn production_context_has_no_uniform_receiver_and_preserves_unknown_relationship() {
        let view = context();
        let unresolved = view.pack_view(ReceiverRelationship::Unresolved).unwrap();
        assert_eq!(unresolved.words[3][3], 0);
        assert_eq!(unresolved.words[4][3], 0);
        assert_eq!(unresolved.words[5], [0; 4]);
        assert_eq!(unresolved.words[6][0], 0);
        let exact = view.pack_view(CONTACT).unwrap();
        assert_eq!(exact.words[3][3], 1);
        assert_eq!(exact.words[4][3], 0);
        let probe = view.pack_receiver([1.0, 2.0, -3.0], CONTACT).unwrap();
        assert_eq!(probe.words[4][3], 1);
        assert_eq!(probe.words[5][2], 1f32.to_bits());
        assert_eq!(probe.words[5][3], 3f32.to_bits());
        assert_eq!(probe.words[6][0], 2f32.to_bits());
    }

    #[test]
    fn negative_offsets_floor_and_reorder_axes() {
        let mapped = context()
            .map_receiver([-0.5, 27.0, 4096.25], CONTACT)
            .unwrap();
        assert_eq!(mapped.cell, CellCoord { x: -1, y: -2 });
        assert_eq!(mapped.local, [4095.5, 4095.75, 27.0]);
    }

    #[test]
    fn far_integer_origin_keeps_sub_unit_local() {
        let mut view = context();
        view.instance_origin = CellCoord {
            x: i32::MAX - 1,
            y: i32::MIN + 1,
        };
        let mapped = view.map_receiver([0.25, 2.0, -0.5], CONTACT).unwrap();
        assert_eq!(mapped.cell, view.instance_origin);
        assert_eq!(mapped.local, [0.25, 0.5, 2.0]);
        assert!(view.pack_receiver([0.25, 2.0, -0.5], CONTACT).is_ok());
        assert_eq!(
            view.map_receiver([8192.0, 0.0, 0.0], CONTACT),
            Err(MappingError::CellOverflow)
        );
    }

    #[test]
    fn portal_world_sun_view_and_origin_are_separate_words() {
        let first = context();
        let original = first.pack_receiver([1.0, 0.0, -1.0], CONTACT).unwrap();
        for which in 0..5 {
            let mut portal = first;
            match which {
                0 => portal.snapshot.worldspace += 1,
                1 => portal.sun_id += 1,
                2 => portal.sun_revision += 1,
                3 => portal.view_generation += 1,
                _ => portal.origin_revision += 1,
            }
            assert_ne!(
                original,
                portal.pack_receiver([1.0, 0.0, -1.0], CONTACT).unwrap()
            );
        }
    }

    #[test]
    fn simplified_receiver_never_gets_a_lift_or_skips_a_blocker() {
        for height in [-10.0, 0.0, 10.0] {
            assert_eq!(
                context().pack_receiver([1.0, height, -1.0], ReceiverRelationship::Unresolved),
                Err(MappingError::ReceiverRelationshipUnknown)
            );
        }
    }

    #[test]
    fn low_sun_is_not_normalized_or_clamped_and_underflow_is_unknown() {
        let mut view = context();
        let packed = view.pack_receiver([0.0; 3], CONTACT).unwrap();
        assert_eq!(packed.words[4][2], ((1.0f32) / 1_048_576.0).to_bits());
        view.toward_sun[2] = 1e-300;
        assert_eq!(
            view.pack_receiver([0.0; 3], CONTACT),
            Err(MappingError::PrecisionUnknown)
        );
    }

    #[test]
    fn inexact_local_and_invalid_context_are_explicit() {
        let mut view = context();
        assert_eq!(
            view.pack_receiver([0.1, 0.0, 0.0], CONTACT),
            Err(MappingError::PrecisionUnknown)
        );
        view.snapshot.cell_size_bits = 2048f64.to_bits();
        assert_eq!(
            view.map_receiver([0.0; 3], CONTACT),
            Err(MappingError::SpacingIdentityMismatch)
        );
        view = context();
        view.toward_sun = [0.0; 3];
        assert_eq!(
            view.map_receiver([0.0; 3], CONTACT),
            Err(MappingError::InvalidSun)
        );
        view = context();
        assert_eq!(
            view.map_receiver([f64::NAN, 0.0, 0.0], CONTACT),
            Err(MappingError::NonFinitePosition)
        );
    }
}

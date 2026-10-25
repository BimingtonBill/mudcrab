//! Allocation-local validity of the eight-stage cluster selection result.
//!
//! Writers publish revisions; checking an unchanged view never walks its instances,
//! materials, masks, or source-order words. Uniform equality is checked separately
//! against the exact bytes already uploaded to this view's parameter allocation.

/// One process-wide namespace for both cull results and raster-only dependencies.
pub(super) fn next_revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_update(
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
        |revision| revision.checked_add(1),
    )
    .expect("cluster output revision exhausted")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CullInputs {
    pub gpu: u64,
    pub layout: u64,
    pub materials: u64,
    pub selection: u64,
    pub plan: u64,
    pub readiness: u64,
    pub pipelines: u64,
    pub origin: [u32; 3],
    /// Full view inputs also invalidate when an explicit clip_from_world makes
    /// one changed projection/pose word irrelevant to the derived uniform.
    pub world_from_view: [u32; 16],
    pub clip_from_view: [u32; 16],
    pub clip_from_world: Option<[u32; 16]>,
    pub viewport: [u32; 4],
    pub invert_culling: bool,
}

/// Owned by one ViewGpu and destroyed with its output allocations.
#[derive(Default)]
pub(super) struct CullReuse {
    encoded: Option<CullInputs>,
    revision: u64,
}

impl CullReuse {
    /// An input change withdraws the old result before any GPU work is encoded.
    pub fn needs_recompute(&mut self, inputs: CullInputs, uniform_changed: bool) -> bool {
        if !uniform_changed && self.encoded == Some(inputs) {
            return false;
        }
        self.invalidate();
        true
    }

    /// Publish only after all eight stages were encoded successfully.
    pub fn encoded(&mut self, inputs: CullInputs) {
        // A recreated ViewGpu must never alias the revision published by its
        // predecessor, including when its retained entity and inputs match.
        self.revision = next_revision();
        self.encoded = Some(inputs);
    }

    pub fn invalidate(&mut self) {
        self.encoded = None;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_inputs_reuse_only_a_complete_result() {
        let inputs = CullInputs::default();
        let mut reuse = CullReuse::default();
        assert!(reuse.needs_recompute(inputs, false));
        // Planning or uploading parameters alone does not certify GPU outputs.
        assert!(reuse.needs_recompute(inputs, false));
        reuse.encoded(inputs);
        let revision = reuse.revision();
        assert_ne!(revision, 0);
        for _ in 0..1000 {
            assert!(!reuse.needs_recompute(inputs, false));
            assert_eq!(reuse.revision(), revision);
        }
    }

    #[test]
    fn each_mutation_recomputes_in_the_same_frame() {
        let initial = CullInputs::default();
        let mut changes = [initial; 10];
        changes[0].gpu = 1;
        changes[1].layout = 1;
        changes[2].materials = 1;
        changes[3].selection = 1;
        changes[4].plan = 1;
        changes[5].readiness = 1;
        changes[6].pipelines = 1;
        changes[7].origin[0] = 1;
        changes[8].origin[1] = 1;
        changes[9].origin[2] = 1;
        for changed in changes {
            let mut reuse = CullReuse::default();
            reuse.encoded(initial);
            assert!(reuse.needs_recompute(changed, false), "{changed:?}");
            // A failed encode must not revive the previous output on a reversion.
            assert!(reuse.needs_recompute(initial, false));
            reuse.encoded(changed);
            assert!(!reuse.needs_recompute(changed, false));
        }
    }

    #[test]
    fn changed_uniform_and_replaced_allocations_withdraw_old_outputs() {
        let inputs = CullInputs::default();
        let mut reuse = CullReuse::default();
        reuse.encoded(inputs);
        assert!(reuse.needs_recompute(inputs, true));
        assert!(reuse.needs_recompute(inputs, false));
        reuse.encoded(inputs);
        assert!(!reuse.needs_recompute(inputs, false));
        let old_revision = reuse.revision();
        let mut replacement = CullReuse::default();
        assert!(replacement.needs_recompute(inputs, false));
        replacement.encoded(inputs);
        assert_ne!(replacement.revision(), old_revision);
    }

    #[test]
    fn every_view_input_bit_invalidates_even_if_the_derived_uniform_matches() {
        let initial = CullInputs::default();
        for matrix in 0..3 {
            for word in 0..16 {
                let mut changed = initial;
                match matrix {
                    0 => changed.world_from_view[word] = 1,
                    1 => changed.clip_from_view[word] = 1,
                    _ => changed.clip_from_world.get_or_insert([0; 16])[word] = 1,
                }
                let mut reuse = CullReuse::default();
                reuse.encoded(initial);
                assert!(reuse.needs_recompute(changed, false));
                reuse.encoded(changed);
                assert!(!reuse.needs_recompute(changed, false));
            }
        }
        for word in 0..4 {
            let mut changed = initial;
            changed.viewport[word] = 1;
            let mut reuse = CullReuse::default();
            reuse.encoded(initial);
            assert!(reuse.needs_recompute(changed, false));
        }
        for changed in [
            CullInputs {
                invert_culling: true,
                ..initial
            },
            CullInputs {
                clip_from_world: Some([0; 16]),
                ..initial
            },
        ] {
            let mut reuse = CullReuse::default();
            reuse.encoded(initial);
            assert!(reuse.needs_recompute(changed, false));
        }
    }
}

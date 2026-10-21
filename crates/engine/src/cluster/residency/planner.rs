//! Which cluster blocks to load, keep and retire for a camera (design note section B2 to B4).
//!
//! A pure function of the block table, the camera, the active worldspace and what is resident, so
//! the policy is tested without the engine. Distances use the block's bounding box, not its
//! centre, and the unload distance exceeds the load distance by a margin so a block at the edge
//! does not flap.

use std::collections::{HashMap, HashSet};

/// One row of the block table, reduced to what the planner needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockInfo {
    pub id: u32,
    /// The worldspace whose blocks these are (a parent such as Tamriel; children have none).
    pub worldspace: u32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

/// Child worldspace id -> parent worldspace id, written by the converter (a child city's
/// objects live in the parent's blocks).
#[derive(Clone, Debug, Default)]
pub struct WorldTree {
    parents: HashMap<u32, u32>,
}

impl WorldTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the tree from `(child, parent)` pairs. A later pair for the same child wins.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (u32, u32)>) -> Self {
        Self {
            parents: pairs.into_iter().collect(),
        }
    }

    /// Declares `parent` as the parent of `child`, replacing an earlier parent.
    pub fn set_parent(&mut self, child: u32, parent: u32) {
        self.parents.insert(child, parent);
    }

    /// `world`, then its parent, then its parent's parent, and so on up to the root. A cycle (or
    /// a self-parent) ends the walk at the first repeated id instead of looping.
    pub fn ancestors_or_self(&self, world: u32) -> Vec<u32> {
        let mut chain = vec![world];
        let mut seen = HashSet::from([world]);
        let mut current = world;
        while let Some(&parent) = self.parents.get(&current) {
            if !seen.insert(parent) {
                break;
            }
            chain.push(parent);
            current = parent;
        }
        chain
    }
}

/// The planner's thresholds, both in world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannerConfig {
    /// A block whose box is within this distance of the camera is wanted. The caller takes it
    /// from the camera far distance (the largest configured LOD reach), not a constant here.
    pub far_distance: f32,
    /// A resident block is kept until it is this much farther than `far_distance`.
    pub retire_margin: f32,
}

/// The planner's answer. Every list is in a deterministic order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Block ids to load, nearest first (ties by id).
    pub wanted: Vec<u32>,
    /// Resident block ids to leave alone, ascending.
    pub keep: Vec<u32>,
    /// Resident block ids to retire, ascending.
    pub retire: Vec<u32>,
}

/// Euclidean distance from `point` to the axis-aligned box (0 inside it).
pub fn distance_to_box(point: [f32; 3], min: [f32; 3], max: [f32; 3]) -> f32 {
    let mut sum = 0.0f32;
    for axis in 0..3 {
        let gap = (min[axis] - point[axis])
            .max(point[axis] - max[axis])
            .max(0.0);
        sum += gap * gap;
    }
    sum.sqrt()
}

/// Sorts the blocks into wanted, keep and retire.
///
/// - **In scope**: the block's worldspace is `active_world` or one of its ancestors in `worlds`,
///   so entering a child worldspace keeps the parent's blocks.
/// - **Wanted**: in scope, within `far_distance`, not resident.
/// - **Keep**: resident, and either in scope within `far_distance + retire_margin`, or pinned.
/// - **Retire**: resident, not pinned, and out of scope or beyond `far_distance + retire_margin`.
///
/// Resident or pinned ids missing from `blocks` (the table changed under them) count as out of
/// scope: they are retired unless pinned. Pinned ids that are not resident are ignored.
pub fn plan(
    blocks: &[BlockInfo],
    camera: [f32; 3],
    active_world: u32,
    worlds: &WorldTree,
    config: &PlannerConfig,
    resident: &HashSet<u32>,
    pinned: &HashSet<u32>,
) -> Plan {
    let scope: HashSet<u32> = worlds.ancestors_or_self(active_world).into_iter().collect();
    let keep_distance = config.far_distance + config.retire_margin;

    let mut wanted: Vec<(f32, u32)> = Vec::new();
    let mut keep = Vec::new();
    let mut retire = Vec::new();
    let mut seen: HashSet<u32> = HashSet::with_capacity(blocks.len());

    for block in blocks {
        if !seen.insert(block.id) {
            continue;
        }
        let in_scope = scope.contains(&block.worldspace);
        let distance = distance_to_box(camera, block.bounds_min, block.bounds_max);
        if resident.contains(&block.id) {
            let in_keep_range = in_scope && distance <= keep_distance;
            if in_keep_range || pinned.contains(&block.id) {
                keep.push(block.id);
            } else {
                retire.push(block.id);
            }
        } else if in_scope && distance <= config.far_distance {
            wanted.push((distance, block.id));
        }
    }
    for &id in resident {
        if !seen.contains(&id) {
            if pinned.contains(&id) {
                keep.push(id);
            } else {
                retire.push(id);
            }
        }
    }

    wanted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    keep.sort_unstable();
    retire.sort_unstable();
    Plan {
        wanted: wanted.into_iter().map(|(_, id)| id).collect(),
        keep,
        retire,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAMRIEL: u32 = 60;
    const WHITERUN: u32 = 61;
    const SOLSTHEIM: u32 = 200;

    /// A 100-unit cube on the x axis at `x`.
    fn block(id: u32, worldspace: u32, x: f32) -> BlockInfo {
        BlockInfo {
            id,
            worldspace,
            bounds_min: [x, 0.0, 0.0],
            bounds_max: [x + 100.0, 100.0, 100.0],
        }
    }

    fn config() -> PlannerConfig {
        PlannerConfig {
            far_distance: 1000.0,
            retire_margin: 200.0,
        }
    }

    fn tree() -> WorldTree {
        WorldTree::from_pairs([(WHITERUN, TAMRIEL)])
    }

    fn set(ids: &[u32]) -> HashSet<u32> {
        ids.iter().copied().collect()
    }

    fn run(
        blocks: &[BlockInfo],
        camera: [f32; 3],
        world: u32,
        resident: &[u32],
        pinned: &[u32],
    ) -> Plan {
        plan(
            blocks,
            camera,
            world,
            &tree(),
            &config(),
            &set(resident),
            &set(pinned),
        )
    }

    #[test]
    fn empty_block_list_plans_nothing() {
        assert_eq!(run(&[], [0.0; 3], TAMRIEL, &[], &[]), Plan::default());
        // Resident ids with no table row are retired, pinned ones kept.
        let plan = run(&[], [0.0; 3], TAMRIEL, &[4, 5], &[5]);
        assert_eq!(plan.retire, vec![4]);
        assert_eq!(plan.keep, vec![5]);
        assert!(plan.wanted.is_empty());
    }

    #[test]
    fn camera_inside_a_block_has_distance_zero() {
        let b = block(1, TAMRIEL, 0.0);
        assert_eq!(
            distance_to_box([50.0, 50.0, 50.0], b.bounds_min, b.bounds_max),
            0.0
        );
        let tight = PlannerConfig {
            far_distance: 0.0,
            retire_margin: 0.0,
        };
        let got = plan(
            &[b],
            [50.0, 50.0, 50.0],
            TAMRIEL,
            &tree(),
            &tight,
            &set(&[]),
            &set(&[]),
        );
        assert_eq!(got.wanted, vec![1]);
    }

    #[test]
    fn distance_is_euclidean_to_the_box_not_the_centre_or_chebyshev() {
        let b = block(1, TAMRIEL, 0.0);
        // 3-4-5 triangle off the corner (100, 100, 100)... on x and y only.
        let d = distance_to_box([103.0, 104.0, 50.0], b.bounds_min, b.bounds_max);
        assert!((d - 5.0).abs() < 1e-4, "{d}");
        // Far diagonal: Euclidean exceeds the largest single-axis gap.
        let diag = distance_to_box([700.0, 800.0, 50.0], b.bounds_min, b.bounds_max);
        assert!(diag > 700.0);
        // Off the corner, 700 along each axis is 990 away (in reach at 1000), 720 is 1018 (out),
        // although a per-axis (Chebyshev) test would call both in reach.
        let near_corner = run(&[b], [-700.0, -700.0, 50.0], TAMRIEL, &[], &[]);
        assert_eq!(near_corner.wanted, vec![1]);
        let past_corner = run(&[b], [-720.0, -720.0, 50.0], TAMRIEL, &[], &[]);
        assert!(past_corner.wanted.is_empty());
    }

    #[test]
    fn wanted_is_nearest_first_with_ties_by_id() {
        let blocks = [
            block(9, TAMRIEL, 400.0),
            block(3, TAMRIEL, 100.0),
            block(7, TAMRIEL, 100.0), // same box as 3: the tie is broken by id
            block(1, TAMRIEL, 800.0),
            block(2, TAMRIEL, 5000.0), // out of reach
        ];
        // Camera at x=-50: blocks 3 and 7 are both 150 away (identical boxes), 9 is 450, 1 is 850.
        let got = run(&blocks, [-50.0, 50.0, 50.0], TAMRIEL, &[], &[]);
        assert_eq!(got.wanted, vec![3, 7, 9, 1]);
        // Shuffled input gives the same answer.
        let mut shuffled = blocks;
        shuffled.reverse();
        let again = run(&shuffled, [-50.0, 50.0, 50.0], TAMRIEL, &[], &[]);
        assert_eq!(again, got);
    }

    #[test]
    fn resident_blocks_are_not_wanted_again() {
        let blocks = [block(1, TAMRIEL, 0.0), block(2, TAMRIEL, 300.0)];
        let got = run(&blocks, [0.0; 3], TAMRIEL, &[1], &[]);
        assert_eq!(got.wanted, vec![2]);
        assert_eq!(got.keep, vec![1]);
        assert!(got.retire.is_empty());
    }

    #[test]
    fn hysteresis_band_neither_loads_nor_retires() {
        // Block's near face is at x = 1100 from a camera at the origin edge: between far (1000)
        // and far + margin (1200).
        let band = block(1, TAMRIEL, 1100.0);
        let cam = [0.0, 50.0, 50.0];
        assert_eq!(run(&[band], cam, TAMRIEL, &[], &[]), Plan::default());
        let resident = run(&[band], cam, TAMRIEL, &[1], &[]);
        assert_eq!(resident.keep, vec![1]);
        assert!(resident.retire.is_empty() && resident.wanted.is_empty());
    }

    #[test]
    fn boundaries_do_not_flap() {
        let cam = [0.0, 50.0, 50.0];
        // Exactly at the load distance: wanted. A hair beyond: not.
        assert_eq!(
            run(&[block(1, TAMRIEL, 1000.0)], cam, TAMRIEL, &[], &[]).wanted,
            vec![1]
        );
        assert!(
            run(&[block(1, TAMRIEL, 1000.5)], cam, TAMRIEL, &[], &[])
                .wanted
                .is_empty()
        );
        // Exactly at the keep distance: kept. A hair beyond: retired.
        assert_eq!(
            run(&[block(1, TAMRIEL, 1200.0)], cam, TAMRIEL, &[1], &[]).keep,
            vec![1]
        );
        assert_eq!(
            run(&[block(1, TAMRIEL, 1200.5)], cam, TAMRIEL, &[1], &[]).retire,
            vec![1]
        );

        // A camera jittering across the load edge, with the block loaded after it first appears,
        // never produces a retire and never asks for it twice.
        let mut resident: HashSet<u32> = HashSet::new();
        let blocks = [block(1, TAMRIEL, 1000.0)];
        for step in 0..40 {
            let wobble = if step % 2 == 0 { -30.0 } else { 150.0 };
            let got = plan(
                &blocks,
                [wobble, 50.0, 50.0],
                TAMRIEL,
                &tree(),
                &config(),
                &resident,
                &HashSet::new(),
            );
            assert!(got.retire.is_empty(), "step {step} retired");
            if resident.is_empty() {
                // x = -30 puts the box at 1030: not wanted; x = 150 puts it at 850: wanted.
                assert_eq!(got.wanted.is_empty(), step % 2 == 0, "step {step}");
            } else {
                assert!(got.wanted.is_empty());
            }
            resident.extend(got.wanted);
        }
        assert_eq!(resident, set(&[1]));
    }

    #[test]
    fn pinned_blocks_are_never_retired() {
        let blocks = [
            block(1, TAMRIEL, 9000.0), // far away
            block(2, SOLSTHEIM, 0.0),  // unrelated world
            block(3, TAMRIEL, 9000.0), // far away, not pinned
        ];
        let got = run(&blocks, [0.0; 3], TAMRIEL, &[1, 2, 3], &[1, 2]);
        assert_eq!(got.keep, vec![1, 2]);
        assert_eq!(got.retire, vec![3]);
        // Pinning a block that is not resident does not make it wanted or kept.
        let got = run(&blocks, [0.0; 3], TAMRIEL, &[], &[1, 2]);
        assert_eq!(got, Plan::default());
    }

    #[test]
    fn entering_a_child_worldspace_keeps_the_parents_blocks() {
        let blocks = [block(1, TAMRIEL, 0.0), block(2, TAMRIEL, 400.0)];
        // The player stands in Whiterun (child of Tamriel): the Tamriel blocks stay in scope.
        let got = run(&blocks, [0.0; 3], WHITERUN, &[1], &[]);
        assert_eq!(got.keep, vec![1]);
        assert_eq!(got.wanted, vec![2]);
        assert!(got.retire.is_empty());
        // Equality on the active id would have retired it.
        assert_ne!(blocks[0].worldspace, WHITERUN);
    }

    #[test]
    fn an_unrelated_worldspace_retires_everything_and_wants_its_own() {
        let blocks = [
            block(1, TAMRIEL, 0.0),
            block(2, TAMRIEL, 300.0),
            block(10, SOLSTHEIM, 0.0),
        ];
        let got = run(&blocks, [0.0; 3], SOLSTHEIM, &[1, 2], &[]);
        assert_eq!(got.retire, vec![1, 2]);
        assert!(got.keep.is_empty());
        assert_eq!(got.wanted, vec![10]);
    }

    #[test]
    fn a_parent_is_not_in_scope_of_its_children_blocks() {
        // Standing in Tamriel, blocks owned by the child world are out of scope.
        let blocks = [block(1, WHITERUN, 0.0)];
        let got = run(&blocks, [0.0; 3], TAMRIEL, &[], &[]);
        assert!(got.wanted.is_empty());
    }

    #[test]
    fn world_tree_walks_up_and_survives_cycles() {
        let mut worlds = WorldTree::new();
        worlds.set_parent(3, 2);
        worlds.set_parent(2, 1);
        assert_eq!(worlds.ancestors_or_self(3), vec![3, 2, 1]);
        assert_eq!(worlds.ancestors_or_self(1), vec![1]);
        assert_eq!(worlds.ancestors_or_self(99), vec![99]);

        worlds.set_parent(1, 3); // 3 -> 2 -> 1 -> 3
        assert_eq!(worlds.ancestors_or_self(3), vec![3, 2, 1]);
        worlds.set_parent(5, 5); // self parent
        assert_eq!(worlds.ancestors_or_self(5), vec![5]);

        // A cycle in the data never hangs the planner either.
        let got = plan(
            &[block(1, 1, 0.0)],
            [0.0; 3],
            3,
            &worlds,
            &config(),
            &set(&[]),
            &set(&[]),
        );
        assert_eq!(got.wanted, vec![1]);
    }

    #[test]
    fn every_resident_block_lands_in_exactly_one_of_keep_and_retire() {
        let blocks: Vec<BlockInfo> = (0..30)
            .map(|i| {
                block(
                    i,
                    if i % 3 == 0 { SOLSTHEIM } else { TAMRIEL },
                    i as f32 * 150.0,
                )
            })
            .collect();
        let resident: HashSet<u32> = (0..30).filter(|i| i % 2 == 0).collect();
        let pinned = set(&[4, 28]);
        let got = plan(
            &blocks,
            [0.0; 3],
            TAMRIEL,
            &tree(),
            &config(),
            &resident,
            &pinned,
        );
        let mut both: Vec<u32> = got.keep.iter().chain(&got.retire).copied().collect();
        both.sort_unstable();
        let mut expected: Vec<u32> = resident.iter().copied().collect();
        expected.sort_unstable();
        assert_eq!(both, expected);
        assert!(got.wanted.iter().all(|id| !resident.contains(id)));
        assert!(pinned.iter().all(|id| got.keep.contains(id)));
    }
}

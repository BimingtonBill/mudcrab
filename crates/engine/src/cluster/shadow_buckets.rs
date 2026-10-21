//! The plan for drawing a shadow view's clusters in buckets of equal triangle count
//! (`docs/research/cluster-performance-change-plan-2026-10-16.md`, section 1).
//!
//! Today the cull writes one indirect command per selected cluster, so a view's argument list is as
//! long as the scene has cluster slots. A bucket draws every selected cluster of one triangle count
//! with a single instanced command: `3 * triangles` vertices, one instance per selected cluster, the
//! first instance at the bucket's region of the visible list. The shadow vertex shader already reads
//! its cluster from `visible[instance_index]`, so it does not change.
//!
//! This module is the CPU side only: from what is resident, how many commands a draw group needs and
//! how large each bucket's region of the visible list must be. Every number derives from the meshes'
//! own cluster tables, none is a tuned constant, and the sums are checked: a plan that does not fit
//! 32-bit indices is an error, never a wrapped number.

use std::collections::BTreeMap;

/// How many clusters of each triangle count a mesh has, over all of its clusters (a view can select
/// any of them).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TriangleHistogram {
    /// `(triangle count, clusters)`, ascending by triangle count, each count once, no zero entries.
    pub buckets: Vec<(u32, u32)>,
}

impl TriangleHistogram {
    /// Builds the histogram from each cluster's triangle count.
    pub fn from_triangle_counts(counts: impl IntoIterator<Item = u32>) -> Self {
        let mut map: BTreeMap<u32, u32> = BTreeMap::new();
        for count in counts {
            *map.entry(count).or_insert(0) += 1;
        }
        Self {
            buckets: map.into_iter().collect(),
        }
    }

    /// All clusters of the mesh.
    pub fn clusters(&self) -> u64 {
        self.buckets
            .iter()
            .map(|&(_, clusters)| u64::from(clusters))
            .sum()
    }
}

/// One command of a draw group: every selected cluster with `triangles` triangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bucket {
    pub triangles: u32,
    /// The most clusters of this size the group's instances can select: the bucket's share of the
    /// visible list.
    pub capacity: u32,
    /// Where the bucket's region starts in the view's visible list, in slots.
    pub first_slot: u32,
}

/// The commands of one draw group, in ascending triangle count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupBuckets {
    /// Index of the group's first command in the view's argument list.
    pub first_command: u32,
    pub buckets: Vec<Bucket>,
}

/// The commands and visible-list regions of every draw group of a view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BucketPlan {
    pub groups: Vec<GroupBuckets>,
    /// Commands over all groups: what the argument list holds.
    pub commands: u32,
    /// Slots over all groups: what the visible list holds. Equals the sum of every group's
    /// clusters over its instances, the same as the per-cluster layout needs.
    pub slots: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// A count or a sum does not fit the 32-bit indices the shaders use.
    Overflow(&'static str),
    /// A group names a mesh the histogram table does not have.
    UnknownMesh(u32),
    /// A publicly constructed plan violates the bucket layout contract.
    InvalidPlan(&'static str),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow(what) => write!(f, "{what} do not fit 32-bit indices"),
            Self::UnknownMesh(mesh) => write!(f, "draw group names mesh {mesh} with no histogram"),
            Self::InvalidPlan(reason) => write!(f, "invalid shadow bucket plan: {reason}"),
        }
    }
}

impl std::error::Error for PlanError {}

/// Plans the buckets of a view. `histograms[mesh]` is the mesh's cluster histogram; each group is
/// the list of `(mesh, instances)` its batches draw.
///
/// A group gets one bucket per distinct triangle count with nonzero capacity, computed as
/// `sum(instances * clusters of that size)`, so every possible cluster belongs to exactly one bucket
/// and the regions tile the visible list with no gap and no overlap.
pub fn plan_buckets(
    histograms: &[TriangleHistogram],
    groups: &[Vec<(u32, u32)>],
) -> Result<BucketPlan, PlanError> {
    let mut plan = BucketPlan::default();
    let mut next_slot = 0u64;
    let mut next_command = 0u64;
    for group in groups {
        let mut capacity: BTreeMap<u32, u64> = BTreeMap::new();
        for &(mesh, instances) in group {
            let histogram = histograms
                .get(mesh as usize)
                .ok_or(PlanError::UnknownMesh(mesh))?;
            for &(triangles, clusters) in &histogram.buckets {
                let share = u64::from(instances)
                    .checked_mul(u64::from(clusters))
                    .ok_or(PlanError::Overflow("cluster slots"))?;
                let entry = capacity.entry(triangles).or_insert(0);
                *entry = entry
                    .checked_add(share)
                    .ok_or(PlanError::Overflow("cluster slots"))?;
            }
        }
        let first_command =
            u32::try_from(next_command).map_err(|_| PlanError::Overflow("draw commands"))?;
        let mut buckets = Vec::with_capacity(capacity.len());
        for (triangles, slots) in capacity {
            if slots == 0 {
                continue;
            }
            let first_slot =
                u32::try_from(next_slot).map_err(|_| PlanError::Overflow("cluster slots"))?;
            let size = u32::try_from(slots).map_err(|_| PlanError::Overflow("cluster slots"))?;
            next_slot = next_slot
                .checked_add(slots)
                .ok_or(PlanError::Overflow("cluster slots"))?;
            buckets.push(Bucket {
                triangles,
                capacity: size,
                first_slot,
            });
        }
        next_command = next_command
            .checked_add(
                u64::try_from(buckets.len()).map_err(|_| PlanError::Overflow("draw commands"))?,
            )
            .ok_or(PlanError::Overflow("draw commands"))?;
        plan.groups.push(GroupBuckets {
            first_command,
            buckets,
        });
    }
    plan.commands =
        u32::try_from(next_command).map_err(|_| PlanError::Overflow("draw commands"))?;
    plan.slots = u32::try_from(next_slot).map_err(|_| PlanError::Overflow("cluster slots"))?;
    Ok(plan)
}

/// GPU arguments and sparse triangle-count lookup for a validated bucket plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuBucketTables {
    /// Per absolute command: [triangles * 3, 0 instances, 0 first vertex, first slot], LE.
    pub args_template: Vec<u8>,
    /// First two words per group: [absolute word offset, entry count]. Each group's
    /// following sorted entries are [triangle count, absolute command index].
    pub lookup_words: Vec<u32>,
}

/// Builds the shadow cull's immutable lookup and the argument template reset each frame.
/// Publicly constructed plans are checked before allocating the output tables. Empty
/// groups still have headers; no zero-capacity bucket may consume a command or slot.
/// These are format/index checks, not an adapter storage-size approval.
pub fn gpu_tables(plan: &BucketPlan) -> Result<GpuBucketTables, PlanError> {
    let group_count =
        u32::try_from(plan.groups.len()).map_err(|_| PlanError::Overflow("lookup group count"))?;
    let header_words = group_count
        .checked_mul(2)
        .ok_or(PlanError::Overflow("lookup headers"))?;
    let mut commands = 0u32;
    let mut slots = 0u32;
    for group in &plan.groups {
        if group.first_command != commands {
            return Err(PlanError::InvalidPlan("noncontiguous group commands"));
        }
        let mut previous = None;
        for bucket in &group.buckets {
            if bucket.capacity == 0 {
                return Err(PlanError::InvalidPlan("zero-capacity bucket"));
            }
            if previous.is_some_and(|triangles| triangles >= bucket.triangles) {
                return Err(PlanError::InvalidPlan(
                    "triangle counts are not sorted and unique",
                ));
            }
            previous = Some(bucket.triangles);
            if bucket.first_slot != slots {
                return Err(PlanError::InvalidPlan("noncontiguous bucket slots"));
            }
            bucket
                .triangles
                .checked_mul(3)
                .ok_or(PlanError::Overflow("vertex count"))?;
            slots = slots
                .checked_add(bucket.capacity)
                .ok_or(PlanError::Overflow("cluster slots"))?;
            commands = commands
                .checked_add(1)
                .ok_or(PlanError::Overflow("draw commands"))?;
        }
    }
    if commands != plan.commands || slots != plan.slots {
        return Err(PlanError::InvalidPlan(
            "command or slot totals do not match buckets",
        ));
    }
    let lookup_words = header_words
        .checked_add(
            commands
                .checked_mul(2)
                .ok_or(PlanError::Overflow("lookup entries"))?,
        )
        .ok_or(PlanError::Overflow("lookup word offsets"))?;
    let args_bytes = usize::try_from(commands)
        .ok()
        .and_then(|count| count.checked_mul(16))
        .ok_or(PlanError::Overflow("argument bytes"))?;
    let lookup_length =
        usize::try_from(lookup_words).map_err(|_| PlanError::Overflow("lookup length"))?;
    lookup_length
        .checked_mul(std::mem::size_of::<u32>())
        .ok_or(PlanError::Overflow("lookup bytes"))?;
    let mut tables = GpuBucketTables {
        args_template: Vec::with_capacity(args_bytes),
        lookup_words: Vec::with_capacity(lookup_length),
    };
    let mut next_word = header_words;
    for group in &plan.groups {
        let count = u32::try_from(group.buckets.len())
            .map_err(|_| PlanError::Overflow("group commands"))?;
        tables.lookup_words.extend_from_slice(&[next_word, count]);
        next_word = next_word
            .checked_add(
                count
                    .checked_mul(2)
                    .ok_or(PlanError::Overflow("group lookup entries"))?,
            )
            .ok_or(PlanError::Overflow("lookup word offsets"))?;
    }
    for group in &plan.groups {
        for (index, bucket) in group.buckets.iter().enumerate() {
            let command = group
                .first_command
                .checked_add(
                    u32::try_from(index).map_err(|_| PlanError::Overflow("group command index"))?,
                )
                .ok_or(PlanError::Overflow("absolute command index"))?;
            let vertices = bucket
                .triangles
                .checked_mul(3)
                .ok_or(PlanError::Overflow("vertex count"))?;
            for word in [vertices, 0, 0, bucket.first_slot] {
                tables.args_template.extend_from_slice(&word.to_le_bytes());
            }
            tables
                .lookup_words
                .extend_from_slice(&[bucket.triangles, command]);
        }
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram(counts: &[u32]) -> TriangleHistogram {
        TriangleHistogram::from_triangle_counts(counts.iter().copied())
    }

    #[test]
    fn a_histogram_counts_each_triangle_count_once_in_ascending_order() {
        let h = histogram(&[128, 64, 128, 10, 64, 128]);
        assert_eq!(h.buckets, vec![(10, 1), (64, 2), (128, 3)]);
        assert_eq!(h.clusters(), 6);
        assert!(histogram(&[]).buckets.is_empty());
    }

    #[test]
    fn capacity_is_instances_times_the_clusters_of_that_size() {
        let meshes = [histogram(&[128, 128, 64]), histogram(&[128, 12])];
        let plan = plan_buckets(&meshes, &[vec![(0, 3), (1, 2)]]).unwrap();
        let group = &plan.groups[0];
        assert_eq!(
            group.buckets,
            vec![
                Bucket {
                    triangles: 12,
                    capacity: 2,
                    first_slot: 0
                },
                Bucket {
                    triangles: 64,
                    capacity: 3,
                    first_slot: 2
                },
                // 3 instances * 2 clusters of mesh 0 + 2 instances * 1 cluster of mesh 1.
                Bucket {
                    triangles: 128,
                    capacity: 8,
                    first_slot: 5
                },
            ]
        );
        assert_eq!(plan.commands, 3);
    }

    #[test]
    fn regions_tile_the_visible_list_across_groups_and_match_the_per_cluster_total() {
        let meshes = [histogram(&[128, 128, 64]), histogram(&[128, 12, 12])];
        let groups = vec![vec![(0, 5)], vec![(1, 4), (0, 1)], vec![]];
        let plan = plan_buckets(&meshes, &groups).unwrap();
        // The per-cluster layout needs one slot per cluster per instance.
        let per_cluster: u64 = groups
            .iter()
            .flatten()
            .map(|&(mesh, instances)| u64::from(instances) * meshes[mesh as usize].clusters())
            .sum();
        assert_eq!(u64::from(plan.slots), per_cluster);
        let mut end = 0;
        let mut command = 0;
        for group in &plan.groups {
            assert_eq!(group.first_command, command);
            for bucket in &group.buckets {
                assert_eq!(bucket.first_slot, end, "no gap and no overlap");
                end += bucket.capacity;
                command += 1;
            }
        }
        assert_eq!(end, plan.slots);
        assert_eq!(command, plan.commands);
        assert!(plan.groups[2].buckets.is_empty());
    }

    #[test]
    fn commands_scale_with_distinct_sizes_not_with_slots() {
        let mesh = TriangleHistogram::from_triangle_counts((0..1000).map(|i| 1 + i % 3));
        let plan = plan_buckets(&[mesh], &[vec![(0, 100_000)]]).unwrap();
        assert_eq!(plan.commands, 3);
        assert_eq!(plan.slots, 100_000_000);
    }

    #[test]
    fn a_zero_triangle_bucket_with_live_instances_keeps_its_slots_and_command() {
        let plan = plan_buckets(&[histogram(&[0, 0, 7])], &[vec![(0, 3)]]).unwrap();
        assert_eq!((plan.commands, plan.slots), (2, 9));
        assert_eq!(
            plan.groups[0].buckets[0],
            Bucket {
                triangles: 0,
                capacity: 6,
                first_slot: 0,
            }
        );
        let tables = gpu_tables(&plan).unwrap();
        let args: Vec<_> = tables
            .args_template
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        assert_eq!(args, [0, 0, 0, 0, 21, 0, 0, 6]);
        assert_eq!(tables.lookup_words, [2, 2, 0, 0, 7, 1]);
    }

    #[test]
    fn an_unknown_mesh_is_an_error() {
        let result = plan_buckets(&[histogram(&[1])], &[vec![(1, 1)]]);
        assert_eq!(result, Err(PlanError::UnknownMesh(1)));
    }

    #[test]
    fn sums_past_32_bits_are_errors_not_wrapped_numbers() {
        let mesh = TriangleHistogram::from_triangle_counts([128; 1000]);
        let result = plan_buckets(&[mesh], &[vec![(0, u32::MAX)]]);
        assert_eq!(result, Err(PlanError::Overflow("cluster slots")));
        // Two groups that each fit but together do not.
        let mesh = TriangleHistogram::from_triangle_counts([128; 2]);
        let result = plan_buckets(
            &[mesh],
            &[vec![(0, 2_000_000_000)], vec![(0, 2_000_000_000)]],
        );
        assert_eq!(result, Err(PlanError::Overflow("cluster slots")));
    }
    #[test]
    fn gpu_tables_encode_exact_arguments_and_sparse_absolute_lookup() {
        let plan = plan_buckets(
            &[histogram(&[257, 17, 17]), histogram(&[5])],
            &[vec![(0, 2)], vec![], vec![(1, 3)]],
        )
        .unwrap();
        let tables = gpu_tables(&plan).unwrap();
        let words: Vec<_> = tables
            .args_template
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        assert_eq!(words, vec![51, 0, 0, 0, 771, 0, 0, 4, 15, 0, 0, 6]);
        assert_eq!(
            tables.args_template[16..20],
            [3, 3, 0, 0],
            "771 is encoded little-endian"
        );
        assert_eq!(
            tables.lookup_words,
            vec![6, 2, 10, 0, 10, 1, 17, 0, 257, 1, 5, 2]
        );
        for (group_id, group) in plan.groups.iter().enumerate() {
            let header = &tables.lookup_words[group_id * 2..group_id * 2 + 2];
            let entries =
                &tables.lookup_words[header[0] as usize..(header[0] + header[1] * 2) as usize];
            assert_eq!(entries.len(), group.buckets.len() * 2);
            for (entry, bucket) in entries.as_chunks::<2>().0.iter().zip(&group.buckets) {
                assert_eq!(entry[0], bucket.triangles);
                let args = &words[entry[1] as usize * 4..entry[1] as usize * 4 + 4];
                assert_eq!(args[3], bucket.first_slot);
                assert_eq!(args[0] / 3, entry[0]);
                assert_eq!(&args[1..3], &[0, 0]);
            }
        }
    }

    #[test]
    fn sparse_lookup_stays_bounded_for_many_single_cluster_groups() {
        let groups = vec![vec![(0, 1)]; 10_000];
        let plan = plan_buckets(&[histogram(&[513])], &groups).unwrap();
        let tables = gpu_tables(&plan).unwrap();
        assert_eq!(plan.commands, 10_000);
        assert_eq!(tables.lookup_words.len() * 4, plan.slots as usize * 16);
        assert_eq!(&tables.lookup_words[..2], &[20_000, 1]);
        assert_eq!(
            &tables.lookup_words[tables.lookup_words.len() - 2..],
            &[513, 9_999]
        );
    }

    #[test]
    fn zero_instance_groups_allocate_headers_but_no_buckets_or_arguments() {
        let meshes = [
            histogram(&[17, 257]),
            TriangleHistogram {
                buckets: vec![(5, 0)],
            },
        ];
        let plan = plan_buckets(&meshes, &[vec![(0, 0)], vec![], vec![(1, 4)]]).unwrap();
        assert_eq!(plan.commands, 0);
        assert_eq!(plan.slots, 0);
        assert!(plan.groups.iter().all(|group| group.buckets.is_empty()));
        let tables = gpu_tables(&plan).unwrap();
        assert!(tables.args_template.is_empty());
        assert_eq!(tables.lookup_words, vec![6, 0, 6, 0, 6, 0]);
        assert_eq!(
            gpu_tables(&BucketPlan::default()).unwrap(),
            GpuBucketTables::default()
        );
        let mixed = plan_buckets(&meshes, &[vec![(0, 0), (0, 1)]]).unwrap();
        assert_eq!(mixed.commands, 2);
        assert_eq!(mixed.slots, 2);
    }

    #[test]
    fn publicly_constructed_malformed_plans_are_rejected_before_table_allocation() {
        let valid = plan_buckets(&[histogram(&[17, 257])], &[vec![(0, 1)], vec![]]).unwrap();
        for change in 0..8 {
            let mut plan = valid.clone();
            match change {
                0 => plan.groups[0].first_command = 1,
                1 => plan.groups[0].buckets[0].capacity = 0,
                2 => plan.groups[0].buckets[1].triangles = 17,
                3 => plan.groups[0].buckets[0].triangles = 300,
                4 => plan.groups[0].buckets[1].first_slot = 0,
                5 => plan.commands = u32::MAX,
                6 => plan.slots = u32::MAX,
                7 => plan.groups[1].first_command = 0,
                _ => unreachable!(),
            }
            assert!(
                matches!(gpu_tables(&plan), Err(PlanError::InvalidPlan(_))),
                "malformed case {change}"
            );
        }
    }

    #[test]
    fn gpu_tables_reject_vertex_and_slot_overflow_without_large_allocations() {
        let mut plan = BucketPlan {
            groups: vec![GroupBuckets {
                first_command: 0,
                buckets: vec![Bucket {
                    triangles: u32::MAX / 3 + 1,
                    capacity: 1,
                    first_slot: 0,
                }],
            }],
            commands: 1,
            slots: 1,
        };
        assert_eq!(gpu_tables(&plan), Err(PlanError::Overflow("vertex count")));
        plan.groups[0].buckets[0].triangles = u32::MAX / 3;
        let boundary = gpu_tables(&plan).unwrap();
        assert_eq!(&boundary.args_template[..4], &u32::MAX.to_le_bytes());
        plan.groups[0].buckets = vec![
            Bucket {
                triangles: 1,
                capacity: u32::MAX,
                first_slot: 0,
            },
            Bucket {
                triangles: 2,
                capacity: 1,
                first_slot: u32::MAX,
            },
        ];
        plan.commands = 2;
        plan.slots = u32::MAX;
        assert_eq!(gpu_tables(&plan), Err(PlanError::Overflow("cluster slots")));
    }
}

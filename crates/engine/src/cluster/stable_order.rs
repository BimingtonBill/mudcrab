//! Immutable source-order metadata, rebuilt only when a view's draw groups change.
//!
//! No visibility is decided here. The GPU prefixes current selected counts in this order.
//! Words: header (groups, tiles, ordered-instance-count, large-list-base), four words per group
//! (first tile, tile count, argument base, reserved), four per tile (first instance word,
//! instance count, group, reserved), then source instance indices, then a count and
//! indices of instances with more than 64 clusters (cooperative scatter work).

pub(crate) const TILE_SIZE: usize = 256;

pub(crate) fn source_order(
    mut instances: Vec<Vec<u32>>,
    bases: &[u32],
    large_instances: &[u32],
    ordered_instances: u32,
) -> Vec<u32> {
    assert_eq!(instances.len(), bases.len());
    let tile_count: usize = instances
        .iter()
        .map(|ids| ids.len().div_ceil(TILE_SIZE))
        .sum();
    let instance_base = 4 + instances.len() * 4 + tile_count * 4;
    let mut words = vec![
        instances.len() as u32,
        tile_count as u32,
        ordered_instances,
        0,
    ];
    let mut first_tile = 0;
    for (ids, &base) in instances.iter().zip(bases) {
        let tiles = ids.len().div_ceil(TILE_SIZE) as u32;
        words.extend([first_tile, tiles, base, 0]);
        first_tile += tiles;
    }
    let mut next_instance = instance_base as u32;
    for (group, ids) in instances.iter_mut().enumerate() {
        ids.sort_unstable();
        for tile in ids.chunks(TILE_SIZE) {
            words.extend([next_instance, tile.len() as u32, group as u32, 0]);
            next_instance += tile.len() as u32;
        }
    }
    for ids in instances {
        words.extend(ids);
    }
    words[3] = words.len() as u32;
    words.push(large_instances.len() as u32);
    words.extend_from_slice(large_instances);
    words
}

/// Includes the partial tile of every group, even if each group has just one instance.
pub(crate) fn order_capacity(instances: u32, groups: usize) -> u64 {
    let tiles = instances.div_ceil(TILE_SIZE as u32) as u64 + groups as u64;
    (5 + groups as u64 * 4 + tiles * 4 + u64::from(instances) * 2) * 4
}

pub(crate) fn prefix_capacity(instances: u32, groups: usize) -> u64 {
    u64::from(instances.div_ceil(64))
        .max(u64::from(instances.div_ceil(TILE_SIZE as u32)) + groups as u64)
        .max(1)
        .saturating_add(1)
        * 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_preserve_source_order_across_groups_and_boundaries() {
        let ids: Vec<_> = (0..513).rev().collect();
        let words = source_order(
            vec![ids, vec![], vec![900, 700]],
            &[0, 600, 600],
            &[900],
            901,
        );
        assert_eq!(
            &words[..16],
            &[3, 4, 901, 547, 0, 3, 0, 0, 3, 0, 600, 0, 3, 1, 600, 0]
        );
        assert_eq!(
            &words[16..32],
            &[32, 256, 0, 0, 288, 256, 0, 0, 544, 1, 0, 0, 545, 2, 2, 0]
        );
        assert_eq!(&words[32..545], &(0..513).collect::<Vec<_>>());
        assert_eq!(&words[545..], &[700, 900, 1, 900]);
        assert!(words.len() as u64 * 4 <= order_capacity(515, 3));
    }

    #[test]
    fn empty_and_singleton_groups_fit_the_capacity_bound() {
        for n in [0, 1, 63, 64, 255, 256, 257, 65_536] {
            let words = source_order(
                (0..n).map(|id| vec![id]).collect(),
                &vec![0; n as usize],
                &(0..n).collect::<Vec<_>>(),
                n,
            );
            assert!(words.len() as u64 * 4 <= order_capacity(n, n as usize));
            assert!(prefix_capacity(n, n as usize) >= n as u64 * 8);
        }
    }
}

//! First-fit range allocator over a byte (or element) space, for the growable cluster pools.
//!
//! The allocator only does the bookkeeping; the caller owns the buffer. When an allocation does
//! not fit, the caller computes a new capacity with [`growth_capacity`], creates and fills the
//! bigger buffer, copies `[0, live_extent())` (or `[0, high_water())`) and calls
//! [`RangeAllocator::grow`], after which the same allocations are still valid.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

/// Why a [`RangeAllocator`] call was refused. These are caller bugs or protocol slips; the
/// allocator never panics on them and its state is unchanged after one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AllocError {
    /// The range is not exactly one live allocation: it was never allocated, was already freed
    /// (double free), or only overlaps one or several allocations without being one.
    NotAllocated { start: u64, end: u64 },
    /// `grow` was asked for a capacity below the current one. Pools only grow.
    CannotShrink { capacity: u64, requested: u64 },
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAllocated { start, end } => write!(
                f,
                "range {start}..{end} is not exactly one live allocation (double free or overlap)"
            ),
            Self::CannotShrink {
                capacity,
                requested,
            } => write!(
                f,
                "cannot shrink an allocator from capacity {capacity} to {requested}"
            ),
        }
    }
}

impl std::error::Error for AllocError {}

/// Rounds `value` up to a multiple of `alignment` (at least 1), or `None` on overflow.
fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let alignment = alignment.max(1);
    let rem = value % alignment;
    if rem == 0 {
        Some(value)
    } else {
        value.checked_add(alignment - rem)
    }
}

/// Rounds up, or down when rounding up would overflow.
fn align_capacity(value: u64, alignment: u64) -> u64 {
    align_up(value, alignment).unwrap_or(value - value % alignment.max(1))
}

/// First-fit free list over `[0, capacity)` with coalescing of adjacent free ranges.
///
/// Every allocation starts at a multiple of the alignment and has a length that is a multiple of
/// it, so the whole capacity (also rounded up to the alignment) stays usable.
#[derive(Clone, Debug)]
pub struct RangeAllocator {
    capacity: u64,
    alignment: u64,
    /// Free ranges, start -> length. Never empty, never overlapping, never adjacent.
    free: BTreeMap<u64, u64>,
    /// Live allocations, start -> length (already rounded).
    live: BTreeMap<u64, u64>,
    used: u64,
    high_water: u64,
}

impl RangeAllocator {
    /// An empty allocator over `capacity`, rounded up to a multiple of `alignment` (0 counts as
    /// 1). Create the buffer with [`capacity`](Self::capacity), not the number passed here.
    pub fn new(capacity: u64, alignment: u64) -> Self {
        let alignment = alignment.max(1);
        let capacity = align_capacity(capacity, alignment);
        let mut free = BTreeMap::new();
        if capacity > 0 {
            free.insert(0, capacity);
        }
        Self {
            capacity,
            alignment,
            free,
            live: BTreeMap::new(),
            used: 0,
            high_water: 0,
        }
    }

    /// Reserves `size` units rounded up to the alignment, at the lowest address that fits.
    /// `None` when `size` is zero (there is nothing to reserve; skip empty payloads) or when no
    /// free range is big enough (grow, then retry; see [`largest_free`](Self::largest_free)).
    pub fn allocate(&mut self, size: u64) -> Option<Range<u64>> {
        if size == 0 {
            return None;
        }
        let rounded = align_up(size, self.alignment)?;
        let (start, len) = self
            .free
            .iter()
            .find(|(_, len)| **len >= rounded)
            .map(|(start, len)| (*start, *len))?;
        self.free.remove(&start);
        if len > rounded {
            self.free.insert(start + rounded, len - rounded);
        }
        self.live.insert(start, rounded);
        self.used += rounded;
        self.high_water = self.high_water.max(start + rounded);
        Some(start..start + rounded)
    }

    /// Returns an allocation, merging it with free neighbours. `range` must be exactly a range
    /// returned by [`allocate`](Self::allocate) and not yet freed; anything else is an error and
    /// changes nothing.
    pub fn free(&mut self, range: Range<u64>) -> Result<(), AllocError> {
        let not_allocated = AllocError::NotAllocated {
            start: range.start,
            end: range.end,
        };
        let Some(len) = range.end.checked_sub(range.start) else {
            return Err(not_allocated);
        };
        if self.live.get(&range.start) != Some(&len) {
            return Err(not_allocated);
        }
        self.live.remove(&range.start);
        self.used -= len;

        let mut start = range.start;
        let mut total = len;
        if let Some((&prev_start, &prev_len)) = self.free.range(..start).next_back()
            && prev_start + prev_len == start
        {
            self.free.remove(&prev_start);
            start = prev_start;
            total += prev_len;
        }
        if let Some(next_len) = self.free.remove(&range.end) {
            total += next_len;
        }
        self.free.insert(start, total);
        Ok(())
    }

    /// Raises the capacity (rounded up to the alignment) and returns the new capacity. Live
    /// allocations keep their ranges. Equal capacity is a no-op; a smaller one is an error.
    pub fn grow(&mut self, new_capacity: u64) -> Result<u64, AllocError> {
        if new_capacity < self.capacity {
            return Err(AllocError::CannotShrink {
                capacity: self.capacity,
                requested: new_capacity,
            });
        }
        let new_capacity = align_capacity(new_capacity, self.alignment);
        if new_capacity == self.capacity {
            return Ok(self.capacity);
        }
        let old = self.capacity;
        let added = new_capacity - old;
        match self.free.range(..old).next_back() {
            Some((&start, &len)) if start + len == old => {
                self.free.insert(start, len + added);
            }
            _ => {
                self.free.insert(old, added);
            }
        }
        self.capacity = new_capacity;
        Ok(new_capacity)
    }

    /// Total size of the space, a multiple of the alignment.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Bytes in live allocations (rounded sizes).
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Highest end of any range ever allocated; only rises. `[0, high_water)` holds everything
    /// that was ever written.
    pub fn high_water(&self) -> u64 {
        self.high_water
    }

    /// End of the highest live allocation (0 when nothing is live). This is the extent a growth
    /// copy needs; compare it with [`used`](Self::used) to measure fragmentation.
    pub fn live_extent(&self) -> u64 {
        self.live
            .iter()
            .next_back()
            .map_or(0, |(start, len)| start + len)
    }

    /// Bytes not in a live allocation (`capacity - used`), spread over any number of ranges.
    pub fn free_bytes(&self) -> u64 {
        self.capacity - self.used
    }

    /// The longest single free range, i.e. the biggest request that would fit now.
    pub fn largest_free(&self) -> u64 {
        self.free.values().copied().max().unwrap_or(0)
    }

    /// Number of live allocations.
    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    /// Number of separate free ranges (1 after everything is freed).
    pub fn free_range_count(&self) -> usize {
        self.free.len()
    }

    #[cfg(test)]
    fn check_invariants(&self) {
        let mut cursor = 0u64;
        let mut free_total = 0u64;
        let mut live_total = 0u64;
        let mut all: Vec<(u64, u64, bool)> = self
            .free
            .iter()
            .map(|(s, l)| (*s, *l, true))
            .chain(self.live.iter().map(|(s, l)| (*s, *l, false)))
            .collect();
        all.sort();
        let mut prev_free = false;
        for (start, len, is_free) in all {
            assert!(len > 0, "empty range at {start}");
            assert_eq!(start, cursor, "gap or overlap at {start}");
            assert_eq!(start % self.alignment, 0, "unaligned start {start}");
            assert_eq!(len % self.alignment, 0, "unaligned length {len}");
            assert!(!(is_free && prev_free), "adjacent free ranges at {start}");
            if is_free {
                free_total += len;
            } else {
                live_total += len;
            }
            prev_free = is_free;
            cursor = start + len;
        }
        assert_eq!(cursor, self.capacity, "ranges do not tile the capacity");
        assert_eq!(live_total, self.used);
        assert_eq!(free_total, self.free_bytes());
        assert!(self.high_water >= self.live_extent());
    }
}

/// The capacity to grow a pool to: at least double `current`, and at least `needed_total` (the
/// bytes that must fit in all: what is used plus the request, rounded as the caller sees fit).
/// Doubling keeps the number of growth copies logarithmic in the final size.
pub fn growth_capacity(current: u64, needed_total: u64) -> u64 {
    current.saturating_mul(2).max(needed_total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xorshift64*, deterministic, no dependencies.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn overlaps(a: &Range<u64>, b: &Range<u64>) -> bool {
        a.start < b.end && b.start < a.end
    }

    #[test]
    fn first_fit_takes_the_lowest_gap_and_rounds_up() {
        let mut alloc = RangeAllocator::new(1024, 256);
        let a = alloc.allocate(1).unwrap();
        let b = alloc.allocate(300).unwrap();
        let c = alloc.allocate(256).unwrap();
        assert_eq!((a.clone(), b, c.clone()), (0..256, 256..768, 768..1024));
        alloc.free(a).unwrap();
        alloc.free(c).unwrap();
        // The gap at 0 is first even though the tail is also free.
        assert_eq!(alloc.allocate(10), Some(0..256));
        assert_eq!(alloc.used(), 256 + 512);
        alloc.check_invariants();
    }

    #[test]
    fn random_sequences_never_overlap_and_bytes_balance() {
        for seed in 1..=8u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let mut alloc = RangeAllocator::new(4096, 16);
            let mut live: Vec<Range<u64>> = Vec::new();
            for _ in 0..3000 {
                if live.is_empty() || rng.below(100) < 55 {
                    let size = 1 + rng.below(300);
                    let range = match alloc.allocate(size) {
                        Some(range) => range,
                        None => {
                            let needed = alloc.used() + align_up(size, 16).unwrap();
                            alloc
                                .grow(growth_capacity(alloc.capacity(), needed))
                                .unwrap();
                            alloc.allocate(size).unwrap_or_else(|| {
                                panic!(
                                    "request {size} still does not fit after growth to {} \
                                     (largest free {})",
                                    alloc.capacity(),
                                    alloc.largest_free()
                                )
                            })
                        }
                    };
                    assert!(range.end - range.start >= size);
                    assert_eq!(range.start % 16, 0);
                    for other in &live {
                        assert!(!overlaps(&range, other), "{range:?} overlaps {other:?}");
                    }
                    live.push(range);
                } else {
                    let index = rng.below(live.len() as u64) as usize;
                    let range = live.swap_remove(index);
                    alloc.free(range).unwrap();
                }
                let sum: u64 = live.iter().map(|r| r.end - r.start).sum();
                assert_eq!(alloc.used(), sum);
                assert_eq!(alloc.used() + alloc.free_bytes(), alloc.capacity());
                assert_eq!(alloc.live_count(), live.len());
                alloc.check_invariants();
            }
            for range in live.drain(..) {
                alloc.free(range).unwrap();
            }
            assert_eq!(alloc.used(), 0);
            assert_eq!(alloc.free_range_count(), 1);
            assert_eq!(alloc.largest_free(), alloc.capacity());
        }
    }

    #[test]
    fn freeing_everything_coalesces_to_one_range_in_any_order() {
        for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1]] {
            let mut alloc = RangeAllocator::new(1024, 16);
            let ranges: Vec<_> = (0..4).map(|_| alloc.allocate(256).unwrap()).collect();
            assert!(alloc.allocate(1).is_none());
            for index in order {
                alloc.free(ranges[index].clone()).unwrap();
                alloc.check_invariants();
            }
            assert_eq!(alloc.free_range_count(), 1);
            assert_eq!(alloc.largest_free(), 1024);
            assert_eq!(alloc.allocate(1024), Some(0..1024));
        }
    }

    #[test]
    fn double_free_and_foreign_ranges_are_errors_and_change_nothing() {
        let mut alloc = RangeAllocator::new(1024, 16);
        let a = alloc.allocate(64).unwrap();
        let b = alloc.allocate(64).unwrap();
        // Never allocated, partly overlapping, spanning two allocations, reversed, out of space.
        assert!(alloc.free(512..576).is_err());
        assert!(alloc.free(a.start + 16..a.end).is_err());
        assert!(alloc.free(a.start..a.end - 16).is_err());
        assert!(alloc.free(a.start..b.end).is_err());
        let reversed = Range {
            start: 64,
            end: 0u64,
        };
        assert!(alloc.free(reversed).is_err());
        assert!(alloc.free(u64::MAX - 8..u64::MAX).is_err());
        assert_eq!(alloc.used(), 128);
        alloc.check_invariants();
        alloc.free(a.clone()).unwrap();
        assert_eq!(
            alloc.free(a.clone()),
            Err(AllocError::NotAllocated {
                start: a.start,
                end: a.end
            })
        );
        alloc.free(b).unwrap();
        assert_eq!(alloc.used(), 0);
        alloc.check_invariants();
    }

    #[test]
    fn growth_keeps_live_ranges_valid_and_merges_the_tail() {
        let mut alloc = RangeAllocator::new(512, 64);
        let a = alloc.allocate(256).unwrap();
        let b = alloc.allocate(256).unwrap();
        assert!(alloc.allocate(64).is_none());
        assert_eq!(alloc.grow(growth_capacity(512, 576)), Ok(1024));
        assert_eq!(alloc.capacity(), 1024);
        // The old ranges are still the live ones.
        assert_eq!(alloc.high_water(), 512);
        assert_eq!(alloc.live_extent(), 512);
        let c = alloc.allocate(64).unwrap();
        assert_eq!(c, 512..576);
        alloc.free(a).unwrap();
        alloc.free(b).unwrap();
        alloc.free(c).unwrap();
        assert_eq!(alloc.free_range_count(), 1);
        assert_eq!(alloc.largest_free(), 1024);

        // A free range ending at the old capacity merges with the new tail.
        let mut alloc = RangeAllocator::new(256, 64);
        let x = alloc.allocate(128).unwrap();
        alloc.grow(512).unwrap();
        assert_eq!(alloc.free_range_count(), 1);
        assert_eq!(alloc.largest_free(), 384);
        alloc.free(x).unwrap();
        assert_eq!(alloc.largest_free(), 512);
        alloc.check_invariants();
    }

    #[test]
    fn grow_never_shrinks_and_rounds_up() {
        let mut alloc = RangeAllocator::new(100, 64);
        assert_eq!(alloc.capacity(), 128);
        assert_eq!(alloc.grow(128), Ok(128));
        assert_eq!(
            alloc.grow(64),
            Err(AllocError::CannotShrink {
                capacity: 128,
                requested: 64
            })
        );
        assert_eq!(alloc.grow(129), Ok(192));
        alloc.check_invariants();
    }

    #[test]
    fn growth_capacity_at_least_doubles_and_covers_the_need() {
        assert_eq!(growth_capacity(1024, 1100), 2048);
        assert_eq!(growth_capacity(1024, 10_000), 10_000);
        assert_eq!(growth_capacity(0, 4096), 4096);
        assert_eq!(growth_capacity(0, 0), 0);
        assert_eq!(growth_capacity(u64::MAX, 5), u64::MAX);
        for current in [1u64, 7, 256, 1 << 20] {
            for needed in [0u64, 1, current, current + 1, current * 3] {
                let grown = growth_capacity(current, needed);
                assert!(grown >= current * 2 && grown >= needed);
            }
        }
    }

    #[test]
    fn zero_size_and_oversize_requests_return_none_and_change_nothing() {
        let mut alloc = RangeAllocator::new(1000, 256);
        assert_eq!(alloc.capacity(), 1024);
        assert!(alloc.allocate(0).is_none());
        assert!(alloc.allocate(1025).is_none());
        assert!(alloc.allocate(u64::MAX).is_none());
        assert_eq!(alloc.used(), 0);
        assert_eq!(alloc.high_water(), 0);
        assert_eq!(alloc.allocate(1024), Some(0..1024));
        assert!(alloc.allocate(1).is_none());
        alloc.check_invariants();

        let mut empty = RangeAllocator::new(0, 0);
        assert_eq!(empty.capacity(), 0);
        assert!(empty.allocate(1).is_none());
        empty.grow(10).unwrap();
        assert_eq!(empty.allocate(10), Some(0..10));
        empty.check_invariants();
    }

    #[test]
    fn high_water_only_rises_while_live_extent_follows_the_top() {
        let mut alloc = RangeAllocator::new(1024, 16);
        let a = alloc.allocate(256).unwrap();
        let b = alloc.allocate(256).unwrap();
        assert_eq!((alloc.high_water(), alloc.live_extent()), (512, 512));
        alloc.free(b).unwrap();
        assert_eq!((alloc.high_water(), alloc.live_extent()), (512, 256));
        alloc.free(a).unwrap();
        assert_eq!((alloc.high_water(), alloc.live_extent()), (512, 0));
    }
}

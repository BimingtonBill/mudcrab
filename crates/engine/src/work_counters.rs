//! Counts of full work done by change-driven systems, split at the first time the world is ready.
//!
//! Systems that cache their result and redo it only when an input changed call [`record`] each
//! time they do the full work (rebuild a map, walk a hierarchy, rescan every chunk). Once the
//! world is ready and the camera holds still, none of that should happen: a count after ready
//! names the cache that is being invalidated every frame. The benchmark report carries the counts
//! ([`SettledWork`]) and says whether the settled part of the run was clean.
//!
//! The counters are process-wide so the render world and worker threads can record into them; a
//! call takes a lock, which is fine because a settled frame makes none.

use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

static READY: AtomicBool = AtomicBool::new(false);
/// Per counter: full-work count before and after the world was first ready.
static COUNTS: Mutex<BTreeMap<&'static str, [u64; 2]>> = Mutex::new(BTreeMap::new());

/// One full piece of work by the named system ("module/what", e.g. "terrain_handoff/surface_map").
pub fn record(name: &'static str) {
    let after_ready = usize::from(READY.load(Ordering::Relaxed));
    let mut counts = COUNTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.entry(name).or_default()[after_ready] += 1;
}

/// The world was ready for the first time; later work counts as settled-frame work.
pub fn mark_world_ready() {
    READY.store(true, Ordering::Relaxed);
}

/// The counts so far, for the benchmark report.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct SettledWork {
    /// Whether the world became ready during the run; without it nothing counts as settled.
    pub world_ready: bool,
    /// Full work done after the world was first ready, per counter. Empty in a clean run.
    pub after_ready: BTreeMap<String, u64>,
    /// Full work done while loading, per counter, for context.
    pub before_ready: BTreeMap<String, u64>,
    /// The world became ready and nothing did full work afterwards.
    pub clean: bool,
}

pub fn snapshot() -> SettledWork {
    let world_ready = READY.load(Ordering::Relaxed);
    let counts = COUNTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let pick = |index: usize| -> BTreeMap<String, u64> {
        counts
            .iter()
            .filter(|(_, count)| count[index] != 0)
            .map(|(name, count)| ((*name).to_owned(), count[index]))
            .collect()
    };
    let after_ready = pick(1);
    SettledWork {
        world_ready,
        clean: world_ready && after_ready.is_empty(),
        before_ready: pick(0),
        after_ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The counters are process-wide, so one test covers the whole sequence.
    #[test]
    fn counts_split_at_world_ready() {
        record("test/loading");
        record("test/loading");
        let loading = snapshot();
        assert_eq!(loading.before_ready.get("test/loading"), Some(&2));
        assert!(!loading.clean, "a run that never became ready is not clean");
        mark_world_ready();
        let settled = snapshot();
        assert!(settled.world_ready);
        assert!(!settled.after_ready.contains_key("test/loading"));
        record("test/settled");
        let busy = snapshot();
        assert_eq!(busy.after_ready.get("test/settled"), Some(&1));
        assert!(!busy.clean);
    }
}

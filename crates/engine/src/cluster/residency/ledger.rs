//! Bookkeeping that lets blocks share meshes and lets the main and render worlds agree on what
//! state each block is in: the reference-counted [`MeshLedger`] and the [`BlockStates`] protocol
//! state machine (design note section C3, D1 and D4).

use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

/// Reference counts of shared payloads (meshes keyed by `(mesh_hash, primitive_index)`) across the
/// resident blocks that name them. The count of a key is the number of acquires not yet released,
/// so the result depends only on which blocks hold which keys, never on the order they came or
/// went.
#[derive(Clone, Debug)]
pub struct MeshLedger<K: Hash + Eq + Clone> {
    refs: HashMap<K, u32>,
}

impl<K: Hash + Eq + Clone> Default for MeshLedger<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone> MeshLedger<K> {
    pub fn new() -> Self {
        Self {
            refs: HashMap::new(),
        }
    }

    /// Adds one reference. `true` on the first reference: the caller must read and upload the
    /// payload. `false`: it is already resident, share it.
    pub fn acquire(&mut self, key: K) -> bool {
        let count = self.refs.entry(key).or_insert(0);
        *count += 1;
        *count == 1
    }

    /// Drops one reference. `true` when it was the last: the caller must free the payload.
    /// Releasing a key with no references returns `false` and changes nothing (there is nothing
    /// to free); [`refs`](Self::refs) tells the two `false` cases apart.
    pub fn release(&mut self, key: &K) -> bool {
        let Some(count) = self.refs.get_mut(key) else {
            return false;
        };
        *count -= 1;
        if *count == 0 {
            self.refs.remove(key);
            true
        } else {
            false
        }
    }

    /// References held on `key` (0 when it is not resident).
    pub fn refs(&self, key: &K) -> u32 {
        self.refs.get(key).copied().unwrap_or(0)
    }

    /// Number of distinct keys with at least one reference.
    pub fn len(&self) -> usize {
        self.refs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }
}

/// Where a block is in the publish and retire protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockState {
    /// Wanted by the planner; a worker is reading and validating it.
    Requested,
    /// Validated and GPU-ready on the main side.
    Prepared,
    /// Handed to the render world; nothing uploaded yet.
    Published,
    /// Being uploaded over several frames.
    Uploading,
    /// Fully uploaded; its block-table row exists and is disabled.
    Resident,
    /// Drawn; the stock objects it replaces are hidden.
    Enabled,
    /// Not drawn but still resident (grace time; may be re-enabled).
    Disabled,
    /// Being released on the render side.
    Retiring,
    /// Everything released. May be requested again with a higher generation.
    Freed,
    /// Refused or lost. May be requested again with a higher generation.
    Failed,
}

/// What happened, from either world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockEvent {
    /// The planner wants the block (carries the new generation).
    Request,
    /// The worker finished preparing it.
    Prepared,
    /// The main world put it in the delta.
    Publish,
    /// The render world started a multi-frame upload.
    UploadStarted,
    /// The render world holds all of it.
    Resident,
    /// Draw it.
    Enable,
    /// Stop drawing it.
    Disable,
    /// Release it (after the grace time).
    Retire,
    /// The render world released it.
    Freed,
    /// Refused, invalid or lost. Legal from every live state.
    Fail,
}

/// Result of a legal call to [`BlockStates::transition`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The block moved. `from` is `None` when the block was not known before the request.
    Applied {
        from: Option<BlockState>,
        to: BlockState,
    },
    /// A report for an older generation than the block's current one: dropped on purpose.
    Ignored,
}

/// A protocol slip. Names the block, state, event and generation so a log line is enough.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    /// The event is not allowed in the block's current state.
    Illegal {
        block: String,
        state: BlockState,
        event: BlockEvent,
        generation: u64,
    },
    /// An event other than `Request` for a block never requested.
    Unknown {
        block: String,
        event: BlockEvent,
        generation: u64,
    },
    /// A report for a generation that was never requested.
    FutureGeneration {
        block: String,
        state: BlockState,
        event: BlockEvent,
        current: u64,
        got: u64,
    },
    /// A request on a Freed or Failed block whose generation does not exceed the last one.
    RequestNotNewer {
        block: String,
        state: BlockState,
        current: u64,
        got: u64,
    },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Illegal {
                block,
                state,
                event,
                generation,
            } => write!(
                f,
                "block {block}: event {event:?} is illegal in state {state:?} (generation {generation})"
            ),
            Self::Unknown {
                block,
                event,
                generation,
            } => write!(
                f,
                "block {block}: event {event:?} for a block never requested (generation {generation})"
            ),
            Self::FutureGeneration {
                block,
                state,
                event,
                current,
                got,
            } => write!(
                f,
                "block {block}: event {event:?} names generation {got}, newer than the requested {current} (state {state:?})"
            ),
            Self::RequestNotNewer {
                block,
                state,
                current,
                got,
            } => write!(
                f,
                "block {block}: request with generation {got} does not exceed {current} (state {state:?})"
            ),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// The state of every block the protocol has seen, each with the generation of its latest
/// request. Generations make publish, retire and re-publish of one block safe in any message
/// order: a report for an older generation is [`Outcome::Ignored`].
///
/// Legal flow: `Requested -> Prepared -> Published -> (Uploading) -> Resident -> Enabled ->
/// Disabled -> Retiring -> Freed`, with `Disabled -> Enabled` (revive within the grace time),
/// `Fail` from any live state, and `Request` again from `Freed` or `Failed` with a higher
/// generation. One extension to that flow: `Retire` is also legal from `Published`, `Uploading`
/// and `Resident`, so a block that never got enabled (a worldspace change mid-upload) can still be
/// released through `Retiring` and `Freed`.
#[derive(Clone, Debug)]
pub struct BlockStates<B: Hash + Eq + Clone + fmt::Debug> {
    blocks: HashMap<B, (BlockState, u64)>,
}

impl<B: Hash + Eq + Clone + fmt::Debug> Default for BlockStates<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Hash + Eq + Clone + fmt::Debug> BlockStates<B> {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    /// Current state of `block`, or `None` when it was never requested.
    pub fn state(&self, block: &B) -> Option<BlockState> {
        self.blocks.get(block).map(|(state, _)| *state)
    }

    /// Generation of the latest request of `block`.
    pub fn generation(&self, block: &B) -> Option<u64> {
        self.blocks.get(block).map(|(_, generation)| *generation)
    }

    /// Number of blocks the protocol knows (including Freed and Failed ones).
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Applies `event` for `generation` to `block`.
    ///
    /// - `Request` starts a block (unknown, or Freed/Failed with a higher generation).
    /// - Any other event with a generation below the block's is `Ignored`; above it is an error.
    /// - Otherwise the event must be legal in the current state, else an error naming all of it.
    ///
    /// An error never changes the state.
    pub fn transition(
        &mut self,
        block: B,
        generation: u64,
        event: BlockEvent,
    ) -> Result<Outcome, ProtocolError> {
        let name = || format!("{block:?}");
        let Some(&(state, current)) = self.blocks.get(&block) else {
            if event != BlockEvent::Request {
                return Err(ProtocolError::Unknown {
                    block: name(),
                    event,
                    generation,
                });
            }
            self.blocks
                .insert(block, (BlockState::Requested, generation));
            return Ok(Outcome::Applied {
                from: None,
                to: BlockState::Requested,
            });
        };

        if event == BlockEvent::Request {
            return match state {
                BlockState::Freed | BlockState::Failed if generation > current => {
                    self.blocks
                        .insert(block, (BlockState::Requested, generation));
                    Ok(Outcome::Applied {
                        from: Some(state),
                        to: BlockState::Requested,
                    })
                }
                BlockState::Freed | BlockState::Failed => Err(ProtocolError::RequestNotNewer {
                    block: name(),
                    state,
                    current,
                    got: generation,
                }),
                _ => Err(ProtocolError::Illegal {
                    block: name(),
                    state,
                    event,
                    generation,
                }),
            };
        }

        if generation < current {
            return Ok(Outcome::Ignored);
        }
        if generation > current {
            return Err(ProtocolError::FutureGeneration {
                block: name(),
                state,
                event,
                current,
                got: generation,
            });
        }

        use BlockEvent as E;
        use BlockState as S;
        let next = match (state, event) {
            (S::Requested, E::Prepared) => S::Prepared,
            (S::Prepared, E::Publish) => S::Published,
            (S::Published, E::UploadStarted) => S::Uploading,
            (S::Published | S::Uploading, E::Resident) => S::Resident,
            (S::Resident | S::Disabled, E::Enable) => S::Enabled,
            (S::Enabled, E::Disable) => S::Disabled,
            (S::Published | S::Uploading | S::Resident | S::Disabled, E::Retire) => S::Retiring,
            (S::Retiring, E::Freed) => S::Freed,
            (
                S::Requested
                | S::Prepared
                | S::Published
                | S::Uploading
                | S::Resident
                | S::Enabled
                | S::Disabled
                | S::Retiring,
                E::Fail,
            ) => S::Failed,
            _ => {
                return Err(ProtocolError::Illegal {
                    block: name(),
                    state,
                    event,
                    generation,
                });
            }
        };
        self.blocks.insert(block, (next, current));
        Ok(Outcome::Applied {
            from: Some(state),
            to: next,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    type Key = (u64, u32);

    /// Mesh keys named by three blocks; (1,0) is shared by all, (2,0) by A and B.
    fn blocks() -> Vec<(&'static str, Vec<Key>)> {
        vec![
            ("A", vec![(1, 0), (2, 0), (3, 0), (3, 1)]),
            ("B", vec![(1, 0), (2, 0), (4, 0)]),
            ("C", vec![(1, 0), (5, 0), (3, 1)]),
        ]
    }

    fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
        if items.len() <= 1 {
            return vec![items.to_vec()];
        }
        let mut out = Vec::new();
        for index in 0..items.len() {
            let mut rest = items.to_vec();
            let head = rest.remove(index);
            for mut tail in permutations(&rest) {
                tail.insert(0, head);
                out.push(tail);
            }
        }
        out
    }

    #[test]
    fn ledger_dedupes_and_counts_across_blocks() {
        let mut ledger = MeshLedger::new();
        let mut uploads = Vec::new();
        for (_, keys) in blocks() {
            for key in keys {
                if ledger.acquire(key) {
                    uploads.push(key);
                }
            }
        }
        // 4 + 3 + 3 acquires but only the distinct meshes are uploaded, each once.
        let distinct: BTreeSet<Key> = uploads.iter().copied().collect();
        assert_eq!(uploads.len(), distinct.len());
        assert_eq!(ledger.len(), 6);
        assert_eq!(ledger.refs(&(1, 0)), 3);
        assert_eq!(ledger.refs(&(2, 0)), 2);
        assert_eq!(ledger.refs(&(3, 1)), 2);
        assert_eq!(ledger.refs(&(4, 0)), 1);
        assert_eq!(ledger.refs(&(9, 9)), 0);
        assert!(!ledger.is_empty());
    }

    #[test]
    fn ledger_retire_order_does_not_matter() {
        let all = blocks();
        for order in permutations(&[0, 1, 2]) {
            let mut ledger = MeshLedger::new();
            for (_, keys) in &all {
                for key in keys {
                    ledger.acquire(*key);
                }
            }
            let mut freed: BTreeMap<Key, u32> = BTreeMap::new();
            let mut remaining: Vec<usize> = vec![0, 1, 2];
            for &index in &order {
                for key in &all[index].1 {
                    if ledger.release(key) {
                        *freed.entry(*key).or_default() += 1;
                    }
                }
                remaining.retain(|r| *r != index);
                // The counts are those of the blocks still held, whatever order got here.
                let mut expected: BTreeMap<Key, u32> = BTreeMap::new();
                for &r in &remaining {
                    for key in &all[r].1 {
                        *expected.entry(*key).or_default() += 1;
                    }
                }
                for (_, keys) in &all {
                    for key in keys {
                        assert_eq!(ledger.refs(key), expected.get(key).copied().unwrap_or(0));
                    }
                }
                assert_eq!(ledger.len(), expected.len());
            }
            // Every mesh was freed exactly once, no matter the order.
            assert_eq!(freed.len(), 6);
            assert!(freed.values().all(|n| *n == 1), "{order:?}: {freed:?}");
            assert!(ledger.is_empty());
        }
    }

    #[test]
    fn ledger_reacquire_after_full_release_uploads_again() {
        let mut ledger = MeshLedger::new();
        assert!(ledger.acquire("m"));
        assert!(!ledger.acquire("m"));
        assert!(!ledger.release(&"m"));
        assert!(ledger.release(&"m"));
        // Releasing a key that is gone frees nothing and does not underflow.
        assert!(!ledger.release(&"m"));
        assert_eq!(ledger.refs(&"m"), 0);
        assert!(ledger.acquire("m"));
    }

    fn apply(states: &mut BlockStates<u32>, generation: u64, events: &[BlockEvent]) {
        for event in events {
            states
                .transition(7, generation, *event)
                .unwrap_or_else(|err| panic!("{err}"));
        }
    }

    use BlockEvent as E;
    use BlockState as S;

    #[test]
    fn full_flow_with_and_without_the_upload_step() {
        let mut states = BlockStates::new();
        assert_eq!(
            states.transition(7, 1, E::Request),
            Ok(Outcome::Applied {
                from: None,
                to: S::Requested
            })
        );
        let flow = [
            (E::Prepared, S::Prepared),
            (E::Publish, S::Published),
            (E::UploadStarted, S::Uploading),
            (E::Resident, S::Resident),
            (E::Enable, S::Enabled),
            (E::Disable, S::Disabled),
            (E::Retire, S::Retiring),
            (E::Freed, S::Freed),
        ];
        for (event, to) in flow {
            let from = states.state(&7);
            assert_eq!(
                states.transition(7, 1, event),
                Ok(Outcome::Applied { from, to })
            );
            assert_eq!(states.state(&7), Some(to));
        }
        // Small blocks skip Uploading.
        apply(
            &mut states,
            2,
            &[E::Request, E::Prepared, E::Publish, E::Resident],
        );
        assert_eq!(states.state(&7), Some(S::Resident));
        assert_eq!(states.generation(&7), Some(2));
    }

    #[test]
    fn disabled_block_may_be_revived_within_the_grace_time() {
        let mut states = BlockStates::new();
        apply(
            &mut states,
            1,
            &[
                E::Request,
                E::Prepared,
                E::Publish,
                E::Resident,
                E::Enable,
                E::Disable,
                E::Enable,
                E::Disable,
                E::Retire,
                E::Freed,
            ],
        );
        assert_eq!(states.state(&7), Some(S::Freed));
    }

    #[test]
    fn publish_retire_republish_with_generations() {
        let mut states = BlockStates::new();
        apply(
            &mut states,
            1,
            &[
                E::Request,
                E::Prepared,
                E::Publish,
                E::Resident,
                E::Enable,
                E::Disable,
                E::Retire,
            ],
        );
        // Re-requested while the render world is still retiring generation 1: not allowed yet.
        let err = states.transition(7, 2, E::Request).unwrap_err();
        assert!(matches!(
            err,
            ProtocolError::Illegal {
                state: S::Retiring,
                event: E::Request,
                ..
            }
        ));
        assert_eq!(states.state(&7), Some(S::Retiring));
        apply(&mut states, 1, &[E::Freed]);
        // Now generation 2 starts; a late generation-1 report is ignored, not an error.
        apply(&mut states, 2, &[E::Request]);
        assert_eq!(states.transition(7, 1, E::Freed), Ok(Outcome::Ignored));
        assert_eq!(states.transition(7, 1, E::Resident), Ok(Outcome::Ignored));
        assert_eq!(states.state(&7), Some(S::Requested));
        apply(
            &mut states,
            2,
            &[E::Prepared, E::Publish, E::Resident, E::Enable],
        );
        assert_eq!(states.state(&7), Some(S::Enabled));
        // A report from the future is a bug.
        assert!(matches!(
            states.transition(7, 3, E::Disable),
            Err(ProtocolError::FutureGeneration {
                current: 2,
                got: 3,
                ..
            })
        ));
    }

    #[test]
    fn stale_reports_are_ignored_in_every_state() {
        let path = [
            E::Request,
            E::Prepared,
            E::Publish,
            E::UploadStarted,
            E::Resident,
            E::Enable,
            E::Disable,
            E::Retire,
            E::Freed,
        ];
        for stop in 0..path.len() {
            let mut states = BlockStates::new();
            apply(&mut states, 5, &path[..=stop]);
            let before = states.state(&7);
            for stale in [E::Prepared, E::Resident, E::Enable, E::Freed, E::Fail] {
                assert_eq!(states.transition(7, 4, stale), Ok(Outcome::Ignored));
            }
            assert_eq!(states.state(&7), before);
            assert_eq!(states.generation(&7), Some(5));
        }
    }

    #[test]
    fn failure_from_every_live_state_and_request_again_after() {
        let path = [
            E::Request,
            E::Prepared,
            E::Publish,
            E::UploadStarted,
            E::Resident,
            E::Enable,
            E::Disable,
            E::Retire,
        ];
        for stop in 0..path.len() {
            let mut states = BlockStates::new();
            apply(&mut states, 1, &path[..=stop]);
            let from = states.state(&7);
            assert_eq!(
                states.transition(7, 1, E::Fail),
                Ok(Outcome::Applied {
                    from,
                    to: S::Failed
                }),
                "failure after {:?}",
                path[stop]
            );
            // A second failure report is a slip, not a state change.
            assert!(states.transition(7, 1, E::Fail).is_err());
            // Same or lower generation cannot restart it; a higher one can.
            assert!(matches!(
                states.transition(7, 1, E::Request),
                Err(ProtocolError::RequestNotNewer { .. })
            ));
            assert_eq!(
                states.transition(7, 2, E::Request),
                Ok(Outcome::Applied {
                    from: Some(S::Failed),
                    to: S::Requested
                })
            );
        }
        // Freed is not a live state: it cannot fail.
        let mut states = BlockStates::new();
        apply(
            &mut states,
            1,
            &[
                E::Request,
                E::Prepared,
                E::Publish,
                E::Resident,
                E::Retire,
                E::Freed,
            ],
        );
        assert!(states.transition(7, 1, E::Fail).is_err());
        assert_eq!(states.state(&7), Some(S::Freed));
    }

    #[test]
    fn retire_without_ever_enabling_goes_through_retiring() {
        for early in [
            vec![E::Request, E::Prepared, E::Publish],
            vec![E::Request, E::Prepared, E::Publish, E::UploadStarted],
            vec![E::Request, E::Prepared, E::Publish, E::Resident],
        ] {
            let mut states = BlockStates::new();
            apply(&mut states, 1, &early);
            apply(&mut states, 1, &[E::Retire, E::Freed]);
            assert_eq!(states.state(&7), Some(S::Freed));
        }
    }

    #[test]
    fn illegal_transitions_are_errors_naming_block_state_and_event() {
        let mut states = BlockStates::new();
        apply(&mut states, 3, &[E::Request, E::Prepared]);
        let err = states.transition(7, 3, E::Enable).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::Illegal {
                block: "7".into(),
                state: S::Prepared,
                event: E::Enable,
                generation: 3
            }
        );
        let text = err.to_string();
        assert!(text.contains('7') && text.contains("Prepared") && text.contains("Enable"));
        assert_eq!(states.state(&7), Some(S::Prepared));

        // Enabled cannot be retired without being disabled first; Requested cannot be freed.
        apply(&mut states, 3, &[E::Publish, E::Resident, E::Enable]);
        assert!(states.transition(7, 3, E::Retire).is_err());
        assert!(states.transition(7, 3, E::Freed).is_err());
        assert!(states.transition(7, 3, E::Request).is_err());
        assert_eq!(states.state(&7), Some(S::Enabled));
        // Events for a block never requested.
        assert!(matches!(
            states.transition(8, 1, E::Prepared),
            Err(ProtocolError::Unknown { .. })
        ));
        assert_eq!(states.state(&8), None);
        assert_eq!(states.len(), 1);
        assert!(!states.is_empty());
    }

    #[test]
    fn blocks_are_independent() {
        let mut states = BlockStates::new();
        states.transition(1, 1, E::Request).unwrap();
        states.transition(2, 9, E::Request).unwrap();
        states.transition(1, 1, E::Prepared).unwrap();
        assert_eq!(states.state(&1), Some(S::Prepared));
        assert_eq!(states.state(&2), Some(S::Requested));
        assert_eq!(states.generation(&2), Some(9));
    }
}

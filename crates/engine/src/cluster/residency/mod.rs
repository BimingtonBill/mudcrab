//! Residency of cluster blocks (design note `cluster-streaming-design-2026-10-16.md`, sections B to D).
//!
//! Pure bookkeeping with no GPU, ECS or asset types, so every rule here is tested without a
//! device:
//! - [`alloc`]: a first-fit range allocator with coalescing for the growable GPU pools, and the
//!   growth rule.
//! - [`ledger`]: the reference-counted mesh ledger and the per-block protocol state machine
//!   between the main world and the render world.
//! - [`planner`]: the pure function from the block table, camera and active worldspace to the
//!   wanted, kept and retired block sets.

pub mod alloc;
pub mod ledger;
pub mod planner;

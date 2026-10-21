//! Far-terrain sun-shadow reference: an exact CPU answer to "does terrain block the sun here?".
//!
//! The mathematical modules are a certified-input reference, not a renderer feature.
//! The optional reader adapter uses the existing certified package; nothing here
//! registers systems, jobs, resources or options. Work runs only when explicitly called.
//!
//! * [`heightfield`]: the validated `33 x 33` height grid, the canonical base triangulation
//!   (`[a, b, c]`, `[b, d, c]`, checked against `d349ba57`
//!   `crates/converter/src/cluster/terrain_geometry.rs:321-327`) and exact ray-triangle tests.
//! * [`inventory`]: the immutable per-snapshot inventory (identity, authored validity versus the
//!   selected rendered surface, sparse-world attestations) and the ray query that returns
//!   `Occluded`, `Clear` or `Unknown` with the reasons.
//!
//! The canonical base surface is the converter's base surface only. It does not certify the
//! simplified GPU cluster triangles nor the baked LOD fan topology; other topologies are
//! `Unknown`, and a caller that compares against drawn geometry must supply the final post-weld
//! surface. Seams between cells are never welded or filled.
//!
//! Identity and digests are inputs to the mathematical modules from a certified reader.
//! They compare identities exactly and hash grid bytes against supplied digests.
//! The optional reader separately snapshots the existing certified database/cache.
//!
//! The optional startup plugin installs ownership and renderer adapters only when explicitly enabled.

pub mod arithmetic;
pub mod contact;
pub mod gpu;
pub mod heightfield;
pub mod inventory;
pub mod packet;
// Certified adapters retain the exact source allocation through GPU publication.
pub(crate) mod reader;
pub mod receiving;
pub mod triangle;
pub mod view;

#[cfg(test)]
pub(crate) mod bindings_audit;
#[cfg(test)]
mod forward_abi_tests;
pub(crate) mod layout;
#[cfg(test)]
pub(crate) mod lighting_tests;
pub(crate) mod placement;
#[cfg(test)]
mod reader_tests;
pub(crate) mod render;
pub(crate) mod runtime;
pub(crate) mod runtime_owner;
mod selection;
#[cfg(test)]
mod wgsl_tests;
mod world_height_tree;

// gpu_numeric_tests.rs is a cfg(test) CHILD of renderer_init, not this module:
// its owner-controlled GPU test reuses that private production initializer.

pub use heightfield::{
    CELL_INTERVALS, CellCoord, CellHit, CellSpacing, CellTrace, Digest32, GRID_BYTES,
    GridRejection, Half, HeightGrid, NumericError, PreparedRay, Ray, RayError, SAMPLES_PER_CELL,
    SAMPLES_PER_SIDE, SpacingError, TriangleId, encode_heights_le, sha256,
};
pub use inventory::{
    AuthoredStatus, CanonicalRecipeIdentity, CellRect, CellUnknown, CellView, ClearProof,
    ClosedWorldAttestation, CoreCacheIdentity, DatabaseIdentity, InventoryBuilder, InventoryError,
    KnownNoSurfaceAttestation, Occlusion, RawGrid, RayResult, SnapshotIdentity, SurfaceInput,
    SurfaceKind, SurfaceQuery, SurfaceSource, SurfaceState, SurfaceTriangulation, UnknownReason,
    WorldHeightInventory,
};

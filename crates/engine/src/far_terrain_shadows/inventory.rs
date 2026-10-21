//! The immutable height inventory a snapshot publishes, and the sun-ray query over it.
//!
//! The inventory answers one question for a ray in world space: is the sun blocked by terrain
//! (`Occluded`), provably not (`Clear`), or can this inventory not say (`Unknown`, with explicit
//! reasons). `Clear` is only ever returned with a proof over the WHOLE positive ray; there is no
//! distance or step limit that could stand in for one.
//!
//! # What is certified, and by whom
//!
//! This library never hashes a database, a cache or a package. The identity fields
//! ([`SnapshotIdentity`]) and the expected grid digests are inputs from a certified reader. The
//! library only (a) compares identities for exact equality before it answers anything, (b)
//! hashes each grid's bytes and compares that with the digest it was given, and (c) keeps
//! authored validity and the surface that was selected for rendering apart, so a finite number
//! never turns into "authored".
//!
//! # Cell states
//!
//! Per cell the caller states [`AuthoredStatus`] (`AbsentLand`, `MissingVhgt`, `VhgtPresent`) and
//! files zero or more surfaces with an explicit [`SurfaceKind`]:
//!
//! | authored status | allowed surface kinds |
//! |---|---|
//! | `VhgtPresent` | `Authored`, `CanonicalFromAuthored` |
//! | `MissingVhgt` | `RenderedDefault`, `CanonicalFromRenderedDefault` |
//! | `AbsentLand` | none |
//!
//! A surface whose bytes are malformed, non-finite or do not match their digest is kept as a
//! rejected state with its reason (`Invalid` / `Stale` data). A query selecting that surface is
//! `Unknown`; it never silently falls back to another supplied surface.
//!
//! # Sparse worlds
//!
//! The domain is a finite inclusive rectangle of cells. Inside it, a cell nobody listed is
//! `Unknown` (uninventoried). An `AbsentLand` entry alone is also `Unknown`; only a separate
//! [`KnownNoSurfaceAttestation`] bound to the exact snapshot makes a cell resolved-and-empty
//! (never a height-zero plane). Outside the rectangle everything is `Unknown` unless a
//! [`ClosedWorldAttestation`] bound to the exact snapshot says there is no terrain there. Nothing
//! is inherited from a parent world and no constructor defaults to a closed world.
//!
//! # Rays
//!
//! `t > 0` only: the receiver's own contact at `t == 0` is excluded and nothing finite is
//! skipped. The cell boxes are closed, so a ray that merely touches the edge of an unresolved
//! neighbour stays `Unknown` (the neighbour's side of a shared boundary is not discarded). A
//! known occluder wins over unresolved parts of the same ray. The query scans listed cells;
//! sparse coverage is checked by splitting the domain only where a touched region is not fully
//! inventoried. Work follows inventory content and integer subdivision depth, not the number of
//! omitted cells in its bounding rectangle. There is no camera, view or residency dependency.

use std::collections::{BTreeMap, BTreeSet};

use super::heightfield::{
    CellCoord, CellHit, CellSpacing, Digest32, GridRejection, HeightGrid, NumericError,
    PreparedRay, Ray, RayError, SpacingError, TriangleId,
};

/// Database identity: the content hash the reader computed and its schema revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DatabaseIdentity {
    pub schema_revision: u32,
    pub content_hash: Digest32,
}

/// Core / cell-cache identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CoreCacheIdentity {
    pub version: u32,
    pub hash: Digest32,
}

/// How a surface's quads are split into triangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceTriangulation {
    /// `[a, b, c]` and `[b, d, c]`, see the `heightfield` module (`d349ba57`
    /// `terrain_geometry.rs:321-327`). The converter's base surface.
    CanonicalBase,
    /// Any other topology (simplified cluster triangles, baked LOD fans, ...), tagged by the
    /// adapter. Never queried: a query for it is `Unknown`.
    Unsupported(u32),
}

/// Identity of a canonical recipe output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanonicalRecipeIdentity {
    pub package: Digest32,
    pub catalog: Digest32,
    pub content_digest: Digest32,
    pub recipe_revision: u32,
    pub triangulation: SurfaceTriangulation,
}

/// Everything a result is only valid for. Compared field by field for exact equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotIdentity {
    /// The worldspace FormID, given explicitly: it is never inferred from a parent.
    pub worldspace: u32,
    /// Exact `f64::to_bits()` of world units per cell. Results and attestations cannot be
    /// reused at another spacing even when the same height bytes are supplied.
    pub cell_size_bits: u64,
    pub database: DatabaseIdentity,
    pub core_cache: CoreCacheIdentity,
    pub canonical: Option<CanonicalRecipeIdentity>,
}

/// "There is no terrain outside the domain rectangle" for exactly this snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedWorldAttestation {
    pub identity: SnapshotIdentity,
    pub domain: CellRect,
}

impl ClosedWorldAttestation {
    pub fn new(identity: SnapshotIdentity, domain: CellRect) -> Self {
        Self { identity, domain }
    }
}

/// "This cell has no terrain surface at all" for exactly this snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownNoSurfaceAttestation {
    pub identity: SnapshotIdentity,
    pub cell: CellCoord,
}

impl KnownNoSurfaceAttestation {
    pub fn new(identity: SnapshotIdentity, cell: CellCoord) -> Self {
        Self { identity, cell }
    }
}

/// A finite inclusive rectangle of cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    min: CellCoord,
    max: CellCoord,
}

impl CellRect {
    pub fn new(min: CellCoord, max: CellCoord) -> Result<Self, InventoryError> {
        if min.x > max.x || min.y > max.y {
            return Err(InventoryError::InvertedDomain);
        }
        Ok(Self { min, max })
    }

    pub fn min(&self) -> CellCoord {
        self.min
    }

    pub fn max(&self) -> CellCoord {
        self.max
    }

    pub fn contains(&self, cell: CellCoord) -> bool {
        (self.min.x..=self.max.x).contains(&cell.x) && (self.min.y..=self.max.y).contains(&cell.y)
    }
}

/// What the world's authored LAND record says about a cell (not what is rendered).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoredStatus {
    /// No LAND record. Not automatically known-empty.
    AbsentLand,
    /// LAND exists but has no VHGT. Never inferred from a finite cache.
    MissingVhgt,
    /// LAND with VHGT.
    VhgtPresent,
}

/// Which surface a grid is. Carries the provenance; the grid itself carries none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// Heights decoded from the authored VHGT.
    Authored,
    /// The renderer's / cache's default heights where VHGT is missing.
    RenderedDefault,
    /// Canonical recipe output built from the authored heights.
    CanonicalFromAuthored,
    /// Canonical recipe output built from the rendered default.
    CanonicalFromRenderedDefault,
}

impl SurfaceKind {
    /// The authored status this kind is the surface of.
    pub fn required_authored(self) -> AuthoredStatus {
        match self {
            SurfaceKind::Authored | SurfaceKind::CanonicalFromAuthored => {
                AuthoredStatus::VhgtPresent
            }
            SurfaceKind::RenderedDefault | SurfaceKind::CanonicalFromRenderedDefault => {
                AuthoredStatus::MissingVhgt
            }
        }
    }

    pub fn is_canonical(self) -> bool {
        matches!(
            self,
            SurfaceKind::CanonicalFromAuthored | SurfaceKind::CanonicalFromRenderedDefault
        )
    }

    fn is_rendered_default(self) -> bool {
        matches!(
            self,
            SurfaceKind::RenderedDefault | SurfaceKind::CanonicalFromRenderedDefault
        )
    }
}

/// Which family of surfaces a query reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceSource {
    /// `Authored` where the cell has VHGT, `RenderedDefault` where it does not.
    AuthoredOrRenderedDefault,
    /// The canonical recipe output for the same provenance.
    Canonical,
}

/// Raw grid bytes plus the digest the certified reader expected for them.
#[derive(Debug, Clone, Copy)]
pub struct RawGrid<'a> {
    /// `33 * 33` little-endian `f32` runtime unit heights, row-major south to north.
    pub bytes: &'a [u8],
    pub expected_sha256: Digest32,
}

/// One surface to file in a cell.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceInput<'a> {
    pub kind: SurfaceKind,
    pub raw: RawGrid<'a>,
}

/// A filed surface: usable, or rejected with the reason.
#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceState {
    Valid(HeightGrid),
    /// Malformed, non-finite or stale; never used to answer a query.
    Rejected(GridRejection),
}

#[derive(Debug, Clone)]
struct CellRecord {
    authored: AuthoredStatus,
    surfaces: Vec<(SurfaceKind, SurfaceState)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryError {
    InvalidSpacing(SpacingError),
    SpacingIdentityMismatch,
    InvertedDomain,
    /// A domain corner does not convert to a finite world coordinate.
    NonFiniteDomainCoordinates,
    CellOutsideDomain(CellCoord),
    DuplicateCell(CellCoord),
    DuplicateSurfaceKind {
        cell: CellCoord,
        kind: SurfaceKind,
    },
    /// The surface kind does not belong to the stated authored status.
    ProvenanceMismatch {
        cell: CellCoord,
        kind: SurfaceKind,
        authored: AuthoredStatus,
    },
    /// A canonical surface was filed but the identity has no canonical recipe.
    CanonicalWithoutRecipe {
        cell: CellCoord,
        kind: SurfaceKind,
    },
    /// The attestation names another snapshot.
    AttestationIdentityMismatch,
    AttestationDomainMismatch,
    DuplicateAttestation(CellCoord),
    DuplicateClosedWorld,
    /// A no-surface attestation and a cell that has a LAND or a surface.
    NoSurfaceAttestationConflict(CellCoord),
}

/// What a cell is, for inspection.
#[derive(Debug, Clone, PartialEq)]
pub enum CellView<'a> {
    /// Inside the domain, nobody listed it.
    Uninventoried,
    KnownNoSurface,
    Listed {
        authored: AuthoredStatus,
        surfaces: &'a [(SurfaceKind, SurfaceState)],
    },
}

/// Builds a [`WorldHeightInventory`]. Invalid, duplicate and out-of-domain input is an error and
/// never overwrites.
#[derive(Debug)]
pub struct InventoryBuilder {
    identity: SnapshotIdentity,
    spacing: CellSpacing,
    domain: CellRect,
    cells: BTreeMap<CellCoord, CellRecord>,
    no_surface: BTreeSet<CellCoord>,
    closed_world: bool,
}

impl InventoryBuilder {
    pub fn new(
        identity: SnapshotIdentity,
        cell_size: f64,
        domain: CellRect,
    ) -> Result<Self, InventoryError> {
        let spacing = CellSpacing::new(cell_size).map_err(InventoryError::InvalidSpacing)?;
        let corners = [domain.min(), domain.max()];
        if corners
            .iter()
            .any(|corner| spacing.cell_extent(*corner).is_none())
        {
            return Err(InventoryError::NonFiniteDomainCoordinates);
        }
        if identity.cell_size_bits != cell_size.to_bits() {
            return Err(InventoryError::SpacingIdentityMismatch);
        }
        Ok(Self {
            identity,
            spacing,
            domain,
            cells: BTreeMap::new(),
            no_surface: BTreeSet::new(),
            closed_world: false,
        })
    }

    /// Files a cell with its authored status and surfaces. Bad bytes are stored as rejected
    /// states (the cell stays `Unknown`); structural mistakes are errors.
    pub fn add_cell(
        &mut self,
        cell: CellCoord,
        authored: AuthoredStatus,
        surfaces: &[SurfaceInput<'_>],
    ) -> Result<(), InventoryError> {
        if !self.domain.contains(cell) {
            return Err(InventoryError::CellOutsideDomain(cell));
        }
        if self.cells.contains_key(&cell) {
            return Err(InventoryError::DuplicateCell(cell));
        }
        if self.no_surface.contains(&cell)
            && (authored != AuthoredStatus::AbsentLand || !surfaces.is_empty())
        {
            return Err(InventoryError::NoSurfaceAttestationConflict(cell));
        }
        let mut stored: Vec<(SurfaceKind, SurfaceState)> = Vec::with_capacity(surfaces.len());
        for input in surfaces {
            let kind = input.kind;
            if kind.required_authored() != authored {
                return Err(InventoryError::ProvenanceMismatch {
                    cell,
                    kind,
                    authored,
                });
            }
            if kind.is_canonical() && self.identity.canonical.is_none() {
                return Err(InventoryError::CanonicalWithoutRecipe { cell, kind });
            }
            if stored.iter().any(|(existing, _)| *existing == kind) {
                return Err(InventoryError::DuplicateSurfaceKind { cell, kind });
            }
            let state = match HeightGrid::from_le_bytes(input.raw.bytes, &input.raw.expected_sha256)
            {
                Ok(grid) => SurfaceState::Valid(grid),
                Err(rejection) => SurfaceState::Rejected(rejection),
            };
            stored.push((kind, state));
        }
        self.cells.insert(
            cell,
            CellRecord {
                authored,
                surfaces: stored,
            },
        );
        Ok(())
    }

    pub fn attest_known_no_surface(
        &mut self,
        attestation: KnownNoSurfaceAttestation,
    ) -> Result<(), InventoryError> {
        if attestation.identity != self.identity {
            return Err(InventoryError::AttestationIdentityMismatch);
        }
        let cell = attestation.cell;
        if !self.domain.contains(cell) {
            return Err(InventoryError::CellOutsideDomain(cell));
        }
        if self.no_surface.contains(&cell) {
            return Err(InventoryError::DuplicateAttestation(cell));
        }
        if let Some(record) = self.cells.get(&cell)
            && (record.authored != AuthoredStatus::AbsentLand || !record.surfaces.is_empty())
        {
            return Err(InventoryError::NoSurfaceAttestationConflict(cell));
        }
        self.no_surface.insert(cell);
        Ok(())
    }

    pub fn attest_closed_world(
        &mut self,
        attestation: ClosedWorldAttestation,
    ) -> Result<(), InventoryError> {
        if attestation.identity != self.identity {
            return Err(InventoryError::AttestationIdentityMismatch);
        }
        if attestation.domain != self.domain {
            return Err(InventoryError::AttestationDomainMismatch);
        }
        if self.closed_world {
            return Err(InventoryError::DuplicateClosedWorld);
        }
        self.closed_world = true;
        Ok(())
    }

    pub fn build(self) -> WorldHeightInventory {
        let [x_min, _, y_min, _] = self
            .spacing
            .cell_extent(self.domain.min())
            .expect("domain corners were checked as finite");
        let [_, x_max, _, y_max] = self
            .spacing
            .cell_extent(self.domain.max())
            .expect("domain corners were checked as finite");
        WorldHeightInventory {
            identity: self.identity,
            spacing: self.spacing,
            domain: self.domain,
            extent: [x_min, x_max, y_min, y_max],
            cells: self.cells,
            no_surface: self.no_surface,
            closed_world: self.closed_world,
        }
    }
}

/// Immutable per-snapshot height inventory.
#[derive(Debug, Clone)]
pub struct WorldHeightInventory {
    identity: SnapshotIdentity,
    spacing: CellSpacing,
    domain: CellRect,
    /// `[x_min, x_max, y_min, y_max]` of the whole domain in world units.
    extent: [f64; 4],
    cells: BTreeMap<CellCoord, CellRecord>,
    no_surface: BTreeSet<CellCoord>,
    closed_world: bool,
}

/// What a trace reads and which snapshot it must be for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceQuery {
    /// The identity the caller needs the answer to be valid for.
    pub expected: SnapshotIdentity,
    pub source: SurfaceSource,
    /// Must be `CanonicalBase`: that is the only topology this reference certifies.
    pub triangulation: SurfaceTriangulation,
}

impl SurfaceQuery {
    pub fn canonical_base(expected: SnapshotIdentity, source: SurfaceSource) -> Self {
        Self {
            expected,
            source,
            triangulation: SurfaceTriangulation::CanonicalBase,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellUnknown {
    /// Inside the domain, never listed.
    Uninventoried,
    /// LAND is absent and no certified no-surface attestation covers the cell.
    AbsentLandUnattested,
    /// The cell exists but the wanted surface was not supplied.
    SurfaceNotSupplied {
        authored: AuthoredStatus,
        wanted: SurfaceKind,
    },
    /// The wanted surface's data was malformed, non-finite or stale.
    Rejected {
        kind: SurfaceKind,
        rejection: GridRejection,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnknownReason {
    InvalidRay(RayError),
    IdentityMismatch {
        expected: Box<SnapshotIdentity>,
        observed: Box<SnapshotIdentity>,
    },
    UnsupportedTriangulation(SurfaceTriangulation),
    /// A canonical query on an inventory that has no canonical recipe.
    CanonicalUnavailable,
    Cell {
        cell: CellCoord,
        reason: CellUnknown,
    },
    /// The ray leaves the domain rectangle and no closed-world attestation covers the outside.
    OutsideInventory,
    /// The ray lies in the plane of a triangle it touches.
    Coplanar {
        cell: CellCoord,
        triangle: TriangleId,
        surface: SurfaceKind,
    },
    Numeric {
        cell: CellCoord,
        error: NumericError,
    },
}

/// The nearest proven hit among supplied surfaces. Unknown coverage may contain a closer hit;
/// this is an occlusion witness, not a global closest-hit guarantee.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Occlusion {
    pub cell: CellCoord,
    pub triangle: TriangleId,
    pub surface: SurfaceKind,
    /// `f64` approximation of the exact parameter: `position = origin + t * direction`.
    pub t: f64,
    pub position: [f64; 3],
}

/// What the `Clear` proof covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearProof {
    /// Cells whose surface the ray was tested against.
    pub surface_cells: usize,
    /// Touched cells that carry a certified no-surface attestation.
    pub known_empty_cells: usize,
    /// How many of `surface_cells` are rendered defaults (authored VHGT was missing).
    pub rendered_default_cells: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RayResult {
    Occluded(Occlusion),
    Clear(ClearProof),
    Unknown(Vec<UnknownReason>),
}

enum Resolution<'a> {
    Surface {
        grid: &'a HeightGrid,
        kind: SurfaceKind,
    },
    KnownNoSurface,
    Unresolved(CellUnknown),
}

impl WorldHeightInventory {
    pub fn identity(&self) -> &SnapshotIdentity {
        &self.identity
    }

    pub fn spacing(&self) -> &CellSpacing {
        &self.spacing
    }

    pub fn domain(&self) -> CellRect {
        self.domain
    }

    pub fn is_closed_world(&self) -> bool {
        self.closed_world
    }

    pub fn cell_view(&self, cell: CellCoord) -> Option<CellView<'_>> {
        if !self.domain.contains(cell) {
            return None;
        }
        if self.no_surface.contains(&cell) {
            return Some(CellView::KnownNoSurface);
        }
        Some(match self.cells.get(&cell) {
            None => CellView::Uninventoried,
            Some(record) => CellView::Listed {
                authored: record.authored,
                surfaces: &record.surfaces,
            },
        })
    }

    fn resolve(&self, cell: CellCoord, source: SurfaceSource) -> Resolution<'_> {
        if self.no_surface.contains(&cell) {
            return Resolution::KnownNoSurface;
        }
        let Some(record) = self.cells.get(&cell) else {
            return Resolution::Unresolved(CellUnknown::Uninventoried);
        };
        let wanted = match (source, record.authored) {
            (_, AuthoredStatus::AbsentLand) => {
                return Resolution::Unresolved(CellUnknown::AbsentLandUnattested);
            }
            (SurfaceSource::AuthoredOrRenderedDefault, AuthoredStatus::VhgtPresent) => {
                SurfaceKind::Authored
            }
            (SurfaceSource::AuthoredOrRenderedDefault, AuthoredStatus::MissingVhgt) => {
                SurfaceKind::RenderedDefault
            }
            (SurfaceSource::Canonical, AuthoredStatus::VhgtPresent) => {
                SurfaceKind::CanonicalFromAuthored
            }
            (SurfaceSource::Canonical, AuthoredStatus::MissingVhgt) => {
                SurfaceKind::CanonicalFromRenderedDefault
            }
        };
        match record.surfaces.iter().find(|(kind, _)| *kind == wanted) {
            None => Resolution::Unresolved(CellUnknown::SurfaceNotSupplied {
                authored: record.authored,
                wanted,
            }),
            Some((_, SurfaceState::Valid(grid))) => Resolution::Surface { grid, kind: wanted },
            Some((_, SurfaceState::Rejected(rejection))) => {
                Resolution::Unresolved(CellUnknown::Rejected {
                    kind: wanted,
                    rejection: *rejection,
                })
            }
        }
    }

    /// Whether the positive ray's XY path has a point outside the closed domain rectangle.
    /// A non-vertical ray always does; a vertical one only if its point is outside.
    fn path_leaves_domain(&self, ray: &Ray) -> bool {
        let [ox, oy, _] = ray.origin();
        let [dx, dy, _] = ray.direction();
        if dx != 0.0 || dy != 0.0 {
            return true;
        }
        let [x_min, x_max, y_min, y_max] = self.extent;
        // On the outer edge, the positive ray also touches an exterior neighbour's closed
        // footprint. That neighbour is unresolved without the exterior attestation.
        !(ox > x_min && ox < x_max && oy > y_min && oy < y_max)
    }

    /// Find one touched omitted cell without enumerating the rectangle's area. The list is
    /// distinct and confined to `rect`. A full rectangle needs no search. Otherwise exact
    /// closed-footprint tests prune untouched regions, and integer bisection reaches a witness
    /// in at most 64 subdivisions (the two i32 coordinate widths). Both sides of shared edges
    /// are considered. No travel-distance or step budget is used to certify coverage.
    fn first_uninventoried(
        &self,
        ray: &PreparedRay,
        rect: CellRect,
        listed: &[CellCoord],
    ) -> Result<Option<CellCoord>, NumericError> {
        let width = i64::from(rect.max.x) - i64::from(rect.min.x) + 1;
        let height = i64::from(rect.max.y) - i64::from(rect.min.y) + 1;
        if listed.len() as u128 == width as u128 * height as u128
            || !ray.touches_cell_rect(&self.spacing, rect.min, rect.max)?
        {
            return Ok(None);
        }
        if width == 1 && height == 1 {
            return Ok(Some(rect.min));
        }
        let (low, high, first, second) = if width >= height && width > 1 {
            let mid = (i64::from(rect.min.x) + (width - 1) / 2) as i32;
            let (first, second): (Vec<_>, Vec<_>) =
                listed.iter().copied().partition(|cell| cell.x <= mid);
            (
                CellRect {
                    min: rect.min,
                    max: CellCoord {
                        x: mid,
                        y: rect.max.y,
                    },
                },
                CellRect {
                    min: CellCoord {
                        x: (i64::from(mid) + 1) as i32,
                        y: rect.min.y,
                    },
                    max: rect.max,
                },
                first,
                second,
            )
        } else {
            let mid = (i64::from(rect.min.y) + (height - 1) / 2) as i32;
            let (first, second): (Vec<_>, Vec<_>) =
                listed.iter().copied().partition(|cell| cell.y <= mid);
            (
                CellRect {
                    min: rect.min,
                    max: CellCoord {
                        x: rect.max.x,
                        y: mid,
                    },
                },
                CellRect {
                    min: CellCoord {
                        x: rect.min.x,
                        y: (i64::from(mid) + 1) as i32,
                    },
                    max: rect.max,
                },
                first,
                second,
            )
        };
        if let Some(cell) = self.first_uninventoried(ray, low, &first)? {
            return Ok(Some(cell));
        }
        self.first_uninventoried(ray, high, &second)
    }

    /// Traces the positive ray `origin + t * direction` (`t > 0`, world space
    /// `[east, north, up]`, `direction` toward the sun, any non-zero finite length).
    ///
    /// The identity check comes first: a mismatch is `Unknown` before any hit or `Clear`.
    pub fn trace(&self, query: &SurfaceQuery, origin: [f64; 3], direction: [f64; 3]) -> RayResult {
        if query.expected != self.identity {
            return RayResult::Unknown(vec![UnknownReason::IdentityMismatch {
                expected: Box::new(query.expected),
                observed: Box::new(self.identity),
            }]);
        }
        if query.triangulation != SurfaceTriangulation::CanonicalBase {
            return RayResult::Unknown(vec![UnknownReason::UnsupportedTriangulation(
                query.triangulation,
            )]);
        }
        if query.source == SurfaceSource::Canonical {
            match &self.identity.canonical {
                None => return RayResult::Unknown(vec![UnknownReason::CanonicalUnavailable]),
                Some(recipe) if recipe.triangulation != SurfaceTriangulation::CanonicalBase => {
                    return RayResult::Unknown(vec![UnknownReason::UnsupportedTriangulation(
                        recipe.triangulation,
                    )]);
                }
                Some(_) => {}
            }
        }
        let ray = match Ray::new(origin, direction) {
            Ok(ray) => ray,
            Err(error) => return RayResult::Unknown(vec![UnknownReason::InvalidRay(error)]),
        };
        let prepared = PreparedRay::new(ray);

        let mut nearest: Option<(CellCoord, SurfaceKind, CellHit)> = None;
        let mut unresolved: Vec<UnknownReason> = Vec::new();
        let mut proof = ClearProof {
            surface_cells: 0,
            known_empty_cells: 0,
            rendered_default_cells: 0,
        };
        let listed: Vec<_> = self
            .cells
            .keys()
            .chain(self.no_surface.iter())
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for &cell in &listed {
            match prepared.touches_cell(&self.spacing, cell) {
                Err(error) => {
                    unresolved.push(UnknownReason::Numeric { cell, error });
                    continue;
                }
                Ok(false) => continue,
                Ok(true) => {}
            }
            match self.resolve(cell, query.source) {
                Resolution::KnownNoSurface => proof.known_empty_cells += 1,
                Resolution::Unresolved(reason) => {
                    unresolved.push(UnknownReason::Cell { cell, reason });
                }
                Resolution::Surface { grid, kind } => {
                    match prepared.trace_cell(&self.spacing, cell, grid) {
                        Err(error) => unresolved.push(UnknownReason::Numeric { cell, error }),
                        Ok(trace) => {
                            proof.surface_cells += 1;
                            if kind.is_rendered_default() {
                                proof.rendered_default_cells += 1;
                            }
                            for triangle in trace.coplanar {
                                unresolved.push(UnknownReason::Coplanar {
                                    cell,
                                    triangle,
                                    surface: kind,
                                });
                            }
                            if let Some(hit) = trace.nearest {
                                let better = match &nearest {
                                    None => true,
                                    Some((_, _, current)) => hit.is_nearer_than(current),
                                };
                                if better {
                                    nearest = Some((cell, kind, hit));
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some((cell, surface, hit)) = nearest {
            match hit.position_on(prepared.ray()) {
                Ok(position) => {
                    return RayResult::Occluded(Occlusion {
                        cell,
                        triangle: hit.triangle,
                        surface,
                        t: hit.t,
                        position,
                    });
                }
                Err(error) => unresolved.push(UnknownReason::Numeric { cell, error }),
            }
        }
        match self.first_uninventoried(&prepared, self.domain, &listed) {
            Ok(Some(cell)) => unresolved.push(UnknownReason::Cell {
                cell,
                reason: CellUnknown::Uninventoried,
            }),
            Ok(None) => {}
            Err(error) => unresolved.push(UnknownReason::Numeric {
                cell: self.domain.min,
                error,
            }),
        }
        if !self.closed_world && self.path_leaves_domain(prepared.ray()) {
            unresolved.push(UnknownReason::OutsideInventory);
        }
        if unresolved.is_empty() {
            RayResult::Clear(proof)
        } else {
            RayResult::Unknown(unresolved)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::heightfield::{
        GRID_BYTES, Half, SAMPLES_PER_SIDE, encode_heights_le, sha256,
    };
    use super::*;

    const CELL: f64 = 32.0; // one sample step is exactly 1.0 world unit

    fn digest(seed: u8) -> Digest32 {
        [seed; 32]
    }

    fn ident() -> SnapshotIdentity {
        SnapshotIdentity {
            worldspace: 7,
            cell_size_bits: CELL.to_bits(),
            database: DatabaseIdentity {
                schema_revision: 3,
                content_hash: digest(1),
            },
            core_cache: CoreCacheIdentity {
                version: 2,
                hash: digest(2),
            },
            canonical: None,
        }
    }

    fn ident_canonical() -> SnapshotIdentity {
        SnapshotIdentity {
            canonical: Some(CanonicalRecipeIdentity {
                package: digest(3),
                catalog: digest(4),
                content_digest: digest(5),
                recipe_revision: 7,
                triangulation: SurfaceTriangulation::CanonicalBase,
            }),
            ..ident()
        }
    }

    fn cc(x: i32, y: i32) -> CellCoord {
        CellCoord { x, y }
    }

    fn rect(min: (i32, i32), max: (i32, i32)) -> CellRect {
        CellRect::new(cc(min.0, min.1), cc(max.0, max.1)).unwrap()
    }

    fn builder(id: SnapshotIdentity, min: (i32, i32), max: (i32, i32)) -> InventoryBuilder {
        InventoryBuilder::new(id, CELL, rect(min, max)).unwrap()
    }

    fn heights(f: impl Fn(usize, usize) -> f32) -> Vec<f32> {
        let mut out = Vec::new();
        for north in 0..SAMPLES_PER_SIDE {
            for east in 0..SAMPLES_PER_SIDE {
                out.push(f(east, north));
            }
        }
        out
    }

    fn put(
        b: &mut InventoryBuilder,
        cell: (i32, i32),
        authored: AuthoredStatus,
        kind: SurfaceKind,
        values: &[f32],
    ) {
        let bytes = encode_heights_le(values);
        let input = SurfaceInput {
            kind,
            raw: RawGrid {
                bytes: &bytes,
                expected_sha256: sha256(&bytes),
            },
        };
        b.add_cell(cc(cell.0, cell.1), authored, &[input]).unwrap();
    }

    fn put_authored(b: &mut InventoryBuilder, cell: (i32, i32), values: &[f32]) {
        put(
            b,
            cell,
            AuthoredStatus::VhgtPresent,
            SurfaceKind::Authored,
            values,
        );
    }

    fn flat(z: f32) -> Vec<f32> {
        heights(|_, _| z)
    }

    /// A north-south ridge of height 10 along sample column 16.
    fn ridge() -> Vec<f32> {
        heights(|east, _| if east == 16 { 10.0 } else { 0.0 })
    }

    fn close_world(b: &mut InventoryBuilder, id: SnapshotIdentity) {
        b.attest_closed_world(ClosedWorldAttestation::new(id, b.domain))
            .unwrap();
    }

    fn trace(inv: &WorldHeightInventory, origin: [f64; 3], direction: [f64; 3]) -> RayResult {
        inv.trace(
            &SurfaceQuery::canonical_base(
                *inv.identity(),
                SurfaceSource::AuthoredOrRenderedDefault,
            ),
            origin,
            direction,
        )
    }

    fn occluded(result: RayResult) -> Occlusion {
        match result {
            RayResult::Occluded(hit) => hit,
            other => panic!("expected Occluded, got {other:?}"),
        }
    }

    fn unknown(result: RayResult) -> Vec<UnknownReason> {
        match result {
            RayResult::Unknown(reasons) => reasons,
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    fn assert_clear(result: RayResult) -> ClearProof {
        match result {
            RayResult::Clear(proof) => proof,
            other => panic!("expected Clear, got {other:?}"),
        }
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    fn single_flat(z: f32, closed: bool) -> WorldHeightInventory {
        let mut b = builder(ident(), (0, 0), (0, 0));
        put_authored(&mut b, (0, 0), &flat(z));
        if closed {
            close_world(&mut b, ident());
        }
        b.build()
    }

    #[test]
    fn flat_world_clear_and_occluded() {
        let inv = single_flat(0.0, true);
        let proof = assert_clear(trace(&inv, [16.0, 16.0, 10.0], [0.3, 0.2, 1.0]));
        assert_eq!(proof.surface_cells, 1);
        let hit = occluded(trace(&inv, [16.5, 16.5, 10.0], [0.0, 0.0, -1.0]));
        near(hit.t, 10.0);
        near(hit.position[2], 0.0);
        assert_eq!(hit.cell, cc(0, 0));
        assert_eq!(hit.surface, SurfaceKind::Authored);
    }

    #[test]
    fn outside_is_unknown_without_a_closed_world() {
        let inv = single_flat(0.0, false);
        // Clear inside the cell, but the ray leaves the rectangle and nothing covers there.
        let reasons = unknown(trace(&inv, [16.0, 16.0, 10.0], [0.3, 0.2, 1.0]));
        assert!(reasons.contains(&UnknownReason::OutsideInventory));
        // A vertical ray stays inside the rectangle and can be proven clear.
        assert_clear(trace(&inv, [16.5, 16.5, 5.0], [0.0, 0.0, 1.0]));
        // A vertical ray outside the rectangle is unknown.
        let reasons = unknown(trace(&inv, [50.0, 16.5, 5.0], [0.0, 0.0, 1.0]));
        assert!(reasons.contains(&UnknownReason::OutsideInventory));
        // A ray that never touches the rectangle is still unknown: outside is not empty.
        let reasons = unknown(trace(&inv, [100.0, 100.0, 5.0], [1.0, 0.0, 1.0]));
        assert_eq!(reasons, vec![UnknownReason::OutsideInventory]);
    }

    #[test]
    fn closed_world_covers_the_outside_only() {
        let mut b = builder(ident(), (0, 0), (1, 0));
        put_authored(&mut b, (0, 0), &flat(0.0));
        close_world(&mut b, ident());
        let inv = b.build();
        // Cell (1,0) is inside the rectangle and unlisted: the closed world does not cover it.
        let reasons = unknown(trace(&inv, [48.0, 16.0, 5.0], [0.0, 0.0, 1.0]));
        assert!(reasons.contains(&UnknownReason::Cell {
            cell: cc(1, 0),
            reason: CellUnknown::Uninventoried
        }));
        assert!(!reasons.contains(&UnknownReason::OutsideInventory));
        // Far outside to the west: covered.
        assert_clear(trace(&inv, [-500.0, 16.0, 5.0], [-1.0, 0.0, 1.0]));
    }

    #[test]
    fn ridge_blocks_a_low_sun_not_a_high_sun() {
        let mut b = builder(ident(), (0, 0), (0, 0));
        put_authored(&mut b, (0, 0), &ridge());
        close_world(&mut b, ident());
        let inv = b.build();
        // West of the ridge, low sun in the east: 150.8 / 9.9 = 15.23.. on the west slope.
        let hit = occluded(trace(&inv, [2.0, 16.0, 1.0], [1.0, 0.0, 0.1]));
        near(hit.t, 150.8 / 9.9 - 2.0);
        assert_eq!(hit.cell, cc(0, 0));
        // High sun clears the crest (z = 27 at x = 15).
        assert_clear(trace(&inv, [2.0, 16.0, 1.0], [1.0, 0.0, 2.0]));
        // Sun reversed: the same receiver, sun in the west, is lit.
        assert_clear(trace(&inv, [2.0, 16.0, 1.0], [-1.0, 0.0, 0.1]));
        // East of the ridge, sun in the west: blocked by the east slope.
        let hit = occluded(trace(&inv, [30.0, 16.0, 1.0], [-1.0, 0.0, 0.1]));
        near(hit.t, 30.0 - 166.0 / 9.9);
    }

    #[test]
    fn one_sample_peak_is_found_and_not_erased_by_a_tolerance() {
        let mut b = builder(ident(), (0, 0), (0, 0));
        put_authored(
            &mut b,
            (0, 0),
            &heights(|east, north| if (east, north) == (16, 16) { 10.0 } else { 0.0 }),
        );
        close_world(&mut b, ident());
        let inv = b.build();
        // Along the column x = 16 the peak's slope reaches z = 5 half a sample before it.
        let hit = occluded(trace(&inv, [16.0, 1.0, 5.0], [0.0, 1.0, 0.0]));
        near(hit.t, 14.5);
        // Exactly through the peak touches it (closed convention).
        let hit = occluded(trace(&inv, [16.0, 1.0, 10.0], [0.0, 1.0, 0.0]));
        near(hit.t, 15.0);
        // One ulp above the peak is clear.
        let above = f64::from_bits(10f64.to_bits() + 1);
        assert_clear(trace(&inv, [16.0, 1.0, above], [0.0, 1.0, 0.0]));
    }

    #[test]
    fn negative_cells_and_corner_travel() {
        let mut b = builder(ident(), (-1, -1), (0, 0));
        put_authored(&mut b, (-1, -1), &flat(3.0));
        put_authored(&mut b, (0, 0), &flat(6.0));
        put_authored(&mut b, (-1, 0), &flat(0.0));
        put_authored(&mut b, (0, -1), &flat(0.0));
        close_world(&mut b, ident());
        let inv = b.build();
        let hit = occluded(trace(&inv, [-10.0, -10.0, 10.0], [0.0, 0.0, -1.0]));
        assert_eq!(hit.cell, cc(-1, -1));
        near(hit.t, 7.0);
        let hit = occluded(trace(&inv, [-30.0, -30.0, 10.0], [1.0, 1.0, -1.0]));
        assert_eq!(hit.cell, cc(-1, -1));
        near(hit.t, 7.0);
        // Through the shared corner (0,0) at height 60, then down onto cell (0,0): 100-4t = 6.
        let hit = occluded(trace(&inv, [-10.0, -10.0, 100.0], [1.0, 1.0, -4.0]));
        assert_eq!(hit.cell, cc(0, 0));
        near(hit.t, 23.5);
        near(hit.position[0], 13.5);
    }

    #[test]
    fn vertical_rays_at_vertices_and_corners() {
        let mut b = builder(ident(), (0, 0), (1, 1));
        let spike = heights(|east, north| if (east, north) == (16, 16) { 10.0 } else { 0.0 });
        put_authored(&mut b, (0, 0), &spike);
        for cell in [(1, 0), (0, 1), (1, 1)] {
            put_authored(&mut b, cell, &flat(1.0));
        }
        let inv = b.build();
        // Straight down onto a vertex shared by six triangles.
        let hit = occluded(trace(&inv, [16.0, 16.0, 20.0], [0.0, 0.0, -1.0]));
        near(hit.t, 10.0);
        // Down onto the lattice corner shared by four cells.
        let hit = occluded(trace(&inv, [32.0, 32.0, 5.0], [0.0, 0.0, -1.0]));
        near(hit.t, 4.0);
        // Up from below the terrain.
        let hit = occluded(trace(&inv, [40.5, 8.5, -5.0], [0.0, 0.0, 1.0]));
        near(hit.t, 6.0);
        // Up from above it, inside the rectangle, no closed world needed.
        assert_clear(trace(&inv, [40.5, 8.5, 5.0], [0.0, 0.0, 1.0]));
        // An axis-parallel horizontal ray over the terrain.
        let proof_or_unknown = trace(&inv, [1.0, 8.5, 50.0], [1.0, 0.0, 0.0]);
        assert!(matches!(proof_or_unknown, RayResult::Unknown(_)));
    }

    #[test]
    fn hole_inside_the_rectangle_is_unknown_but_an_occluder_still_wins() {
        let mut b = builder(ident(), (0, 0), (2, 0));
        put_authored(&mut b, (0, 0), &ridge());
        put_authored(&mut b, (2, 0), &flat(0.0));
        close_world(&mut b, ident());
        let inv = b.build();
        let hole = UnknownReason::Cell {
            cell: cc(1, 0),
            reason: CellUnknown::Uninventoried,
        };
        assert!(unknown(trace(&inv, [48.0, 16.0, 5.0], [0.0, 0.0, 1.0])).contains(&hole));
        assert_clear(trace(&inv, [16.0, 16.0, 15.0], [0.0, 0.0, 1.0]));
        // Over cell 0 east: the ray runs into the hole.
        assert!(unknown(trace(&inv, [20.0, 16.0, 5.0], [1.0, 0.0, 0.0])).contains(&hole));
        // The ridge is hit before the ray reaches the hole: occlusion is proof enough.
        let hit = occluded(trace(&inv, [2.0, 16.0, 1.0], [1.0, 0.0, 0.1]));
        assert_eq!(hit.cell, cc(0, 0));
        // Touching only the boundary shared with the unlisted cell is not discarded.
        assert!(unknown(trace(&inv, [32.0, 16.0, 5.0], [0.0, 0.0, 1.0])).contains(&hole));
    }

    #[test]
    fn absent_land_needs_a_certified_no_surface_attestation() {
        let build = |attest: bool| {
            let mut b = builder(ident(), (0, 0), (1, 0));
            put_authored(&mut b, (0, 0), &flat(0.0));
            b.add_cell(cc(1, 0), AuthoredStatus::AbsentLand, &[])
                .unwrap();
            if attest {
                b.attest_known_no_surface(KnownNoSurfaceAttestation::new(ident(), cc(1, 0)))
                    .unwrap();
            }
            close_world(&mut b, ident());
            b.build()
        };
        let reasons = unknown(trace(&build(false), [48.0, 16.0, 5.0], [0.0, 0.0, 1.0]));
        assert!(reasons.contains(&UnknownReason::Cell {
            cell: cc(1, 0),
            reason: CellUnknown::AbsentLandUnattested
        }));
        let attested = build(true);
        let proof = assert_clear(trace(&attested, [48.0, 16.0, 5.0], [0.0, 0.0, 1.0]));
        assert_eq!(proof.known_empty_cells, 1);
        assert_eq!(attested.cell_view(cc(1, 0)), Some(CellView::KnownNoSurface));
        // An unlisted cell can be attested too; a cell with a surface cannot.
        let mut b = builder(ident(), (0, 0), (1, 0));
        b.attest_known_no_surface(KnownNoSurfaceAttestation::new(ident(), cc(1, 0)))
            .unwrap();
        let bytes = encode_heights_le(&flat(0.0));
        let input = SurfaceInput {
            kind: SurfaceKind::Authored,
            raw: RawGrid {
                bytes: &bytes,
                expected_sha256: sha256(&bytes),
            },
        };
        assert_eq!(
            b.add_cell(cc(1, 0), AuthoredStatus::VhgtPresent, &[input]),
            Err(InventoryError::NoSurfaceAttestationConflict(cc(1, 0)))
        );
        let mut b = builder(ident(), (0, 0), (1, 0));
        b.add_cell(cc(0, 0), AuthoredStatus::MissingVhgt, &[])
            .unwrap();
        assert_eq!(
            b.attest_known_no_surface(KnownNoSurfaceAttestation::new(ident(), cc(0, 0))),
            Err(InventoryError::NoSurfaceAttestationConflict(cc(0, 0)))
        );
    }

    #[test]
    fn missing_vhgt_is_not_authored_and_the_default_is_explicit() {
        let down = [0.0, 0.0, -1.0];
        let origin = [16.5, 16.5, 10.0];
        // Missing VHGT and nothing supplied: unknown, not zero ground.
        let mut b = builder(ident(), (0, 0), (0, 0));
        b.add_cell(cc(0, 0), AuthoredStatus::MissingVhgt, &[])
            .unwrap();
        close_world(&mut b, ident());
        let reasons = unknown(trace(&b.build(), origin, down));
        assert!(reasons.contains(&UnknownReason::Cell {
            cell: cc(0, 0),
            reason: CellUnknown::SurfaceNotSupplied {
                authored: AuthoredStatus::MissingVhgt,
                wanted: SurfaceKind::RenderedDefault
            }
        }));
        // The rendered default is used, labelled as such.
        let mut b = builder(ident(), (0, 0), (0, 0));
        put(
            &mut b,
            (0, 0),
            AuthoredStatus::MissingVhgt,
            SurfaceKind::RenderedDefault,
            &flat(2.0),
        );
        let inv = b.build();
        let hit = occluded(trace(&inv, origin, down));
        near(hit.t, 8.0);
        assert_eq!(hit.surface, SurfaceKind::RenderedDefault);
        // A finite cache grid cannot be filed as authored for a cell that has no VHGT.
        let mut b = builder(ident(), (0, 0), (0, 0));
        let bytes = encode_heights_le(&flat(2.0));
        let input = SurfaceInput {
            kind: SurfaceKind::Authored,
            raw: RawGrid {
                bytes: &bytes,
                expected_sha256: sha256(&bytes),
            },
        };
        assert_eq!(
            b.add_cell(cc(0, 0), AuthoredStatus::MissingVhgt, &[input]),
            Err(InventoryError::ProvenanceMismatch {
                cell: cc(0, 0),
                kind: SurfaceKind::Authored,
                authored: AuthoredStatus::MissingVhgt
            })
        );
        // A rendered default cannot be filed for absent LAND either.
        let input = SurfaceInput {
            kind: SurfaceKind::RenderedDefault,
            raw: RawGrid {
                bytes: &bytes,
                expected_sha256: sha256(&bytes),
            },
        };
        assert!(matches!(
            b.add_cell(cc(0, 0), AuthoredStatus::AbsentLand, &[input]),
            Err(InventoryError::ProvenanceMismatch { .. })
        ));
    }

    #[test]
    fn malformed_nonfinite_and_stale_grids_keep_their_reason() {
        let good = encode_heights_le(&flat(0.0));
        let mut with_nan = flat(0.0);
        with_nan[5] = f32::NAN;
        let nan_bytes = encode_heights_le(&with_nan);
        let short = vec![0u8; 100];
        let wrong_digest = digest(0xAB);
        let cases: Vec<(&[u8], Digest32, GridRejection)> = vec![
            (
                short.as_slice(),
                sha256(&short),
                GridRejection::BadLength {
                    expected: GRID_BYTES,
                    found: 100,
                },
            ),
            (
                nan_bytes.as_slice(),
                sha256(&nan_bytes),
                GridRejection::NonFinite { index: 5 },
            ),
            (
                good.as_slice(),
                wrong_digest,
                GridRejection::Stale {
                    expected: wrong_digest,
                    observed: sha256(&good),
                },
            ),
        ];
        for (bytes, expected_sha256, rejection) in cases {
            let mut b = builder(ident(), (0, 0), (0, 0));
            b.add_cell(
                cc(0, 0),
                AuthoredStatus::VhgtPresent,
                &[SurfaceInput {
                    kind: SurfaceKind::Authored,
                    raw: RawGrid {
                        bytes,
                        expected_sha256,
                    },
                }],
            )
            .unwrap();
            close_world(&mut b, ident());
            let inv = b.build();
            let reasons = unknown(trace(&inv, [16.5, 16.5, 10.0], [0.0, 0.0, -1.0]));
            assert_eq!(
                reasons,
                vec![UnknownReason::Cell {
                    cell: cc(0, 0),
                    reason: CellUnknown::Rejected {
                        kind: SurfaceKind::Authored,
                        rejection
                    }
                }]
            );
        }
    }

    #[test]
    fn any_identity_difference_is_unknown_before_a_hit() {
        let id = ident_canonical();
        let mut b = builder(id, (0, 0), (0, 0));
        put_authored(&mut b, (0, 0), &flat(0.0));
        close_world(&mut b, id);
        let inv = b.build();
        let origin = [16.5, 16.5, 10.0];
        let down = [0.0, 0.0, -1.0];
        let source = SurfaceSource::AuthoredOrRenderedDefault;
        // The exact identity answers.
        occluded(inv.trace(&SurfaceQuery::canonical_base(id, source), origin, down));
        let recipe = id.canonical.unwrap();
        let mutations: Vec<(&str, SnapshotIdentity)> = vec![
            (
                "worldspace",
                SnapshotIdentity {
                    worldspace: 8,
                    ..id
                },
            ),
            (
                "schema",
                SnapshotIdentity {
                    database: DatabaseIdentity {
                        schema_revision: 4,
                        ..id.database
                    },
                    ..id
                },
            ),
            (
                "db hash",
                SnapshotIdentity {
                    database: DatabaseIdentity {
                        content_hash: digest(9),
                        ..id.database
                    },
                    ..id
                },
            ),
            (
                "core version",
                SnapshotIdentity {
                    core_cache: CoreCacheIdentity {
                        version: 3,
                        ..id.core_cache
                    },
                    ..id
                },
            ),
            (
                "core hash",
                SnapshotIdentity {
                    core_cache: CoreCacheIdentity {
                        hash: digest(9),
                        ..id.core_cache
                    },
                    ..id
                },
            ),
            (
                "no canonical",
                SnapshotIdentity {
                    canonical: None,
                    ..id
                },
            ),
            (
                "package",
                SnapshotIdentity {
                    canonical: Some(CanonicalRecipeIdentity {
                        package: digest(9),
                        ..recipe
                    }),
                    ..id
                },
            ),
            (
                "catalog",
                SnapshotIdentity {
                    canonical: Some(CanonicalRecipeIdentity {
                        catalog: digest(9),
                        ..recipe
                    }),
                    ..id
                },
            ),
            (
                "content digest",
                SnapshotIdentity {
                    canonical: Some(CanonicalRecipeIdentity {
                        content_digest: digest(9),
                        ..recipe
                    }),
                    ..id
                },
            ),
            (
                "recipe revision",
                SnapshotIdentity {
                    canonical: Some(CanonicalRecipeIdentity {
                        recipe_revision: 8,
                        ..recipe
                    }),
                    ..id
                },
            ),
            (
                "triangulation",
                SnapshotIdentity {
                    canonical: Some(CanonicalRecipeIdentity {
                        triangulation: SurfaceTriangulation::Unsupported(1),
                        ..recipe
                    }),
                    ..id
                },
            ),
        ];
        for (name, expected) in mutations {
            let reasons = unknown(inv.trace(
                &SurfaceQuery::canonical_base(expected, source),
                origin,
                down,
            ));
            assert_eq!(reasons.len(), 1, "{name}");
            assert!(
                matches!(&reasons[0], UnknownReason::IdentityMismatch { expected: e, observed: o }
                    if **e == expected && **o == id),
                "{name}"
            );
        }
        // And the inventory without a recipe against a query that expects one.
        let plain = single_flat(0.0, true);
        let reasons = unknown(plain.trace(&SurfaceQuery::canonical_base(id, source), origin, down));
        assert!(matches!(reasons[0], UnknownReason::IdentityMismatch { .. }));
    }

    #[test]
    fn authored_and_canonical_surfaces_coexist_with_explicit_provenance() {
        let id = ident_canonical();
        let mut b = builder(id, (0, 0), (0, 0));
        let authored = encode_heights_le(&flat(0.0));
        let canonical = encode_heights_le(&flat(5.0));
        b.add_cell(
            cc(0, 0),
            AuthoredStatus::VhgtPresent,
            &[
                SurfaceInput {
                    kind: SurfaceKind::Authored,
                    raw: RawGrid {
                        bytes: &authored,
                        expected_sha256: sha256(&authored),
                    },
                },
                SurfaceInput {
                    kind: SurfaceKind::CanonicalFromAuthored,
                    raw: RawGrid {
                        bytes: &canonical,
                        expected_sha256: sha256(&canonical),
                    },
                },
            ],
        )
        .unwrap();
        let inv = b.build();
        let origin = [16.5, 16.5, 10.0];
        let down = [0.0, 0.0, -1.0];
        let base = occluded(inv.trace(
            &SurfaceQuery::canonical_base(id, SurfaceSource::AuthoredOrRenderedDefault),
            origin,
            down,
        ));
        near(base.t, 10.0);
        assert_eq!(base.surface, SurfaceKind::Authored);
        let canon = occluded(inv.trace(
            &SurfaceQuery::canonical_base(id, SurfaceSource::Canonical),
            origin,
            down,
        ));
        near(canon.t, 5.0);
        assert_eq!(canon.surface, SurfaceKind::CanonicalFromAuthored);
        // Canonical surfaces need a recipe in the identity.
        let mut b = builder(ident(), (0, 0), (0, 0));
        let input = SurfaceInput {
            kind: SurfaceKind::CanonicalFromAuthored,
            raw: RawGrid {
                bytes: &canonical,
                expected_sha256: sha256(&canonical),
            },
        };
        assert_eq!(
            b.add_cell(cc(0, 0), AuthoredStatus::VhgtPresent, &[input]),
            Err(InventoryError::CanonicalWithoutRecipe {
                cell: cc(0, 0),
                kind: SurfaceKind::CanonicalFromAuthored
            })
        );
        // A canonical query on an inventory without a recipe, or with an unsupported one.
        let plain = single_flat(0.0, true);
        let reasons = unknown(plain.trace(
            &SurfaceQuery::canonical_base(ident(), SurfaceSource::Canonical),
            origin,
            down,
        ));
        assert_eq!(reasons, vec![UnknownReason::CanonicalUnavailable]);
        let odd = SnapshotIdentity {
            canonical: Some(CanonicalRecipeIdentity {
                triangulation: SurfaceTriangulation::Unsupported(2),
                ..id.canonical.unwrap()
            }),
            ..id
        };
        let mut b = builder(odd, (0, 0), (0, 0));
        put_authored(&mut b, (0, 0), &flat(0.0));
        let inv = b.build();
        let reasons = unknown(inv.trace(
            &SurfaceQuery::canonical_base(odd, SurfaceSource::Canonical),
            origin,
            down,
        ));
        assert_eq!(
            reasons,
            vec![UnknownReason::UnsupportedTriangulation(
                SurfaceTriangulation::Unsupported(2)
            )]
        );
        // The base surface of the same inventory is still queryable.
        occluded(inv.trace(
            &SurfaceQuery::canonical_base(odd, SurfaceSource::AuthoredOrRenderedDefault),
            origin,
            down,
        ));
    }

    #[test]
    fn unsupported_topology_queries_are_unknown() {
        let inv = single_flat(0.0, true);
        let query = SurfaceQuery {
            expected: ident(),
            source: SurfaceSource::AuthoredOrRenderedDefault,
            triangulation: SurfaceTriangulation::Unsupported(9),
        };
        let reasons = unknown(inv.trace(&query, [16.5, 16.5, 10.0], [0.0, 0.0, -1.0]));
        assert_eq!(
            reasons,
            vec![UnknownReason::UnsupportedTriangulation(
                SurfaceTriangulation::Unsupported(9)
            )]
        );
    }

    #[test]
    fn canonical_base_diagonal_is_not_bilinear_or_the_other_diagonal() {
        let mut b = builder(ident(), (0, 0), (0, 0));
        // a=(0,0)=0 b=(1,0)=1 c=(0,1)=1 d=(1,1)=0.
        put_authored(
            &mut b,
            (0, 0),
            &heights(|east, north| match (east, north) {
                (1, 0) | (0, 1) => 1.0,
                _ => 0.0,
            }),
        );
        let inv = b.build();
        let up = [0.0, 0.0, 1.0];
        // Centre of the quad: 1 on the b-c diagonal (bilinear 0.5, a-d diagonal 0).
        near(occluded(trace(&inv, [0.5, 0.5, 0.75], up)).t, 0.25);
        near(occluded(trace(&inv, [0.5, 0.5, 0.25], up)).t, 0.75);
        // Interior of [a,b,c]: z = x + y.
        let lower = occluded(trace(&inv, [0.25, 0.25, 0.0], up));
        near(lower.t, 0.5);
        assert_eq!(lower.triangle.half, Half::Lower);
        // Interior of [b,d,c]: z = 2 - x - y.
        let upper = occluded(trace(&inv, [0.75, 0.75, 0.0], up));
        near(upper.t, 0.5);
        assert_eq!(upper.triangle.half, Half::Upper);
    }

    #[test]
    fn seams_are_not_welded_and_far_upstream_cells_occlude() {
        // Two flat cells at different heights: the cliff between them is not a surface.
        let mut b = builder(ident(), (0, 0), (1, 0));
        put_authored(&mut b, (0, 0), &flat(0.0));
        put_authored(&mut b, (1, 0), &flat(10.0));
        close_world(&mut b, ident());
        let inv = b.build();
        assert_clear(trace(&inv, [16.0, 16.0, 20.0], [1.0, 0.0, -1.0]));
        assert_clear(trace(&inv, [40.0, 16.0, 5.0], [-1.0, 0.0, 0.0]));
        let hit = occluded(trace(&inv, [40.0, 16.0, 12.0], [-1.0, 0.0, -1.0]));
        assert_eq!(hit.cell, cc(1, 0));
        near(hit.t, 2.0);

        // A tall wall four cells upstream blocks a low sun with no camera involved.
        let mut b = builder(ident(), (-3, 0), (0, 0));
        for x in -2..=0 {
            put_authored(&mut b, (x, 0), &flat(0.0));
        }
        put_authored(
            &mut b,
            (-3, 0),
            &heights(|east, _| if east <= 4 { 50.0 } else { 0.0 }),
        );
        close_world(&mut b, ident());
        let inv = b.build();
        let hit = occluded(trace(&inv, [16.0, 16.0, 1.0], [-1.0, 0.0, 0.1]));
        assert_eq!(hit.cell, cc(-3, 0));
        assert!(hit.position[0] > -92.0 && hit.position[0] < -91.0);
        near(hit.position[2], 50.0 * (-91.0 - hit.position[0]));
    }

    #[test]
    fn direction_scale_does_not_change_the_answer() {
        let mut b = builder(ident(), (0, 0), (0, 0));
        put_authored(&mut b, (0, 0), &ridge());
        close_world(&mut b, ident());
        let inv = b.build();
        let base = occluded(trace(&inv, [2.0, 16.0, 1.0], [1.0, 0.0, 0.1]));
        for scale in [2f64.powi(10), 2f64.powi(-30), 2f64.powi(200), 0.5] {
            let hit = occluded(trace(&inv, [2.0, 16.0, 1.0], [scale, 0.0, 0.1 * scale]));
            assert_eq!((hit.cell, hit.triangle), (base.cell, base.triangle));
            near(hit.t * scale, base.t);
        }
        // A scale that is not a power of two only rounds the direction slightly.
        let hit = occluded(trace(&inv, [2.0, 16.0, 1.0], [3.0, 0.0, 0.3]));
        assert_eq!(hit.cell, base.cell);
        // Clear stays clear under scaling too.
        for scale in [1.0, 1e-3, 1e6, 2f64.powi(-400)] {
            assert_clear(trace(&inv, [2.0, 16.0, 1.0], [scale, 0.0, 2.0 * scale]));
        }
    }

    #[test]
    fn coplanar_and_self_contact_rays_are_not_optimistically_clear() {
        let inv = single_flat(0.0, true);
        let reasons = unknown(trace(&inv, [4.0, 4.0, 0.0], [1.0, 0.0, 0.0]));
        assert!(
            reasons
                .iter()
                .any(|reason| matches!(reason, UnknownReason::Coplanar { .. }))
        );
        // Parallel and strictly above: clear.
        assert_clear(trace(&inv, [4.0, 4.0, 0.5], [1.0, 0.0, 0.0]));
        // The receiver's own contact (t = 0) is excluded, nothing finite is skipped.
        assert_clear(trace(&inv, [16.5, 16.5, 0.0], [0.0, 0.0, 1.0]));
        assert_clear(trace(&inv, [16.5, 16.5, 0.0], [0.0, 0.0, -1.0]));
        // A hair below the surface going up is blocked, not skipped.
        let below = -f64::from_bits(1);
        near(
            occluded(trace(&inv, [16.5, 16.5, below], [0.0, 0.0, 1.0])).t,
            f64::from_bits(1),
        );
    }

    #[test]
    fn invalid_rays_are_unknown() {
        let inv = single_flat(0.0, true);
        assert_eq!(
            unknown(trace(&inv, [f64::NAN, 0.0, 0.0], [0.0, 0.0, 1.0])),
            vec![UnknownReason::InvalidRay(RayError::NonFiniteOrigin)]
        );
        assert_eq!(
            unknown(trace(&inv, [0.0; 3], [0.0, f64::INFINITY, 1.0])),
            vec![UnknownReason::InvalidRay(RayError::NonFiniteDirection)]
        );
        assert_eq!(
            unknown(trace(&inv, [0.0; 3], [0.0; 3])),
            vec![UnknownReason::InvalidRay(RayError::ZeroDirection)]
        );
    }

    #[test]
    fn cell_size_is_a_parameter_and_extreme_cells_work() {
        for (cell_size, mid) in [(4096.0f64, 2048.0), (30.0, 15.0), (1.0, 0.5)] {
            let id = SnapshotIdentity {
                cell_size_bits: cell_size.to_bits(),
                ..ident()
            };
            let mut b = InventoryBuilder::new(id, cell_size, rect((0, 0), (0, 0))).unwrap();
            put_authored(&mut b, (0, 0), &flat(0.0));
            let inv = b.build();
            let hit = occluded(trace(&inv, [mid, mid, 3.0], [0.0, 0.0, -1.0]));
            near(hit.t, 3.0);
        }
        // A 45 degree ray over 4096-unit cells lands 1000 units away.
        let id = SnapshotIdentity {
            cell_size_bits: 4096.0f64.to_bits(),
            ..ident()
        };
        let mut b = InventoryBuilder::new(id, 4096.0, rect((0, 0), (0, 0))).unwrap();
        put_authored(&mut b, (0, 0), &flat(0.0));
        let inv = b.build();
        let hit = occluded(trace(&inv, [2048.0, 2048.0, 1000.0], [1.0, 1.0, -1.0]));
        near(hit.t, 1000.0);
        near(hit.position[0], 3048.0);
        // Cell indices at the i32 limits.
        for edge in [i32::MAX, i32::MIN] {
            let mut b = builder(ident(), (edge, edge), (edge, edge));
            put_authored(&mut b, (edge, edge), &flat(0.0));
            let inv = b.build();
            let x = f64::from(edge) * CELL + 16.5;
            let hit = occluded(trace(&inv, [x, x, 4.0], [0.0, 0.0, -1.0]));
            near(hit.t, 4.0);
            assert_eq!(hit.cell, cc(edge, edge));
        }
    }

    #[test]
    fn builder_rejects_structural_mistakes() {
        assert_eq!(
            CellRect::new(cc(1, 0), cc(0, 0)),
            Err(InventoryError::InvertedDomain)
        );
        assert_eq!(
            InventoryBuilder::new(ident(), -1.0, rect((0, 0), (0, 0))).unwrap_err(),
            InventoryError::InvalidSpacing(SpacingError::NotFinitePositive)
        );
        assert_eq!(
            InventoryBuilder::new(ident(), 1e308, rect((0, 0), (i32::MAX, 0))).unwrap_err(),
            InventoryError::NonFiniteDomainCoordinates
        );
        let mut b = builder(ident(), (0, 0), (0, 0));
        assert_eq!(
            b.add_cell(cc(5, 5), AuthoredStatus::AbsentLand, &[]),
            Err(InventoryError::CellOutsideDomain(cc(5, 5)))
        );
        b.add_cell(cc(0, 0), AuthoredStatus::AbsentLand, &[])
            .unwrap();
        assert_eq!(
            b.add_cell(cc(0, 0), AuthoredStatus::AbsentLand, &[]),
            Err(InventoryError::DuplicateCell(cc(0, 0)))
        );
        // Attestations name the exact snapshot.
        let other = SnapshotIdentity {
            worldspace: 1,
            ..ident()
        };
        assert_eq!(
            b.attest_closed_world(ClosedWorldAttestation::new(other, b.domain)),
            Err(InventoryError::AttestationIdentityMismatch)
        );
        assert_eq!(
            b.attest_known_no_surface(KnownNoSurfaceAttestation::new(other, cc(0, 0))),
            Err(InventoryError::AttestationIdentityMismatch)
        );
        assert_eq!(
            b.attest_known_no_surface(KnownNoSurfaceAttestation::new(ident(), cc(9, 9))),
            Err(InventoryError::CellOutsideDomain(cc(9, 9)))
        );
        b.attest_known_no_surface(KnownNoSurfaceAttestation::new(ident(), cc(0, 0)))
            .unwrap();
        assert_eq!(
            b.attest_known_no_surface(KnownNoSurfaceAttestation::new(ident(), cc(0, 0))),
            Err(InventoryError::DuplicateAttestation(cc(0, 0)))
        );
        close_world(&mut b, ident());
        assert_eq!(
            b.attest_closed_world(ClosedWorldAttestation::new(ident(), b.domain)),
            Err(InventoryError::DuplicateClosedWorld)
        );
        // Two grids of the same kind in one cell.
        let mut b = builder(ident(), (0, 0), (0, 0));
        let bytes = encode_heights_le(&flat(0.0));
        let input = SurfaceInput {
            kind: SurfaceKind::Authored,
            raw: RawGrid {
                bytes: &bytes,
                expected_sha256: sha256(&bytes),
            },
        };
        assert_eq!(
            b.add_cell(cc(0, 0), AuthoredStatus::VhgtPresent, &[input, input]),
            Err(InventoryError::DuplicateSurfaceKind {
                cell: cc(0, 0),
                kind: SurfaceKind::Authored
            })
        );
        // Nothing was stored by the failed calls.
        assert_eq!(b.build().cell_view(cc(0, 0)), Some(CellView::Uninventoried));
    }

    #[test]
    fn outer_edge_requires_an_exterior_coverage_attestation() {
        let open = single_flat(0.0, false);
        for xy in [[0.0, 16.5], [32.0, 16.5], [16.5, 0.0], [32.0, 32.0]] {
            let reasons = unknown(trace(&open, [xy[0], xy[1], 1.0], [0.0, 0.0, 1.0]));
            assert!(reasons.contains(&UnknownReason::OutsideInventory));
            assert_clear(trace(
                &single_flat(0.0, true),
                [xy[0], xy[1], 1.0],
                [0.0, 0.0, 1.0],
            ));
        }
    }

    #[test]
    fn spacing_and_closed_world_scope_cannot_be_relabelled() {
        assert_eq!(
            InventoryBuilder::new(ident(), 64.0, rect((0, 0), (0, 0))).unwrap_err(),
            InventoryError::SpacingIdentityMismatch
        );
        let mut b = builder(ident(), (0, 0), (0, 0));
        assert_eq!(
            b.attest_closed_world(ClosedWorldAttestation::new(ident(), rect((-1, -1), (1, 1)))),
            Err(InventoryError::AttestationDomainMismatch)
        );
        put_authored(&mut b, (0, 0), &flat(0.0));
        let inv = b.build();
        let wrong_scale = SnapshotIdentity {
            cell_size_bits: 64.0f64.to_bits(),
            ..ident()
        };
        assert!(matches!(
            inv.trace(
                &SurfaceQuery::canonical_base(
                    wrong_scale,
                    SurfaceSource::AuthoredOrRenderedDefault
                ),
                [16.5, 16.5, 1.0],
                [0.0, 0.0, -1.0]
            ),
            RayResult::Unknown(_)
        ));
    }

    #[test]
    fn sparse_full_integer_domain_does_not_hide_a_remote_upstream_occluder() {
        let mut b = builder(ident(), (i32::MIN, i32::MIN), (i32::MAX, i32::MAX));
        put_authored(&mut b, (10_000, 0), &ridge());
        let inv = b.build();
        // The only provided cell is remote and behind unresolved space. It is still an
        // occlusion witness. The query must scan supplied content, not 2^64 omitted cells.
        let hit = occluded(trace(&inv, [16.5, 16.5, 1.0], [1.0, 0.0, 0.000001]));
        assert_eq!(hit.cell, cc(10_000, 0));
        let empty = builder(ident(), (i32::MIN, i32::MIN), (i32::MAX, i32::MAX)).build();
        assert!(
            unknown(trace(&empty, [16.5, 16.5, 1.0], [1.0, 0.0, 1.0]))
                .iter()
                .any(|reason| matches!(
                    reason,
                    UnknownReason::Cell {
                        reason: CellUnknown::Uninventoried,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn unknown_corner_neighbours_are_not_discarded() {
        let mut b = builder(ident(), (-1, -1), (0, 0));
        put_authored(&mut b, (-1, -1), &flat(0.0));
        put_authored(&mut b, (0, 0), &flat(0.0));
        close_world(&mut b, ident());
        let inv = b.build();
        // The missing other diagonal cells are touched at t=1 even though the ray only
        // traverses the two provided interiors. A zero-width positive-time gap still matters.
        assert!(
            unknown(trace(&inv, [-1.0, -1.0, 2.0], [1.0, 1.0, 1.0]))
                .iter()
                .any(|reason| matches!(
                    reason,
                    UnknownReason::Cell {
                        reason: CellUnknown::Uninventoried,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn canonical_rendered_default_keeps_missing_vhgt_provenance() {
        let id = ident_canonical();
        let mut b = builder(id, (0, 0), (0, 0));
        put(
            &mut b,
            (0, 0),
            AuthoredStatus::MissingVhgt,
            SurfaceKind::CanonicalFromRenderedDefault,
            &flat(2.0),
        );
        let inv = b.build();
        assert!(matches!(
            inv.cell_view(cc(0, 0)),
            Some(CellView::Listed {
                authored: AuthoredStatus::MissingVhgt,
                ..
            })
        ));
        let query = SurfaceQuery::canonical_base(id, SurfaceSource::Canonical);
        let hit = occluded(inv.trace(&query, [16.5, 16.5, 10.0], [0.0, 0.0, -1.0]));
        assert_eq!(hit.surface, SurfaceKind::CanonicalFromRenderedDefault);
        let proof = assert_clear(inv.trace(&query, [16.5, 16.5, 10.0], [0.0, 0.0, 1.0]));
        assert_eq!(proof.rendered_default_cells, 1);
        // A different requested surface is unavailable, despite the valid finite canonical grid.
        assert!(matches!(
            inv.trace(
                &SurfaceQuery::canonical_base(id, SurfaceSource::AuthoredOrRenderedDefault),
                [16.5, 16.5, 10.0],
                [0.0, 0.0, -1.0]
            ),
            RayResult::Unknown(_)
        ));
    }

    #[test]
    fn stale_canonical_bytes_never_fall_back_to_authored_heights() {
        let id = ident_canonical();
        let mut b = builder(id, (0, 0), (0, 0));
        let bytes = encode_heights_le(&flat(0.0));
        b.add_cell(
            cc(0, 0),
            AuthoredStatus::VhgtPresent,
            &[
                SurfaceInput {
                    kind: SurfaceKind::Authored,
                    raw: RawGrid {
                        bytes: &bytes,
                        expected_sha256: sha256(&bytes),
                    },
                },
                SurfaceInput {
                    kind: SurfaceKind::CanonicalFromAuthored,
                    raw: RawGrid {
                        bytes: &bytes,
                        expected_sha256: digest(99),
                    },
                },
            ],
        )
        .unwrap();
        let inv = b.build();
        assert!(
            unknown(inv.trace(
                &SurfaceQuery::canonical_base(id, SurfaceSource::Canonical),
                [16.5, 16.5, 1.0],
                [0.0, 0.0, -1.0]
            ))
            .iter()
            .any(|reason| matches!(
                reason,
                UnknownReason::Cell {
                    reason: CellUnknown::Rejected {
                        rejection: GridRejection::Stale { .. },
                        ..
                    },
                    ..
                }
            ))
        );
        occluded(inv.trace(
            &SurfaceQuery::canonical_base(id, SurfaceSource::AuthoredOrRenderedDefault),
            [16.5, 16.5, 1.0],
            [0.0, 0.0, -1.0],
        ));
    }
}

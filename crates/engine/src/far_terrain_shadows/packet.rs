//! Version-1 sparse canonical-base shadow packet. No GPU allocation or traversal.
//!
//! All records are explicit little-endian u32 words; see PORT.md for proposed bindings.
//! Directory order is signed (east, north), not unsigned bit-pattern order. Missing keys
//! are UNKNOWN, including inside DB bounds. The reader supplies raw VHGT provenance
//! through inventory AuthoredStatus: raw-tag validation or the bounded positive-only
//! projected-VHGT proof described in PORT.md. Cache/canonical samples alone never
//! establish authored presence; empty projected bytes remain Unknown.

use crate::far_terrain_shadows::{
    AuthoredStatus, CellCoord, CellView, GridRejection, HeightGrid, SnapshotIdentity, SurfaceKind,
    SurfaceState, SurfaceTriangulation, WorldHeightInventory,
};

pub const VERSION: u32 = 1;
pub const NONE: u32 = u32::MAX;
pub const HEADER_WORDS: usize = 16;
pub const DIRECTORY_WORDS: usize = 8;
pub const NODES_PER_PAGE: usize = 1365; // 32² + 16² + 8² + 4² + 2² + 1.

pub mod coverage {
    pub const HAS_SURFACE: u32 = 1;
    pub const COMPLETE: u32 = 2;
    pub const KNOWN_EMPTY: u32 = 4;
    pub const HAS_BOUNDS: u32 = 8;
}

/// Bits describe unresolved coverage, never a zero-height substitute.
pub mod reason {
    pub const UNLISTED: u32 = 1;
    pub const ABSENT_LAND_UNATTESTED: u32 = 1 << 1;
    pub const SURFACE_NOT_SUPPLIED: u32 = 1 << 2;
    pub const INVALID_BYTES: u32 = 1 << 3;
    pub const STALE_BYTES: u32 = 1 << 4;
    pub const INCOMPLETE_SOURCE: u32 = 1 << 5;
    pub const OUTSIDE_DOMAIN: u32 = 1 << 6;
    pub const POST_WELD_UNCONFIRMED: u32 = 1 << 7;
    pub const PAGE_PENDING: u32 = 1 << 8;
    pub const IDENTITY_MISMATCH: u32 = 1 << 9;
    pub const RECEIVER_UNRESOLVED: u32 = 1 << 10;
    pub const NUMERIC_UNCERTAINTY: u32 = 1 << 11;
    pub const WORK_BUDGET: u32 = 1 << 12;
}

/// Source facts, not inferred from row counts, domain bounds, or sample values.
#[derive(Debug, Clone, Copy)]
pub struct SourceStatus {
    pub complete: bool,
    pub record_data_complete: bool,
    /// Reader has selected final canonical grids AFTER worldspace seam welding.
    /// This is a reader assertion, not a weld performed by the packet builder.
    pub post_worldspace_weld: bool,
}

impl SourceStatus {
    fn complete(self) -> bool {
        self.complete && self.record_data_complete
    }
}

/// Reader-supplied VHGT status. Raw-tag validation via validate_inhouse_land can
/// distinguish absence from present-invalid (including a present-empty raw tag).
/// PresentValid also permits the bounded positive-only projected-VHGT proof in
/// PORT.md: nonempty winning projection, strict producer-order decode and exact
/// 33x33 cache-bit correspondence under the retained certified core guard. Empty
/// projected bytes remain Unknown; cache/canonical samples alone prove nothing.
/// This second path does not attest original ESM winner authority or source parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RawVhgtStatus {
    Unknown = 0,
    NoLand = 1,
    Absent = 2,
    PresentValid = 3,
    PresentInvalid = 4,
}

#[derive(Debug, Clone, Copy)]
pub struct CellInput {
    pub cell: CellCoord,
    pub vhgt: RawVhgtStatus,
}

/// [minimum f32 bits, maximum f32 bits, coverage bits, reason bits].
/// Bounds enclose KNOWN surfaces only. Skipping all coverage additionally requires COMPLETE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeNode(pub [u32; 4]);

impl RangeNode {
    pub fn unknown(reasons: u32) -> Self {
        Self([0, 0, 0, reasons])
    }

    pub fn empty() -> Self {
        Self([0, 0, coverage::COMPLETE | coverage::KNOWN_EMPTY, 0])
    }

    fn surface(min: f32, max: f32) -> Self {
        Self([
            min.to_bits(),
            max.to_bits(),
            coverage::HAS_SURFACE | coverage::COMPLETE | coverage::HAS_BOUNDS,
            0,
        ])
    }

    /// A missing child must be passed as unknown, never omitted from this array.
    pub fn merge(children: [Self; 4]) -> Self {
        let mut flags = 0;
        let mut reasons = 0;
        let mut complete = true;
        let mut empty = true;
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for Self(words) in children {
            reasons |= words[3];
            complete &= words[2] & coverage::COMPLETE != 0;
            empty &= words[2] & coverage::KNOWN_EMPTY != 0;
            flags |= words[2] & (coverage::HAS_SURFACE | coverage::HAS_BOUNDS);
            if words[2] & coverage::HAS_BOUNDS != 0 {
                min = min.min(f32::from_bits(words[0]));
                max = max.max(f32::from_bits(words[1]));
            }
        }
        if complete {
            flags |= coverage::COMPLETE;
        }
        if empty {
            flags |= coverage::KNOWN_EMPTY;
        }
        Self([
            if min.is_finite() { min.to_bits() } else { 0 },
            if max.is_finite() { max.to_bits() } else { 0 },
            flags,
            reasons,
        ])
    }
}

/// Each page preserves every sample. Leaves bound the four vertices of a base quad;
/// linear triangles have no interior extrema beyond their vertices.
fn hierarchy(grid: &HeightGrid) -> Vec<RangeNode> {
    let mut nodes = Vec::with_capacity(NODES_PER_PAGE);
    for north in 0..32 {
        for east in 0..32 {
            let heights = [
                grid.height(east, north).unwrap(),
                grid.height(east + 1, north).unwrap(),
                grid.height(east, north + 1).unwrap(),
                grid.height(east + 1, north + 1).unwrap(),
            ];
            nodes.push(RangeNode::surface(
                heights.into_iter().fold(f32::INFINITY, f32::min),
                heights.into_iter().fold(f32::NEG_INFINITY, f32::max),
            ));
        }
    }
    let mut start = 0;
    let mut side = 32;
    while side > 1 {
        let next = nodes.len();
        for y in 0..side / 2 {
            for x in 0..side / 2 {
                let a = start + 2 * y * side + 2 * x;
                nodes.push(RangeNode::merge([
                    nodes[a],
                    nodes[a + 1],
                    nodes[a + side],
                    nodes[a + side + 1],
                ]));
            }
        }
        start = next;
        side /= 2;
    }
    nodes
}

/// [cell east i32 bits, north i32 bits, page index, root node index,
/// coverage bits, reason bits, raw authored status, selected canonical provenance].
/// Raw authored field status: RawVhgtStatus discriminant; 0=unknown/unlisted.
/// Surface: 0=none, 1=canonical authored, 2=canonical rendered default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryEntry(pub [u32; DIRECTORY_WORDS]);

#[derive(Debug, PartialEq, Eq)]
pub enum PacketError {
    CanonicalRecipeMissingOrUnsupported,
    DuplicateCell(CellCoord),
    CellOutsideDomain(CellCoord),
    RawProvenanceMismatch(CellCoord),
    AddressOverflow,
}

#[derive(Debug)]
pub struct ShadowPacket {
    /// Full host identity must match before publishing any of these buffers.
    pub identity: SnapshotIdentity,
    pub header: [u32; HEADER_WORDS],
    pub directory: Vec<DirectoryEntry>,
    pub heights: Vec<u32>,
    pub nodes: Vec<RangeNode>,
}

impl ShadowPacket {
    /// Caller supplies the certified reader's list of cells, independent of camera or
    /// mesh residency. Omitted entries stay unknown; this list NEVER closes coverage.
    pub fn build(
        inventory: &WorldHeightInventory,
        cells: impl IntoIterator<Item = CellInput>,
        source: SourceStatus,
    ) -> Result<Self, PacketError> {
        let identity = *inventory.identity();
        if !matches!(identity.canonical, Some(recipe) if recipe.triangulation == SurfaceTriangulation::CanonicalBase)
        {
            return Err(PacketError::CanonicalRecipeMissingOrUnsupported);
        }
        let mut cells: Vec<_> = cells.into_iter().collect();
        cells.sort_unstable_by_key(|input| input.cell);
        for pair in cells.windows(2) {
            if pair[0].cell == pair[1].cell {
                return Err(PacketError::DuplicateCell(pair[0].cell));
            }
        }
        let domain = inventory.domain();
        let closed = source.complete() && inventory.is_closed_world();
        let mut packet = Self {
            identity,
            header: [
                VERSION,
                identity.worldspace,
                u32::try_from(cells.len()).map_err(|_| PacketError::AddressOverflow)?,
                0, // page count filled after packing
                domain.min().x as u32,
                domain.min().y as u32,
                domain.max().x as u32,
                domain.max().y as u32,
                u32::from(closed),
                if closed { 0 } else { reason::OUTSIDE_DOMAIN },
                reason::UNLISTED, // directory misses, regardless of source completeness
                u32::from(source.complete),
                u32::from(source.record_data_complete),
                u32::from(source.post_worldspace_weld),
                0,
                0,
            ],
            directory: Vec::with_capacity(cells.len()),
            heights: Vec::new(),
            nodes: Vec::new(),
        };
        for CellInput { cell, vhgt } in cells {
            let view = inventory
                .cell_view(cell)
                .ok_or(PacketError::CellOutsideDomain(cell))?;
            let mut entry = [
                cell.x as u32,
                cell.y as u32,
                NONE,
                NONE,
                0,
                0,
                vhgt as u32,
                0,
            ];
            let mut grid = None;
            if matches!(&view, CellView::KnownNoSurface)
                && !matches!(vhgt, RawVhgtStatus::Unknown | RawVhgtStatus::NoLand)
            {
                return Err(PacketError::RawProvenanceMismatch(cell));
            }
            match view {
                CellView::Uninventoried => {
                    if vhgt != RawVhgtStatus::Unknown {
                        return Err(PacketError::RawProvenanceMismatch(cell));
                    }
                    entry[5] = reason::UNLISTED;
                }
                CellView::KnownNoSurface if source.complete() => {
                    entry[4] = coverage::COMPLETE | coverage::KNOWN_EMPTY;
                }
                CellView::KnownNoSurface => entry[5] = reason::INCOMPLETE_SOURCE,
                CellView::Listed { authored, surfaces } => {
                    let compatible = matches!(
                        (authored, vhgt),
                        (AuthoredStatus::AbsentLand, RawVhgtStatus::NoLand)
                            | (AuthoredStatus::MissingVhgt, RawVhgtStatus::Absent)
                            | (
                                AuthoredStatus::VhgtPresent,
                                RawVhgtStatus::PresentValid | RawVhgtStatus::PresentInvalid
                            )
                    ) || vhgt == RawVhgtStatus::Unknown;
                    if !compatible {
                        return Err(PacketError::RawProvenanceMismatch(cell));
                    }
                    let wanted = match authored {
                        AuthoredStatus::AbsentLand => {
                            entry[5] = reason::ABSENT_LAND_UNATTESTED;
                            None
                        }
                        AuthoredStatus::MissingVhgt => {
                            entry[7] = 2;
                            Some(SurfaceKind::CanonicalFromRenderedDefault)
                        }
                        AuthoredStatus::VhgtPresent => {
                            entry[7] = 1;
                            Some(SurfaceKind::CanonicalFromAuthored)
                        }
                    };
                    if let Some(wanted) = wanted {
                        match surfaces.iter().find(|(kind, _)| *kind == wanted) {
                            None => entry[5] = reason::SURFACE_NOT_SUPPLIED,
                            Some((_, SurfaceState::Rejected(GridRejection::Stale { .. }))) => {
                                entry[5] = reason::STALE_BYTES;
                            }
                            Some((_, SurfaceState::Rejected(_))) => {
                                entry[5] = reason::INVALID_BYTES;
                            }
                            Some((_, SurfaceState::Valid(value))) => grid = Some(value),
                        }
                    }
                    if vhgt == RawVhgtStatus::PresentInvalid {
                        grid = None;
                        entry[5] |= reason::INVALID_BYTES;
                    } else if vhgt == RawVhgtStatus::Unknown {
                        grid = None;
                        entry[5] |= reason::INCOMPLETE_SOURCE;
                    }
                }
            }
            if let Some(grid) = grid {
                if source.post_worldspace_weld {
                    entry[2] = packet.header[3];
                    entry[3] = u32::try_from(packet.nodes.len() + NODES_PER_PAGE - 1)
                        .map_err(|_| PacketError::AddressOverflow)?;
                    // WGSL indexes every sample with u32; reserve NONE as sentinel.
                    u32::try_from(packet.heights.len() + grid.heights().len())
                        .ok()
                        .filter(|&end| end < NONE)
                        .ok_or(PacketError::AddressOverflow)?;
                    packet
                        .heights
                        .extend(grid.heights().iter().map(|v| v.to_bits()));
                    packet.nodes.extend(hierarchy(grid));
                    entry[4] = coverage::COMPLETE | coverage::HAS_SURFACE | coverage::HAS_BOUNDS;
                    packet.header[3] += 1;
                } else {
                    entry[5] |= reason::POST_WELD_UNCONFIRMED;
                }
            }
            packet.directory.push(DirectoryEntry(entry));
        }
        Ok(packet)
    }

    pub fn lookup(&self, cell: CellCoord) -> Option<&DirectoryEntry> {
        self.directory
            .binary_search_by_key(&(cell.x, cell.y), |entry| {
                (entry.0[0] as i32, entry.0[1] as i32)
            })
            .ok()
            .map(|index| &self.directory[index])
    }

    /// Binding payloads, each separately uploaded. Do not transmute Rust structs.
    pub fn buffers_le(&self) -> [Vec<u8>; 4] {
        fn bytes(words: impl IntoIterator<Item = u32>) -> Vec<u8> {
            words.into_iter().flat_map(u32::to_le_bytes).collect()
        }
        [
            bytes(self.header),
            bytes(self.directory.iter().flat_map(|entry| entry.0)),
            bytes(self.heights.iter().copied()),
            bytes(self.nodes.iter().flat_map(|node| node.0)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::far_terrain_shadows::*;

    fn identity() -> SnapshotIdentity {
        SnapshotIdentity {
            worldspace: 9,
            cell_size_bits: 32.0f64.to_bits(),
            database: DatabaseIdentity {
                schema_revision: 1,
                content_hash: [1; 32],
            },
            core_cache: CoreCacheIdentity {
                version: 1,
                hash: [2; 32],
            },
            canonical: Some(CanonicalRecipeIdentity {
                package: [3; 32],
                catalog: [4; 32],
                content_digest: [5; 32],
                recipe_revision: 1,
                triangulation: SurfaceTriangulation::CanonicalBase,
            }),
        }
    }

    fn source() -> SourceStatus {
        SourceStatus {
            complete: false,
            record_data_complete: false,
            post_worldspace_weld: true,
        }
    }

    fn input(x: i32) -> CellInput {
        CellInput {
            cell: CellCoord { x, y: -2 },
            vhgt: RawVhgtStatus::PresentValid,
        }
    }

    fn builder() -> InventoryBuilder {
        InventoryBuilder::new(
            identity(),
            32.0,
            CellRect::new(CellCoord { x: -2, y: -2 }, CellCoord { x: 3, y: -2 }).unwrap(),
        )
        .unwrap()
    }

    fn file(
        builder: &mut InventoryBuilder,
        x: i32,
        authored: AuthoredStatus,
        kind: SurfaceKind,
        samples: &[f32],
    ) {
        let bytes = encode_heights_le(samples);
        builder
            .add_cell(
                CellCoord { x, y: -2 },
                authored,
                &[SurfaceInput {
                    kind,
                    raw: RawGrid {
                        bytes: &bytes,
                        expected_sha256: sha256(&bytes),
                    },
                }],
            )
            .unwrap();
    }

    #[test]
    fn narrow_obstacle_survives_every_hierarchy_level_and_low_sun() {
        let mut samples = vec![0.0; SAMPLES_PER_CELL];
        samples[16 * SAMPLES_PER_SIDE + 16] = 8.0;
        let mut builder = builder();
        file(
            &mut builder,
            -1,
            AuthoredStatus::VhgtPresent,
            SurfaceKind::CanonicalFromAuthored,
            &samples,
        );
        let packet = ShadowPacket::build(&builder.build(), [input(-1)], source()).unwrap();
        assert_eq!(packet.heights[16 * SAMPLES_PER_SIDE + 16], 8.0f32.to_bits());
        let mut start = 0;
        for side in [32, 16, 8, 4, 2, 1] {
            assert_eq!(
                packet.nodes[start..start + side * side]
                    .iter()
                    .map(|node| f32::from_bits(node.0[1]))
                    .fold(0.0, f32::max),
                8.0
            );
            start += side * side;
        }
        assert_eq!(start, NODES_PER_PAGE);
        // Trace the packed samples themselves: a shallow sun ray still hits the one-sample ridge.
        let bytes: Vec<_> = packet
            .heights
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        let grid = HeightGrid::from_le_bytes(&bytes, &sha256(&bytes)).unwrap();
        let ray = PreparedRay::new(Ray::new([-31.0, -48.0, 1.0], [1.0, 0.0, 0.001]).unwrap());
        assert!(
            ray.trace_cell(&CellSpacing::new(32.0).unwrap(), input(-1).cell, &grid)
                .unwrap()
                .nearest
                .is_some()
        );
    }

    #[test]
    fn unknown_child_keeps_known_bounds_but_cannot_certify_clear() {
        let known = RangeNode::surface(0.0, 8.0);
        let parent = RangeNode::merge([
            known,
            RangeNode::empty(),
            known,
            RangeNode::unknown(reason::PAGE_PENDING),
        ]);
        assert_eq!(parent.0[2] & coverage::COMPLETE, 0);
        assert_ne!(parent.0[2] & coverage::HAS_BOUNDS, 0);
        assert_eq!(f32::from_bits(parent.0[1]), 8.0);
        assert_eq!(parent.0[3], reason::PAGE_PENDING);
        let grandparent = RangeNode::merge([parent, known, known, known]);
        assert_eq!(grandparent.0[2] & coverage::COMPLETE, 0);
    }

    #[test]
    fn incomplete_source_cannot_publish_attested_absence_or_close_bounds() {
        let mut builder = builder();
        let domain = CellRect::new(CellCoord { x: -2, y: -2 }, CellCoord { x: 3, y: -2 }).unwrap();
        builder
            .attest_known_no_surface(KnownNoSurfaceAttestation::new(identity(), input(0).cell))
            .unwrap();
        builder
            .attest_closed_world(ClosedWorldAttestation::new(identity(), domain))
            .unwrap();
        let inv = builder.build();
        let cell = CellInput {
            cell: input(0).cell,
            vhgt: RawVhgtStatus::NoLand,
        };
        let packet = ShadowPacket::build(&inv, [cell], source()).unwrap();
        assert_eq!(packet.header[8], 0);
        assert_eq!(packet.directory[0].0[4], 0);
        assert_eq!(packet.directory[0].0[5], reason::INCOMPLETE_SOURCE);
        assert!(packet.lookup(input(1).cell).is_none());
        for status in [
            SourceStatus {
                complete: true,
                ..source()
            },
            SourceStatus {
                record_data_complete: true,
                ..source()
            },
        ] {
            let packet = ShadowPacket::build(&inv, [cell], status).unwrap();
            assert_eq!(packet.header[8], 0);
            assert_eq!(packet.directory[0].0[4], 0);
        }
        let packet = ShadowPacket::build(
            &inv,
            [cell],
            SourceStatus {
                complete: true,
                record_data_complete: true,
                ..source()
            },
        )
        .unwrap();
        assert_eq!(packet.header[8], 1);
        assert_ne!(packet.directory[0].0[4] & coverage::KNOWN_EMPTY, 0);
        assert_eq!(packet.header[10], reason::UNLISTED); // complete != missing directory row is empty
    }

    #[test]
    fn raw_present_invalid_and_absent_are_distinct_even_with_finite_canonical_grids() {
        let mut builder = builder();
        let samples = vec![0.0; SAMPLES_PER_CELL];
        file(
            &mut builder,
            0,
            AuthoredStatus::VhgtPresent,
            SurfaceKind::CanonicalFromAuthored,
            &samples,
        );
        file(
            &mut builder,
            1,
            AuthoredStatus::MissingVhgt,
            SurfaceKind::CanonicalFromRenderedDefault,
            &samples,
        );
        let inv = builder.build();
        let packet = ShadowPacket::build(
            &inv,
            [
                CellInput {
                    cell: input(0).cell,
                    vhgt: RawVhgtStatus::PresentInvalid,
                },
                CellInput {
                    cell: input(1).cell,
                    vhgt: RawVhgtStatus::Absent,
                },
            ],
            source(),
        )
        .unwrap();
        assert_eq!(packet.directory[0].0[2], NONE);
        assert_eq!(packet.directory[0].0[5], reason::INVALID_BYTES);
        assert_eq!(
            packet.directory[0].0[6],
            RawVhgtStatus::PresentInvalid as u32
        );
        assert_eq!(packet.directory[1].0[2], 0);
        assert_eq!(packet.directory[1].0[7], 2);
        assert_eq!(packet.header[3], 1);
        assert!(matches!(
            ShadowPacket::build(&inv, [input(1)], source()),
            Err(PacketError::RawProvenanceMismatch(_))
        ));
    }

    #[test]
    fn signed_sparse_lookup_little_endian_and_post_weld_guard() {
        let mut builder = builder();
        let samples = vec![2.0; SAMPLES_PER_CELL];
        for x in [-2, 3] {
            file(
                &mut builder,
                x,
                AuthoredStatus::VhgtPresent,
                SurfaceKind::CanonicalFromAuthored,
                &samples,
            );
        }
        let inv = builder.build();
        let packet = ShadowPacket::build(&inv, [input(3), input(-2)], source()).unwrap();
        assert_eq!(packet.lookup(input(-2).cell).unwrap().0[2], 0);
        assert_eq!(packet.lookup(input(3).cell).unwrap().0[2], 1);
        assert!(packet.lookup(input(0).cell).is_none());
        let buffers = packet.buffers_le();
        assert_eq!(buffers[0].len(), HEADER_WORDS * 4);
        assert_eq!(buffers[1].len(), DIRECTORY_WORDS * 4 * 2);
        assert_eq!(&buffers[1][..4], &(-2i32).to_le_bytes());
        assert_eq!(buffers[2].len(), SAMPLES_PER_CELL * 4 * 2);
        assert_eq!(buffers[3].len(), NODES_PER_PAGE * 16 * 2);
        let packet = ShadowPacket::build(
            &inv,
            [input(-2)],
            SourceStatus {
                post_worldspace_weld: false,
                ..source()
            },
        )
        .unwrap();
        assert_eq!(packet.header[3], 0);
        assert_eq!(packet.directory[0].0[5], reason::POST_WELD_UNCONFIRMED);
    }
}

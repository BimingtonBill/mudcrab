//! Source-owned world partition for the optional receiving-height upload.
//! Unknown coverage is retained explicitly and cannot supply conservative known-height bounds.
//! Supplied cells are sorted once, then partitioned in place per split. Absent cells are never
//! enumerated. Counting and emission each visit at most 64 binary split levels per supplied cell.

use std::fmt;
use std::mem::size_of;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridRect {
    pub min: [i32; 2],
    pub max: [i32; 2],
}

impl GridRect {
    fn valid(self) -> bool {
        (0..2).all(|axis| self.min[axis] <= self.max[axis])
    }

    fn contains(self, cell: [i32; 2]) -> bool {
        (0..2).all(|axis| self.min[axis] <= cell[axis] && cell[axis] <= self.max[axis])
    }

    fn lengths(self) -> [i64; 2] {
        [0, 1].map(|axis| i64::from(self.max[axis]) - i64::from(self.min[axis]) + 1)
    }

    fn area(self) -> u128 {
        let [x, y] = self.lengths();
        // A valid full-i32-span rectangle contains at most 2^64 cells, which fits u128.
        (x as u128) * (y as u128)
    }

    fn split(self) -> Option<(usize, Self, Self)> {
        let lengths = self.lengths();
        if lengths == [1, 1] {
            return None;
        }
        let axis = usize::from(lengths[1] > lengths[0]);
        let midpoint = i64::from(self.min[axis]) + (lengths[axis] - 1) / 2;
        let mut left = self;
        let mut right = self;
        // Both values lie inside the original i32 interval; no signed subtraction occurs in i32.
        left.max[axis] = midpoint as i32;
        right.min[axis] = (midpoint + 1) as i32;
        Some((axis, left, right))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CellKind {
    Surface { page: u32, min: f32, max: f32 },
    KnownEmpty,
    Unknown { reasons: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellInput {
    pub cell: [i32; 2],
    pub kind: CellKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
    /// Bounds exist only when the entire rectangle is known and contains a surface.
    Branch {
        bounds: Option<[f32; 2]>,
    },
    Surface {
        page: u32,
        min: f32,
        max: f32,
    },
    KnownEmpty,
    Unknown {
        reasons: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    pub rect: GridRect,
    /// First index after this node's entire subtree; nodes.len() is the final escape.
    pub escape: u32,
    pub kind: NodeKind,
}

#[derive(Debug, PartialEq)]
pub struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    /// Immutable DFS-preorder nodes. The tree exposes no mutable node access.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn domain(&self) -> GridRect {
        self.nodes[0].rect
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    InvalidRect,
    ZeroMissingReason,
    OutsideDomain { cell: [i32; 2] },
    DuplicateCell { cell: [i32; 2] },
    NonFiniteSurface { cell: [i32; 2] },
    InvertedSurface { cell: [i32; 2] },
    ZeroUnknownReasons { cell: [i32; 2] },
    NodeLimit { limit: usize },
    CountOverflow,
    AddressLimit,
    AllocationLimit,
    AllocationFailed,
    DepthLimit,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "height tree build refused: {self:?}")
    }
}

impl std::error::Error for BuildError {}

fn check_allocation<T>(count: usize) -> Result<(), BuildError> {
    let bytes = count
        .checked_mul(size_of::<T>())
        .ok_or(BuildError::AllocationLimit)?;
    if bytes > isize::MAX as usize {
        return Err(BuildError::AllocationLimit);
    }
    Ok(())
}

fn check_node_count(count: usize, max_nodes: usize) -> Result<usize, BuildError> {
    if count > max_nodes {
        return Err(BuildError::NodeLimit { limit: max_nodes });
    }
    u32::try_from(count).map_err(|_| BuildError::AddressLimit)?;
    check_allocation::<Node>(count)?;
    Ok(count)
}

fn combined_count(left: usize, right: usize, max_nodes: usize) -> Result<usize, BuildError> {
    let count = left
        .checked_add(right)
        .and_then(|sum| sum.checked_add(1))
        .ok_or(BuildError::CountOverflow)?;
    check_node_count(count, max_nodes)
}

fn terminal(rect: GridRect, cells: &[CellInput], missing_reason: u32) -> Option<NodeKind> {
    if cells.is_empty() {
        return Some(NodeKind::Unknown {
            reasons: missing_reason,
        });
    }
    if rect.area() == cells.len() as u128
        && cells.iter().all(|cell| cell.kind == CellKind::KnownEmpty)
    {
        return Some(NodeKind::KnownEmpty);
    }
    if rect.min != rect.max {
        return None;
    }
    Some(match cells[0].kind {
        CellKind::Surface { page, min, max } => NodeKind::Surface { page, min, max },
        CellKind::KnownEmpty => NodeKind::KnownEmpty,
        CellKind::Unknown { reasons } => NodeKind::Unknown { reasons },
    })
}

fn partition(cells: &mut [CellInput], axis: usize, midpoint: i32) -> usize {
    let mut next_left = 0;
    for index in 0..cells.len() {
        if cells[index].cell[axis] <= midpoint {
            cells.swap(index, next_left);
            next_left += 1;
        }
    }
    next_left
}

fn count_nodes(
    rect: GridRect,
    cells: &mut [CellInput],
    missing_reason: u32,
    max_nodes: usize,
    depth: u8,
) -> Result<usize, BuildError> {
    if depth > 64 {
        return Err(BuildError::DepthLimit);
    }
    if terminal(rect, cells, missing_reason).is_some() {
        return check_node_count(1, max_nodes);
    }
    let (axis, left_rect, right_rect) = rect.split().ok_or(BuildError::InvalidRect)?;
    let midpoint = partition(cells, axis, left_rect.max[axis]);
    let (left_cells, right_cells) = cells.split_at_mut(midpoint);
    let left = count_nodes(left_rect, left_cells, missing_reason, max_nodes, depth + 1)?;
    let right = count_nodes(
        right_rect,
        right_cells,
        missing_reason,
        max_nodes,
        depth + 1,
    )?;
    combined_count(left, right, max_nodes)
}

#[derive(Clone, Copy)]
struct Summary {
    known: bool,
    bounds: Option<[f32; 2]>,
}

fn join(left: Summary, right: Summary) -> Summary {
    let known = left.known && right.known;
    let bounds = if !known {
        None
    } else {
        match (left.bounds, right.bounds) {
            (Some(a), Some(b)) => Some([a[0].min(b[0]), a[1].max(b[1])]),
            (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
            (None, None) => None,
        }
    };
    Summary { known, bounds }
}

fn emit(
    rect: GridRect,
    cells: &mut [CellInput],
    missing_reason: u32,
    nodes: &mut Vec<Node>,
) -> Result<Summary, BuildError> {
    let index = nodes.len();
    // Counting reserved the exact final length. Refuse an internal counting mismatch instead of
    // allowing Vec::push to start an unbounded/infallible allocation path.
    if index == nodes.capacity() {
        return Err(BuildError::AllocationLimit);
    }
    let kind = terminal(rect, cells, missing_reason);
    nodes.push(Node {
        rect,
        escape: 0,
        kind: kind.unwrap_or(NodeKind::Branch { bounds: None }),
    });
    let summary = match kind {
        Some(NodeKind::Surface { min, max, .. }) => Summary {
            known: true,
            bounds: Some([min, max]),
        },
        Some(NodeKind::KnownEmpty) => Summary {
            known: true,
            bounds: None,
        },
        Some(NodeKind::Unknown { .. }) => Summary {
            known: false,
            bounds: None,
        },
        None => {
            let (axis, left_rect, right_rect) = rect.split().ok_or(BuildError::InvalidRect)?;
            let midpoint = partition(cells, axis, left_rect.max[axis]);
            let (left_cells, right_cells) = cells.split_at_mut(midpoint);
            let left = emit(left_rect, left_cells, missing_reason, nodes)?;
            let right = emit(right_rect, right_cells, missing_reason, nodes)?;
            let combined = join(left, right);
            nodes[index].kind = NodeKind::Branch {
                bounds: combined.bounds,
            };
            combined
        }
        Some(NodeKind::Branch { .. }) => return Err(BuildError::InvalidRect),
    };
    nodes[index].escape = u32::try_from(nodes.len()).map_err(|_| BuildError::AddressLimit)?;
    Ok(summary)
}

/// Builds all-or-nothing. max_nodes bounds represented nodes, not the domain's cell count.
/// All allocation requests are checked and fallible; no absent grid cells are materialized.
pub fn build(
    domain: GridRect,
    inputs: &[CellInput],
    missing_reason: u32,
    max_nodes: usize,
) -> Result<Tree, BuildError> {
    if !domain.valid() {
        return Err(BuildError::InvalidRect);
    }
    if missing_reason == 0 {
        return Err(BuildError::ZeroMissingReason);
    }
    check_node_count(1, max_nodes)?;
    check_allocation::<CellInput>(inputs.len())?;
    for input in inputs {
        if !domain.contains(input.cell) {
            return Err(BuildError::OutsideDomain { cell: input.cell });
        }
        match input.kind {
            CellKind::Surface { min, max, .. } => {
                if !min.is_finite() || !max.is_finite() {
                    return Err(BuildError::NonFiniteSurface { cell: input.cell });
                }
                if min > max {
                    return Err(BuildError::InvertedSurface { cell: input.cell });
                }
            }
            CellKind::Unknown { reasons: 0 } => {
                return Err(BuildError::ZeroUnknownReasons { cell: input.cell });
            }
            _ => {}
        }
    }
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(inputs.len())
        .map_err(|_| BuildError::AllocationFailed)?;
    cells.extend_from_slice(inputs);
    // In-place unstable sorting allocates no sorting workspace; unique cell positions make the
    // initial order canonical. Later partition order cannot change any spatial split or leaf.
    cells.sort_unstable_by_key(|input| input.cell);
    for pair in cells.windows(2) {
        if pair[0].cell == pair[1].cell {
            return Err(BuildError::DuplicateCell { cell: pair[0].cell });
        }
    }
    let count = count_nodes(domain, &mut cells, missing_reason, max_nodes, 0)?;
    let mut nodes = Vec::new();
    nodes
        .try_reserve_exact(count)
        .map_err(|_| BuildError::AllocationFailed)?;
    emit(domain, &mut cells, missing_reason, &mut nodes)?;
    if nodes.len() != count {
        return Err(BuildError::CountOverflow);
    }
    Ok(Tree { nodes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn rect(min: [i32; 2], max: [i32; 2]) -> GridRect {
        GridRect { min, max }
    }

    fn cell(cell: [i32; 2], kind: CellKind) -> CellInput {
        CellInput { cell, kind }
    }

    fn surface(page: u32, min: f32, max: f32) -> CellKind {
        CellKind::Surface { page, min, max }
    }

    // Independent area/partition checks, deliberately separate from builder split()/area().
    fn area(rect: GridRect) -> u128 {
        let x = i128::from(rect.max[0]) - i128::from(rect.min[0]) + 1;
        let y = i128::from(rect.max[1]) - i128::from(rect.min[1]) + 1;
        assert!(x > 0 && y > 0);
        (x * y) as u128
    }

    fn inside(parent: GridRect, child: GridRect) -> bool {
        (0..2).all(|a| parent.min[a] <= child.min[a] && child.max[a] <= parent.max[a])
    }

    fn covers(rect: GridRect, cell: [i32; 2]) -> bool {
        (0..2).all(|a| rect.min[a] <= cell[a] && cell[a] <= rect.max[a])
    }

    fn topology(nodes: &[Node], index: usize, depth: usize) -> usize {
        assert!(depth <= 64);
        let node = nodes[index];
        assert!(node.escape as usize > index && node.escape as usize <= nodes.len());
        match node.kind {
            NodeKind::Branch { .. } => {
                let left = index + 1;
                let right = topology(nodes, left, depth + 1);
                assert!(right < node.escape as usize);
                let end = topology(nodes, right, depth + 1);
                assert_eq!(end, node.escape as usize);
                let a = nodes[left].rect;
                let b = nodes[right].rect;
                assert!(inside(node.rect, a) && inside(node.rect, b));
                assert!(
                    (0..2).any(|axis| { a.max[axis] < b.min[axis] || b.max[axis] < a.min[axis] })
                );
                assert_eq!(area(a) + area(b), area(node.rect));
                // Check the documented split independently, including tie-breaking.
                let widths = [0, 1].map(|axis| {
                    i128::from(node.rect.max[axis]) - i128::from(node.rect.min[axis]) + 1
                });
                let axis = usize::from(widths[1] > widths[0]);
                assert_eq!(a.min, node.rect.min);
                assert_eq!(b.max, node.rect.max);
                assert_eq!(i64::from(a.max[axis]) + 1, i64::from(b.min[axis]));
                assert_eq!(a.max[1 - axis], node.rect.max[1 - axis]);
                assert_eq!(b.min[1 - axis], node.rect.min[1 - axis]);
                let left_width = i128::from(a.max[axis]) - i128::from(a.min[axis]) + 1;
                assert_eq!(left_width, (widths[axis] + 1) / 2);
            }
            NodeKind::Surface { .. } => {
                assert_eq!(node.rect.min, node.rect.max);
                assert_eq!(node.escape as usize, index + 1);
            }
            _ => assert_eq!(node.escape as usize, index + 1),
        }
        node.escape as usize
    }

    fn leaf_kind(kind: NodeKind) -> Option<CellKind> {
        match kind {
            NodeKind::Branch { .. } => None,
            NodeKind::Surface { page, min, max } => Some(surface(page, min, max)),
            NodeKind::KnownEmpty => Some(CellKind::KnownEmpty),
            NodeKind::Unknown { reasons } => Some(CellKind::Unknown { reasons }),
        }
    }

    fn audit(tree: &Tree, inputs: &[CellInput], missing: u32) {
        let nodes = tree.nodes();
        assert!(!nodes.is_empty());
        assert_eq!(topology(nodes, 0, 0), nodes.len());
        assert_eq!(nodes.last().unwrap().escape as usize, nodes.len());
        let expected: BTreeMap<_, _> = inputs
            .iter()
            .map(|input| (input.cell, input.kind))
            .collect();
        // Small-domain enumeration is test-only and independent of the tree traversal.
        for y in tree.domain().min[1]..=tree.domain().max[1] {
            for x in tree.domain().min[0]..=tree.domain().max[0] {
                let kinds: Vec<_> = nodes
                    .iter()
                    .filter_map(|node| {
                        covers(node.rect, [x, y])
                            .then(|| leaf_kind(node.kind))
                            .flatten()
                    })
                    .collect();
                assert_eq!(kinds.len(), 1, "coverage at {x},{y}");
                assert_eq!(
                    kinds[0],
                    *expected
                        .get(&[x, y])
                        .unwrap_or(&CellKind::Unknown { reasons: missing })
                );
            }
        }
        for (index, node) in nodes.iter().enumerate() {
            let NodeKind::Branch { bounds } = node.kind else {
                continue;
            };
            let descendants = &nodes[index + 1..node.escape as usize];
            let mut unknown = false;
            let mut heights = Vec::new();
            for descendant in descendants {
                match descendant.kind {
                    NodeKind::Unknown { .. } => unknown = true,
                    NodeKind::Surface { min, max, .. } => heights.push([min, max]),
                    _ => {}
                }
            }
            if unknown || heights.is_empty() {
                assert_eq!(bounds, None);
            } else {
                let expected_min = heights.iter().map(|h| h[0]).fold(f32::INFINITY, f32::min);
                let expected_max = heights
                    .iter()
                    .map(|h| h[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                assert_eq!(bounds, Some([expected_min, expected_max]));
            }
        }
    }

    #[test]
    fn mixed_small_domain_keeps_explicit_and_missing_unknown_separate() {
        let inputs = [
            cell([-2, -1], surface(9, -30.0, 10.0)),
            cell([-1, -1], CellKind::KnownEmpty),
            cell([0, -1], CellKind::Unknown { reasons: 8 }),
            cell([1, 0], surface(99, 11.0, 80.0)),
            cell([-2, 1], CellKind::KnownEmpty),
        ];
        let tree = build(rect([-2, -1], [1, 1]), &inputs, 2, 200).unwrap();
        audit(&tree, &inputs, 2);
        assert_eq!(tree.nodes()[0].kind, NodeKind::Branch { bounds: None });
        assert!(tree.nodes().iter().any(|node| {
            matches!(node.kind, NodeKind::Unknown { reasons: 2 }) && area(node.rect) > 1
        }));
    }

    #[test]
    fn fully_known_empty_collapses_but_missing_cells_do_not() {
        let inputs: Vec<_> = (0..3)
            .flat_map(|x| (0..2).map(move |y| cell([x, y], CellKind::KnownEmpty)))
            .collect();
        let domain = rect([0, 0], [2, 1]);
        let tree = build(domain, &inputs, 1, 1).unwrap();
        assert_eq!(
            tree.nodes(),
            &[Node {
                rect: domain,
                escape: 1,
                kind: NodeKind::KnownEmpty
            }]
        );
        audit(&tree, &inputs, 1);
        let partial = build(domain, &inputs[..5], 1, 100).unwrap();
        audit(&partial, &inputs[..5], 1);
        assert_eq!(partial.nodes()[0].kind, NodeKind::Branch { bounds: None });
        assert_eq!(
            build(domain, &inputs[..5], 1, 1),
            Err(BuildError::NodeLimit { limit: 1 })
        );
    }

    #[test]
    fn known_surface_aggregation_includes_extrema_and_empty_cells() {
        let inputs = [
            cell([0, 0], surface(4, -100.0, -10.0)),
            cell([0, 1], CellKind::KnownEmpty),
            cell([1, 0], surface(7, 2.0, 200.0)),
            cell([1, 1], surface(8, -8.0, 3.0)),
        ];
        let tree = build(rect([0, 0], [1, 1]), &inputs, 1, 100).unwrap();
        audit(&tree, &inputs, 1);
        assert_eq!(
            tree.nodes()[0].kind,
            NodeKind::Branch {
                bounds: Some([-100.0, 200.0])
            }
        );
        let one = build(
            rect([-4, 9], [-4, 9]),
            &[cell([-4, 9], surface(123, -1.0, -1.0))],
            1,
            1,
        )
        .unwrap();
        assert_eq!(
            one.nodes()[0].kind,
            NodeKind::Surface {
                page: 123,
                min: -1.0,
                max: -1.0
            }
        );
        assert_eq!(one.nodes()[0].escape, 1);
    }

    #[test]
    fn every_small_input_permutation_builds_the_same_tree() {
        let mut inputs = [
            cell([-1, 0], surface(1, -1.0, 2.0)),
            cell([0, 0], CellKind::KnownEmpty),
            cell([1, 1], CellKind::Unknown { reasons: 16 }),
            cell([0, 1], surface(2, 10.0, 20.0)),
        ];
        let domain = rect([-1, -1], [1, 1]);
        let reference = build(domain, &inputs, 1, 100).unwrap();
        fn permute(inputs: &mut [CellInput], at: usize, domain: GridRect, reference: &Tree) {
            if at == inputs.len() {
                assert_eq!(&build(domain, inputs, 1, 100).unwrap(), reference);
                return;
            }
            for index in at..inputs.len() {
                inputs.swap(at, index);
                permute(inputs, at + 1, domain, reference);
                inputs.swap(at, index);
            }
        }
        permute(&mut inputs, 0, domain, &reference);
    }

    #[test]
    fn rejects_invalid_and_ambiguous_input_without_partial_output() {
        let domain = rect([0, 0], [1, 1]);
        assert_eq!(
            build(rect([1, 0], [0, 1]), &[], 1, 100),
            Err(BuildError::InvalidRect)
        );
        assert_eq!(
            build(domain, &[], 0, 100),
            Err(BuildError::ZeroMissingReason)
        );
        assert_eq!(
            build(domain, &[], 1, 0),
            Err(BuildError::NodeLimit { limit: 0 })
        );
        assert_eq!(
            build(domain, &[cell([2, 0], CellKind::KnownEmpty)], 1, 100),
            Err(BuildError::OutsideDomain { cell: [2, 0] })
        );
        assert_eq!(
            build(
                domain,
                &[cell([0, 0], CellKind::Unknown { reasons: 0 })],
                1,
                100
            ),
            Err(BuildError::ZeroUnknownReasons { cell: [0, 0] })
        );
        let duplicate = [
            cell([0, 0], CellKind::KnownEmpty),
            cell([0, 0], surface(1, 0.0, 1.0)),
        ];
        assert_eq!(
            build(domain, &duplicate, 1, 100),
            Err(BuildError::DuplicateCell { cell: [0, 0] })
        );
        assert_eq!(
            build(domain, &[cell([0, 0], surface(1, 2.0, 1.0))], 1, 100),
            Err(BuildError::InvertedSurface { cell: [0, 0] })
        );
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for kind in [surface(1, bad, 1.0), surface(1, 0.0, bad)] {
                assert_eq!(
                    build(domain, &[cell([0, 0], kind)], 1, 100),
                    Err(BuildError::NonFiniteSurface { cell: [0, 0] })
                );
            }
        }
        let sparse = [cell([0, 0], surface(0, 0.0, 1.0))];
        let complete = build(domain, &sparse, 1, 100).unwrap();
        let count = complete.nodes().len();
        assert_eq!(
            build(domain, &sparse, 1, count - 1),
            Err(BuildError::NodeLimit { limit: count - 1 })
        );
        assert_eq!(build(domain, &sparse, 1, count).unwrap(), complete);
    }

    #[test]
    fn checks_count_address_and_allocation_arithmetic() {
        assert_eq!(
            combined_count(usize::MAX, 1, usize::MAX),
            Err(BuildError::CountOverflow)
        );
        assert_eq!(
            check_allocation::<Node>(usize::MAX),
            Err(BuildError::AllocationLimit)
        );
        assert_eq!(
            check_allocation::<CellInput>(usize::MAX),
            Err(BuildError::AllocationLimit)
        );
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            check_node_count(u32::MAX as usize + 1, usize::MAX),
            Err(BuildError::AddressLimit)
        );
    }

    #[test]
    fn sparse_full_signed_span_never_enumerates_missing_cells() {
        let domain = rect([i32::MIN; 2], [i32::MAX; 2]);
        let inputs = [
            cell([i32::MIN; 2], surface(77, -50.0, 100.0)),
            cell([i32::MAX; 2], CellKind::KnownEmpty),
            cell([i32::MIN, i32::MAX], CellKind::Unknown { reasons: 32 }),
        ];
        let tree = build(domain, &inputs, 4, 400).unwrap();
        assert_eq!(topology(tree.nodes(), 0, 0), tree.nodes().len());
        assert!(tree.nodes().len() <= 1 + inputs.len() * 128);
        assert_eq!(tree.nodes()[0].kind, NodeKind::Branch { bounds: None });
        for input in inputs
            .iter()
            .copied()
            .chain([cell([0, 0], CellKind::Unknown { reasons: 4 })])
        {
            let leaves: Vec<_> = tree
                .nodes()
                .iter()
                .filter_map(|node| {
                    covers(node.rect, input.cell)
                        .then(|| leaf_kind(node.kind))
                        .flatten()
                })
                .collect();
            assert_eq!(leaves, vec![input.kind]);
        }
        let empty = build(domain, &[], 4, 1).unwrap();
        assert_eq!(
            empty.nodes(),
            &[Node {
                rect: domain,
                escape: 1,
                kind: NodeKind::Unknown { reasons: 4 }
            }]
        );
        let reversed: Vec<_> = inputs.into_iter().rev().collect();
        assert_eq!(build(domain, &reversed, 4, 400).unwrap(), tree);
    }
}

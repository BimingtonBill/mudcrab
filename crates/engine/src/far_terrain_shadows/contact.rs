//! Canonical-surface receiver contact, independent of simplified rendered height.
//!
//! Inputs must belong to one exact certified snapshot and its post-worldspace-weld
//! canonical base surface. This type neither establishes provenance nor welds grids.
//! Identity exclusions below prove shared contact features, not floating-point GPU
//! intersection correctness. No height lift, finite ray bias or obstacle skip occurs.

use super::heightfield::{CELL_INTERVALS, CellCoord, Half, HeightGrid, TriangleId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactError {
    /// Coordinates must be finite and within the half-open canonical cell.
    InvalidSampleCoordinates,
}

/// A receiver DEFINED on the canonical base triangle at the supplied sample XY.
/// The visible fragment's simplified Z is deliberately not an input.
#[derive(Debug, Clone)]
pub struct CanonicalContact {
    cell: CellCoord,
    triangle: TriangleId,
    fractions: [f32; 2],
    /// Quad samples [a,b,c,d], copied from the certified post-weld grid.
    heights: [f32; 4],
}

/// Compare x+y with 1 without rounding the sum onto the diagonal. Each
/// subtraction below is exact (Sterbenz); the both-small branch is strict.
fn diagonal_order(x: f64, y: f64) -> std::cmp::Ordering {
    if x >= 0.5 {
        y.partial_cmp(&(1.0 - x)).unwrap()
    } else if y >= 0.5 {
        x.partial_cmp(&(1.0 - y)).unwrap()
    } else {
        std::cmp::Ordering::Less
    }
}

fn samples(grid: &HeightGrid, q: TriangleId) -> Option<[f32; 4]> {
    let x = usize::from(q.quad_east);
    let y = usize::from(q.quad_north);
    if x >= CELL_INTERVALS || y >= CELL_INTERVALS {
        return None;
    }
    Some([
        grid.height(x, y)?,
        grid.height(x + 1, y)?,
        grid.height(x, y + 1)?,
        grid.height(x + 1, y + 1)?,
    ])
}

impl CanonicalContact {
    pub fn from_sample_xy(
        cell: CellCoord,
        sample_xy: [f32; 2],
        post_weld_grid: &HeightGrid,
    ) -> Result<Self, ContactError> {
        if !sample_xy
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v < CELL_INTERVALS as f32)
        {
            return Err(ContactError::InvalidSampleCoordinates);
        }
        let [x, y] = sample_xy;
        let fractions = [x - x.floor(), y - y.floor()];
        let half = if diagonal_order(f64::from(fractions[0]), f64::from(fractions[1]))
            == std::cmp::Ordering::Greater
        {
            Half::Upper
        } else {
            Half::Lower
        };
        let triangle = TriangleId {
            quad_east: x.floor() as u8,
            quad_north: y.floor() as u8,
            half,
        };
        Ok(Self {
            cell,
            triangle,
            fractions,
            heights: samples(post_weld_grid, triangle).unwrap(),
        })
    }

    pub fn cell(&self) -> CellCoord {
        self.cell
    }

    /// Exact copied canonical quad samples [a,b,c,d], for the numeric query mirror.
    pub fn heights(&self) -> [f32; 4] {
        self.heights
    }

    pub fn triangle(&self) -> TriangleId {
        self.triangle
    }

    pub fn fractions(&self) -> [f32; 2] {
        self.fractions
    }

    /// Approximate interpolation for ordinary numeric fixtures only. Arbitrary
    /// f64 arithmetic here is NOT an exact contact origin for the ray oracle.
    pub fn approximate_height_for_fixture(&self) -> f64 {
        let [x, y] = self.fractions.map(f64::from);
        let [a, b, c, d] = self.heights.map(f64::from);
        match self.triangle.half {
            Half::Lower => a + x * (b - a) + y * (c - a),
            Half::Upper => d + (1.0 - x) * (c - d) + (1.0 - y) * (b - d),
        }
    }

    /// Approximate canonical-plane dot product for fixture inspection. It may
    /// round to zero; callers MUST NOT use it as a proof of coplanarity or of
    /// noncoplanarity. Robust traversal supplies its own bounded predicate.
    pub fn approximate_plane_dot_for_fixture(&self, direction: [f64; 3], spacing: f64) -> f64 {
        let [a, b, c, d] = self.heights.map(f64::from);
        let (east, north) = match self.triangle.half {
            Half::Lower => (b - a, c - a),
            Half::Upper => (d - c, d - b),
        };
        spacing * direction[2] - east * direction[0] - north * direction[1]
    }

    /// Sufficient, epsilon-free proof that a target triangle contains the SAME
    /// canonical 3D contact. False can mean unproved; it never authorizes a skip.
    /// Caller must first establish identical snapshot/recipe/world identity.
    /// Exclusion is valid only after a separate noncoplanarity proof.
    pub fn same_3d_contact_triangle(
        &self,
        target_cell: CellCoord,
        target: TriangleId,
        target_grid: &HeightGrid,
    ) -> bool {
        let Some(h) = samples(target_grid, target) else {
            return false;
        };
        let dx = i64::from(target_cell.x) - i64::from(self.cell.x);
        let dy = i64::from(target_cell.y) - i64::from(self.cell.y);
        if dx.abs() > 1 || dy.abs() > 1 {
            return false;
        }
        // Bounded cell deltas avoid cell*32 overflow and absolute-float collapse.
        let tx = i64::from(self.triangle.quad_east)
            - i64::from(target.quad_east)
            - dx * CELL_INTERVALS as i64;
        let ty = i64::from(self.triangle.quad_north)
            - i64::from(target.quad_north)
            - dy * CELL_INTERVALS as i64;
        let [fx, fy] = self.fractions.map(f64::from);
        let x = tx as f64 + fx;
        let y = ty as f64 + fy;
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return false;
        }
        let diagonal = diagonal_order(x, y);
        if (target.half == Half::Lower && diagonal == std::cmp::Ordering::Greater)
            || (target.half == Half::Upper && diagonal == std::cmp::Ordering::Less)
        {
            return false;
        }
        if fx == 0.0 && fy == 0.0 {
            let index = (x as usize) + 2 * (y as usize);
            return h[index] == self.heights[0];
        }
        if fx == 0.0 {
            // Source contact lies on the ascending-north a-c edge.
            let index = x as usize;
            return (x == 0.0 || x == 1.0)
                && ty == 0
                && h[index] == self.heights[0]
                && h[index + 2] == self.heights[2];
        }
        if fy == 0.0 {
            // Source contact lies on the ascending-east a-b edge.
            let index = 2 * (y as usize);
            return (y == 0.0 || y == 1.0)
                && tx == 0
                && h[index] == self.heights[0]
                && h[index + 1] == self.heights[1];
        }
        if target_cell != self.cell || tx != 0 || ty != 0 {
            return false;
        }
        if diagonal_order(fx, fy) == std::cmp::Ordering::Equal {
            return h[1] == self.heights[1] && h[2] == self.heights[2];
        }
        let indices = match self.triangle.half {
            Half::Lower => [0, 1, 2],
            Half::Upper => [1, 3, 2],
        };
        target.half == self.triangle.half && indices.iter().all(|&i| h[i] == self.heights[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::far_terrain_shadows::heightfield::{SAMPLES_PER_CELL, encode_heights_le, sha256};

    fn grid(f: impl Fn(usize, usize) -> f32) -> HeightGrid {
        let values: Vec<_> = (0..SAMPLES_PER_CELL).map(|i| f(i % 33, i / 33)).collect();
        let bytes = encode_heights_le(&values);
        HeightGrid::from_le_bytes(&bytes, &sha256(&bytes)).unwrap()
    }

    fn tri(x: u8, y: u8, half: Half) -> TriangleId {
        TriangleId {
            quad_east: x,
            quad_north: y,
            half,
        }
    }

    #[test]
    fn interior_and_diagonal_identity_do_not_skip_distinct_obstacles() {
        let g = grid(|x, y| (x + y) as f32);
        let cell = CellCoord { x: -2, y: 9 };
        let c = CanonicalContact::from_sample_xy(cell, [4.25, 7.25], &g).unwrap();
        assert!(c.same_3d_contact_triangle(cell, tri(4, 7, Half::Lower), &g));
        assert!(!c.same_3d_contact_triangle(cell, tri(4, 7, Half::Upper), &g));
        // Even the immediately adjoining quad remains a distinct obstacle.
        assert!(!c.same_3d_contact_triangle(cell, tri(5, 7, Half::Lower), &g));
        let c = CanonicalContact::from_sample_xy(cell, [4.25, 7.75], &g).unwrap();
        for half in [Half::Lower, Half::Upper] {
            assert!(c.same_3d_contact_triangle(cell, tri(4, 7, half), &g));
        }
        assert_eq!(c.approximate_height_for_fixture(), 12.0);
    }

    #[test]
    fn edge_and_vertex_require_same_3d_contact_at_discontinuous_seam() {
        let west = CellCoord { x: -1, y: 0 };
        let east = CellCoord { x: 0, y: 0 };
        let g = grid(|_, y| y as f32);
        let raised = grid(|_, y| y as f32 + 1.0);
        let edge = CanonicalContact::from_sample_xy(east, [0.0, 4.25], &g).unwrap();
        assert!(edge.same_3d_contact_triangle(west, tri(31, 4, Half::Upper), &g));
        assert!(!edge.same_3d_contact_triangle(west, tri(31, 4, Half::Upper), &raised));
        assert!(!edge.same_3d_contact_triangle(west, tri(31, 4, Half::Lower), &g));
        let vertex = CanonicalContact::from_sample_xy(east, [0.0, 4.0], &g).unwrap();
        for q in [tri(31, 3, Half::Upper), tri(31, 4, Half::Lower)] {
            assert!(vertex.same_3d_contact_triangle(west, q, &g));
            assert!(!vertex.same_3d_contact_triangle(west, q, &raised));
        }
    }

    #[test]
    fn negative_extreme_cells_and_horizontal_edge_do_not_overflow() {
        let g = grid(|x, _| x as f32);
        let cell = CellCoord {
            x: i32::MIN,
            y: i32::MAX,
        };
        let c = CanonicalContact::from_sample_xy(cell, [3.25, 4.0], &g).unwrap();
        assert!(c.same_3d_contact_triangle(cell, tri(3, 3, Half::Upper), &g));
        assert!(!c.same_3d_contact_triangle(
            CellCoord {
                x: i32::MAX,
                y: i32::MIN
            },
            tri(3, 3, Half::Upper),
            &g
        ));
    }

    #[test]
    fn rounded_sum_does_not_create_false_diagonal_contact() {
        // f32 addition rounds onto 1, while exact represented inputs are below it.
        let below_half = f32::from_bits(0.5_f32.to_bits() - 1);
        assert_eq!(0.5 + below_half, 1.0);
        assert_eq!(
            diagonal_order(0.5, f64::from(below_half)),
            std::cmp::Ordering::Less
        );
        let g = grid(|_, _| 0.0);
        let c = CanonicalContact::from_sample_xy(CellCoord { x: 0, y: 0 }, [0.5, below_half], &g)
            .unwrap();
        assert!(!c.same_3d_contact_triangle(c.cell(), tri(0, 0, Half::Upper), &g));
    }

    #[test]
    fn canonical_plane_fixture_covers_flat_ramp_and_low_sun() {
        let cell = CellCoord { x: 0, y: 0 };
        let flat = grid(|_, _| 3.0);
        let c = CanonicalContact::from_sample_xy(cell, [1.25, 1.25], &flat).unwrap();
        assert_eq!(
            c.approximate_plane_dot_for_fixture([1.0, 0.0, 0.0], 1.0),
            0.0
        );
        assert!(c.approximate_plane_dot_for_fixture([1.0, 0.0, 1e-8], 1.0) > 0.0);
        let ramp = grid(|x, _| x as f32);
        let c = CanonicalContact::from_sample_xy(cell, [1.75, 1.75], &ramp).unwrap();
        assert_eq!(
            c.approximate_plane_dot_for_fixture([1.0, 0.0, 1.0], 1.0),
            0.0
        );
        assert!(c.approximate_plane_dot_for_fixture([1.0, 0.0, 0.5], 1.0) < 0.0);
    }

    #[test]
    fn invalid_half_open_coordinates_refused() {
        let g = grid(|_, _| 0.0);
        for xy in [
            [32.0, 0.0],
            [-1.0, 0.0],
            [f32::NAN, 0.0],
            [0.0, f32::INFINITY],
        ] {
            assert_eq!(
                CanonicalContact::from_sample_xy(CellCoord { x: 0, y: 0 }, xy, &g).unwrap_err(),
                ContactError::InvalidSampleCoordinates
            );
        }
    }
}

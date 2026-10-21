//! CPU numeric mirror of `receiving.wgsl`'s canonical triangle predicate.
//!
//! This mirrors interval composition and branch decisions; it does not execute WGSL
//! or prove GPU behavior. Contact and target must belong to the same certified
//! post-weld snapshot. Direction XY is in canonical sample units, Z in height units.

use crate::far_terrain_shadows::arithmetic::Interval;
use crate::far_terrain_shadows::contact::CanonicalContact;
use crate::far_terrain_shadows::{CellCoord, Half, HeightGrid, TriangleId};

pub const NUMERIC: u32 = 2048;
pub const COPLANAR: u32 = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriangleOutcome {
    Miss,
    Hit { t: [f32; 2] },
    Unknown { reasons: u32 },
}

fn numeric() -> TriangleOutcome {
    TriangleOutcome::Unknown { reasons: NUMERIC }
}

fn coplanar() -> TriangleOutcome {
    TriangleOutcome::Unknown {
        reasons: NUMERIC | COPLANAR,
    }
}

fn slope(h: [f32; 4], half: Half, axis: usize) -> Interval {
    let p = Interval::point;
    match half {
        Half::Lower => p(h[axis + 1]).minus(p(h[0])),
        Half::Upper => p(h[3]).minus(p(h[2 - axis])),
    }
}

fn surface(h: [f32; 4], half: Half, x: Interval, y: Interval) -> Interval {
    let p = Interval::point;
    match half {
        Half::Lower => p(h[0])
            .plus(p(h[1]).minus(p(h[0])).times(x))
            .plus(p(h[2]).minus(p(h[0])).times(y)),
        Half::Upper => p(h[3])
            .plus(p(h[2]).minus(p(h[3])).times(p(1.0).minus(x)))
            .plus(p(h[1]).minus(p(h[3])).times(p(1.0).minus(y))),
    }
}

pub fn classify_triangle(
    contact: &CanonicalContact,
    target_cell: CellCoord,
    triangle: TriangleId,
    target_grid: &HeightGrid,
    direction_sample: [f32; 3],
) -> TriangleOutcome {
    let p = Interval::point;
    let dx = i64::from(target_cell.x) - i64::from(contact.cell().x);
    let dy = i64::from(target_cell.y) - i64::from(contact.cell().y);
    if dx.abs() > 524_287 || dy.abs() > 524_287 {
        return numeric();
    }
    let qe = usize::from(triangle.quad_east);
    let qn = usize::from(triangle.quad_north);
    if qe >= 32 || qn >= 32 {
        return numeric();
    }
    let h = [
        target_grid.height(qe, qn).unwrap(),
        target_grid.height(qe + 1, qn).unwrap(),
        target_grid.height(qe, qn + 1).unwrap(),
        target_grid.height(qe + 1, qn + 1).unwrap(),
    ];
    let [fx, fy] = contact.fractions();
    // Quad+fraction reconstructs the original f32 sample exactly for contacts
    // constructed from sample XY; no large absolute world float is formed.
    let sample_x = f32::from(contact.triangle().quad_east) + fx;
    let sample_y = f32::from(contact.triangle().quad_north) + fy;
    let origin_x = p(sample_x).minus(p((dx * 32) as f32));
    let origin_y = p(sample_y).minus(p((dy * 32) as f32));
    let contact_height = surface(contact.heights(), contact.triangle().half, p(fx), p(fy));
    let [dir_x, dir_y, dir_z] = direction_sample;
    let denominator = p(dir_z)
        .minus(slope(h, triangle.half, 0).times(p(dir_x)))
        .minus(slope(h, triangle.half, 1).times(p(dir_y)));
    if !denominator.ok() {
        return numeric();
    }
    if contact.same_3d_contact_triangle(target_cell, triangle, target_grid) {
        return if denominator.0 > 0.0 || denominator.1 < 0.0 {
            TriangleOutcome::Miss
        } else {
            coplanar()
        };
    }
    let x = origin_x.minus(p(qe as f32));
    let y = origin_y.minus(p(qn as f32));
    let offset = contact_height.minus(surface(h, triangle.half, x, y));
    if !offset.ok() {
        return numeric();
    }
    if denominator.0 <= 0.0 && denominator.1 >= 0.0 {
        return if denominator == Interval::ZERO && (offset.0 > 0.0 || offset.1 < 0.0) {
            TriangleOutcome::Miss
        } else {
            coplanar()
        };
    }
    let t = offset.negated().divided_by(denominator);
    if !t.ok() {
        return numeric();
    }
    if t.1 <= 0.0 {
        return TriangleOutcome::Miss;
    }
    let tx = x.plus(t.times(p(dir_x)));
    let ty = y.plus(t.times(p(dir_y)));
    if !tx.ok() || !ty.ok() {
        return numeric();
    }
    if tx.1 < 0.0 || ty.1 < 0.0 || tx.0 > 1.0 || ty.0 > 1.0 {
        return TriangleOutcome::Miss;
    }
    let sum = tx.plus(ty);
    if !sum.ok() {
        return numeric();
    }
    if (triangle.half == Half::Lower && sum.0 > 1.0)
        || (triangle.half == Half::Upper && sum.1 < 1.0)
    {
        return TriangleOutcome::Miss;
    }
    if t.0 > 0.0
        && tx.0 >= 0.0
        && ty.0 >= 0.0
        && tx.1 <= 1.0
        && ty.1 <= 1.0
        && ((triangle.half == Half::Lower && sum.1 <= 1.0)
            || (triangle.half == Half::Upper && sum.0 >= 1.0))
    {
        TriangleOutcome::Hit { t: [t.0, t.1] }
    } else {
        numeric()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::far_terrain_shadows::{
        CellSpacing, PreparedRay, Ray, SAMPLES_PER_CELL, encode_heights_le, sha256,
    };

    const CELL: CellCoord = CellCoord { x: 0, y: 0 };

    fn grid(f: impl Fn(usize, usize) -> f32) -> HeightGrid {
        let values: Vec<_> = (0..SAMPLES_PER_CELL).map(|i| f(i % 33, i / 33)).collect();
        let bytes = encode_heights_le(&values);
        HeightGrid::from_le_bytes(&bytes, &sha256(&bytes)).unwrap()
    }

    fn triangle(x: u8, y: u8, half: Half) -> TriangleId {
        TriangleId {
            quad_east: x,
            quad_north: y,
            half,
        }
    }

    fn exact_fixture_ray(contact: &CanonicalContact, direction: [f32; 3]) -> PreparedRay {
        // Every fixture below has hand-chosen dyadic XY and heights, with exact
        // f64 interpolation. This helper is NOT a general contact-origin proof.
        let [fx, fy] = contact.fractions().map(f64::from);
        let origin = [
            f64::from(contact.triangle().quad_east) + fx,
            f64::from(contact.triangle().quad_north) + fy,
            contact.approximate_height_for_fixture(),
        ];
        PreparedRay::new(Ray::new(origin, direction.map(f64::from)).unwrap())
    }

    fn assert_hit_matches_exact(
        grid: &HeightGrid,
        xy: [f32; 2],
        target: TriangleId,
        dir: [f32; 3],
    ) {
        let contact = CanonicalContact::from_sample_xy(CELL, xy, grid).unwrap();
        assert!(!contact.same_3d_contact_triangle(CELL, target, grid));
        let exact = exact_fixture_ray(&contact, dir)
            .trace_cell(&CellSpacing::new(32.0).unwrap(), CELL, grid)
            .unwrap()
            .nearest
            .unwrap();
        assert_eq!(exact.triangle, target);
        let TriangleOutcome::Hit { t } = classify_triangle(&contact, CELL, target, grid, dir)
        else {
            panic!("expected bounded hit");
        };
        assert!(f64::from(t[0]) <= exact.t && exact.t <= f64::from(t[1]));
        assert!(t[0] > 0.0);
    }

    #[test]
    fn narrow_ridge_low_sun_distinct_upstream_triangle_matches_exact() {
        let g = grid(|x, _| if x == 4 { 2.0 } else { 0.0 });
        assert_hit_matches_exact(
            &g,
            [0.25, 0.25],
            triangle(3, 0, Half::Lower),
            [1.0, 0.0, 0.0625],
        );
    }

    #[test]
    fn upper_half_selection_matches_exact_and_lower_half_misses() {
        let g = grid(|x, _| if x == 4 { 2.0 } else { 0.0 });
        let xy = [0.25, 0.9375];
        let dir = [1.0, 0.0, 0.0625];
        assert_hit_matches_exact(&g, xy, triangle(3, 0, Half::Upper), dir);
        let c = CanonicalContact::from_sample_xy(CELL, xy, &g).unwrap();
        assert_eq!(
            classify_triangle(&c, CELL, triangle(3, 0, Half::Lower), &g, dir),
            TriangleOutcome::Miss
        );
    }

    #[test]
    fn flat_source_vertical_sun_excludes_only_zero_contact() {
        let g = grid(|_, _| 0.0);
        let c = CanonicalContact::from_sample_xy(CELL, [0.25, 0.25], &g).unwrap();
        assert_eq!(
            classify_triangle(&c, CELL, c.triangle(), &g, [0.0, 0.0, 1.0]),
            TriangleOutcome::Miss
        );
        let exact = exact_fixture_ray(&c, [0.0, 0.0, 1.0])
            .trace_cell(&CellSpacing::new(32.0).unwrap(), CELL, &g)
            .unwrap();
        assert!(exact.nearest.is_none() && exact.coplanar.is_empty());
    }

    #[test]
    fn canonical_plane_tangent_stays_unknown() {
        let g = grid(|x, _| x as f32);
        let c = CanonicalContact::from_sample_xy(CELL, [0.25, 0.25], &g).unwrap();
        assert_eq!(
            classify_triangle(&c, CELL, c.triangle(), &g, [1.0, 0.0, 1.0]),
            coplanar()
        );
        let exact = exact_fixture_ray(&c, [1.0, 0.0, 1.0])
            .trace_cell(&CellSpacing::new(32.0).unwrap(), CELL, &g)
            .unwrap();
        assert!(exact.coplanar.contains(&c.triangle()));
    }

    #[test]
    fn close_positive_obstacle_has_no_finite_bias() {
        let g = grid(|x, _| if x == 2 { 1.0 } else { 0.0 });
        assert_hit_matches_exact(
            &g,
            [1.0 - 1.0 / 1024.0, 0.25],
            triangle(1, 0, Half::Lower),
            [1.0, 0.0, 0.5],
        );
    }

    #[test]
    fn unsupported_relative_range_and_subnormal_direction_are_unknown() {
        let g = grid(|_, _| 0.0);
        let c = CanonicalContact::from_sample_xy(CELL, [0.25, 0.25], &g).unwrap();
        assert_eq!(
            classify_triangle(
                &c,
                CellCoord { x: 524_288, y: 0 },
                c.triangle(),
                &g,
                [0.0, 0.0, 1.0]
            ),
            numeric()
        );
        assert_eq!(
            classify_triangle(&c, CELL, c.triangle(), &g, [0.0, 0.0, f32::from_bits(1)]),
            numeric()
        );
    }
}

//! Height grids and the exact ray geometry over them.
//!
//! One cell is `32 x 32` intervals, `33 x 33` samples, stored row-major from south to north
//! (`index = north * 33 + east`) and read from raw little-endian `f32` bytes the caller decoded
//! (runtime unit heights, not raw VHGT deltas). This module never decodes a vanilla record.
//!
//! # Coordinates
//!
//! World space is Creation space, `[east, north, up]`. The position of global sample index `n`
//! (`cell * 32 + local`, an `i64`, so no cell index overflows) on one axis is `n as f64 * step`
//! with `step = cell_size / 32`. Neighbouring cells compute their shared edge from the same `n`
//! and therefore get the identical `f64`; the vertices are defined to be exactly those `f64`
//! values. This defines the mathematical canonical grid. The converter at `d349ba57`
//! (`crates/converter/src/cluster/terrain_geometry.rs`) instead places anchor-relative vertices
//! into `f32` buffers. A reader must establish coordinate/recipe compatibility before treating
//! this reference as the rendered surface; shared topology alone does not establish that.
//!
//! # Surface triangulation
//!
//! The canonical BASE surface splits every quad `a=(x,y) b=(x+1,y) c=(x,y+1) d=(x+1,y+1)` into
//! `[a, b, c]` and `[b, d, c]` (the `b`-`c` diagonal), checked against
//! `d349ba57:crates/converter/src/cluster/terrain_geometry.rs:321-327`
//! (`indices.extend_from_slice(&[a, b, c, b, d, c])`). This is the converter's base surface only.
//! It is NOT a statement about the simplified GPU cluster triangles nor the baked LOD fan
//! topology; a caller that needs to compare against the drawn geometry must supply the final
//! post-weld surface, and this module never welds, mutates or fills seams between cells.
//!
//! # Exactness
//!
//! Every predicate is decided in the internal exact dyadic arithmetic module on `f64` inputs, so the
//! outcome has no tolerance and no rounding: a ray `1 ulp` above a single-sample peak is clear, a
//! ray through the peak touches it, and scaling the direction by a power of two changes nothing.
//! Hit positions and `t` are reported as `f64` approximations of the exact value. Coverage and
//! geometric exclusion use the same exact predicates, with no approximate rejection filter.

use sha2::{Digest, Sha256};
use std::cmp::Ordering;

use self::exact::{Dy, Rat};

pub const CELL_INTERVALS: usize = 32;
pub const SAMPLES_PER_SIDE: usize = CELL_INTERVALS + 1;
pub const SAMPLES_PER_CELL: usize = SAMPLES_PER_SIDE * SAMPLES_PER_SIDE;
/// Length of one raw grid: `33 * 33` little-endian `f32`.
pub const GRID_BYTES: usize = SAMPLES_PER_CELL * 4;
const CELL_INTERVALS_I64: i64 = CELL_INTERVALS as i64;

/// A SHA-256 digest.
pub type Digest32 = [u8; 32];

/// SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> Digest32 {
    Sha256::digest(bytes).into()
}

/// Encodes unit heights as the raw little-endian `f32` bytes [`HeightGrid::from_le_bytes`] reads.
pub fn encode_heights_le(heights: &[f32]) -> Vec<u8> {
    heights.iter().flat_map(|h| h.to_le_bytes()).collect()
}

/// Exterior cell coordinate (`x` east, `y` north).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacingError {
    /// The cell size is not finite and positive.
    NotFinitePositive,
    /// `cell_size / 32` is zero or subnormal.
    StepNotNormal,
}

/// World units per cell, configurable and never a hardcoded game value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellSpacing {
    cell_size: f64,
    step: f64,
}

impl CellSpacing {
    pub fn new(cell_size: f64) -> Result<Self, SpacingError> {
        if !(cell_size.is_finite() && cell_size > 0.0) {
            return Err(SpacingError::NotFinitePositive);
        }
        let step = cell_size / CELL_INTERVALS as f64;
        if !step.is_normal() {
            return Err(SpacingError::StepNotNormal);
        }
        Ok(Self { cell_size, step })
    }

    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }

    /// Distance between two samples, `cell_size / 32` (exact: a division by a power of two).
    pub fn step(&self) -> f64 {
        self.step
    }

    /// Position on one axis of global sample index `index` (may be non-finite for absurd sizes).
    pub fn coord(&self, index: i64) -> f64 {
        index as f64 * self.step
    }

    /// `[x_min, x_max, y_min, y_max]` of one cell, or `None` if a corner is not finite.
    pub fn cell_extent(&self, cell: CellCoord) -> Option<[f64; 4]> {
        let gx = i64::from(cell.x) * CELL_INTERVALS_I64;
        let gy = i64::from(cell.y) * CELL_INTERVALS_I64;
        let extent = [
            self.coord(gx),
            self.coord(gx + CELL_INTERVALS_I64),
            self.coord(gy),
            self.coord(gy + CELL_INTERVALS_I64),
        ];
        extent.iter().all(|v| v.is_finite()).then_some(extent)
    }
}

/// Why raw grid bytes were not accepted. Kept as data so a caller can report it, never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridRejection {
    BadLength {
        expected: usize,
        found: usize,
    },
    /// The bytes do not hash to the digest the certified reader expected.
    Stale {
        expected: Digest32,
        observed: Digest32,
    },
    /// Sample `index` (row-major) is NaN or infinite.
    NonFinite {
        index: usize,
    },
}

/// A validated `33 x 33` grid of finite runtime unit heights.
///
/// The only constructor checks length, SHA-256 and finiteness. The grid says nothing about
/// whether the heights are authored or a rendered default: the caller states that provenance
/// when it files the grid (see `inventory`).
#[derive(Debug, Clone, PartialEq)]
pub struct HeightGrid {
    heights: Vec<f32>,
}

impl HeightGrid {
    /// Checks in this order: length, digest, finiteness.
    pub fn from_le_bytes(bytes: &[u8], expected_sha256: &Digest32) -> Result<Self, GridRejection> {
        if bytes.len() != GRID_BYTES {
            return Err(GridRejection::BadLength {
                expected: GRID_BYTES,
                found: bytes.len(),
            });
        }
        let observed = sha256(bytes);
        if &observed != expected_sha256 {
            return Err(GridRejection::Stale {
                expected: *expected_sha256,
                observed,
            });
        }
        let mut heights = Vec::with_capacity(SAMPLES_PER_CELL);
        for (index, chunk) in bytes.as_chunks::<4>().0.iter().enumerate() {
            let value = f32::from_le_bytes(*chunk);
            if !value.is_finite() {
                return Err(GridRejection::NonFinite { index });
            }
            heights.push(value);
        }
        Ok(Self { heights })
    }

    /// Height at sample `east`, `north` (each `0..=32`), or `None` outside the grid.
    pub fn height(&self, east: usize, north: usize) -> Option<f32> {
        (east < SAMPLES_PER_SIDE && north < SAMPLES_PER_SIDE)
            .then(|| self.heights[north * SAMPLES_PER_SIDE + east])
    }

    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    fn at(&self, east: usize, north: usize) -> f32 {
        self.heights[north * SAMPLES_PER_SIDE + east]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RayError {
    NonFiniteOrigin,
    NonFiniteDirection,
    ZeroDirection,
}

/// A validated ray `origin + t * direction`, `t > 0`, in world space `[east, north, up]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    origin: [f64; 3],
    direction: [f64; 3],
}

impl Ray {
    pub fn new(origin: [f64; 3], direction: [f64; 3]) -> Result<Self, RayError> {
        if !origin.iter().all(|v| v.is_finite()) {
            return Err(RayError::NonFiniteOrigin);
        }
        if !direction.iter().all(|v| v.is_finite()) {
            return Err(RayError::NonFiniteDirection);
        }
        if direction.iter().all(|v| *v == 0.0) {
            return Err(RayError::ZeroDirection);
        }
        Ok(Self { origin, direction })
    }

    pub fn origin(&self) -> [f64; 3] {
        self.origin
    }

    pub fn direction(&self) -> [f64; 3] {
        self.direction
    }
}

/// Which half of a quad: `Lower` is `[a, b, c]`, `Upper` is `[b, d, c]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    Lower,
    Upper,
}

/// One triangle of a cell's base surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriangleId {
    pub quad_east: u8,
    pub quad_north: u8,
    pub half: Half,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericError {
    /// A cell corner coordinate is not finite (absurd cell size or index).
    NonFiniteCoordinate,
    /// A proven hit is further than an `f64` can hold.
    NonFiniteHit,
    /// A positive exact hit parameter underflowed to zero in its public `f64` representation.
    UnrepresentableHitParameter,
}

/// The nearest proven hit inside one cell.
#[derive(Debug, Clone)]
pub struct CellHit {
    pub triangle: TriangleId,
    /// `f64` approximation of the exact ray parameter (`position = origin + t * direction`).
    pub t: f64,
    param: Rat,
}

impl CellHit {
    /// Exact comparison of the ray parameters of two hits.
    pub fn is_nearer_than(&self, other: &CellHit) -> bool {
        self.param.compare(&other.param) == Ordering::Less
    }

    /// Evaluate the witness without multiplying a rounded parameter by a possibly huge
    /// direction, which could overflow or cancel away the surface position.
    pub fn position_on(&self, ray: &Ray) -> Result<[f64; 3], NumericError> {
        let position =
            std::array::from_fn(|axis| self.param.affine(ray.origin[axis], ray.direction[axis]));
        if position.iter().all(|value| value.is_finite()) {
            Ok(position)
        } else {
            Err(NumericError::NonFiniteHit)
        }
    }
}

/// What the ray does inside one cell's surface.
#[derive(Debug, Clone)]
pub struct CellTrace {
    pub nearest: Option<CellHit>,
    /// Triangles the ray lies in (or whose plane it cannot be separated from). Not proof of
    /// anything: the caller must treat them as unresolved.
    pub coplanar: Vec<TriangleId>,
}

/// A ray with its exact copy, so many cells reuse one conversion.
#[derive(Debug, Clone)]
pub struct PreparedRay {
    ray: Ray,
    origin: [Dy; 3],
    direction: [Dy; 3],
}

#[derive(Debug, Clone)]
enum TriOutcome {
    Miss,
    Hit(Rat),
    Coplanar,
}

impl PreparedRay {
    pub fn new(ray: Ray) -> Self {
        let origin = ray.origin.map(Dy::from_finite);
        let direction = ray.direction.map(Dy::from_finite);
        Self {
            ray,
            origin,
            direction,
        }
    }

    pub fn ray(&self) -> &Ray {
        &self.ray
    }

    /// Whether the positive ray (`t > 0`) touches the closed XY box of the cell, boundary
    /// included. Exact.
    pub fn touches_cell(
        &self,
        spacing: &CellSpacing,
        cell: CellCoord,
    ) -> Result<bool, NumericError> {
        let [x0, x1, y0, y1] = spacing
            .cell_extent(cell)
            .ok_or(NumericError::NonFiniteCoordinate)?;
        Ok(self.touches_box(x0, x1, y0, y1))
    }

    /// Closed footprint of a rectangle of cells. Used to prune sparse coverage searches;
    /// cell index arithmetic is widened before forming a far edge.
    pub(crate) fn touches_cell_rect(
        &self,
        spacing: &CellSpacing,
        min: CellCoord,
        max: CellCoord,
    ) -> Result<bool, NumericError> {
        let [x0, _, y0, _] = spacing
            .cell_extent(min)
            .ok_or(NumericError::NonFiniteCoordinate)?;
        let [_, x1, _, y1] = spacing
            .cell_extent(max)
            .ok_or(NumericError::NonFiniteCoordinate)?;
        Ok(self.touches_box(x0, x1, y0, y1))
    }

    /// Intersects the positive ray with every triangle of the cell's base surface. Brute force:
    /// every quad the ray touches is tested exactly, no matter what a camera sees.
    pub fn trace_cell(
        &self,
        spacing: &CellSpacing,
        cell: CellCoord,
        grid: &HeightGrid,
    ) -> Result<CellTrace, NumericError> {
        spacing
            .cell_extent(cell)
            .ok_or(NumericError::NonFiniteCoordinate)?;
        let gx0 = i64::from(cell.x) * CELL_INTERVALS_I64;
        let gy0 = i64::from(cell.y) * CELL_INTERVALS_I64;
        let mut nearest: Option<(TriangleId, Rat)> = None;
        let mut coplanar = Vec::new();
        for qn in 0..CELL_INTERVALS {
            for qe in 0..CELL_INTERVALS {
                let x0 = spacing.coord(gx0 + qe as i64);
                let x1 = spacing.coord(gx0 + qe as i64 + 1);
                let y0 = spacing.coord(gy0 + qn as i64);
                let y1 = spacing.coord(gy0 + qn as i64 + 1);
                if !self.touches_box(x0, x1, y0, y1) {
                    continue;
                }
                let a = vertex(x0, y0, grid.at(qe, qn));
                let b = vertex(x1, y0, grid.at(qe + 1, qn));
                let c = vertex(x0, y1, grid.at(qe, qn + 1));
                let d = vertex(x1, y1, grid.at(qe + 1, qn + 1));
                for (half, corners) in [(Half::Lower, [&a, &b, &c]), (Half::Upper, [&b, &d, &c])] {
                    let id = TriangleId {
                        quad_east: qe as u8,
                        quad_north: qn as u8,
                        half,
                    };
                    match self.trace_triangle(corners[0], corners[1], corners[2]) {
                        TriOutcome::Miss => {}
                        TriOutcome::Coplanar => coplanar.push(id),
                        TriOutcome::Hit(param) => {
                            let better = match &nearest {
                                None => true,
                                Some((_, current)) => param.compare(current) == Ordering::Less,
                            };
                            if better {
                                nearest = Some((id, param));
                            }
                        }
                    }
                }
            }
        }
        let nearest = match nearest {
            None => None,
            Some((triangle, param)) => {
                let t = param.to_f64();
                if !t.is_finite() {
                    return Err(NumericError::NonFiniteHit);
                }
                if t <= 0.0 {
                    return Err(NumericError::UnrepresentableHitParameter);
                }
                Some(CellHit { triangle, t, param })
            }
        };
        Ok(CellTrace { nearest, coplanar })
    }

    fn touches_box(&self, x0: f64, x1: f64, y0: f64, y1: f64) -> bool {
        let one = Dy::from_i64(1);
        let minus_one = Dy::from_i64(-1);
        let zero = Dy::zero();
        let constraints = [
            (
                minus_one.clone(),
                zero.clone(),
                Dy::from_finite(x0).negated(),
            ),
            (one.clone(), zero.clone(), Dy::from_finite(x1)),
            (zero.clone(), minus_one, Dy::from_finite(y0).negated()),
            (zero, one, Dy::from_finite(y1)),
        ];
        self.touches_region(&constraints)
    }

    /// Whether some `t > 0` puts the ray's XY point inside every half-plane `a*x + b*y <= c`.
    fn touches_region(&self, constraints: &[(Dy, Dy, Dy)]) -> bool {
        let mut interval = Interval::new();
        for (a, b, c) in constraints {
            let k = a
                .times(&self.direction[0])
                .plus(&b.times(&self.direction[1]));
            let r = c
                .minus(&a.times(&self.origin[0]))
                .minus(&b.times(&self.origin[1]));
            interval.clip(&k, &r);
        }
        interval.nonempty_positive()
    }

    /// Moeller-Trumbore with signs decided exactly. The hit is closed (edges and vertices
    /// count) and requires `t > 0`; `t == 0` is the receiver's own contact and is excluded.
    fn trace_triangle(&self, p0: &[Dy; 3], p1: &[Dy; 3], p2: &[Dy; 3]) -> TriOutcome {
        let e1 = exact::sub3(p1, p0);
        let e2 = exact::sub3(p2, p0);
        let pvec = exact::cross(&self.direction, &e2);
        let mut det = exact::dot(&e1, &pvec);
        let offset = exact::sub3(&self.origin, p0);
        if det.sign() == 0 {
            return self.trace_parallel(p0, p1, p2, &e1, &e2, &offset);
        }
        let mut u = exact::dot(&offset, &pvec);
        let qvec = exact::cross(&offset, &e1);
        let mut v = exact::dot(&self.direction, &qvec);
        let mut t = exact::dot(&e2, &qvec);
        if det.sign() < 0 {
            det = det.negated();
            u = u.negated();
            v = v.negated();
            t = t.negated();
        }
        if u.sign() < 0 || v.sign() < 0 || t.sign() <= 0 {
            return TriOutcome::Miss;
        }
        if u.plus(&v).minus(&det).sign() > 0 {
            return TriOutcome::Miss;
        }
        TriOutcome::Hit(Rat::new(t, det))
    }

    /// The ray is parallel to the triangle's plane. Strictly off the plane is a miss; in the
    /// plane the intersection is a segment or a grazing contact, which this module does not
    /// certify either way, so a ray that touches the triangle there is reported as coplanar.
    fn trace_parallel(
        &self,
        p0: &[Dy; 3],
        p1: &[Dy; 3],
        p2: &[Dy; 3],
        e1: &[Dy; 3],
        e2: &[Dy; 3],
        offset: &[Dy; 3],
    ) -> TriOutcome {
        let normal = exact::cross(e1, e2);
        if exact::dot(&normal, offset).sign() != 0 {
            return TriOutcome::Miss;
        }
        // The ray lies in the plane. The plane is a height field plane (never vertical), so the
        // XY projection is exact: test the projected line against the projected triangle.
        let area2 = e1[0].times(&e2[1]).minus(&e1[1].times(&e2[0]));
        let counter_clockwise = match area2.sign() {
            0 => return TriOutcome::Coplanar,
            sign => sign > 0,
        };
        let (q1, q2) = if counter_clockwise {
            (p1, p2)
        } else {
            (p2, p1)
        };
        let constraints = [
            edge_constraint(p0, q1),
            edge_constraint(q1, q2),
            edge_constraint(q2, p0),
        ];
        if self.touches_region(&constraints) {
            TriOutcome::Coplanar
        } else {
            TriOutcome::Miss
        }
    }
}

fn vertex(x: f64, y: f64, z: f32) -> [Dy; 3] {
    [
        Dy::from_finite(x),
        Dy::from_finite(y),
        Dy::from_finite(f64::from(z)),
    ]
}

/// Half-plane `a*x + b*y <= c` holding on the inner side of the counter-clockwise edge `p -> q`.
fn edge_constraint(p: &[Dy; 3], q: &[Dy; 3]) -> (Dy, Dy, Dy) {
    let ex = q[0].minus(&p[0]);
    let ey = q[1].minus(&p[1]);
    let c = ey.times(&p[0]).minus(&ex.times(&p[1]));
    (ey, ex.negated(), c)
}

/// Parameter interval of `t` built from half-plane constraints `k * t <= r`.
struct Interval {
    lower: Option<Rat>,
    upper: Option<Rat>,
    empty: bool,
}

impl Interval {
    fn new() -> Self {
        Self {
            lower: None,
            upper: None,
            empty: false,
        }
    }

    fn clip(&mut self, k: &Dy, r: &Dy) {
        if self.empty {
            return;
        }
        match k.sign() {
            0 => {
                if r.sign() < 0 {
                    self.empty = true;
                }
            }
            sign if sign > 0 => {
                let bound = Rat::new(r.clone(), k.clone());
                self.upper = Some(match self.upper.take() {
                    Some(current) if current.compare(&bound) != Ordering::Greater => current,
                    _ => bound,
                });
            }
            _ => {
                let bound = Rat::new(r.negated(), k.negated());
                self.lower = Some(match self.lower.take() {
                    Some(current) if current.compare(&bound) != Ordering::Less => current,
                    _ => bound,
                });
            }
        }
    }

    /// Non-empty and containing some `t > 0`.
    fn nonempty_positive(&self) -> bool {
        if self.empty {
            return false;
        }
        if let (Some(lower), Some(upper)) = (&self.lower, &self.upper)
            && lower.compare(upper) == Ordering::Greater
        {
            return false;
        }
        match &self.upper {
            Some(upper) => upper.is_positive(),
            None => true,
        }
    }
}

/// Exact dyadic arithmetic: `sign * magnitude * 2^exp` with an unbounded magnitude, so every
/// finite `f64` converts exactly and sums and products never round.
mod exact {
    use std::cmp::Ordering;

    fn trim(mag: &mut Vec<u32>) {
        while mag.last() == Some(&0) {
            mag.pop();
        }
    }

    /// Compares two trimmed magnitudes.
    fn mag_cmp(a: &[u32], b: &[u32]) -> Ordering {
        if a.len() != b.len() {
            return a.len().cmp(&b.len());
        }
        for i in (0..a.len()).rev() {
            if a[i] != b[i] {
                return a[i].cmp(&b[i]);
            }
        }
        Ordering::Equal
    }

    fn mag_add(a: &[u32], b: &[u32]) -> Vec<u32> {
        let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
        let mut out = Vec::with_capacity(long.len() + 1);
        let mut carry = 0u64;
        for (i, word) in long.iter().enumerate() {
            let other = short.get(i).copied().unwrap_or(0);
            let sum = u64::from(*word) + u64::from(other) + carry;
            out.push(sum as u32);
            carry = sum >> 32;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        out
    }

    /// `a - b` for `a >= b`.
    fn mag_sub(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0i64;
        for (i, word) in a.iter().enumerate() {
            let other = b.get(i).copied().unwrap_or(0);
            let mut diff = i64::from(*word) - borrow - i64::from(other);
            if diff < 0 {
                diff += 1 << 32;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(diff as u32);
        }
        trim(&mut out);
        out
    }

    fn mag_shl(a: &[u32], bits: u32) -> Vec<u32> {
        if a.is_empty() {
            return Vec::new();
        }
        let words = (bits / 32) as usize;
        let rem = bits % 32;
        let mut out = vec![0u32; words];
        if rem == 0 {
            out.extend_from_slice(a);
        } else {
            let mut carry = 0u32;
            for word in a {
                out.push((word << rem) | carry);
                carry = word >> (32 - rem);
            }
            if carry > 0 {
                out.push(carry);
            }
        }
        out
    }

    fn mag_mul(a: &[u32], b: &[u32]) -> Vec<u32> {
        if a.is_empty() || b.is_empty() {
            return Vec::new();
        }
        let mut out = vec![0u32; a.len() + b.len()];
        for (i, left) in a.iter().enumerate() {
            let mut carry = 0u64;
            for (j, right) in b.iter().enumerate() {
                let current = u64::from(out[i + j]) + u64::from(*left) * u64::from(*right) + carry;
                out[i + j] = current as u32;
                carry = current >> 32;
            }
            out[i + b.len()] = carry as u32;
        }
        trim(&mut out);
        out
    }

    /// `±mag * 2^exp`; `mag` is little-endian, trimmed, and empty for zero.
    #[derive(Debug, Clone)]
    pub struct Dy {
        neg: bool,
        mag: Vec<u32>,
        exp: i32,
    }

    impl Dy {
        pub fn zero() -> Dy {
            Dy {
                neg: false,
                mag: Vec::new(),
                exp: 0,
            }
        }

        pub fn from_i64(value: i64) -> Dy {
            let abs = value.unsigned_abs();
            let mut mag = vec![abs as u32, (abs >> 32) as u32];
            trim(&mut mag);
            Dy {
                neg: value < 0 && !mag.is_empty(),
                mag,
                exp: 0,
            }
        }

        /// Exact value of a finite `f64`; `None` for NaN and infinities.
        pub fn from_f64(value: f64) -> Option<Dy> {
            if !value.is_finite() {
                return None;
            }
            let bits = value.to_bits();
            let neg = (bits >> 63) == 1;
            let exponent = ((bits >> 52) & 0x7ff) as i32;
            let fraction = bits & ((1u64 << 52) - 1);
            let (mantissa, exp) = if exponent == 0 {
                (fraction, -1074)
            } else {
                (fraction | (1u64 << 52), exponent - 1075)
            };
            let mut mag = vec![mantissa as u32, (mantissa >> 32) as u32];
            trim(&mut mag);
            Some(Dy {
                neg: neg && !mag.is_empty(),
                mag,
                exp,
            })
        }

        /// For values the caller already validated as finite.
        pub fn from_finite(value: f64) -> Dy {
            Dy::from_f64(value).expect("value was validated as finite")
        }

        pub fn sign(&self) -> i32 {
            if self.mag.is_empty() {
                0
            } else if self.neg {
                -1
            } else {
                1
            }
        }

        pub fn negated(&self) -> Dy {
            Dy {
                neg: !self.neg && !self.mag.is_empty(),
                mag: self.mag.clone(),
                exp: self.exp,
            }
        }

        pub fn plus(&self, other: &Dy) -> Dy {
            if self.mag.is_empty() {
                return other.clone();
            }
            if other.mag.is_empty() {
                return self.clone();
            }
            let exp = self.exp.min(other.exp);
            let a = mag_shl(&self.mag, (self.exp - exp) as u32);
            let b = mag_shl(&other.mag, (other.exp - exp) as u32);
            if self.neg == other.neg {
                return Dy {
                    neg: self.neg,
                    mag: mag_add(&a, &b),
                    exp,
                };
            }
            match mag_cmp(&a, &b) {
                Ordering::Equal => Dy::zero(),
                Ordering::Greater => Dy {
                    neg: self.neg,
                    mag: mag_sub(&a, &b),
                    exp,
                },
                Ordering::Less => Dy {
                    neg: other.neg,
                    mag: mag_sub(&b, &a),
                    exp,
                },
            }
        }

        pub fn minus(&self, other: &Dy) -> Dy {
            self.plus(&other.negated())
        }

        pub fn times(&self, other: &Dy) -> Dy {
            if self.mag.is_empty() || other.mag.is_empty() {
                return Dy::zero();
            }
            Dy {
                neg: self.neg != other.neg,
                mag: mag_mul(&self.mag, &other.mag),
                exp: self.exp + other.exp,
            }
        }

        fn bit_len(&self) -> u32 {
            match self.mag.last() {
                None => 0,
                Some(top) => (self.mag.len() as u32 - 1) * 32 + (32 - top.leading_zeros()),
            }
        }

        /// `(m, e)` with `value ~= m * 2^e`, `m` carrying the top 64 bits (truncated).
        pub fn to_scaled(&self) -> (f64, i32) {
            if self.mag.is_empty() {
                return (0.0, 0);
            }
            let bits = self.bit_len();
            let shift = bits.saturating_sub(64);
            let mut top = 0u64;
            for k in 0..(bits - shift) {
                let index = shift + k;
                let bit = (self.mag[(index / 32) as usize] >> (index % 32)) & 1;
                top |= u64::from(bit) << k;
            }
            let mantissa = top as f64;
            (
                if self.neg { -mantissa } else { mantissa },
                self.exp + shift as i32,
            )
        }
    }

    pub fn sub3(a: &[Dy; 3], b: &[Dy; 3]) -> [Dy; 3] {
        [a[0].minus(&b[0]), a[1].minus(&b[1]), a[2].minus(&b[2])]
    }

    pub fn cross(a: &[Dy; 3], b: &[Dy; 3]) -> [Dy; 3] {
        [
            a[1].times(&b[2]).minus(&a[2].times(&b[1])),
            a[2].times(&b[0]).minus(&a[0].times(&b[2])),
            a[0].times(&b[1]).minus(&a[1].times(&b[0])),
        ]
    }

    pub fn dot(a: &[Dy; 3], b: &[Dy; 3]) -> Dy {
        a[0].times(&b[0])
            .plus(&a[1].times(&b[1]))
            .plus(&a[2].times(&b[2]))
    }

    /// An exact fraction with a positive denominator.
    #[derive(Debug, Clone)]
    pub struct Rat {
        num: Dy,
        den: Dy,
    }

    impl Rat {
        /// `den` must be non-zero; the sign is moved onto the numerator.
        pub fn new(num: Dy, den: Dy) -> Rat {
            if den.sign() < 0 {
                Rat {
                    num: num.negated(),
                    den: den.negated(),
                }
            } else {
                Rat { num, den }
            }
        }

        pub fn is_positive(&self) -> bool {
            self.num.sign() > 0
        }

        pub fn compare(&self, other: &Rat) -> Ordering {
            let left = self.num.times(&other.den);
            let right = other.num.times(&self.den);
            left.minus(&right).sign().cmp(&0)
        }

        pub fn affine(&self, origin: f64, direction: f64) -> f64 {
            let num = Dy::from_finite(origin)
                .times(&self.den)
                .plus(&self.num.times(&Dy::from_finite(direction)));
            Self::new(num, self.den.clone()).to_f64()
        }

        /// Approximation (about 1e-16 relative); non-finite if the value does not fit.
        pub fn to_f64(&self) -> f64 {
            let (num, num_exp) = self.num.to_scaled();
            let (den, den_exp) = self.den.to_scaled();
            if den == 0.0 {
                return f64::NAN;
            }
            let exp = num_exp - den_exp;
            let half = exp / 2;
            (num / den) * 2f64.powi(half) * 2f64.powi(exp - half)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::exact::{Dy, Rat};
    use super::*;

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 11
        }

        fn range(&mut self, lo: i64, hi: i64) -> i64 {
            lo + (self.next() % (hi - lo) as u64) as i64
        }
    }

    fn dy(value: f64) -> Dy {
        Dy::from_f64(value).unwrap()
    }

    fn spacing_one() -> CellSpacing {
        // 32 world units per cell, so one sample step is exactly 1.0.
        CellSpacing::new(32.0).unwrap()
    }

    fn flat_grid(height: f32) -> HeightGrid {
        grid_from(|_, _| height)
    }

    fn grid_from(f: impl Fn(usize, usize) -> f32) -> HeightGrid {
        let mut heights = Vec::new();
        for north in 0..SAMPLES_PER_SIDE {
            for east in 0..SAMPLES_PER_SIDE {
                heights.push(f(east, north));
            }
        }
        let bytes = encode_heights_le(&heights);
        HeightGrid::from_le_bytes(&bytes, &sha256(&bytes)).unwrap()
    }

    fn prepared(origin: [f64; 3], direction: [f64; 3]) -> PreparedRay {
        PreparedRay::new(Ray::new(origin, direction).unwrap())
    }

    const ORIGIN_CELL: CellCoord = CellCoord { x: 0, y: 0 };

    #[test]
    fn dyadic_signs_match_i128() {
        let mut rng = Lcg(1);
        for _ in 0..3000 {
            let a = rng.range(-(1 << 40), 1 << 40);
            let b = rng.range(-(1 << 40), 1 << 40);
            let c = rng.range(-(1 << 40), 1 << 40);
            let d = rng.range(-(1 << 40), 1 << 40);
            let (da, db, dc, dd) = (dy(a as f64), dy(b as f64), dy(c as f64), dy(d as f64));
            let want = (i128::from(a) * i128::from(b) - i128::from(c) * i128::from(d)).signum();
            assert_eq!(da.times(&db).minus(&dc.times(&dd)).sign(), want as i32);
            let want_sum = (i128::from(a) + i128::from(b) - i128::from(c)).signum();
            assert_eq!(da.plus(&db).minus(&dc).sign(), want_sum as i32);
        }
    }

    #[test]
    fn dyadic_exponents_align_across_a_huge_range() {
        let mut rng = Lcg(7);
        for _ in 0..500 {
            let a = rng.range(-(1 << 30), 1 << 30);
            let b = rng.range(-(1 << 30), 1 << 30);
            let c = rng.range(-(1 << 30), 1 << 30);
            let d = rng.range(-(1 << 30), 1 << 30);
            // (a 2^500)(b 2^-400) - (c 2^100)(d) has the sign of ab - cd.
            let left = dy(a as f64 * 2f64.powi(500)).times(&dy(b as f64 * 2f64.powi(-400)));
            let right = dy(c as f64 * 2f64.powi(100)).times(&dy(d as f64));
            let want = (i128::from(a) * i128::from(b) - i128::from(c) * i128::from(d)).signum();
            assert_eq!(left.minus(&right).sign(), want as i32);
        }
        // A subnormal and a huge value still add exactly.
        let tiny = dy(f64::from_bits(1));
        let huge = dy(2f64.powi(1000));
        assert_eq!(huge.plus(&tiny).minus(&huge).sign(), 1);
        assert_eq!(huge.plus(&tiny).minus(&huge).minus(&tiny).sign(), 0);
    }

    #[test]
    fn fractions_compare_like_cross_multiplication() {
        let mut rng = Lcg(3);
        for _ in 0..2000 {
            let a = rng.range(-(1 << 30), 1 << 30);
            let b = rng.range(1, 1 << 30) * if rng.next() & 1 == 0 { 1 } else { -1 };
            let c = rng.range(-(1 << 30), 1 << 30);
            let d = rng.range(1, 1 << 30) * if rng.next() & 1 == 0 { 1 } else { -1 };
            let left = Rat::new(dy(a as f64), dy(b as f64));
            let right = Rat::new(dy(c as f64), dy(d as f64));
            // Fold the denominator signs into the numerators, then cross-multiply.
            let (a, b) = if b < 0 { (-a, -b) } else { (a, b) };
            let (c, d) = if d < 0 { (-c, -d) } else { (c, d) };
            let want = (i128::from(a) * i128::from(d) - i128::from(c) * i128::from(b)).signum();
            assert_eq!(left.compare(&right), (want as i32).cmp(&0));
        }
        assert_eq!(Rat::new(dy(10.0), dy(1.0)).to_f64(), 10.0);
        assert!((Rat::new(dy(1.0), dy(3.0)).to_f64() - 1.0 / 3.0).abs() < 1e-15);
        assert!((Rat::new(dy(-1.0), dy(-4.0)).to_f64() - 0.25).abs() < 1e-15);
    }

    #[test]
    fn grid_constructor_rejects_without_panicking() {
        let heights = vec![1.0f32; SAMPLES_PER_CELL];
        let bytes = encode_heights_le(&heights);
        let digest = sha256(&bytes);
        assert!(HeightGrid::from_le_bytes(&bytes, &digest).is_ok());
        assert_eq!(
            HeightGrid::from_le_bytes(&bytes[..100], &digest),
            Err(GridRejection::BadLength {
                expected: GRID_BYTES,
                found: 100
            })
        );
        let wrong = [9u8; 32];
        assert_eq!(
            HeightGrid::from_le_bytes(&bytes, &wrong),
            Err(GridRejection::Stale {
                expected: wrong,
                observed: digest
            })
        );
        let mut bad = heights.clone();
        bad[40] = f32::NAN;
        let bad_bytes = encode_heights_le(&bad);
        assert_eq!(
            HeightGrid::from_le_bytes(&bad_bytes, &sha256(&bad_bytes)),
            Err(GridRejection::NonFinite { index: 40 })
        );
        bad[40] = f32::INFINITY;
        let inf_bytes = encode_heights_le(&bad);
        assert_eq!(
            HeightGrid::from_le_bytes(&inf_bytes, &sha256(&inf_bytes)),
            Err(GridRejection::NonFinite { index: 40 })
        );
        let grid = HeightGrid::from_le_bytes(&bytes, &digest).unwrap();
        assert_eq!(grid.height(32, 32), Some(1.0));
        assert_eq!(grid.height(33, 0), None);
    }

    #[test]
    fn spacing_and_ray_validation() {
        assert_eq!(CellSpacing::new(0.0), Err(SpacingError::NotFinitePositive));
        assert_eq!(CellSpacing::new(-1.0), Err(SpacingError::NotFinitePositive));
        assert_eq!(
            CellSpacing::new(f64::NAN),
            Err(SpacingError::NotFinitePositive)
        );
        assert_eq!(CellSpacing::new(1e-320), Err(SpacingError::StepNotNormal));
        assert_eq!(CellSpacing::new(4096.0).unwrap().step(), 128.0);
        let near_max = CellSpacing::new(1e308).unwrap();
        assert!(
            near_max
                .cell_extent(CellCoord { x: i32::MAX, y: 0 })
                .is_none()
        );
        assert_eq!(
            Ray::new([f64::NAN, 0.0, 0.0], [0.0, 0.0, 1.0]),
            Err(RayError::NonFiniteOrigin)
        );
        assert_eq!(
            Ray::new([0.0; 3], [0.0, f64::INFINITY, 1.0]),
            Err(RayError::NonFiniteDirection)
        );
        assert_eq!(
            Ray::new([0.0; 3], [0.0, -0.0, 0.0]),
            Err(RayError::ZeroDirection)
        );
    }

    #[test]
    fn coverage_is_direction_scale_invariant_including_subnormals() {
        for scale in [f64::from_bits(1), 1e-300, 1.0, 1e300] {
            let toward = prepared([-1.0, -1.0, 0.0], [scale, scale, 0.0]);
            let away = prepared([-1.0, -1.0, 0.0], [-scale, -scale, 0.0]);
            assert!(toward.touches_cell(&spacing_one(), ORIGIN_CELL).unwrap());
            assert!(!away.touches_cell(&spacing_one(), ORIGIN_CELL).unwrap());
        }
    }

    #[test]
    fn a_positive_hit_parameter_cannot_be_reported_as_zero() {
        let ray = prepared([0.25, 0.25, 1e-250], [0.0, 0.0, -1e300]);
        assert_eq!(
            ray.trace_cell(&spacing_one(), ORIGIN_CELL, &flat_grid(0.0))
                .unwrap_err(),
            NumericError::UnrepresentableHitParameter
        );
    }

    #[test]
    fn hit_position_does_not_overflow_a_rounded_parameter_product() {
        // The exact hit is on z=0; evaluating rounded t * direction first overflows.
        let ray = prepared([16.0, 16.0, 1e308], [0.0, 0.0, -1e308]);
        let hit = ray
            .trace_cell(&spacing_one(), ORIGIN_CELL, &flat_grid(0.0))
            .unwrap()
            .nearest
            .unwrap();
        assert_eq!(hit.position_on(ray.ray()).unwrap(), [16.0, 16.0, 0.0]);
    }

    #[test]
    fn flat_cell_hit_and_miss_are_exact() {
        let spacing = spacing_one();
        let grid = flat_grid(0.0);
        let down = prepared([16.5, 16.5, 10.0], [0.0, 0.0, -1.0]);
        let trace = down.trace_cell(&spacing, ORIGIN_CELL, &grid).unwrap();
        let hit = trace.nearest.unwrap();
        assert!((hit.t - 10.0).abs() < 1e-12);
        assert!(trace.coplanar.is_empty());
        let up = prepared([16.5, 16.5, 10.0], [0.3, 0.2, 1.0]);
        let trace = up.trace_cell(&spacing, ORIGIN_CELL, &grid).unwrap();
        assert!(trace.nearest.is_none() && trace.coplanar.is_empty());
    }

    #[test]
    fn origin_on_the_surface_is_not_a_self_hit() {
        let spacing = spacing_one();
        let grid = flat_grid(0.0);
        for direction in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [1.0, 1.0, 1.0]] {
            let ray = prepared([16.5, 16.5, 0.0], direction);
            let trace = ray.trace_cell(&spacing, ORIGIN_CELL, &grid).unwrap();
            assert!(trace.nearest.is_none(), "{direction:?}");
        }
    }

    #[test]
    fn ray_in_the_plane_is_reported_not_cleared() {
        let spacing = spacing_one();
        let grid = flat_grid(0.0);
        let inside = prepared([4.0, 4.0, 0.0], [1.0, 0.0, 0.0]);
        let trace = inside.trace_cell(&spacing, ORIGIN_CELL, &grid).unwrap();
        assert!(trace.nearest.is_none());
        assert!(!trace.coplanar.is_empty());
        let above = prepared([4.0, 4.0, 0.5], [1.0, 0.0, 0.0]);
        let trace = above.trace_cell(&spacing, ORIGIN_CELL, &grid).unwrap();
        assert!(trace.nearest.is_none() && trace.coplanar.is_empty());
    }

    #[test]
    fn canonical_triangulation_is_the_b_c_diagonal() {
        let spacing = spacing_one();
        // a=(0,0)=0, b=(1,0)=1, c=(0,1)=1, d=(1,1)=0: a saddle quad.
        let grid = grid_from(|east, north| match (east, north) {
            (1, 0) | (0, 1) => 1.0,
            _ => 0.0,
        });
        // On the b-c diagonal the surface is exactly 1; bilinear would say 0.5 and the other
        // diagonal 0.
        let from_below = prepared([0.5, 0.5, 0.75], [0.0, 0.0, 1.0]);
        let hit = from_below
            .trace_cell(&spacing, ORIGIN_CELL, &grid)
            .unwrap()
            .nearest
            .unwrap();
        assert!((hit.t - 0.25).abs() < 1e-12);
        let lower = prepared([0.25, 0.25, 0.0], [0.0, 0.0, 1.0]);
        let hit = lower
            .trace_cell(&spacing, ORIGIN_CELL, &grid)
            .unwrap()
            .nearest
            .unwrap();
        assert!((hit.t - 0.5).abs() < 1e-12);
        assert_eq!(
            hit.triangle,
            TriangleId {
                quad_east: 0,
                quad_north: 0,
                half: Half::Lower
            }
        );
        let upper = prepared([0.75, 0.75, 0.0], [0.0, 0.0, 1.0]);
        let hit = upper
            .trace_cell(&spacing, ORIGIN_CELL, &grid)
            .unwrap()
            .nearest
            .unwrap();
        assert!((hit.t - 0.5).abs() < 1e-12);
        assert_eq!(hit.triangle.half, Half::Upper);
    }

    #[test]
    fn touching_a_cell_is_closed_and_positive_only() {
        let spacing = spacing_one();
        // Cell (0,0) is [0,32]^2.
        let on_edge = prepared([32.0, 10.0, 5.0], [0.0, 1.0, 0.0]);
        assert!(on_edge.touches_cell(&spacing, ORIGIN_CELL).unwrap());
        let corner = prepared([40.0, 40.0, 5.0], [-1.0, -1.0, 0.0]);
        assert!(corner.touches_cell(&spacing, ORIGIN_CELL).unwrap());
        let away = prepared([32.0, 32.0, 5.0], [1.0, 1.0, 0.0]);
        assert!(!away.touches_cell(&spacing, ORIGIN_CELL).unwrap());
        let behind = prepared([-1.0, 10.0, 5.0], [-1.0, 0.0, 0.0]);
        assert!(!behind.touches_cell(&spacing, ORIGIN_CELL).unwrap());
    }
}

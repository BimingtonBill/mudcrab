//! Small f32 enclosure operations mirrored by `arithmetic.wgsl`.
//!
//! Inputs and endpoints are finite normal values or zero. Subnormal point inputs are
//! rejected because WGSL permits flushing them. Results near underflow include zero
//! and the minimum normal magnitude plus the operation's widening allowance.
//! Add/subtract/multiply widen one representable
//! step; division widens eight, with its divisor restricted to the WGSL accuracy
//! range. Bit-pattern widening forms a rounding boundary between operations.
//! Before arithmetic, every nonzero operand endpoint must be in [2^-60, 2^60]
//! in absolute magnitude. This prevents overflow/underflow BEFORE an operation:
//! WGSL finite-math overflow can otherwise produce indeterminate finite values
//! that no post-check detects. Exact zero/point identities need no arithmetic.
//! Out-of-domain operations return invalid, mapped by traversal to NUMERIC Unknown;
//! this is a numeric limitation, not a geometric tolerance or ray bias.
//! These CPU fixtures check the numerical contract, not actual GPU execution.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval(pub f32, pub f32);

impl Interval {
    pub const INVALID: Self = Self(1.0, -1.0);
    pub const ZERO: Self = Self(0.0, 0.0);

    pub fn point(value: f32) -> Self {
        if endpoint(value) {
            Self(value, value)
        } else {
            Self::INVALID
        }
    }

    pub fn ok(self) -> bool {
        endpoint(self.0) && endpoint(self.1) && self.0 <= self.1
    }

    fn is_zero(self) -> bool {
        self.0 == 0.0 && self.1 == 0.0
    }
    fn is_point(self) -> bool {
        self.0 == self.1
    }

    pub fn negated(self) -> Self {
        if self.ok() {
            Self(-self.1, -self.0)
        } else {
            Self::INVALID
        }
    }

    pub fn plus(self, rhs: Self) -> Self {
        if !self.ok() || !rhs.ok() {
            return Self::INVALID;
        }
        if rhs.is_zero() {
            return self;
        }
        if self.is_zero() {
            return rhs;
        }
        if self.is_point() && rhs.is_point() && self.0 == -rhs.0 {
            return Self::ZERO;
        }
        if !operands(self, rhs) {
            return Self::INVALID;
        }
        bounds(self.0 + rhs.0, self.1 + rhs.1, 1)
    }

    pub fn minus(self, rhs: Self) -> Self {
        if !self.ok() || !rhs.ok() {
            return Self::INVALID;
        }
        if rhs.is_zero() {
            return self;
        }
        if self.is_point() && rhs.is_point() && self.0 == rhs.0 {
            return Self::ZERO;
        }
        if !operands(self, rhs) {
            return Self::INVALID;
        }
        bounds(self.0 - rhs.1, self.1 - rhs.0, 1)
    }

    pub fn times(self, rhs: Self) -> Self {
        if !self.ok() || !rhs.ok() {
            return Self::INVALID;
        }
        if self.is_zero() || rhs.is_zero() {
            return Self::ZERO;
        }
        if !operands(self, rhs) {
            return Self::INVALID;
        }
        four(
            [
                self.0 * rhs.0,
                self.0 * rhs.1,
                self.1 * rhs.0,
                self.1 * rhs.1,
            ],
            1,
        )
    }

    pub fn divided_by(self, rhs: Self) -> Self {
        if !self.ok() || !rhs.ok() || (rhs.0 <= 0.0 && rhs.1 >= 0.0) {
            return Self::INVALID;
        }
        if self.is_zero() {
            return Self::ZERO;
        }
        if !operands(self, rhs) {
            return Self::INVALID;
        }
        four(
            [
                self.0 / rhs.0,
                self.0 / rhs.1,
                self.1 / rhs.0,
                self.1 / rhs.1,
            ],
            8,
        )
    }

    pub fn contains(self, value: f64) -> bool {
        self.ok() && f64::from(self.0) <= value && value <= f64::from(self.1)
    }
}

fn endpoint(value: f32) -> bool {
    let bits = value.to_bits() & 0x7fff_ffff;
    bits == 0 || (0x0080_0000..0x7f80_0000).contains(&bits)
}

fn operand(value: f32) -> bool {
    let bits = value.to_bits() & 0x7fff_ffff;
    bits == 0 || (0x2180_0000..=0x5d80_0000).contains(&bits)
}

fn operands(a: Interval, b: Interval) -> bool {
    [a.0, a.1, b.0, b.1].into_iter().all(operand)
}

fn four(values: [f32; 4], steps: u32) -> Interval {
    if values.iter().any(|value| !value.is_finite()) {
        return Interval::INVALID;
    }
    bounds(
        values.into_iter().fold(f32::INFINITY, f32::min),
        values.into_iter().fold(f32::NEG_INFINITY, f32::max),
        steps,
    )
}

fn bounds(low: f32, high: f32, steps: u32) -> Interval {
    if !low.is_finite() || !high.is_finite() {
        return Interval::INVALID;
    }
    let result = Interval(outward(low, false, steps), outward(high, true, steps));
    if result.ok() {
        result
    } else {
        Interval::INVALID
    }
}

fn outward(value: f32, upper: bool, steps: u32) -> f32 {
    if value.abs() < f32::MIN_POSITIVE {
        // A permitted approximate division can cross the normal/subnormal boundary
        // before flushing. Retain the same ULP allowance outside minimum normal.
        let edge = f32::from_bits(f32::MIN_POSITIVE.to_bits() + steps);
        return if upper { edge } else { -edge };
    }
    // Every caller uses 1 or 8: the sign bit of a finite normal cannot change
    // within eight representable steps, including a crossing into subnormals
    // or infinity. Thus the old loop chose this direction on every iteration.
    debug_assert!(matches!(steps, 1 | 8));
    let bits = value.to_bits();
    let bits = if (bits & 0x8000_0000 != 0) == upper {
        bits - steps
    } else {
        bits + steps
    };
    let result = f32::from_bits(bits);
    if result.abs() < f32::MIN_POSITIVE {
        if upper {
            if result < 0.0 { 0.0 } else { f32::MIN_POSITIVE }
        } else if result > 0.0 {
            0.0
        } else {
            -f32::MIN_POSITIVE
        }
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widening_preserves_stepwise_bits_at_every_exponent_and_domain_boundary() {
        // The previous stepwise recipe is the reference for the enclosure's
        // exact endpoint bits, including zero/normal and normal/infinity edges.
        fn stepwise(value: f32, upper: bool, steps: u32) -> f32 {
            if value.abs() < f32::MIN_POSITIVE {
                let edge = f32::from_bits(f32::MIN_POSITIVE.to_bits() + steps);
                return if upper { edge } else { -edge };
            }
            let mut bits = value.to_bits();
            for _ in 0..steps {
                if (bits & 0x8000_0000 != 0) == upper {
                    bits -= 1;
                } else {
                    bits += 1;
                }
            }
            let result = f32::from_bits(bits);
            if result.abs() < f32::MIN_POSITIVE {
                if upper {
                    if result < 0.0 { 0.0 } else { f32::MIN_POSITIVE }
                } else if result > 0.0 {
                    0.0
                } else {
                    -f32::MIN_POSITIVE
                }
            } else {
                result
            }
        }
        let mut samples = vec![0, 1, 0x007f_ffff, 0x0080_0000, 0x7f7f_ffff];
        for exponent in 1..=254_u32 {
            let edge = exponent << 23;
            for delta in -16_i64..=16 {
                let bits = (i64::from(edge) + delta) as u32;
                if bits < 0x7f80_0000 {
                    samples.push(bits);
                }
            }
        }
        let mut bits = 0x1234_5678_u32;
        for _ in 0..10_000 {
            bits = bits.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if bits & 0x7fff_ffff < 0x7f80_0000 {
                samples.push(bits & 0x7fff_ffff);
            }
        }
        for magnitude in samples {
            for sign in [0, 0x8000_0000] {
                let value = f32::from_bits(magnitude | sign);
                for steps in [1, 8] {
                    for upper in [false, true] {
                        assert_eq!(
                            outward(value, upper, steps).to_bits(),
                            stepwise(value, upper, steps).to_bits(),
                            "value={value:?}, steps={steps}, upper={upper}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn point_and_invalid_contract() {
        for value in [
            f32::NAN,
            f32::INFINITY,
            f32::from_bits(1),
            -f32::from_bits(1),
        ] {
            assert!(!Interval::point(value).ok());
        }
        let one = Interval::point(1.0);
        assert!(!one.plus(Interval::INVALID).ok());
        assert!(!Interval::INVALID.times(Interval::ZERO).ok());
        assert!(!one.divided_by(Interval(-1.0, 1.0)).ok());
        assert!(!one.divided_by(Interval::point(f32::MAX)).ok());
    }

    #[test]
    fn operations_enclose_f64_and_reverse_signs() {
        for a in [-100_000.0_f32, -1.25, -0.0001, 0.125, 1.0, 12_345.5] {
            for b in [-321.25_f32, -0.5, 0.00001, 0.75, 1024.0] {
                let x = Interval::point(a);
                let y = Interval::point(b);
                assert!(x.plus(y).contains(f64::from(a) + f64::from(b)));
                assert!(x.minus(y).contains(f64::from(a) - f64::from(b)));
                assert!(x.times(y).contains(f64::from(a) * f64::from(b)));
                assert!(x.divided_by(y).contains(f64::from(a) / f64::from(b)));
                assert!(x.negated().contains(-f64::from(a)));
            }
        }
    }

    #[test]
    fn exact_identities_do_not_erase_interval_uncertainty() {
        let x = Interval::point(128.0);
        assert_eq!(x.minus(x), Interval::ZERO);
        assert_eq!(x.plus(x.negated()), Interval::ZERO);
        assert_eq!(x.plus(Interval::ZERO), x);
        assert_eq!(x.times(Interval::ZERO), Interval::ZERO);
        let wide = Interval(1.0, 2.0);
        let difference = wide.minus(wide);
        assert!(difference.contains(-1.0) && difference.contains(1.0));
    }

    #[test]
    fn underflow_flush_cancellation_and_overflow_are_explicit() {
        let tiny = Interval::point(f32::MIN_POSITIVE);
        let product = tiny.times(Interval::point(0.5));
        assert!(!product.ok());
        // Division's permitted approximation may enter the subnormal range and
        // flush even when the exact quotient is slightly above minimum normal.
        assert!(bounds(0.0, 0.0, 8).contains(f64::from(f32::from_bits(0x0080_0003))));
        let near = tiny.plus(Interval::point(f32::MIN_POSITIVE));
        assert!(!near.ok());
        assert!(!Interval::point(f32::MAX).times(Interval::point(2.0)).ok());
        assert!(
            !Interval::point(f32::MAX)
                .plus(Interval::point(f32::MAX))
                .ok()
        );
        let cancel = Interval::point(f32::from_bits(0x3f80_0001)).minus(Interval::point(1.0));
        assert!(cancel.contains(2.0_f64.powi(-23)));
    }

    #[test]
    fn low_sun_division_encloses_distance_without_bias() {
        let height = Interval::point(64.0);
        let sun_z = Interval::point(0.000_001);
        let distance = height.divided_by(sun_z);
        assert!(distance.contains(64.0 / f64::from(sun_z.0)));
        assert!(distance.0 > 0.0);
        assert!(
            !Interval::point(1.0)
                .divided_by(Interval::point(f32::MIN_POSITIVE))
                .ok()
        );
    }

    #[test]
    fn arithmetic_domain_refuses_before_operation_but_preserves_identities() {
        let maximum = Interval::point(f32::MAX);
        assert!(maximum.ok());
        assert!(!maximum.times(Interval::point(2.0)).ok());
        assert!(!maximum.plus(maximum).ok());
        assert!(!operands(maximum, Interval::point(2.0)));
        assert_eq!(maximum.minus(maximum), Interval::ZERO);
        assert_eq!(maximum.times(Interval::ZERO), Interval::ZERO);
        assert_eq!(maximum.plus(Interval::ZERO), maximum);
        let low = Interval::point(f32::from_bits(0x2180_0000));
        let high = Interval::point(f32::from_bits(0x5d80_0000));
        assert!(low.times(low).ok());
        assert!(high.times(high).ok());
        assert!(high.divided_by(low).ok());
        assert!(low.divided_by(high).ok());
        assert!(!Interval::point(f32::from_bits(0x217f_ffff)).times(low).ok());
        assert!(!Interval::point(f32::from_bits(0x5d80_0001)).plus(high).ok());
        assert!(
            !Interval::point(1.0)
                .divided_by(Interval::point(f32::from_bits(0x217f_ffff)))
                .ok()
        );
        assert!(Interval::point(1.0).divided_by(Interval::point(1e-6)).ok());
    }
}

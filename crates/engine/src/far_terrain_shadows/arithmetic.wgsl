#define_import_path far_terrain_shadows::arithmetic

// CPU mirror: arithmetic.rs. This is an enclosure contract, not a GPU proof.
// Normal finite/zero endpoints only; WGSL may flush subnormals. +,-,* are
// correctly rounded; division has 2.5 ULP accuracy in its restricted range.
// Bitcast stepping materializes each operation before subsequent arithmetic.
// BEFORE +,-,*,/ every nonzero endpoint must have magnitude in [2^-60,2^60].
// Safe operations then cannot overflow/underflow, including finite-math cases
// where an overflow may produce an indeterminate finite result. Refusal maps
// to NUMERIC Unknown, never a geometric tolerance. Exact identities may bypass.
fn i_endpoint(v: f32) -> bool {
    let b = bitcast<u32>(v) & 0x7fffffffu;
    return b == 0u || (b >= 0x00800000u && b < 0x7f800000u);
}
fn i_point(v: f32) -> vec2<f32> {
    if (!i_endpoint(v)) { return vec2<f32>(1.0, -1.0); }
    return vec2<f32>(v);
}
fn i_ok(v: vec2<f32>) -> bool {
    return i_endpoint(v.x) && i_endpoint(v.y) && v.x <= v.y;
}
fn i_zero(v: vec2<f32>) -> bool { return v.x == 0.0 && v.y == 0.0; }
fn i_operand(v: f32) -> bool {
    let b = bitcast<u32>(v) & 0x7fffffffu;
    return b == 0u || (b >= 0x21800000u && b <= 0x5d800000u);
}
fn i_operands(a: vec2<f32>, b: vec2<f32>) -> bool {
    return i_operand(a.x) && i_operand(a.y) && i_operand(b.x) && i_operand(b.y);
}
fn i_outward(v: f32, upper: bool, steps: u32) -> f32 {
    let min_normal = bitcast<f32>(0x00800000u);
    if (abs(v) < min_normal) {
        // Approximate division may cross into subnormal before flushing.
        let edge = bitcast<f32>(0x00800000u + steps);
        return select(-edge, edge, upper);
    }
    var b = bitcast<u32>(v);
    // All callers widen by 1 or 8. At most eight steps cannot change the sign
    // bit of a finite normal input, so every old loop iteration had this same
    // direction. Keep the underflow corrections before and after unchanged.
    if (((b & 0x80000000u) != 0u) == upper) { b -= steps; }
    else { b += steps; }
    let result = bitcast<f32>(b);
    if (abs(result) < min_normal) {
        if (upper) { return select(min_normal, 0.0, result < 0.0); }
        return select(-min_normal, 0.0, result > 0.0);
    }
    return result;
}
fn i_bounds(lo: f32, hi: f32, steps: u32) -> vec2<f32> {
    if ((bitcast<u32>(lo) & 0x7fffffffu) >= 0x7f800000u ||
        (bitcast<u32>(hi) & 0x7fffffffu) >= 0x7f800000u) { return vec2<f32>(1.0, -1.0); }
    let result = vec2<f32>(i_outward(lo, false, steps), i_outward(hi, true, steps));
    if (!i_ok(result)) { return vec2<f32>(1.0, -1.0); }
    return result;
}
fn i_neg(a: vec2<f32>) -> vec2<f32> {
    if (!i_ok(a)) { return vec2<f32>(1.0, -1.0); }
    return vec2<f32>(-a.y, -a.x);
}
fn i_add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    if (!i_ok(a) || !i_ok(b)) { return vec2<f32>(1.0, -1.0); }
    if (i_zero(b)) { return a; }
    if (i_zero(a)) { return b; }
    if (a.x == a.y && b.x == b.y && a.x == -b.x) { return vec2<f32>(0.0); }
    if (!i_operands(a, b)) { return vec2<f32>(1.0, -1.0); }
    return i_bounds(a.x + b.x, a.y + b.y, 1u);
}
fn i_sub(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    if (!i_ok(a) || !i_ok(b)) { return vec2<f32>(1.0, -1.0); }
    if (i_zero(b)) { return a; }
    if (a.x == a.y && b.x == b.y && a.x == b.x) { return vec2<f32>(0.0); }
    if (!i_operands(a, b)) { return vec2<f32>(1.0, -1.0); }
    return i_bounds(a.x - b.y, a.y - b.x, 1u);
}
fn i_four(v: vec4<f32>, steps: u32) -> vec2<f32> {
    let bits = bitcast<vec4<u32>>(v) & vec4<u32>(0x7fffffffu);
    if (any(bits >= vec4<u32>(0x7f800000u))) { return vec2<f32>(1.0, -1.0); }
    return i_bounds(min(min(v.x, v.y), min(v.z, v.w)),
                    max(max(v.x, v.y), max(v.z, v.w)), steps);
}
fn i_mul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    if (!i_ok(a) || !i_ok(b)) { return vec2<f32>(1.0, -1.0); }
    if (i_zero(a) || i_zero(b)) { return vec2<f32>(0.0); }
    if (!i_operands(a, b)) { return vec2<f32>(1.0, -1.0); }
    return i_four(vec4<f32>(a.x * b.x, a.x * b.y, a.y * b.x, a.y * b.y), 1u);
}
fn i_div(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    if (!i_ok(a) || !i_ok(b) || (b.x <= 0.0 && b.y >= 0.0)) { return vec2<f32>(1.0, -1.0); }
    if (i_zero(a)) { return vec2<f32>(0.0); }
    if (!i_operands(a, b)) { return vec2<f32>(1.0, -1.0); }
    return i_four(vec4<f32>(a.x / b.x, a.x / b.y, a.y / b.x, a.y / b.y), 8u);
}

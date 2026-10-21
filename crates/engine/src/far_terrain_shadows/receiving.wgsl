#define_import_path far_terrain_shadows::receiving

#import far_terrain_shadows::arithmetic::{i_point, i_ok, i_add, i_sub, i_mul, i_div, i_neg}

struct F1Header { words: array<vec4<u32>, 4>, }
struct F1View { words: array<vec4<u32>, 25>, }
@group(4) @binding(7) var<uniform> f1_header: F1Header;
@group(4) @binding(8) var<storage, read> f1_words: array<u32>;
@group(4) @binding(9) var<uniform> f1_view: F1View;

// Unknown=0, Clear=1, Occluded=2. Counters are returned, not atomically written.
struct F1Result {
    state: u32, reasons: u32, cells: u32, nodes: u32,
    triangles: u32, unknown_cells: u32,
}
const F1_UNLISTED: u32 = 1u;
const F1_OUTSIDE: u32 = 64u;
const F1_POST_WELD: u32 = 128u;
const F1_IDENTITY: u32 = 512u;
const F1_RECEIVER: u32 = 1024u;
const F1_NUMERIC: u32 = 2048u;
const F1_WORK: u32 = 4096u;
const F1_COPLANAR: u32 = 8192u;
const F1_PACKET: u32 = 16384u;
const F1_NONE: u32 = 0xffffffffu;

fn f1_h(n: u32) -> u32 { return f1_header.words[n / 4u][n % 4u]; }
fn f1_v(n: u32) -> u32 { return f1_view.words[n / 4u][n % 4u]; }
fn f1_mark(input: F1Result, reason: u32) -> F1Result {
    var result = input;
    result.state = 0u;
    result.reasons |= reason;
    return result;
}
fn f1_unresolved_visit(input: F1Result, reason: u32) -> F1Result {
    var result = f1_mark(input, reason);
    result.unknown_cells++;
    return result;
}
fn f1_visibility(result: F1Result) -> f32 {
    return select(1.0, 0.0, result.state == 2u);
}

struct F1Int { value: i32, ok: bool, }
fn f1_difference(a: i32, b: i32) -> F1Int {
    let d = a - b;
    return F1Int(d, !((a >= b && d < 0) || (a < b && d >= 0)));
}
fn f1_increment(a: i32, by: i32) -> F1Int {
    let sum = a + by;
    return F1Int(sum, !((by > 0 && sum < a) || (by < 0 && sum > a)));
}
fn f1_in_domain(cell: vec2<i32>) -> bool {
    return all(cell >= vec2<i32>(bitcast<i32>(f1_h(4u)), bitcast<i32>(f1_h(5u)))) &&
        all(cell <= vec2<i32>(bitcast<i32>(f1_h(6u)), bitcast<i32>(f1_h(7u))));
}
fn f1_outside_reason() -> u32 {
    return select(F1_OUTSIDE, 0u, f1_h(8u) == 1u && f1_h(11u) == 1u && f1_h(12u) == 1u);
}

struct F1Cell { page: u32, root: u32, coverage: u32, reasons: u32, }
fn f1_lookup(cell: vec2<i32>) -> F1Cell {
    if !f1_in_domain(cell) { return F1Cell(F1_NONE, F1_NONE, 0u, f1_outside_reason()); }
    var lo = 0u;
    var hi = f1_h(2u);
    loop {
        if lo >= hi { break; }
        let mid = lo + (hi - lo) / 2u;
        let at = mid * 8u;
        let key = vec2<i32>(bitcast<i32>(f1_words[at]), bitcast<i32>(f1_words[at + 1u]));
        if key.x < cell.x || (key.x == cell.x && key.y < cell.y) { lo = mid + 1u; }
        else { hi = mid; }
    }
    if lo < f1_h(2u) {
        let at = lo * 8u;
        if bitcast<i32>(f1_words[at]) == cell.x && bitcast<i32>(f1_words[at + 1u]) == cell.y {
            return F1Cell(f1_words[at + 2u], f1_words[at + 3u], f1_words[at + 4u], f1_words[at + 5u]);
        }
    }
    return F1Cell(F1_NONE, F1_NONE, 0u, F1_UNLISTED);
}

struct F1Quad { h: vec4<f32>, ok: bool, }
fn f1_quad(page: u32, q: vec2<u32>) -> F1Quad {
    if page >= f1_h(3u) || any(q >= vec2<u32>(32u)) { return F1Quad(vec4<f32>(0.0), false); }
    let at = f1_h(14u) + page * 1089u + q.y * 33u + q.x;
    let h = bitcast<vec4<f32>>(vec4<u32>(f1_words[at], f1_words[at + 1u], f1_words[at + 33u], f1_words[at + 34u]));
    return F1Quad(h, i_ok(i_point(h.x)) && i_ok(i_point(h.y)) && i_ok(i_point(h.z)) && i_ok(i_point(h.w)));
}

// Exact diagonal comparison for f32 fractions in [0,1]. Do NOT use rounded x+y==1.
// Subtraction 1-x is exact for x>=.5 (Sterbenz); both below .5 are strictly lower.
fn f1_diagonal(f: vec2<f32>) -> i32 {
    if f.x >= 0.5 {
        let edge = 1.0 - f.x;
        return select(select(0, 1, f.y > edge), -1, f.y < edge);
    }
    if f.y >= 0.5 {
        let edge = 1.0 - f.y;
        return select(select(0, 1, f.x > edge), -1, f.x < edge);
    }
    return -1;
}
fn f1_inside(f: vec2<f32>, half: u32) -> bool {
    if any(f < vec2<f32>(0.0)) || any(f > vec2<f32>(1.0)) { return false; }
    let side = f1_diagonal(f);
    return select(side >= 0, side <= 0, half == 0u);
}
fn f1_surface(h: vec4<f32>, half: u32, x: vec2<f32>, y: vec2<f32>) -> vec2<f32> {
    if half == 0u {
        return i_add(i_add(i_point(h.x), i_mul(i_sub(i_point(h.y), i_point(h.x)), x)),
            i_mul(i_sub(i_point(h.z), i_point(h.x)), y));
    }
    return i_add(i_add(i_point(h.w), i_mul(i_sub(i_point(h.z), i_point(h.w)), i_sub(i_point(1.0), x))),
        i_mul(i_sub(i_point(h.y), i_point(h.w)), i_sub(i_point(1.0), y)));
}
fn f1_slope(h: vec4<f32>, half: u32, axis: u32) -> vec2<f32> {
    if half == 0u { return i_sub(i_point(h[axis + 1u]), i_point(h.x)); }
    return i_sub(i_point(h.w), i_point(h[2u - axis]));
}

struct F1Contact {
    cell: vec2<i32>, sample: vec2<f32>, quad: vec2<u32>, fraction: vec2<f32>,
    half: u32, heights: vec4<f32>, height: vec2<f32>,
}
// Proof is topology plus equal canonical endpoint heights, NOT XY proximity.
fn f1_same_contact(contact: F1Contact, cell: vec2<i32>, q: vec2<u32>, half: u32, h: vec4<f32>) -> bool {
    let dx = f1_difference(cell.x, contact.cell.x);
    let dy = f1_difference(cell.y, contact.cell.y);
    if !dx.ok || !dy.ok || dx.value < -1 || dx.value > 1 || dy.value < -1 || dy.value > 1 { return false; }
    let rel = vec2<i32>(dx.value, dy.value) * 32 + vec2<i32>(q) - vec2<i32>(contact.quad);
    var f = contact.fraction;
    for (var axis = 0u; axis < 2u; axis++) {
        if rel[axis] == 0 { continue; }
        if rel[axis] == -1 && contact.fraction[axis] == 0.0 { f[axis] = 1.0; }
        else { return false; }
    }
    if !f1_inside(f, half) { return false; }
    let source = contact.heights;
    if all(contact.fraction == vec2<f32>(0.0)) {
        let vertex = u32(f.x) + 2u * u32(f.y);
        return source.x == h[vertex];
    }
    if contact.fraction.x == 0.0 {
        let x = u32(f.x);
        return source.x == h[x] && source.z == h[x + 2u];
    }
    if contact.fraction.y == 0.0 {
        let y = 2u * u32(f.y);
        return source.x == h[y] && source.y == h[y + 1u];
    }
    return all(rel == vec2<i32>(0)) && (half == contact.half || f1_diagonal(contact.fraction) == 0);
}

// Conservative positive-ray box overlap. Invalid broadphase math means MAYBE,
// never a missed cell/node; the triangle predicates still decide it.
fn f1_box(origin: array<vec2<f32>, 3>, direction: vec3<f32>, low: vec3<f32>, high: vec3<f32>, axes: u32) -> bool {
    var near = 0.0;
    var far = bitcast<f32>(0x7f7fffffu);
    for (var axis = 0u; axis < axes; axis++) {
        if !i_ok(origin[axis]) { return true; }
        if direction[axis] == 0.0 {
            if origin[axis].y < low[axis] || origin[axis].x > high[axis] { return false; }
        } else {
            let a = i_div(i_sub(i_point(low[axis]), origin[axis]), i_point(direction[axis]));
            let b = i_div(i_sub(i_point(high[axis]), origin[axis]), i_point(direction[axis]));
            if !i_ok(a) || !i_ok(b) { return true; }
            near = max(near, min(a.x, b.x));
            far = min(far, max(a.y, b.y));
            // Further slabs only shrink this positive-time interval. A proved miss
            // needs no later arithmetic; invalid later axes cannot erase the proof.
            if near > far || far <= 0.0 { return false; }
        }
    }
    return near <= far && far > 0.0;
}

// Triangle result: 0 miss, 1 proven positive hit, otherwise explicit Unknown bits.
fn f1_triangle(contact: F1Contact, cell: vec2<i32>, q: vec2<u32>, half: u32, h: vec4<f32>,
    origin_x: vec2<f32>, origin_y: vec2<f32>, direction: vec3<f32>) -> u32 {
    let sx = f1_slope(h, half, 0u);
    let sy = f1_slope(h, half, 1u);
    let denominator = i_sub(i_sub(i_point(direction.z), i_mul(sx, i_point(direction.x))), i_mul(sy, i_point(direction.y)));
    if !i_ok(denominator) { return F1_NUMERIC; }
    if f1_same_contact(contact, cell, q, half, h) {
        // Only mathematically t=0 is excluded. Coplanarity uses THIS canonical plane.
        if denominator.x > 0.0 || denominator.y < 0.0 { return 0u; }
        return F1_COPLANAR | F1_NUMERIC;
    }
    let x = i_sub(origin_x, i_point(f32(q.x)));
    let y = i_sub(origin_y, i_point(f32(q.y)));
    let offset = i_sub(contact.height, f1_surface(h, half, x, y));
    if !i_ok(offset) { return F1_NUMERIC; }
    if denominator.x <= 0.0 && denominator.y >= 0.0 {
        if all(denominator == vec2<f32>(0.0)) && (offset.x > 0.0 || offset.y < 0.0) { return 0u; }
        return F1_COPLANAR | F1_NUMERIC;
    }
    let t = i_div(i_neg(offset), denominator);
    if !i_ok(t) { return F1_NUMERIC; }
    if t.y <= 0.0 { return 0u; }
    let tx = i_add(x, i_mul(t, i_point(direction.x)));
    let ty = i_add(y, i_mul(t, i_point(direction.y)));
    if !i_ok(tx) || !i_ok(ty) { return F1_NUMERIC; }
    if tx.y < 0.0 || ty.y < 0.0 || tx.x > 1.0 || ty.x > 1.0 { return 0u; }
    let sum = i_add(tx, ty);
    if !i_ok(sum) { return F1_NUMERIC; }
    if (half == 0u && sum.x > 1.0) || (half == 1u && sum.y < 1.0) { return 0u; }
    if t.x > 0.0 && tx.x >= 0.0 && ty.x >= 0.0 && tx.y <= 1.0 && ty.y <= 1.0 &&
        ((half == 0u && sum.y <= 1.0) || (half == 1u && sum.x >= 1.0)) { return 1u; }
    return F1_NUMERIC;
}

fn f1_visit_cell(cell: vec2<i32>, contact: F1Contact, direction: vec3<f32>, input: F1Result,
    max_cells: u32, max_nodes: u32) -> F1Result {
    var result = input;
    if result.cells >= max_cells { return f1_mark(result, F1_WORK); }
    result.cells++;
    let dx = f1_difference(cell.x, contact.cell.x);
    let dy = f1_difference(cell.y, contact.cell.y);
    // Exact integer sample offsets; wider worlds return Unknown, never wrap or truncate.
    if !dx.ok || !dy.ok || dx.value < -524287 || dx.value > 524287 || dy.value < -524287 || dy.value > 524287 { return f1_unresolved_visit(result, F1_NUMERIC); }
    let ox = i_sub(i_point(contact.sample.x), i_point(f32(dx.value * 32)));
    let oy = i_sub(i_point(contact.sample.y), i_point(f32(dy.value * 32)));
    let origin = array<vec2<f32>, 3>(ox, oy, contact.height);
    if !f1_box(origin, direction, vec3<f32>(0.0), vec3<f32>(32.0, 32.0, 0.0), 2u) { return result; }
    let entry = f1_lookup(cell);
    if entry.page == F1_NONE {
        if (entry.coverage & 6u) == 6u || (!f1_in_domain(cell) && entry.reasons == 0u) { return result; }
        if entry.reasons == 0u { return f1_unresolved_visit(result, F1_PACKET); }
        result.unknown_cells++;
        return f1_mark(result, entry.reasons);
    }
    if entry.page >= f1_h(3u) || entry.root != entry.page * 341u + 340u || entry.coverage != 11u || entry.reasons != 0u {
        return f1_unresolved_visit(result, F1_PACKET);
    }
    var unresolved = false;
    var stack: array<vec3<u32>, 16>;
    stack[0] = vec3<u32>(4u, 0u, 0u); // level,x,y. Max DFS occupancy13.
    var count = 1u;
    let offsets = array<u32, 5>(0u, 256u, 320u, 336u, 340u);
    let sides = array<u32, 5>(16u, 8u, 4u, 2u, 1u);
    loop {
        if count == 0u { break; }
        if result.nodes >= max_nodes { return f1_unresolved_visit(result, F1_WORK); }
        result.nodes++;
        count--;
        let node = stack[count];
        let span = 2u << node.x;
        let low = vec2<u32>(node.y, node.z) * span;
        let at = f1_h(15u) + 2u * (entry.page * 341u + offsets[node.x] + node.z * sides[node.x] + node.y);
        let bounds = bitcast<vec2<f32>>(vec2<u32>(f1_words[at], f1_words[at + 1u]));
        if i_ok(i_point(bounds.x)) && i_ok(i_point(bounds.y)) && bounds.x <= bounds.y {
            if !f1_box(origin, direction, vec3<f32>(vec2<f32>(low), bounds.x),
                vec3<f32>(vec2<f32>(low + vec2<u32>(span)), bounds.y), 3u) { continue; }
        }
        if node.x != 0u {
            if count + 4u > 16u { return f1_unresolved_visit(result, F1_PACKET); }
            for (var y = 0u; y < 2u; y++) {
                for (var x = 0u; x < 2u; x++) {
                    stack[count] = vec3<u32>(node.x - 1u, node.y * 2u + x, node.z * 2u + y);
                    count++;
                }
            }
        } else {
            for (var y = 0u; y < 2u; y++) {
                for (var x = 0u; x < 2u; x++) {
                    let q = low + vec2<u32>(x, y);
                    if !f1_box(origin, direction, vec3<f32>(vec2<f32>(q), 0.0), vec3<f32>(vec2<f32>(q + vec2<u32>(1u)), 0.0), 2u) { continue; }
                    let quad = f1_quad(entry.page, q);
                    if !quad.ok { result = f1_mark(result, F1_NUMERIC); unresolved = true; continue; }
                    for (var half = 0u; half < 2u; half++) {
                        if result.triangles == 0xffffffffu { return f1_unresolved_visit(result, F1_WORK); }
                        result.triangles++;
                        let hit = f1_triangle(contact, cell, q, half, quad.h, ox, oy, direction);
                        if hit == 1u {
                            result.state = 2u;
                            if unresolved { result.unknown_cells++; }
                            return result;
                        }
                        if hit != 0u { result = f1_mark(result, hit); unresolved = true; }
                    }
                }
            }
        }
    }
    if unresolved { result.unknown_cells++; }
    return result;
}

struct F1Mapped { cell: vec2<i32>, sample: vec2<f32>, ok: bool, }
struct F1Float { value: f32, ok: bool, }
// Change only exponent bits. No floating multiply can overflow or flush a ray
// component/coordinate to zero before a post-operation check sees it.
fn f1_power_two(value: f32, shift: i32) -> F1Float {
    let bits = bitcast<u32>(value);
    if (bits & 0x7fffffffu) == 0u { return F1Float(value, true); }
    let exponent = i32((bits >> 23u) & 255u);
    let next = exponent + shift;
    if exponent == 0 || exponent == 255 || next < 1 || next > 254 { return F1Float(0.0, false); }
    return F1Float(bitcast<f32>((bits & 0x807fffffu) | (u32(next) << 23u)), true);
}
struct F1Axis { delta: i32, local: f32, ok: bool, }
// Split a binary f32 sample coordinate by 32 using integer significand bits.
// Negative remainders are accepted only when 32-rem is exactly representable.
// This avoids a TwoDiff/reassociation assumption at cell boundaries.
fn f1_split_samples(value: f32) -> F1Axis {
    let bits = bitcast<u32>(value);
    let absolute = bits & 0x7fffffffu;
    if absolute == 0u { return F1Axis(0, 0.0, true); }
    let raw_exponent = (absolute >> 23u) & 255u;
    if raw_exponent == 0u || raw_exponent > 155u { return F1Axis(0, 0.0, false); }
    let exponent = i32(raw_exponent) - 127;
    var whole = 0u;
    var remainder = bitcast<f32>(absolute);
    if exponent >= 5 {
        let shift = u32(28 - exponent);
        let significand = (absolute & 0x007fffffu) | 0x00800000u;
        whole = significand >> shift;
        let fraction = significand & ((1u << shift) - 1u);
        if fraction == 0u { remainder = 0.0; }
        else {
            let scaled = f1_power_two(f32(fraction), exponent - 23);
            if !scaled.ok { return F1Axis(0, 0.0, false); }
            remainder = scaled.value;
        }
    }
    if (bits & 0x80000000u) == 0u { return F1Axis(i32(whole), remainder, true); }
    if remainder == 0.0 { return F1Axis(-i32(whole), 0.0, true); }
    let rb = bitcast<u32>(remainder);
    let re = i32((rb >> 23u) & 255u) - 127;
    if re < -19 { return F1Axis(0, 0.0, false); }
    if re < 4 {
        let significand = (rb & 0x007fffffu) | 0x00800000u;
        if (significand & ((1u << u32(4 - re)) - 1u)) != 0u { return F1Axis(0, 0.0, false); }
    }
    return F1Axis(-i32(whole) - 1, 32.0 - remainder, true);
}
// Binary-power spacing and exact bitwise remainder only. Failure is Unknown.
fn f1_map(renderer: vec3<f32>, spacing: f32, instance_cell_origin: vec2<i32>) -> F1Mapped {
    let bits = bitcast<u32>(spacing);
    let exponent = (bits >> 23u) & 255u;
    if (bits & 0x807fffffu) != 0u || exponent < 6u || exponent > 253u {
        return F1Mapped(vec2<i32>(0), vec2<f32>(0.0), false);
    }
    let position = vec2<f32>(renderer.x, bitcast<f32>(bitcast<u32>(renderer.z) ^ 0x80000000u));
    var cell = instance_cell_origin;
    var sample = vec2<f32>(0.0);
    for (var axis = 0u; axis < 2u; axis++) {
        if !i_ok(i_point(position[axis])) { return F1Mapped(cell, sample, false); }
        let scaled = f1_power_two(position[axis], 132 - i32(exponent));
        if !scaled.ok { return F1Mapped(cell, sample, false); }
        let split = f1_split_samples(scaled.value);
        if !split.ok { return F1Mapped(cell, sample, false); }
        let next = f1_increment(cell[axis], split.delta);
        if !next.ok { return F1Mapped(cell, sample, false); }
        sample[axis] = split.local;
        cell[axis] = next.value;
    }
    return F1Mapped(cell, sample, true);
}

fn f1_visit_with_parallel_edges(cell: vec2<i32>, contact: F1Contact, direction: vec3<f32>,
    input: F1Result, max_cells: u32, max_nodes: u32) -> F1Result {
    var result = f1_visit_cell(cell, contact, direction, input, max_cells, max_nodes);
    if result.state == 2u || (result.reasons & F1_WORK) != 0u { return result; }
    let west = direction.x == 0.0 && contact.sample.x == 0.0;
    let south = direction.y == 0.0 && contact.sample.y == 0.0;
    for (var y = 0u; y <= select(0u, 1u, south); y++) {
        for (var x = 0u; x <= select(0u, 1u, west); x++) {
            if x == 0u && y == 0u { continue; }
            let cx = f1_increment(cell.x, -i32(x));
            let cy = f1_increment(cell.y, -i32(y));
            if !cx.ok || !cy.ok { result = f1_mark(result, F1_OUTSIDE); continue; }
            result = f1_visit_cell(vec2<i32>(cx.value, cy.value), contact, direction, result, max_cells, max_nodes);
            if result.state == 2u || (result.reasons & F1_WORK) != 0u { return result; }
        }
    }
    return result;
}

fn f1_next_boundary(cell: i32, source: i32, sample: f32, direction: f32) -> vec2<f32> {
    let delta = f1_difference(cell, source);
    if !delta.ok || delta.value < -524286 || delta.value > 524286 { return vec2<f32>(1.0, -1.0); }
    let boundary = (delta.value + select(0, 1, direction > 0.0)) * 32;
    return i_div(i_sub(i_point(f32(boundary)), i_point(sample)), i_point(direction));
}

// Bounds are receiver-relative integer sample coordinates. A missing/Unknown
// region has XY bounds only: a known-surface height bound cannot close coverage.
fn f1_world_box(at: u32, contact: F1Contact, direction: vec3<f32>, bounded: bool) -> bool {
    let low_x = f1_difference(bitcast<i32>(f1_words[at]), contact.cell.x);
    let low_y = f1_difference(bitcast<i32>(f1_words[at + 1u]), contact.cell.y);
    let high_x = f1_difference(bitcast<i32>(f1_words[at + 2u]), contact.cell.x);
    let high_y = f1_difference(bitcast<i32>(f1_words[at + 3u]), contact.cell.y);
    if !low_x.ok || !low_y.ok || !high_x.ok || !high_y.ok ||
        low_x.value < -524287 || low_y.value < -524287 ||
        high_x.value > 524286 || high_y.value > 524286 { return true; }
    let heights = bitcast<vec2<f32>>(vec2<u32>(f1_words[at + 4u], f1_words[at + 5u]));
    if bounded && (!i_ok(i_point(heights.x)) || !i_ok(i_point(heights.y)) || heights.x > heights.y) { return true; }
    return f1_box(array<vec2<f32>, 3>(i_point(contact.sample.x), i_point(contact.sample.y), contact.height),
        direction, vec3<f32>(f32(low_x.value * 32), f32(low_y.value * 32), heights.x),
        vec3<f32>(f32((high_x.value + 1) * 32), f32((high_y.value + 1) * 32), heights.y), select(2u, 3u, bounded));
}

// The origin must belong to this actual instance/local-coordinate pair.
// World/snapshot/publication certification remains the host responsibility.
fn f1_receive_at_origin(instance_local_renderer: vec3<f32>, receiver_world: u32,
    actual_instance_cell_origin: vec2<i32>, expected_view: vec2<u32>, expected_sun: vec2<u32>, max_cells: u32, max_nodes: u32) -> F1Result {
    var result = F1Result(1u, 0u, 0u, 0u, 0u, 0u);
    if receiver_world != f1_h(1u) || receiver_world != f1_v(25u) ||
        any(expected_view != vec2<u32>(f1_v(0u), f1_v(1u))) ||
        any(expected_sun != vec2<u32>(f1_v(4u), f1_v(5u))) ||
        f1_v(15u) != 2u || f1_v(19u) != 0u || f1_v(30u) != 1u || f1_v(73u) != 0u {
        return f1_mark(result, F1_IDENTITY);
    }
    if f1_h(13u) != 1u { return f1_mark(result, F1_POST_WELD); }
    let count = arrayLength(&f1_words);
    if f1_h(0u) != 3u || f1_h(2u) > count / 8u || f1_h(14u) != f1_h(2u) * 8u ||
        f1_h(14u) > f1_h(15u) || f1_h(15u) > count ||
        f1_h(3u) > (f1_h(15u) - f1_h(14u)) / 1089u ||
        f1_h(15u) - f1_h(14u) != f1_h(3u) * 1089u || f1_h(3u) > (count - f1_h(15u)) / 682u {
        return f1_mark(result, F1_PACKET);
    }
    let tree_start = f1_h(15u) + f1_h(3u) * 682u;
    if tree_start >= count || (count - tree_start) % 9u != 0u { return f1_mark(result, F1_PACKET); }
    let tree_count = (count - tree_start) / 9u;
    let mapped = f1_map(instance_local_renderer, bitcast<f32>(f1_v(14u)), actual_instance_cell_origin);
    if !mapped.ok { return f1_mark(result, F1_RECEIVER | F1_NUMERIC); }
    let entry = f1_lookup(mapped.cell);
    if entry.page == F1_NONE { return f1_mark(result, entry.reasons | F1_RECEIVER); }
    if entry.page >= f1_h(3u) || entry.coverage != 11u || entry.reasons != 0u { return f1_mark(result, F1_PACKET); }
    let q = vec2<u32>(floor(mapped.sample));
    let f = mapped.sample - vec2<f32>(q);
    let half = select(0u, 1u, f1_diagonal(f) > 0);
    let quad = f1_quad(entry.page, q);
    if !quad.ok { return f1_mark(result, F1_RECEIVER | F1_NUMERIC); }
    let height = f1_surface(quad.h, half, i_point(f.x), i_point(f.y));
    if !i_ok(height) { return f1_mark(result, F1_RECEIVER | F1_NUMERIC); }
    let contact = F1Contact(mapped.cell, mapped.sample, q, f, half, quad.h, height);
    let exponent = (f1_v(14u) >> 23u) & 255u;
    let sx = f1_power_two(bitcast<f32>(f1_v(16u)), 132 - i32(exponent));
    let sy = f1_power_two(bitcast<f32>(f1_v(17u)), 132 - i32(exponent));
    if !sx.ok || !sy.ok { return f1_mark(result, F1_NUMERIC); }
    let direction = vec3<f32>(sx.value, sy.value, bitcast<f32>(f1_v(18u)));
    if !i_ok(i_point(direction.x)) || !i_ok(i_point(direction.y)) || !i_ok(i_point(direction.z)) || all(direction == vec3<f32>(0.0)) {
        return f1_mark(result, F1_NUMERIC);
    }
    if max_cells == 0u || max_nodes == 0u { return f1_mark(result, F1_WORK); }
    // Immutable DFS partition covers every cell in the declared domain. Escapes
    // skip only a conservatively missed subtree, never an unrepresented gap.
    var index = 0u;
    loop {
        if index >= tree_count { break; }
        if result.nodes >= max_nodes { return f1_mark(result, F1_WORK); }
        result.nodes++;
        let at = tree_start + index * 9u;
        let escape = f1_words[at + 6u];
        let kind = f1_words[at + 7u];
        let low = vec2<i32>(bitcast<i32>(f1_words[at]), bitcast<i32>(f1_words[at + 1u]));
        let high = vec2<i32>(bitcast<i32>(f1_words[at + 2u]), bitcast<i32>(f1_words[at + 3u]));
        if escape <= index || escape > tree_count || kind > 4u || any(low > high) ||
            !f1_in_domain(low) || !f1_in_domain(high) { return f1_mark(result, F1_PACKET); }
        if kind == 3u || !f1_world_box(at, contact, direction, kind == 1u || kind == 2u) {
            index = escape;
            continue;
        }
        if kind <= 1u {
            if escape <= index + 1u { return f1_mark(result, F1_PACKET); }
            index++;
            continue;
        }
        if escape != index + 1u { return f1_mark(result, F1_PACKET); }
        if kind == 2u {
            if any(low != high) { return f1_mark(result, F1_PACKET); }
            result = f1_visit_cell(low, contact, direction, result, max_cells, max_nodes);
        } else {
            let reasons = f1_words[at + 8u];
            if reasons == 0u { return f1_mark(result, F1_PACKET); }
            result = f1_unresolved_visit(result, reasons);
        }
        if result.state == 2u || (result.reasons & F1_WORK) != 0u { return result; }
        index = escape;
    }
    // A finite XY domain never attests the rest of a horizontal ray. The vertical
    // seam cases also touch the western/southern exterior neighbor, as before.
    if direction.x != 0.0 || direction.y != 0.0 ||
        (contact.sample.x == 0.0 && contact.cell.x == bitcast<i32>(f1_h(4u))) ||
        (contact.sample.y == 0.0 && contact.cell.y == bitcast<i32>(f1_h(5u))) {
        let outside = f1_outside_reason();
        if outside != 0u { result = f1_mark(result, outside); }
    }
    return result;
}

// Backwards-compatible single-origin entrypoint; no uniform/layout change.
fn f1_receive(instance_local_renderer: vec3<f32>, receiver_world: u32,
    expected_view: vec2<u32>, expected_sun: vec2<u32>, max_cells: u32, max_nodes: u32) -> F1Result {
    let origin = vec2<i32>(bitcast<i32>(f1_v(12u)), bitcast<i32>(f1_v(13u)));
    return f1_receive_at_origin(instance_local_renderer, receiver_world, origin,
        expected_view, expected_sun, max_cells, max_nodes);
}

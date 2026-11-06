// Port of Shaders/crt.metal: barrel curve, then phosphor bloom, scanlines and vignette.
// Positions are in points like the SwiftUI version, so one font pixel is `zoom` points.
struct U {
    size: vec2f,    // window, points
    src: vec2f,     // framebuffer, font pixels
    scale: f32,     // device pixels per point
    zoom: f32,
    srgb: f32,      // 1 when the surface applies its own gamma
    _pad: f32,
}
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(0) @binding(2) var<uniform> u: U;

const TAU = 6.2831853;
const CURVE = 0.045;
// Same padding and corner as ContentView: 18 at the sides and bottom, 30 at the top.
const PAD = vec2f(18.0, 30.0);
const PAD_TOTAL = vec2f(36.0, 48.0);
const RADIUS = 14.0;
const BG = vec3f(0.07, 0.045, 0.03);

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}

// The flat screen at point p: framebuffer inside the rounded rect, black outside.
fn screen(p: vec2f) -> vec3f {
    let area = u.size - PAD_TOTAL;
    let q = p - PAD;
    let corner = abs(q - area * 0.5) - area * 0.5 + RADIUS;
    let d = length(max(corner, vec2f(0.0))) + min(max(corner.x, corner.y), 0.0) - RADIUS;
    let inside = clamp(0.5 - d * u.scale, 0.0, 1.0);
    let f = q / u.zoom;
    var c = BG;
    if all(f >= vec2f(0.0)) && all(f < u.src) {
        // Sharp bilinear: font pixels stay square, only their edges get smoothed by the curve.
        let k = u.zoom * u.scale;
        let t = floor(f) + 0.5 + clamp((fract(f) - 0.5) * k, vec2f(-0.5), vec2f(0.5));
        c = textureSampleLevel(tex, smp, t / u.src, 0.0).rgb;
    }
    return c * inside;
}

@fragment
fn fs(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let pos = frag.xy / u.scale;
    var uv = pos / u.size * 2.0 - 1.0;
    uv *= 1.0 + CURVE * dot(uv, uv);
    let p = (uv + 1.0) * 0.5 * u.size;

    let r = 2.0 * u.zoom;
    var glow = screen(p + vec2f(r, 0.0)) + screen(p + vec2f(-r, 0.0)) + screen(p + vec2f(0.0, r)) + screen(p + vec2f(0.0, -r))
        + screen(p + vec2f(r, r)) + screen(p + vec2f(-r, r)) + screen(p + vec2f(r, -r)) + screen(p + vec2f(-r, -r));
    glow += (screen(p + vec2f(2.0 * r, 0.0)) + screen(p + vec2f(-2.0 * r, 0.0)) + screen(p + vec2f(0.0, 2.0 * r)) + screen(p + vec2f(0.0, -2.0 * r))) * 0.5;

    var col = screen(p) + glow * 0.07;
    // SwiftUI runs this on the flat layer, one value per device pixel, and bends afterwards. Doing it on the bent
    // position instead draws moire arcs across solid areas. Too fine to resolve means a flat dim on every display.
    let period = u.zoom * u.scale;
    var scan = 0.74;
    if period >= 3.0 { scan += 0.26 * cos((floor(p.y * u.scale) + 0.5) / u.scale * TAU / u.zoom); }
    col *= scan;
    let d = dot(uv, uv);
    col *= 1.0 - 0.22 * d * d;
    if u.srgb > 0.5 { col = pow(max(col, vec3f(0.0)), vec3f(2.2)); }
    return vec4f(col, 1.0);
}

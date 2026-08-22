// The isolines pattern: the milstrike isotri wavy isolines
// (https://www.shadertoy.com/view/Xt3yDS via isotri.frag), triangle hidden,
// tinted by the palette's mover colour.
//
// clock: x = seconds since start, y = beats since start (unwrapped), z = bpm.
// Brightness is applied downstream in the fx pass.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clock: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> color1: vec4<f32>;
// x: drift units/s, y: kick per bar, z: kick ease exponent, w: sway speed.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> motion: vec4<f32>;
// x: camera height, y: distance fade, z: isoline count, w: base thickness.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> look: vec4<f32>;
// x: thickness the kick adds, y: horizon sway range, z: fraction of the bar
// the kick takes to settle.
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> accent: vec4<f32>;

const PI: f32 = 3.14159265359;
const ASPECT: f32 = 1.5;
const TRI_R: f32 = -3.0;

// Simplex(ish) noise, Shane https://www.shadertoy.com/view/ldscWH

fn hash33(p: vec3<f32>) -> vec3<f32> {
    let n = sin(dot(p, vec3<f32>(7.0, 157.0, 113.0)));
    return fract(vec3<f32>(2097152.0, 262144.0, 32768.0) * n) * 2.0 - 1.0;
}

fn tetra_noise(q: vec3<f32>) -> f32 {
    var p = q;
    let i = floor(p + vec3<f32>(dot(p, vec3<f32>(0.333333))));
    p = p - i + vec3<f32>(dot(i, vec3<f32>(0.166666)));
    var i1 = step(p.yzx, p);
    let i2 = max(i1, 1.0 - i1.zxy);
    i1 = min(i1, 1.0 - i1.zxy);
    let p1 = p - i1 + 0.166666;
    let p2 = p - i2 + 0.333333;
    let p3 = p - 0.5;
    let v = max(
        vec4<f32>(0.5) - vec4<f32>(dot(p, p), dot(p1, p1), dot(p2, p2), dot(p3, p3)),
        vec4<f32>(0.0),
    );
    let d = vec4<f32>(
        dot(p, hash33(i)),
        dot(p1, hash33(i + i1)),
        dot(p2, hash33(i + i2)),
        dot(p3, hash33(i + vec3<f32>(1.0))),
    );
    return clamp(dot(d, v * v * v * 8.0) * 1.732 + 0.5, 0.0, 1.0);
}

// Triangle distance, corners kept pointy for the isolines.
fn s_tri(p: vec2<f32>, radius: f32) -> f32 {
    let r = radius / 2.0;
    let a = normalize(vec2<f32>(1.6, 1.0));
    return max(
        dot(p, vec2<f32>(0.0, -1.0)) - r,
        max(dot(p, a) - r, dot(p, a * vec2<f32>(-1.0, 1.0)) - r),
    );
}

// GLSL-style mod: result takes the sign of the divisor.
fn gmod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

// Repeat space as two samples half a period out of phase, blended so each
// sample's seam lands on the continuous part of the other.
fn repeat_start(x: f32, size: f32) -> vec2<f32> {
    return vec2<f32>(gmod(x - size / 2.0, size), gmod(x, size));
}

fn repeat_end(a: f32, b: f32, x: f32, size: f32) -> f32 {
    return mix(a, b, smoothstep(0.0, 1.0, sin((x / size) * PI * 2.0 - PI * 0.5) * 0.5 + 0.5));
}

fn pattern(st: vec2<f32>) -> f32 {
    // Progress through the field: a slow constant drift that every beat
    // kicks forward with an eased catch-up, so the beat reads as motion.
    // Progress through the field: a constant drift, kicked hard once a bar
    // with an ease-out that has fully settled back onto the drift by the
    // accent.z fraction of the bar.
    let bar = clock.y * 0.25;
    let bp = min(fract(bar) / accent.z, 1.0);
    let ease = 1.0 - pow(1.0 - bp, motion.z);
    let t = clock.x * motion.x + (floor(bar) + ease) * motion.y;
    // How fast the kick is moving right now; fattens the lines below.
    let rush = pow(1.0 - bp, max(motion.z - 1.0, 1.0));
    // The scroll also wanders through the noise volume; the kick speeds the
    // wander up too, bending the flow a new way each time.
    let z = 0.5 * sin(t * PI / 4.0) + 0.25 * sin(t * 3.0 * PI / 4.0);

    var uv = vec2<f32>(st.x, 1.0 - st.y) * 2.0 - 1.0;
    uv.x /= ASPECT;

    // Looking slowly up and down: the horizon rides above the top edge.
    let horizon = 1.45 + accent.y * sin(clock.x * motion.w);
    // Project onto a floor going into the distance toward the top, vanishing
    // diagonally at x = 0.35: depth along this row, lateral spread with it.
    // The gap floor keeps the projection finite if the sway ever drops the
    // horizon into frame.
    let depth = look.x / max(horizon - uv.y, 0.08);
    let u = (uv.x - 0.35) * depth * 0.9;
    // Flying away: the floor streams up toward the horizon. Wrapped by the
    // repeat period so every coordinate stays small.
    let repeat_size = 4.0;
    let w = depth - gmod(t * 2.0, repeat_size);

    // Blend noise at different frequencies, moving in different directions.
    var ab = repeat_start(w, repeat_size);
    var noise_a = tetra_noise(16.0 + vec3<f32>(vec2<f32>(ab.x, u) * 1.2, z)) * 0.5;
    var noise_b = tetra_noise(16.0 + vec3<f32>(vec2<f32>(ab.y, u) * 1.2, z)) * 0.5;
    var noise = repeat_end(noise_a, noise_b, w, repeat_size);

    ab = repeat_start(u, repeat_size / 2.0);
    noise_a = tetra_noise(vec3<f32>(vec2<f32>(depth, ab.x) * 0.5, z * 0.5)) * 2.0;
    noise_b = tetra_noise(vec3<f32>(vec2<f32>(depth, ab.y) * 0.5, z * 0.5)) * 2.0;
    noise *= repeat_end(noise_a, noise_b, u, repeat_size / 2.0);

    ab = repeat_start(w, repeat_size);
    noise_a = tetra_noise(9.0 + vec3<f32>(vec2<f32>(ab.x, u) * 0.05, z * 0.2)) * 5.0;
    noise_b = tetra_noise(9.0 + vec3<f32>(vec2<f32>(ab.y, u) * 0.05, z * 0.2)) * 5.0;
    noise *= repeat_end(noise_a, noise_b, w, repeat_size);

    noise *= 0.75;

    // A linear ramp along depth keeps the isolines mostly lateral, contour
    // lines crossing the direction of travel, with a slight tilt. The ramp
    // spans a whole number of lines per repeat so the scroll wrap is
    // invisible.
    let spacing = 1.0 / look.z;
    let ramp = 4.0 * spacing / (0.6 * repeat_size);
    noise = mix(noise, u * -0.02 + w * ramp, 0.6);

    // Even-weight antialiased isolines: sawtooth to triangle wave, scaled by
    // the noise's variation over a pixel.
    let steps = gmod(noise, spacing) / spacing;
    var lines = min(steps * 2.0, 1.0) - max(steps * 2.0 - 1.0, 0.0);
    // True gradient length, not fwidth: |dx|+|dy| is direction-dependent, so
    // lines fattened and stepped at steep angles. The floor keeps saddle
    // points, where the gradient dies as lines merge, from blowing up.
    let grad = length(vec2<f32>(dpdx(noise), dpdy(noise))) / spacing;
    lines /= max(grad, 0.05);
    lines /= 2.0;

    // Lines fatten with the kick's speed and relax as it coasts. Static
    // thickness near zero would push the whole frame negative, i.e. black.
    let wt = 0.75 + rush * accent.x;
    let thickness = look.w + rush * accent.x;

    // Thicker lines inside the triangle's fuzzy border.
    let d = s_tri(uv + vec2<f32>(0.0, 0.1), 0.1 + TRI_R / 3.0);
    var weight = smoothstep(0.0, 0.05, d);
    weight = mix(1.0 + 5.0 * wt, 1.2, weight);
    lines -= weight - 3.0 * (1.0 - thickness);

    // Soft clip: letting the unorm target clamp the value leaves hard edges
    // where lines peel off at shallow angles. A touch darker into the
    // distance sells the perspective.
    return smoothstep(0.0, 1.0, 1.0 - lines) / (1.0 + look.y * depth);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 4x rotated-grid supersample: the isoline distance approximation stays
    // noisy right where lines merge and split, and one sample per texel
    // shimmers there.
    let px = vec2<f32>(dpdx(in.uv.x), dpdy(in.uv.y));
    let v = 0.25
        * (pattern(in.uv + px * vec2<f32>(0.125, 0.375))
            + pattern(in.uv + px * vec2<f32>(-0.375, 0.125))
            + pattern(in.uv + px * vec2<f32>(-0.125, -0.375))
            + pattern(in.uv + px * vec2<f32>(0.375, -0.125)));
    return vec4<f32>(color0.rgb * v, 1.0);
}

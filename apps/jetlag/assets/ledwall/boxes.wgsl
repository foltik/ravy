// Box tunnel: an endless zoom down nested square outlines, each depth slice
// twisted a little further, alternating the palette's two colours and
// falling to black at the vanishing point. Zoom speed pulses on the beat
// upstream.
//
// clock: x = travel (speed-integrated upstream, in box periods).
// params: x = twist per depth unit, y = boxes per depth unit, z = line
//         thickness as a fraction of the gap, w = cutoff radius the deep
//         end fades out below, before it gets too dense to read.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clock: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> color1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;

const ASPECT: f32 = 1.5;

fn gmod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = (vec2<f32>(in.uv.x, 1.0 - in.uv.y) * 2.0 - 1.0) * vec2<f32>(ASPECT, 1.0);

    // Twist rides the tunnel depth, so the stack corkscrews away.
    let z = -log(max(length(uv), 1e-4));
    let a = params.x * z + 0.3 * clock.x;
    let ca = cos(a);
    let sa = sin(a);
    let q = mat2x2<f32>(vec2<f32>(ca, sa), vec2<f32>(-sa, ca)) * uv;

    // Chebyshev radius makes the rings square; log keeps them evenly spaced
    // down the zoom.
    let s = max(max(abs(q.x), abs(q.y)), 1e-4);
    let depth = -log(s) * params.y + clock.x;

    let cell = fract(depth);
    let d = min(cell, 1.0 - cell);
    let aa = length(vec2<f32>(dpdx(depth), dpdy(depth)));
    let line = 1.0 - smoothstep(params.z - aa, params.z + aa, d);

    // The vanishing point would be an infinitely dense white core.
    let fade = smoothstep(params.w, params.w + 0.15, s);
    let tint = mix(color0.rgb, color1.rgb, gmod(floor(depth), 2.0));

    return vec4<f32>(tint * line * fade, 1.0);
}

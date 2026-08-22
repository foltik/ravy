// The reality spiral tunnel (spiral.frag): spinning spokes swirled into a
// tunnel, alternating the palette's two colours on black. Spokes are flat
// fills with hard antialiased boundaries and black gaps between them: flat
// interiors and abrupt edges are what the fx pass's sobel wants, and since
// spokes only ever border black, its lines land in each spoke's own colour.
//
// clock: x = spin phase (speed-integrated upstream), z = amount right now,
//        with the beat pulse already added.
// params: x = swirl, y = spokes, z = cutoff, w = fill threshold (higher is
//         thinner spokes and wider gaps).

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clock: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> color1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;

const TAU: f32 = 6.28318530718;

fn gmod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * 2.0 - 1.0;
    let st = vec2<f32>(length(uv), atan2(uv.y, uv.x));

    let arg = 4.0 * params.x / max(st.x, 1e-4) + params.y * st.y + clock.x;
    let c = 0.5 + 0.5 * sin(arg);
    let aa = length(vec2<f32>(dpdx(c), dpdy(c))) + 1e-4;
    let v = smoothstep(params.w - aa, params.w + aa, c)
        * smoothstep(1.0 - params.z, 1.7 - params.z, st.x);

    // Spokes alternate the palette, split at each lobe's dark trough. The
    // count is forced even upstream: an odd ring of two colours cannot
    // close, and the flip would land on the atan2 cut as a seam.
    let tint = mix(color0.rgb, color1.rgb, gmod(floor(arg / TAU + 0.25), 2.0));
    return vec4<f32>(tint * v * clock.z, 1.0);
}

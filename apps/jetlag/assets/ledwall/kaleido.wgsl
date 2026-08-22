// The kaleido fractal fold, flown through like the isolines floor: the field
// is projected onto a plane receding diagonally toward a horizon above the
// frame and scrolls endlessly forward, the beat surging the flight. Each
// fold leaves a thin bright line, odd and even folds alternating the
// palette's two colours on black; the sum is normalised by the fold count
// and squared, so overlapping cores stay bright while the haze between them
// drops to black, and it all falls off into the distance.
//
// clock: x = travel (speed-integrated upstream, beat surge included),
//        z = fold count.
// params: x = zoom, y = breathe, z = line sharpness, w = distance fade.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clock: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> color1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;

const ASPECT: f32 = 1.5;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = clock.x;
    var uv = (vec2<f32>(in.uv.x, 1.0 - in.uv.y) * 2.0 - 1.0) * vec2<f32>(ASPECT, 1.0);

    // Lens breathing around the centre.
    uv += uv * sin(dot(uv, uv) * 20.0 - t) * params.y;

    // The isolines projection: the horizon rides above the top edge, the
    // plane vanishes diagonally off-centre, and travel streams it past.
    let depth = 2.0 / max(1.45 - uv.y, 0.08);
    var p = vec2<f32>((uv.x - 0.35) * depth * 0.9, depth + t) * params.x;

    let n = clamp(clock.z, 1.0, 8.0);
    var v0 = 0.0;
    var v1 = 0.0;
    for (var ii = 0; ii < 8; ii++) {
        if (f32(ii) >= n) {
            break;
        }
        let i = 0.5 + f32(ii);
        let cs = cos(0.02 * t * i * i + 0.78 * vec4<f32>(1.0, 7.0, 3.0, 1.0));
        p = abs(2.0 * fract(p - 0.5) - 1.0)
            * mat2x2<f32>(vec2<f32>(cs.x, cs.y), vec2<f32>(cs.z, cs.w));
        let a = exp(-abs(p.y) * params.z);
        if (ii % 2 == 0) {
            v0 += a;
        } else {
            v1 += a;
        }
    }

    var rgb = (color0.rgb * v0 + color1.rgb * v1) / n;
    rgb = rgb * rgb * 1.6;
    return vec4<f32>(rgb / (1.0 + params.w * depth), 1.0);
}

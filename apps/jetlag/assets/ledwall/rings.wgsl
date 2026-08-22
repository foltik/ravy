// Sonar rings: every beat launches a ring that leaps off the centre and
// coasts outward, dying over a few beats. Each ring has its own character —
// a wobble with its own spoke count and phase, jerked around a step further
// on every beat — rings alternate the palette's two colours, every bar's
// ring lands white and hard, the kick flashes the core and pumps the whole
// field. Thin bright lines on black, built for LEDs.
//
// clock: x = beats (unwrapped), y = launch snap (ease-out exponent),
//        z = wobble amount, w = field pump right now (computed upstream).
// params: x = expansion reach (units, the frame is 1 tall), y = ring width,
//         z = life (beats), w = core glow.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clock: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> color1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;

const ASPECT: f32 = 1.5;
const TAU: f32 = 6.28318530718;

fn hash(n: f32) -> f32 {
    return fract(sin(n * 127.1) * 43758.5453);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = (vec2<f32>(in.uv.x, 1.0 - in.uv.y) * 2.0 - 1.0) * vec2<f32>(ASPECT, 1.0);
    // The kick blows the whole field up for a moment.
    let r = length(uv) / (1.0 + clock.w);
    let theta = atan2(uv.y, uv.x);
    let beats = clock.x;
    // The beat kicks every ring's lobes a step around, coasting like the
    // launch: still between beats, never smoothly drifting.
    let jerk = floor(beats) + (1.0 - pow(1.0 - fract(beats), clock.y));

    var v = vec3<f32>(0.0);
    for (var k = 0; k < 8; k++) {
        let b = floor(beats) - f32(k);
        if (b < 0.0) {
            continue;
        }
        let age = beats - b;
        if (age > params.z) {
            continue;
        }
        let u = age / params.z;
        // The jerk: the ring bolts off the centre and coasts the rest out.
        let rad = params.x * (1.0 - pow(1.0 - u, clock.y));

        // This ring's own shape: spoke count and phase, the jerk direction
        // alternating ring to ring so neighbours counter-rotate.
        let h = hash(b + 1.0);
        let spokes = 3.0 + floor(h * 4.0);
        let dir = (f32(i32(b) % 2) * 2.0 - 1.0) * (0.8 + 0.8 * h);
        let wobble = clock.z * sin(spokes * theta + h * TAU + dir * jerk);

        let d = r - rad * (1.0 + wobble);
        // Born fat and bright, thinning as it fades.
        let w = params.y * (1.5 - u);
        var ring = exp(-d * d / (w * w)) * (1.0 - u) * (1.0 - u);

        var tint = mix(color0.rgb, color1.rgb, f32(i32(b) % 2));
        // The bar hits white and harder.
        if (i32(b) % 4 == 0) {
            tint = vec3<f32>(1.0);
            ring *= 1.5;
        }
        v += ring * tint;
    }

    // The kick itself, gone before the ring is out.
    v += color0.rgb * params.w * exp(-r * r * 40.0) * pow(1.0 - fract(beats), 2.0);

    return vec4<f32>(v, 1.0);
}

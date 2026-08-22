// The resolve wormhole (wormhole.frag, via shadertoy WtdcWf): a camera
// tumbling through a torus, its surface streaming stripes of traffic. Each
// stripe picks one of the palette's two colours by its id.
//
// clock: x = flight phase (speed-integrated upstream, beat pulse included).
// params: x = tube thickness (warp).

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clock: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> color1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;

const ASPECT: f32 = 1.5;

fn gmod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn hash11(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453123);
}

fn torus(pos: vec3<f32>, t: vec2<f32>, uv: ptr<function, vec2<f32>>) -> f32 {
    let q = vec2<f32>(length(pos.xz) - t.x, pos.y);
    *uv = vec2<f32>(atan2(pos.x, pos.z), atan2(q.x, q.y));
    return length(q) - t.y;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var p = 2.0 * vec2<f32>(in.uv.x, 1.0 - in.uv.y) - 1.0;
    p *= vec2<f32>(ASPECT, 1.0);
    let tt = clock.x;

    let an = tt;
    let ta = 0.5 * vec3<f32>(sin(an), 0.0, -cos(an));
    let ro = vec3<f32>(0.0, sin(an), 0.0);
    let ww = normalize(ta - ro);
    let uu = normalize(cross(ww, vec3<f32>(0.0, 1.0, 0.0)));
    let vv = normalize(cross(uu, ww));
    let rd = normalize(p.x * uu + p.y * vv + 0.4 * ww);

    var t = 0.0;
    var tuv = vec2<f32>(0.0);
    for (var i = 0; i < 200; i++) {
        let h = torus(ro + t * rd, vec2<f32>(1.0, params.x), &tuv);
        if (h < 0.0005) {
            break;
        }
        t += h;
    }

    var col = vec3<f32>(0.0);
    if (t < 20.0) {
        let ts = 0.5 * tt;
        let ttuv = (tuv / 3.1415) / 2.0 + 0.5;

        let tpa = vec2<f32>(8.0, 24.0);
        let pa = sin(tpa.x * tuv.y + tpa.y * tuv.x);

        let pa_id = hash11(floor(gmod(tpa.x * ttuv.y + tpa.y * ttuv.x, tpa.x)) / (tpa.x - 1.0));
        let grad = 2.0 * gmod(tpa.x * ttuv.x + tpa.y * ttuv.y, tpa.x) / tpa.x - 1.0;
        let cars = smoothstep(
            -0.002,
            0.002,
            sin(grad + sign(hash11(pa_id) - 0.5) * 6.0 * ts * pa_id) - hash11(pa_id),
        );
        let distb = smoothstep(
            -0.05,
            0.05,
            sin(tpa.x * tuv.x + tpa.y * tuv.y + sign(hash11(pa_id) - 0.5) * 20.0 * ts)
                + 0.9 + 0.1 * sin(40.0 * ttuv.y),
        );

        let tint = mix(color0.rgb, color1.rgb, step(0.5, hash11(pa_id * 7.0)));
        col = cars * distb * smoothstep(-0.5, 0.05, pa) * tint;
    }

    return vec4<f32>(col, 1.0);
}

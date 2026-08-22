// The milstrike fx chain (edge -> shake/mega -> glitch -> vhs -> pause ->
// invert), collapsed into one pass: each stage's uv warp composes onto the
// samples of the stage before it, colour ops ride along in chain order.
//
// clock: x = seconds since start, y = beats (unwrapped), z = flash, w = red.
// warp: x = glitch, y = vhs, z = shake, w = mega.
// grade: x = pause, y = edge, z = invert, w = brightness.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clock: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> warp: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> grade: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var feed: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var feed_sampler: sampler;

// GLSL-style mod: result takes the sign of the divisor.
fn gmod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn gmod2(x: vec2<f32>, y: f32) -> vec2<f32> {
    return x - y * floor(x / y);
}

fn gmod3(x: vec3<f32>, y: f32) -> vec3<f32> {
    return x - y * floor(x / y);
}

fn inside(x: f32, lo: f32, hi: f32) -> f32 {
    return step(lo, x) - step(hi, x);
}

fn rand(p: vec2<f32>) -> f32 {
    let dt = dot(p, vec2<f32>(12.9898, 78.233));
    return fract(sin(gmod(dt, 3.14)) * 43758.5453);
}

fn rrand(p: vec2<f32>, lo: f32, hi: f32) -> f32 {
    return lo + rand(p) * (hi - lo);
}

fn permute3(x: vec3<f32>) -> vec3<f32> {
    return gmod3((x * 34.0 + 1.0) * x, 289.0);
}

fn snoise(p: vec2<f32>) -> f32 {
    let c = vec4<f32>(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
    var i = floor(p + dot(p, c.yy));
    let x0 = p - i + dot(i, c.xx);
    var i1 = vec2<f32>(0.0, 1.0);
    if (x0.x > x0.y) {
        i1 = vec2<f32>(1.0, 0.0);
    }
    var x12 = x0.xyxy + c.xxzz;
    x12 = vec4<f32>(x12.xy - i1, x12.zw);
    i = gmod2(i, 289.0);
    let pm = permute3(permute3(i.y + vec3<f32>(0.0, i1.y, 1.0)) + i.x + vec3<f32>(0.0, i1.x, 1.0));
    var m = max(
        0.5 - vec3<f32>(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)),
        vec3<f32>(0.0),
    );
    m = m * m;
    m = m * m;
    let x = 2.0 * fract(pm * c.www) - 1.0;
    let h = abs(x) - 0.5;
    let a0 = x - floor(x + 0.5);
    m *= 1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h);
    let g = vec3<f32>(a0.x * x0.x + h.x * x0.y, a0.yz * x12.xz + h.yz * x12.yw);
    return 130.0 * dot(m, g);
}

// Sampled at an explicit level: half the calls sit in non-uniform branches.
fn fetch(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(feed, feed_sampler, uv, 0.0).rgb;
}

fn sobel(uv: vec2<f32>) -> vec3<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(feed));
    let tl = fetch(uv + vec2<f32>(-1.0, 1.0) * px);
    let tm = fetch(uv + vec2<f32>(0.0, 1.0) * px);
    let tr = fetch(uv + vec2<f32>(1.0, 1.0) * px);
    let ml = fetch(uv + vec2<f32>(-1.0, 0.0) * px);
    let mr = fetch(uv + vec2<f32>(1.0, 0.0) * px);
    let bl = fetch(uv + vec2<f32>(-1.0, -1.0) * px);
    let bm = fetch(uv + vec2<f32>(0.0, -1.0) * px);
    let br = fetch(uv + vec2<f32>(1.0, -1.0) * px);
    let gx = -tl + tr - 2.0 * ml + 2.0 * mr - bl + br;
    let gy = tl + 2.0 * tm + tr - bl - 2.0 * bm - br;
    return vec3<f32>(
        length(vec2<f32>(gx.r, gy.r)),
        length(vec2<f32>(gx.g, gy.g)),
        length(vec2<f32>(gx.b, gy.b)),
    );
}

// Feed alpha is a per-pixel edge mask written by the pattern, so a preset
// can crank the sobel on one object without hollowing out the rest.
fn edged(uv: vec2<f32>) -> vec3<f32> {
    let s = textureSampleLevel(feed, feed_sampler, uv, 0.0);
    let amt = min(grade.y * s.a + warp.w, 1.0);
    if (amt > 0.0) {
        return mix(s.rgb, sobel(uv), amt);
    }
    return s.rgb;
}

fn cloth_uv(t: f32, uv: vec2<f32>) -> vec2<f32> {
    let amt = warp.w;
    var u = uv;
    u.x += (rand(vec2<f32>(t, uv.x)) - 0.5) * 0.030 * amt;
    u.y += (rand(vec2<f32>(t)) - 0.5) * 0.030 * amt;
    u += vec2<f32>(
        (rand(vec2<f32>(t)) - 0.5) * 0.10,
        (rand(vec2<f32>(t + 10.0)) - 0.5) * 0.10,
    ) * amt;
    return u;
}

// The shake pass's main: mega cloth blend, red wash, frame jitter, flash.
fn scene(uv: vec2<f32>) -> vec3<f32> {
    let t = clock.x;
    var u = uv;
    if (warp.z > 0.0) {
        u += vec2<f32>(
            (rand(vec2<f32>(t)) - 0.5) * 0.050,
            (rand(vec2<f32>(t + 100.0)) - 0.5) * 0.050,
        ) * warp.z;
    }
    var c = edged(u);
    if (warp.w > 0.0) {
        c = 0.2 * c + edged(cloth_uv(t, u));
    }
    if (clock.w > 0.0) {
        c += vec3<f32>(clock.w, 0.0, 0.0);
    }
    if (clock.z > 0.0) {
        c += vec3<f32>(clock.z);
    }
    return c;
}

fn glitch_blocks(uv: vec2<f32>, tt: f32, amt: f32) -> vec3<f32> {
    var c = scene(uv);
    let t = floor(tt * 10000.0 * 50.0);
    let r = rand(vec2<f32>(t, 0.0));
    let f_skew = 0.08 * amt;
    let f_color = 0.001 * amt;

    for (var i = 0.0; i < 20.0 * f_skew; i += 1.0) {
        let y = rand(vec2<f32>(t, i));
        let h = rand(vec2<f32>(t, i + 1.0)) * 0.25;
        if (inside(uv.y, y, fract(y + h)) == 1.0) {
            let ofs = rrand(vec2<f32>(t, i + 2.0), -f_skew, f_skew);
            c = scene(vec2<f32>(uv.x + ofs, uv.y));
        }
    }

    let cofs = vec2<f32>(
        rrand(vec2<f32>(t, 1.0), -f_color, f_color),
        rrand(vec2<f32>(t, 2.0), -f_color, f_color),
    );
    if (r <= 0.33) {
        c.r = scene(uv + cofs).r;
    } else if (r <= 0.66) {
        c.g = scene(uv + cofs).g;
    } else {
        c.b = scene(uv + cofs).b;
    }
    return c;
}

fn glitched(uv: vec2<f32>) -> vec3<f32> {
    let amt = min(warp.x + warp.w, 1.0);
    if (amt > 0.0) {
        return glitch_blocks(uv, clock.x + amt, amt);
    }
    return scene(uv);
}

fn taped(uv: vec2<f32>) -> vec3<f32> {
    let amt = warp.y;
    if (amt <= 0.0) {
        return glitched(uv);
    }
    let t = clock.x * 100.0;
    let a_wav = max(0.0, snoise(vec2<f32>(t, uv.y * 0.3)) - 0.3) * 0.42857 * 2.0 * amt;
    let b_wav = (snoise(vec2<f32>(t * 10.0, uv.y * 2.4)) - 0.5) * 0.15011 * 2.0 * amt;
    let n = a_wav + b_wav;

    let x = uv.x - n * n * 0.25;
    var c = glitched(vec2<f32>(x, uv.y));
    c = mix(c, vec3<f32>(rand(vec2<f32>(uv.y * t))), n * 0.02);
    if (floor(gmod(uv.y * 0.25, 2.0)) == 0.0) {
        c *= 1.0 - 0.15 * n;
    }
    c.g = mix(c.r, glitched(vec2<f32>(x + n * 0.05, uv.y)).g, 1.0 - 0.5 * amt);
    c.b = mix(c.r, glitched(vec2<f32>(x - n * 0.05, uv.y)).b, 1.0 - 0.5 * amt);
    return c;
}

fn paused(uv: vec2<f32>) -> vec3<f32> {
    let amt = grade.x;
    if (amt <= 0.0) {
        return taped(uv);
    }
    let t = clock.x;
    var u = uv;
    u.x += (rand(vec2<f32>(t, uv.y)) - 0.5) * 0.015 * amt;
    u.y += (rand(vec2<f32>(t)) - 0.5) * 0.030 * amt;
    let img = taped(u);

    let band = img
        * vec3<f32>(
            rand(vec2<f32>(t, uv.y)),
            rand(vec2<f32>(t, uv.y + 1.0)),
            rand(vec2<f32>(t, uv.y + 2.0)),
        )
        * 0.8
        * amt;

    let noise = rand(vec2<f32>(floor(u.y * 80.0), floor(u.x * 50.0)) + vec2<f32>(t, 0.0));
    if (noise <= -18.5 + 30.0 * u.y * amt && noise >= -3.5 + 5.0 * u.y) {
        return vec3<f32>(0.8) + band;
    }
    return img + band;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var c = paused(in.uv);
    c = mix(c, vec3<f32>(1.0) - c, grade.z);
    return vec4<f32>(c * grade.w, 1.0);
}

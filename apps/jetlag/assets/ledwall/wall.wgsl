// The LED panel the sim wall wears. Rebuilds the physical face from the
// wall's frame: one round diffused lens per frame texel, recessed in a square
// well of the molded black mask, with the RGB die triangle inside the lens.
// Everything is antialiased against the fragment footprint so the structure
// melts into the plain frame at distance instead of shimmering into moire,
// and energy-conserved so the wall reads equally bright either way. The lens
// epoxy is pale when unlit, so the base colour carries the mask too.
//
// geometry: xy = emitter counts, one per frame texel, z = well width as a
//           fraction of the pitch, w = lens diameter as a fraction of it.
// tuning:   x = emissive gain, y = off-axis dimming exponent, z = mask bleed.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}

struct LedWall {
    geometry: vec4<f32>,
    tuning: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> led: LedWall;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var frame: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var frame_sampler: sampler;

const PI: f32 = 3.14159265;

// Coverage of a rounded square of half-size h, antialiased over the footprint.
fn rsquare(f: vec2<f32>, h: f32, aa: f32) -> f32 {
    let r = 0.25 * h;
    let q = abs(f) - vec2<f32>(h - r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    return 1.0 - smoothstep(-aa, aa, d);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let grid = led.geometry.xy;
    let well = led.geometry.z;
    let lens_d = led.geometry.w;

    // Position in LED units; each cell is one frame texel, centres at half
    // integers, so butted modules keep an even half-gap border all round.
    let p = in.uv * grid;
    let f = fract(p) - 0.5;
    let vuv = (floor(p) + 0.5) / grid;

    // 2x2 box once the frame minifies, standing in for the mips it lacks.
    let dims = vec2<f32>(textureDimensions(frame));
    let spread = max(fwidth(in.uv) * dims - 1.0, vec2<f32>(0.0)) * 0.375 / dims;
    let flip = vec2<f32>(spread.x, -spread.y);
    let video = 0.25
        * (textureSampleLevel(frame, frame_sampler, vuv + spread, 0.0).rgb
            + textureSampleLevel(frame, frame_sampler, vuv - spread, 0.0).rgb
            + textureSampleLevel(frame, frame_sampler, vuv + flip, 0.0).rgb
            + textureSampleLevel(frame, frame_sampler, vuv - flip, 0.0).rgb);

    // LEDs per fragment: fade the structure out before it can moire.
    let w = max(fwidth(p.x), fwidth(p.y));
    let fade = smoothstep(0.15, 0.5, w);
    let aa = max(w, 0.02);

    // The lens: a diffused dome, the only part that emits.
    let lr = 0.5 * lens_d;
    let lens = 1.0 - smoothstep(lr - aa, lr + aa, length(f));
    let lens_area = PI * lr * lr;

    // The die triangle inside the lens: red at the bottom and a touch larger,
    // green and blue above. The glow leans toward each die without changing
    // the lens's total output; only resolvable once a pixel spans many
    // fragments.
    let q = f / lr;
    let dr = q - vec2<f32>(0.0, 0.42);
    let dg = q - vec2<f32>(-0.36, -0.21);
    let db = q - vec2<f32>(0.36, -0.21);
    let dies = vec3<f32>(
        exp(-dot(dr, dr) / 0.34),
        exp(-dot(dg, dg) / 0.22),
        exp(-dot(db, db) / 0.22),
    );
    let detail = 1.0 - smoothstep(0.02, 0.1, w);
    let chroma = mix(vec3<f32>(1.0), 3.0 * dies / (dies.x + dies.y + dies.z + 1e-3), detail);

    // A little light caught by the mask around lit lenses; the far branch
    // carries its cell average.
    let bleed = led.tuning.z * exp(-12.0 * dot(f, f)) * (1.0 - lens);
    let structure = mix(
        lens * chroma / lens_area + vec3<f32>(bleed),
        vec3<f32>(1.0 + 0.25 * led.tuning.z),
        fade,
    );

    // Panels dim off axis and the recessed wells swallow grazing views.
    let nv = normalize(view.world_position - in.world_position.xyz);
    let cosv = saturate(dot(normalize(in.world_normal), nv));
    let angle = pow(max(cosv, 1e-3), led.tuning.y);

    let e = pbr_input.material.emissive;
    pbr_input.material.emissive =
        vec4<f32>(video * structure * led.tuning.x * angle, e.a);

    // What the house lights see: raised rim between cells, darker well, pale
    // epoxy lens; glossier on the lens. Melts to the averages far off.
    let wc = rsquare(f, 0.5 * well, aa);
    let near_base = mix(mix(0.10, 0.035, wc), 0.55, lens);
    let avg_base = mix(mix(0.10, 0.035, well * well), 0.55, lens_area);
    let base = mix(near_base, avg_base, fade);
    pbr_input.material.base_color = vec4<f32>(vec3<f32>(base), 1.0);
    pbr_input.material.perceptual_roughness =
        mix(mix(0.85, 0.45, lens), 0.75, fade);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

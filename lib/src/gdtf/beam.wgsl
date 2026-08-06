// Volumetric beam. Integrates the light the haze scatters back at the camera
// along the view ray, so the shaft carries real luminance: inverse square from
// the apex, the fixture's own falloff across the cone, and the gobo extruded
// down it.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::prepass_utils
#import bevy_pbr::view_transformations::{frag_coord_to_ndc, position_ndc_to_world}
#import bevy_pbr::mesh_view_bindings as view_bindings
#import bevy_pbr::mesh_view_types::{
    POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    POINT_LIGHT_FLAGS_SPOT_LIGHT_BIT,
    POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE,
}
#import bevy_pbr::shadow_sampling::sample_shadow_map_hardware
#import bevy_render::maths::orthonormalize

struct Beam {
    // rgb: linear sRGB at a luminance of 1. a: peak intensity in candela.
    color: vec4<f32>,
    // xyz: cone apex in world space. w: cosine of the inner angle.
    apex: vec4<f32>,
    // xyz: unit vector down the beam. w: cosine of the outer angle.
    axis: vec4<f32>,
    // xyz: the gobo's u axis. w: axial distance from the apex to the lens.
    right: vec4<f32>,
    // xyz: the gobo's v axis. w: axial distance from the apex to the far end.
    up: vec4<f32>,
    // xy: cos/sin of the gobo's rotation. z: 1 when a gobo is on the wheel.
    // w: scattering coefficient of the air, per metre.
    gobo: vec4<f32>,
}

const PI: f32 = 3.14159265;

// Haze droplets and playa dust are both far larger than a wavelength, so they
// scatter hard forward: looking into a beam is dazzling and looking across it
// is not. Henyey-Greenstein stands in for the Mie lobe, at less than the ~0.8 a
// real droplet measures because single scattering has no further bounces to
// smear the peak out with.
const ASYMMETRY: f32 = 0.6;

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> beam: Beam;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var gobo_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var gobo_sampler: sampler;

// Samples sit a fixed fraction of the way from the apex apart: the integrand
// falls as 1/r^2, so a step that grows with distance resolves the near field
// and the far field the same, and a shaft seen end-on costs what it needs
// rather than what the longest one would.
const RELATIVE_STEP: f32 = 0.05;
const MIN_STEPS: i32 = 16;
const MAX_STEPS: i32 = 96;

fn tan_outer() -> f32 {
    let c = max(beam.axis.w, 1e-4);
    return sqrt(max(1.0 - c * c, 0.0)) / c;
}

// Illuminance a world point receives from the source, per candela of peak
// intensity. Zero outside the cone.
fn lit(p: vec3<f32>, step: f32) -> f32 {
    let offset = p - beam.apex.xyz;
    let depth = dot(offset, beam.axis.xyz);
    if depth < beam.right.w || depth > beam.up.w {
        return 0.0;
    }

    let square = dot(offset, offset);
    // The same ramp bevy squares for its spot falloff, so the volume and the
    // surfaces the beam lands on agree.
    let cosine = depth * inverseSqrt(square);
    let ramp = saturate((cosine - beam.axis.w) / max(beam.apex.w - beam.axis.w, 1e-4));
    var level = ramp * ramp / square;
    if level <= 0.0 || beam.gobo.z < 0.5 {
        return level;
    }

    // Dividing by the cone's radius here is what extrudes the gobo: every
    // cross-section normalises to the same disc, so the pattern opens with the beam.
    let radius = max(depth * tan_outer(), 1e-6);
    let perp = offset - beam.axis.xyz * depth;
    let flat = vec2(dot(perp, beam.right.xyz), dot(perp, beam.up.xyz)) / radius;
    let spun = vec2(
        flat.x * beam.gobo.x - flat.y * beam.gobo.y,
        flat.x * beam.gobo.y + flat.y * beam.gobo.x,
    );
    // Each sample stands for `step` metres of ray, which covers this fraction of
    // the gobo's width. Filtering over that instead of point-sampling is what
    // keeps the pattern clean without paying for more samples.
    let texels = (step / (2.0 * radius)) * f32(textureDimensions(gobo_texture, 0).x);
    let lod = max(log2(max(texels, 1.0)), 0.0);
    return level * textureSampleLevel(gobo_texture, gobo_sampler, spun * 0.5 + 0.5, lod).r;
}

// Range of the ray that lies inside the cone, as distances along `dir`.
// Returns an empty range when the ray misses.
fn cone_span(origin: vec3<f32>, dir: vec3<f32>) -> vec2<f32> {
    let tan = tan_outer();
    let spread = 1.0 + tan * tan;
    let rel = origin - beam.apex.xyz;
    let along = dot(rel, beam.axis.xyz);
    let slope = dot(dir, beam.axis.xyz);

    var near = 0.0;
    var far = 1e9;

    // Quadratic in t for |P-apex|^2 = (1 + tan^2) * axial^2, the cone surface.
    let a = 1.0 - spread * slope * slope;
    let b = 2.0 * (dot(rel, dir) - spread * along * slope);
    let c = dot(rel, rel) - spread * along * along;
    if a > 1e-6 {
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return vec2(0.0, -1.0);
        }
        let root = sqrt(disc);
        near = (-b - root) / (2.0 * a);
        far = (-b + root) / (2.0 * a);
    }
    // Near-axial rays stay inside for a long stretch; the slab below bounds them,
    // and the per-sample radial test rejects whatever slips through.

    // Clip to the slab between the lens and the end of the throw.
    if abs(slope) > 1e-6 {
        let lens = (beam.right.w - along) / slope;
        let end = (beam.up.w - along) / slope;
        near = max(near, min(lens, end));
        far = min(far, max(lens, end));
    } else if along < beam.right.w || along > beam.up.w {
        return vec2(0.0, -1.0);
    }

    return vec2(max(near, 0.0), far);
}

#if AVAILABLE_STORAGE_BUFFER_BINDINGS >= 3
fn spot_direction(light_id: u32) -> vec3<f32> {
    let light = &view_bindings::clustered_lights.data[light_id];
    var dir = vec3((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
    dir.y = sqrt(max(0.0, 1.0 - dir.x * dir.x - dir.z * dir.z));
    if ((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
        dir.y = -dir.y;
    }
    return dir;
}

// The spot light this cone belongs to, or -1 if none of them casts a shadow.
// The material holds no handle on the light, so it is matched on where it sits
// and where it points; the cells of an array all land on the one that casts.
fn shadow_caster() -> i32 {
    let want = beam.apex.xyz + beam.axis.xyz * beam.right.w;
    let mask = POINT_LIGHT_FLAGS_SPOT_LIGHT_BIT | POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT;
    var best = -1;
    var nearest = 0.5;
    let count = arrayLength(&view_bindings::clustered_lights.data);
    for (var i = 0u; i < count; i += 1u) {
        let light = &view_bindings::clustered_lights.data[i];
        if ((*light).flags & mask) != mask {
            continue;
        }
        let gap = distance((*light).position_radius.xyz, want);
        if gap < nearest && dot(spot_direction(i), beam.axis.xyz) > 0.999 {
            nearest = gap;
            best = i32(i);
        }
    }
    return best;
}

// How much of the light reaches a point in the volume, from the light's own
// shadow map. One hardware tap: a march samples this a hundred times a pixel,
// and there is no surface here to carry a normal bias.
fn reaches(light_id: i32, p: vec3<f32>) -> f32 {
    let light = &view_bindings::clustered_lights.data[u32(light_id)];
    let to_light = (*light).position_radius.xyz - p;
    let offset = -to_light + (*light).shadow_depth_bias * normalize(to_light);
    let projected = offset * orthonormalize(-spot_direction(u32(light_id)));
    if projected.z >= 0.0 {
        return 1.0;
    }
    let uv = projected.xy / ((*light).spot_light_tan_angle * -projected.z) * vec2(0.5, -0.5)
        + vec2(0.5, 0.5);
    let depth = (*light).shadow_map_near_z / -projected.z;
    return sample_shadow_map_hardware(uv, depth, light_id + view_bindings::lights.spot_light_shadowmap_offset);
}
#endif

// Breaks up the banding that a fixed step count would otherwise leave.
fn jitter(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

// Fraction of what the haze intercepts that leaves in the view direction, per
// steradian. `cosine` is taken between the photon's heading and that direction,
// so 1 is straight on through and -1 is straight back at the fixture.
fn phase(cosine: f32) -> f32 {
    let g = ASYMMETRY;
    let d = 1.0 + g * g - 2.0 * g * cosine;
    return (1.0 - g * g) / (4.0 * PI * d * sqrt(max(d, 1e-4)));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let sigma = beam.gobo.w;
    if beam.color.a * sigma <= 0.0 {
        return vec4(0.0);
    }

    let origin = view.world_position;
    let dir = normalize(in.world_position.xyz - origin);

    var span = cone_span(origin, dir);

#ifdef DEPTH_PREPASS
    // Stop at whatever the beam lands on. Past the first surface the shaft is
    // behind something, so marching on both leaks light through walls and costs
    // the most exactly where the cone fills the screen.
    let depth = prepass_utils::prepass_depth(in.position, 0u);
    if depth > 0.0 {
        let hit = position_ndc_to_world(vec3(frag_coord_to_ndc(in.position).xy, depth));
        span.y = min(span.y, dot(hit - origin, dir));
    }
#endif

    if span.y <= span.x {
        return vec4(0.0);
    }

    // Sampling only the part of the ray inside the cone keeps the step size tied
    // to the beam, so thin near-field slices resolve as well as the far end.
    let entry = length(origin + dir * span.x - beam.apex.xyz);
    let steps = clamp(
        i32(ceil((span.y - span.x) / max(entry * RELATIVE_STEP, 1e-3))),
        MIN_STEPS,
        MAX_STEPS,
    );
    let step = (span.y - span.x) / f32(steps);
    let start = span.x + step * jitter(in.position.xy);

    // Anything the beam is blocked by shadows the haze behind it too, or the
    // shaft carries on out the far side of whatever it lands on.
    var caster = -1;
#if AVAILABLE_STORAGE_BUFFER_BINDINGS >= 3
    caster = shadow_caster();
#endif

    var total = 0.0;
    // What survives the haze between the camera and where the march starts.
    var transmittance = exp(-sigma * start);
    for (var i = 0; i < steps; i += 1) {
        let p = origin + dir * (start + step * f32(i));
        let offset = p - beam.apex.xyz;
        let reach = length(offset);
        let cosine = dot(offset / max(reach, 1e-6), -dir);
        var level = lit(p, step);
#if AVAILABLE_STORAGE_BUFFER_BINDINGS >= 3
        if caster >= 0 && level > 0.0 {
            level *= reaches(caster, p);
        }
#endif
        // Both legs are attenuated: the source's throw down to this sample, and
        // this sample's scattering back out to the camera.
        total += level * phase(cosine) * exp(-sigma * reach) * transmittance;
        transmittance *= exp(-sigma * step);
    }

    // Luminance in cd/m^2, which is what the rest of the pipeline is in. Alpha
    // stays at zero: the blend is premultiplied, and haze adds without hiding
    // what is behind it.
    let luminance = beam.color.a * sigma * total * step;
    return vec4(beam.color.rgb * luminance, 0.0);
}

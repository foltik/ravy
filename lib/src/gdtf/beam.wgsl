// Volumetric beams. One pass for the whole rig: a pixel marches only the cones
// its screen tile holds, integrating the light the haze scatters back along the
// view ray. A shaft carries real luminance: inverse square from the apex, the
// fixture's own falloff across the cone, and the gobo extruded down it.
//
// Occlusion is a ray query per sample against the scene's acceleration
// structure, so the haze behind a solid is dark and a shaft stops at what it
// lands on instead of carrying on out the far side.

// Does not carry across an import, so every shader tracing a ray repeats it.
enable wgpu_ray_query;

#import bevy_render::view::View
#import bevy_solari::scene_bindings::{trace_ray, RAY_T_MIN}

// A wheel is only ever on a slot between one move and the next, so the cone
// carries two of everything a wheel picks and the edge dividing them.
struct Beam {
    // rgb: linear sRGB at a luminance of 1. a: peak intensity in candela.
    color: vec4<f32>,
    // The same, for the far side of the split.
    color2: vec4<f32>,
    // xyz: cone apex in world space. w: cosine of the inner angle.
    apex: vec4<f32>,
    // xyz: unit vector down the beam. w: cosine of the outer angle.
    axis: vec4<f32>,
    // xyz: the gobo's u axis. w: axial distance from the apex to the lens.
    right: vec4<f32>,
    // xyz: the gobo's v axis. w: axial distance from the apex to the far end.
    up: vec4<f32>,
    // xy: cos/sin of the gobo's rotation. z: layer in the gobo array, 0 for an
    // open gate.
    gobo: vec4<f32>,
    // xy: unit axis the wheel's slots travel along, in the gobo's frame. z: the
    // edge between them across the aperture in radii, +1 for all of the near
    // side. w: the far side's layer in the gobo array.
    split: vec4<f32>,
}

struct Volume {
    // xy: tiles across and down. z: pixels on a tile's side. w: beams a tile holds.
    grid: vec4<u32>,
    // x: scattering coefficient of the air, per metre.
    air: vec4<f32>,
}

// Group 0 is the raytracing scene, which declares itself there. Its bindings are
// visible to compute shaders only, which is why the march is one.
@group(1) @binding(0) var<uniform> view: View;
@group(1) @binding(1) var<uniform> volume: Volume;
@group(1) @binding(2) var<storage, read> beams: array<Beam>;
@group(1) @binding(3) var<storage, read> tiles: array<u32>;
@group(1) @binding(4) var gobos: texture_2d_array<f32>;
@group(1) @binding(5) var gobo_sampler: sampler;
#ifdef MULTISAMPLED
@group(1) @binding(6) var depth_texture: texture_depth_multisampled_2d;
#else
@group(1) @binding(6) var depth_texture: texture_depth_2d;
#endif
@group(1) @binding(7) var output: texture_storage_2d<rgba16float, write>;

const PI: f32 = 3.14159265;

// Haze droplets and playa dust are both far larger than a wavelength, so they
// scatter hard forward: looking into a beam is dazzling and looking across it
// is not. Henyey-Greenstein stands in for the Mie lobe, at less than the ~0.8 a
// real droplet measures because single scattering has no further bounces to
// smear the peak out with.
const ASYMMETRY: f32 = 0.6;

// Budget for a ray that sweeps a half turn about the apex, which is one looking
// straight down a shaft. Everything else scales off the angle it does sweep.
const MIN_STEPS: i32 = 8;
const MAX_STEPS: i32 = 64;

// Occlusion rays a pixel spends, however many cones cross it. What blocks a beam
// changes far more slowly along the ray than the scattering does, so visibility
// is sampled at its own rate and held in between; and cones that overlap are
// blocked by the same things, so they split one budget rather than each taking
// it. A ray costs orders of magnitude more than everything else in a step.
const SHADOW_RAYS: i32 = 24;
const MIN_SHADOW_RAYS: i32 = 3;

fn tan_outer(beam: Beam) -> f32 {
    let c = max(beam.axis.w, 1e-4);
    return sqrt(max(1.0 - c * c, 0.0)) / c;
}

// Illuminance a world point receives from the source, per candela of peak
// intensity, before the gate. Zero outside the cone.
fn falloff(beam: Beam, p: vec3<f32>) -> f32 {
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
    return ramp * ramp / square;
}

// The cone's cross-section at a point: xy in the gobo's frame, normalised so
// every section is the same unit disc, and z the section's radius in metres.
// That normalising is what extrudes a pattern down the beam - it opens with the
// cone instead of staying the size it left the gate.
fn aperture(beam: Beam, p: vec3<f32>) -> vec3<f32> {
    let offset = p - beam.apex.xyz;
    let depth = dot(offset, beam.axis.xyz);
    let radius = max(depth * tan_outer(beam), 1e-6);
    let perp = offset - beam.axis.xyz * depth;
    return vec3(vec2(dot(perp, beam.right.xyz), dot(perp, beam.up.xyz)) / radius, radius);
}

// Which side of the split a point in the aperture falls on: 0 for the near
// slot, 1 for the far one.
fn side(beam: Beam, flat: vec2<f32>) -> f32 {
    return step(beam.split.z, dot(flat, beam.split.xy));
}

// What the gobo lets through at a point in the aperture. Kept apart from the
// falloff so an occluded sample never pays for the texture fetch.
fn gate(beam: Beam, flat: vec2<f32>, radius: f32, pick: f32, step: f32) -> f32 {
    let layer = mix(beam.gobo.z, beam.split.w, pick);
    if layer < 0.5 {
        return 1.0;
    }
    // A wheel handing one gobo over to the next slides it out of the gate as the
    // next slides in. A colour wheel splits the same way but has no pattern to
    // slide, which is what having the same layer on both sides means.
    let sliding = f32(beam.split.w != beam.gobo.z);
    let centre = sliding * (beam.split.z - 1.0 + 2.0 * pick) * beam.split.xy;
    let local = flat - centre;
    let spun = vec2(
        local.x * beam.gobo.x - local.y * beam.gobo.y,
        local.x * beam.gobo.y + local.y * beam.gobo.x,
    );
    // Each sample stands for `step` metres of ray, which covers this fraction of
    // the gobo's width. Filtering over that instead of point-sampling is what
    // keeps the pattern clean without paying for more samples.
    let texels = (step / (2.0 * radius)) * f32(textureDimensions(gobos, 0).x);
    let lod = max(log2(max(texels, 1.0)), 0.0);
    return textureSampleLevel(gobos, gobo_sampler, spun * 0.5 + 0.5, i32(layer), lod).r;
}

// Range of the ray that lies inside the cone, as distances along `dir`.
// Returns an empty range when the ray misses.
fn cone_span(beam: Beam, origin: vec3<f32>, dir: vec3<f32>) -> vec2<f32> {
    let tan = tan_outer(beam);
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

// Whether the source can see a point in the volume. One ray against the scene,
// stopped at the first thing it meets: no shadow map, so no depth bias, no
// resolution to run out of, and no limit on how many beams can cast.
//
// It is aimed at the lens rather than the apex, which sits inside the head. The
// fixtures themselves are kept out of the structure, so nothing here can be
// blocked by the housing it came out of.
fn reaches(beam: Beam, p: vec3<f32>) -> f32 {
    let to_lens = beam.apex.xyz + beam.axis.xyz * beam.right.w - p;
    let span = length(to_lens);
    if span <= RAY_T_MIN {
        return 1.0;
    }
    let hit = trace_ray(p, to_lens / span, RAY_T_MIN, span - RAY_T_MIN, RAY_FLAG_TERMINATE_ON_FIRST_HIT);
    return f32(hit.kind == RAY_QUERY_INTERSECTION_NONE);
}

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

// Scattered light gathered along the view ray from one cone, per candela of its
// peak intensity and per unit of scattering coefficient. Split between the two
// slots the gate is showing, since each carries its own colour.
fn march(beam: Beam, origin: vec3<f32>, dir: vec3<f32>, limit: f32, sigma: f32, seed: vec2<f32>, rays: i32) -> vec2<f32> {
    var span = cone_span(beam, origin, dir);
    span.y = min(span.y, limit);
    if span.y <= span.x {
        return vec2(0.0);
    }

    // Equiangular sampling: the samples are spread evenly in angle about the
    // apex rather than in distance along the ray, which puts their density on
    // the 1/r^2 the source falls off by. Each is divided back out by the density
    // it was drawn at, so this is the same integral resolved by far fewer of them.
    let axial = dot(beam.apex.xyz - origin, dir);
    let perp = max(length(beam.apex.xyz - origin - dir * axial), 1e-3);
    let near = atan((span.x - axial) / perp);
    let arc = atan((span.y - axial) / perp) - near;

    // That arc is what the marched stretch subtends at the apex, so it is also
    // how much of the beam's structure this pixel has to resolve.
    let steps = clamp(i32(ceil(arc * f32(MAX_STEPS) / PI)), MIN_STEPS, MAX_STEPS);
    let jit = jitter(seed);

    // Held between rays, and re-traced every `apart` samples.
    let apart = max(1, (steps + rays - 1) / rays);
    var visible = 1.0;
    var since = steps;

    var total = vec2(0.0);
    for (var i = 0; i < steps; i += 1) {
        let angle = near + arc * (f32(i) + jit) / f32(steps);
        let t = clamp(axial + perp * tan(angle), span.x, span.y);
        let p = origin + dir * t;
        let offset = p - beam.apex.xyz;
        let square = dot(offset, offset);
        let reach = sqrt(square);
        // Metres of ray this sample stands for, which is the reciprocal of the
        // density it was drawn at.
        let width = arc * square / (perp * f32(steps));

        var pick = 0.0;
        var level = falloff(beam, p);
        if level > 0.0 {
            if since >= apart {
                visible = reaches(beam, p);
                since = 0;
            }
            since += 1;
            level *= visible;
            // Only what survives the occlusion is worth a gobo fetch.
            if level > 0.0 {
                let section = aperture(beam, p);
                pick = side(beam, section.xy);
                level *= gate(beam, section.xy, section.z, pick, width);
            }
        }
        // Both legs are attenuated: the source's throw down to this sample, and
        // this sample's scattering back out to the camera.
        let cosine = dot(offset, -dir) / max(reach, 1e-6);
        let lit = level * phase(cosine) * exp(-sigma * (t + reach)) * width;
        total += vec2(1.0 - pick, pick) * lit;
    }
    return total;
}

@compute @workgroup_size(8, 8, 1)
fn march_beams(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if id.x >= size.x || id.y >= size.y {
        return;
    }
    let pixel = vec2<i32>(id.xy);

    let sigma = volume.air.x;
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
    let ndc = vec2(uv.x, 1.0 - uv.y) * 2.0 - 1.0;
    let origin = view.world_position;

    let ahead = view.world_from_clip * vec4(ndc, 0.5, 1.0);
    let dir = normalize(ahead.xyz / ahead.w - origin);

    // Stop at whatever the frame already drew here. Past the first surface a
    // shaft is behind something, and reversed z puts an empty pixel at zero,
    // which is infinitely far off.
    var limit = 1e9;
    // The depth buffer is the frame's own, so it stays full resolution however
    // coarsely the beams are marched.
    let full = vec2<f32>(textureDimensions(depth_texture)) / vec2<f32>(size);
    let depth = textureLoad(depth_texture, vec2<i32>((vec2<f32>(id.xy) + 0.5) * full), 0);
    if depth > 0.0 {
        let surface = view.world_from_clip * vec4(ndc, depth, 1.0);
        limit = dot(surface.xyz / surface.w - origin, dir);
    }

    let tile = min(vec2<u32>(vec2<f32>(id.xy) * full) / volume.grid.z, volume.grid.xy - vec2(1u));
    let slot = (tile.y * volume.grid.x + tile.x) * (volume.grid.w + 1u);
    let count = min(tiles[slot], volume.grid.w);

    // One budget for the pixel, shared out. A shaft crossed by a dozen cones is
    // blocked by the same geometry a dozen times over.
    let rays = max(MIN_SHADOW_RAYS, SHADOW_RAYS / i32(max(count, 1u)));

    var total = vec3(0.0);
    for (var i = 0u; i < count; i += 1u) {
        let beam = beams[tiles[slot + 1u + i]];
        // Offsetting the dither per beam keeps their bands from lining up.
        let seed = vec2<f32>(id.xy) + vec2(f32(i) * 17.0, f32(i) * 29.0);
        let lit = march(beam, origin, dir, limit, sigma, seed, rays) * sigma;
        total += beam.color.rgb * (beam.color.a * lit.x)
            + beam.color2.rgb * (beam.color2.a * lit.y);
    }

    // Luminance in cd/m^2, which is what the rest of the pipeline is in. Every
    // pixel is written: the texture is pooled between frames, so anything left
    // unwritten still holds the last frame that used it.
    textureStore(output, pixel, vec4(total, 1.0));
}

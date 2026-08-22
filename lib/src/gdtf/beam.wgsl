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
#import ravy::trace::{trace_ray, RAY_T_MIN}

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
    // open gate. w: copies the prism makes, negative for a linear prism and 0
    // or 1 for none.
    gobo: vec4<f32>,
    // xy: unit axis the wheel's slots travel along, in the gobo's frame. z: the
    // edge between them across the aperture in radii, +1 for all of the near
    // side. w: the far side's layer in the gobo array.
    split: vec4<f32>,
    // xy: where the first prism copy sits across the aperture, in radii. zw:
    // how to get from one copy to the next, added along for a linear prism and
    // turned about the centre for a circular one.
    prism: vec4<f32>,
}

struct Volume {
    // xy: tiles across and down. z: pixels on a tile's side. w: beams a tile holds.
    grid: vec4<u32>,
    // x: scattering coefficient of the air, per metre. y: how far a beam bleeds
    // past its cone, in cone radii. z: what that bleed is worth at the rim.
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
// Bevy's deferred gbuffer, which is where the surfaces this rig lights get
// their albedo and normal from.
@group(1) @binding(8) var gbuffer: texture_2d<u32>;

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

// What a prism passes. Splitting the beam costs a little at the glass, and each
// copy carries a share of what is left.
const PRISM_TRANSMISSION: f32 = 0.9;

// How far off a surface an occlusion ray starts, in metres, so that it does not
// stop on the triangle it left.
const SURFACE_BIAS: f32 = 0.01;

fn tan_outer(beam: Beam) -> f32 {
    let c = max(beam.axis.w, 1e-4);
    return sqrt(max(1.0 - c * c, 0.0)) / c;
}

// The cone's cross-section at a point: xy in the gobo's frame, normalised so
// every section is the same unit disc, z the section's radius in metres, and w
// the inverse square from the apex. That normalising is what extrudes a pattern
// down the beam - it opens with the cone instead of staying the size it left
// the gate. w is zero outside the slab between the lens and the end of the
// throw, which is the whole of what this rejects: how far across the aperture
// the point lies is left to the caller, since a prism gives it several centres
// to be measured from.
fn aperture(beam: Beam, p: vec3<f32>) -> vec4<f32> {
    let offset = p - beam.apex.xyz;
    let depth = dot(offset, beam.axis.xyz);
    let radius = max(depth * tan_outer(beam), 1e-6);
    let perp = offset - beam.axis.xyz * depth;
    let flat = vec2(dot(perp, beam.right.xyz), dot(perp, beam.up.xyz)) / radius;
    let inside = depth >= beam.right.w && depth <= beam.up.w;
    return vec4(flat, radius, select(0.0, 1.0 / dot(offset, offset), inside));
}

// How far past the rim of a cone the bleed is still worth gathering, in cone
// radii. The tail is exponential, so four widths of it is a fiftieth of what it
// started at.
fn bleed() -> f32 {
    return select(0.0, 4.0 * volume.air.y, volume.air.z > 0.0);
}

// How much of the source reaches a point in the aperture, before the gate and
// the inverse square. Zero well past the edge of the cone.
//
// `flat` is measured from whichever copy of the beam is being asked about, so a
// prism copy falls off from its own axis rather than the fixture's.
fn ramp(beam: Beam, flat: vec2<f32>) -> f32 {
    let radius = length(flat);
    // The tangent of the angle off that axis, since the aperture is normalised
    // by the tangent of the outer one.
    let tangent = radius * tan_outer(beam);
    // The same ramp bevy squares for its spot falloff, so the volume and the
    // surfaces the beam lands on agree.
    let cosine = inverseSqrt(1.0 + tangent * tangent);
    let hard = saturate((cosine - beam.axis.w) / max(beam.apex.w - beam.axis.w, 1e-4));
    // Past the rim, a tail standing in for the light haze has scattered more
    // than once. The march follows only the first bounce, which leaves an edge
    // harder than any real beam in fog has, and no halo at all.
    let soft = volume.air.z * exp(-max(radius - 1.0, 0.0) / max(volume.air.y, 1e-3));
    return max(hard, soft);
}

// Copies the prism makes, at least one.
fn facets(beam: Beam) -> f32 {
    return max(abs(beam.gobo.w), 1.0);
}

// How far the outermost copy sits off the axis, in aperture radii. Both layouts
// put the first copy at the extreme, so this bounds all of them.
fn spread(beam: Beam) -> f32 {
    return length(beam.prism.xy);
}

// Where the copy after `offset` sits. A linear prism steps along a line; a
// circular one turns about the centre.
fn next_facet(beam: Beam, offset: vec2<f32>) -> vec2<f32> {
    let step = beam.prism.zw;
    let turned = vec2(
        offset.x * step.x - offset.y * step.y,
        offset.x * step.y + offset.y * step.x,
    );
    return select(offset + step, turned, beam.gobo.w > 0.0);
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

// How far across the aperture a beam still reaches, in radii: its own rim, plus
// whatever the prism throws clear of it and the bleed hangs past that.
fn extent(beam: Beam) -> f32 {
    return 1.0 + spread(beam) + bleed();
}

// Illuminance a point in the aperture receives per candela of peak intensity,
// split between the two slots the gate is showing since each carries its own
// colour. Every prism copy is summed here: each throws a whole image of the
// gate a fixed way off the axis, so a point can be lit by more than one.
//
// `width` is how much of the world the caller is asking about, in metres, which
// is what the gobo is filtered over.
fn through(beam: Beam, section: vec4<f32>, width: f32) -> vec2<f32> {
    let copies = facets(beam);
    let share = select(1.0, PRISM_TRANSMISSION, copies > 1.0) / copies;
    var centre = beam.prism.xy;
    var lit = vec2(0.0);
    for (var f = 0.0; f < copies; f += 1.0) {
        // Measured from this copy's own axis, so it carries a whole image of
        // the gate rather than a piece of one.
        let local = section.xy - centre;
        let shade = ramp(beam, local);
        if shade > 0.0 {
            let pick = side(beam, local);
            let gated = gate(beam, local, section.z, pick, width);
            lit += vec2(1.0 - pick, pick) * (shade * shade * gated * share);
        }
        centre = next_facet(beam, centre);
    }
    return lit * section.w;
}

// Range of the ray that lies inside the cone, as distances along `dir`.
// Returns an empty range when the ray misses. `tan` widens it to hold every
// prism copy, since each is thrown clear of the cone the fixture alone makes.
fn cone_span(beam: Beam, origin: vec3<f32>, dir: vec3<f32>, tan: f32) -> vec2<f32> {
    let widen = 1.0 + tan * tan;
    let rel = origin - beam.apex.xyz;
    let along = dot(rel, beam.axis.xyz);
    let slope = dot(dir, beam.axis.xyz);

    var near = 0.0;
    var far = 1e9;

    // Quadratic in t for |P-apex|^2 = (1 + tan^2) * axial^2, the cone surface.
    let a = 1.0 - widen * slope * slope;
    let b = 2.0 * (dot(rel, dir) - widen * along * slope);
    let c = dot(rel, rel) - widen * along * along;
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
    let reach_max = extent(beam);
    var span = cone_span(beam, origin, dir, tan_outer(beam) * reach_max);
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

        // Each side of the split gathers separately, since each is its own
        // colour. A prism copy lands wholly on one side or the other, so the
        // copies share the pair rather than needing one each.
        var lit = vec2(0.0);
        let section = aperture(beam, p);
        // Every copy is blocked by the same things, so the ray is traced once
        // for the point and the widest copy decides whether it is worth it.
        if section.w > 0.0 && length(section.xy) <= reach_max {
            if since >= apart {
                visible = reaches(beam, p);
                since = 0;
            }
            since += 1;
            if visible > 0.0 {
                lit = through(beam, section, width) * visible;
            }
        }
        // Both legs are attenuated: the source's throw down to this sample, and
        // this sample's scattering back out to the camera.
        let cosine = dot(offset, -dir) / max(reach, 1e-6);
        total += lit * phase(cosine) * exp(-sigma * (t + reach)) * width;
    }
    return total;
}

// What a surface is made of, as much of it as the shading here needs. The rest
// of the gbuffer is roughness, emissive and the rest, which bevy's own deferred
// pass has already dealt with for the lights it knows about.
struct Surface {
    albedo: vec3<f32>,
    normal: vec3<f32>,
}

// https://jcgt.org/published/0003/02/01/paper.pdf, as bevy packs it.
fn octahedral_decode(v: vec2<f32>) -> vec3<f32> {
    let f = v * 2.0 - 1.0;
    let n = vec3(f.xy, 1.0 - abs(f.x) - abs(f.y));
    let t = saturate(-n.z);
    let w = select(vec2(t), vec2(-t), n.xy >= vec2(0.0));
    return normalize(vec3(n.xy + w, n.z));
}

fn unpack_unorm4x8(v: u32) -> vec4<f32> {
    return vec4(
        f32(v & 0xFFu),
        f32((v >> 8u) & 0xFFu),
        f32((v >> 16u) & 0xFFu),
        f32((v >> 24u) & 0xFFu),
    ) / 255.0;
}

fn unpack_surface(packed: vec4<u32>) -> Surface {
    // Base colour keeps a plain 2.2 curve in the gbuffer, not sRGB's.
    let base = unpack_unorm4x8(packed.r);
    let props = unpack_unorm4x8(packed.b);
    // Metal has no diffuse lobe, and diffuse is the whole of what is shaded here.
    let albedo = pow(base.rgb, vec3(2.2)) * (1.0 - props.g);
    let oct = vec2(f32(packed.a & 0xFFFu), f32((packed.a >> 12u) & 0xFFFu)) / 4095.0;
    return Surface(albedo, octahedral_decode(oct));
}

// Direct light this rig throws onto the surfaces the frame drew. Bevy's own
// lighting handles the ambient and the work light; every fixture here is
// shaded from the same cones the haze is, so what lands on the floor carries
// the gobo, the colour split and the prism that the shaft does.
//
// Diffuse only. A stage lantern's specular on a matte floor is a small part of
// what is seen, and the gbuffer's roughness would cost a lobe per beam.
@compute @workgroup_size(8, 8, 1)
fn light_surfaces(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if id.x >= size.x || id.y >= size.y {
        return;
    }
    let pixel = vec2<i32>(id.xy);

    // Reversed z, so an empty pixel sits at zero and has no surface on it.
    let depth = textureLoad(depth_texture, pixel, 0);
    if depth <= 0.0 {
        textureStore(output, pixel, vec4(0.0));
        return;
    }

    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
    let ndc = vec2(uv.x, 1.0 - uv.y) * 2.0 - 1.0;
    let clip = view.world_from_clip * vec4(ndc, depth, 1.0);
    let p = clip.xyz / clip.w;

    let surface = unpack_surface(textureLoad(gbuffer, pixel, 0));
    let n = surface.normal;

    // World size of this pixel where it landed, which is what the gobo is
    // filtered over so a pattern thrown across the room does not alias.
    let away = length(p - view.world_position);
    let width = 2.0 * away / max(view.clip_from_view[1][1] * f32(size.y), 1e-4);

    let sigma = volume.air.x;
    let tile = min(vec2<u32>(id.xy) / volume.grid.z, volume.grid.xy - vec2(1u));
    let slot = (tile.y * volume.grid.x + tile.x) * (volume.grid.w + 1u);
    let count = min(tiles[slot], volume.grid.w);

    var total = vec3(0.0);
    for (var i = 0u; i < count; i += 1u) {
        let beam = beams[tiles[slot + 1u + i]];

        let section = aperture(beam, p);
        if section.w <= 0.0 || length(section.xy) > extent(beam) {
            continue;
        }
        // Lambert, off the lens rather than the apex: the apex is a construction
        // sat behind the fixture's own front, and this is where the light is.
        let to_lens = beam.apex.xyz + beam.axis.xyz * beam.right.w - p;
        let range = max(length(to_lens), 1e-4);
        let facing = dot(n, to_lens / range);
        if facing <= 0.0 {
            continue;
        }
        // Off the surface a little, or it shadows itself against its own
        // triangle.
        if reaches(beam, p + n * SURFACE_BIAS) <= 0.0 {
            continue;
        }

        let lit = through(beam, section, width);
        let carried = beam.color.rgb * (beam.color.a * lit.x)
            + beam.color2.rgb * (beam.color2.a * lit.y);
        // Haze between the lens and the surface dims the pool the same way it
        // dims the shaft that got there.
        total += carried * (facing * exp(-sigma * range));
    }

    // Lambert sends what it receives out over a hemisphere. Exposed the way
    // bevy exposes its own direct lighting, since this stands in its place for
    // these fixtures and lands in the frame beside it.
    let shaded = total * surface.albedo * (view.exposure / PI);
    textureStore(output, pixel, vec4(shaded, 1.0));
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

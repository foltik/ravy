// The cube: solid faces in the palette's first colour, box edges drawn as a
// wireframe in the second. Flat fills with hard boundaries are what the fx
// pass's sobel wants; any shading or gradient bands under it. Alpha 1 rides
// through as the fx pass's edge mask.
//
// params: x = wire width, fraction of the cube's half extent.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_functions

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // The wire lives on the cube, so undo the tumble: for this rotation-and-
    // uniform-scale model, transpose over scale squared is the inverse.
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    let m = mat3x3<f32>(
        world_from_local[0].xyz,
        world_from_local[1].xyz,
        world_from_local[2].xyz,
    );
    let lp = ((in.world_position.xyz - world_from_local[3].xyz) * m) / dot(m[0], m[0]);

    // On a box edge when the second-largest |axis| is also near 1: the
    // median of the three is exactly that.
    let a = abs(lp);
    let edge = max(min(a.x, a.y), min(max(a.x, a.y), a.z));
    let aa = fwidth(edge) + 1e-4;
    let wire = smoothstep(1.0 - params.x - aa, 1.0 - params.x + aa, edge);

    return vec4<f32>(mix(color0.rgb, color1.rgb, wire), 1.0);
}

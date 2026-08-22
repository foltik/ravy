// The uvbounce ball, unshaded: a checkerboard over the sphere's uv in the
// palette's two colours (the second falls back to black upstream when the
// palette only has one). Alpha 1 keeps the fx edge pass biting on it.
//
// params: x = checker columns around the equator.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color0: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> color1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> params: vec4<f32>;

fn gmod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Half as many rows as columns keeps the cells roughly square.
    let c = gmod(floor(in.uv.x * params.x) + floor(in.uv.y * params.x * 0.5), 2.0);
    return vec4<f32>(mix(color1.rgb, color0.rgb, c), 1.0);
}

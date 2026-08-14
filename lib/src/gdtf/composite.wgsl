// Adds what the beam march gathered into the frame. A compute shader cannot
// blend, so the march writes a texture and this puts it in, filtering it back up
// if it was marched coarser than the frame.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var beams: texture_2d<f32>;
@group(0) @binding(1) var beams_sampler: sampler;
@group(0) @binding(2) var surfaces: texture_2d<f32>;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let scattered = textureSampleLevel(beams, beams_sampler, in.uv, 0.0).rgb;
    // Full resolution, so it is read straight rather than filtered back up:
    // what a gobo throws on a wall has edges, where a shaft does not.
    let lit = textureLoad(surfaces, vec2<i32>(in.position.xy), 0).rgb;
    // Alpha stays at zero: both add without hiding what is behind them.
    return vec4(scattered + lit, 0.0);
}

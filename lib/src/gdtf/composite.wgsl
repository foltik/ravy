// Adds what the beam march gathered into the frame. A compute shader cannot
// blend, so the march writes a texture and this puts it in, filtering it back up
// if it was marched coarser than the frame.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var beams: texture_2d<f32>;
@group(0) @binding(1) var beams_sampler: sampler;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let scattered = textureSampleLevel(beams, beams_sampler, in.uv, 0.0).rgb;
    // Alpha stays at zero: haze adds without hiding what is behind it.
    return vec4(scattered, 0.0);
}

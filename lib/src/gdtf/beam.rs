//! Beam cone material, driven by `beam.wgsl`.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::image::{Image, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::render::render_resource::{
    AsBindGroup, CompareFunction, Extent3d, Face, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use crate::prelude::*;

pub struct BeamPlugin;

impl Plugin for BeamPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "beam.wgsl");
        app.add_plugins(MaterialPlugin::<BeamMaterial>::default());

        // A gobo is always bound, so the shader never sees bevy's white fallback.
        let blank = Image::new_fill(
            Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::RENDER_WORLD,
        );
        let blank = app.world_mut().resource_mut::<Assets<Image>>().add(blank);
        app.insert_resource(NoGobo(blank));
    }
}

/// Stand-in for an open gobo slot.
#[derive(Resource, Clone)]
pub struct NoGobo(pub Handle<Image>);

/// Box-filter a mip chain into an image, and set it up to be sampled with one.
/// Bevy's loader only ever produces a single level, which leaves the beam
/// point-sampling a 1k gobo and aliasing into noise.
pub fn generate_mipmaps(image: &mut Image) -> Option<()> {
    let format = image.texture_descriptor.format;
    if image.texture_descriptor.mip_level_count > 1 {
        return None;
    }
    if !matches!(format, TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm) {
        return None;
    }
    let srgb = format == TextureFormat::Rgba8UnormSrgb;
    // Four decodes per output texel, so table it rather than calling powf 4M times.
    let decode: [f32; 256] = std::array::from_fn(|i| srgb_to_linear(i as f32 / 255.0));

    let (mut width, mut height) = (image.texture_descriptor.size.width, image.texture_descriptor.size.height);
    let mut data = image.data.clone()?;
    let mut level = data.clone();
    let mut levels = 1;

    while width > 1 || height > 1 {
        let (half_w, half_h) = ((width / 2).max(1), (height / 2).max(1));
        let mut next = vec![0u8; (half_w * half_h * 4) as usize];
        for y in 0..half_h {
            for x in 0..half_w {
                for c in 0..4 {
                    // Averaging a light mask has to happen in linear space, or the
                    // mostly-black gobos drift brighter at every level.
                    let mut sum = 0.0;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(width - 1);
                        let sy = (y * 2 + dy).min(height - 1);
                        let v = level[((sy * width + sx) * 4 + c) as usize];
                        sum += match srgb && c < 3 {
                            true => decode[v as usize],
                            false => v as f32 / 255.0,
                        };
                    }
                    let avg = sum / 4.0;
                    let avg = if srgb && c < 3 { linear_to_srgb(avg) } else { avg };
                    next[((y * half_w + x) * 4 + c) as usize] = (avg * 255.0).round() as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        level = next;
        (width, height) = (half_w, half_h);
        levels += 1;
    }

    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    Some(())
}

fn srgb_to_linear(v: f32) -> f32 {
    match v <= 0.04045 {
        true => v / 12.92,
        false => ((v + 0.055) / 1.055).powf(2.4),
    }
}

fn linear_to_srgb(v: f32) -> f32 {
    match v <= 0.0031308 {
        true => v * 12.92,
        false => 1.055 * v.powf(1.0 / 2.4) - 0.055,
    }
}

/// Mirrors `Beam` in beam.wgsl.
#[derive(Clone, Default, ShaderType)]
pub struct Beam {
    /// rgb: linear sRGB at a luminance of 1. a: peak intensity in candela.
    pub color: Vec4,
    /// xyz: cone apex in world space. w: cosine of the inner angle.
    pub apex: Vec4,
    /// xyz: unit vector down the beam. w: cosine of the outer angle.
    pub axis: Vec4,
    /// xyz: the gobo's u axis. w: axial distance from the apex to the lens.
    pub right: Vec4,
    /// xyz: the gobo's v axis. w: axial distance from the apex to the far end.
    pub up: Vec4,
    /// xy: cos/sin of the gobo's rotation. z: 1 when a gobo is on the wheel.
    /// w: scattering coefficient of the air, per metre.
    pub gobo_spin: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct BeamMaterial {
    #[uniform(0)]
    pub beam: Beam,
    #[texture(1)]
    #[sampler(2)]
    pub gobo: Handle<Image>,
}

impl Material for BeamMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://lib/gdtf/beam.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    /// The cone is a volume, not a surface, so neither the rasteriser's culling
    /// nor its depth test means what they usually do.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Back faces only: the march clips itself to the cone, so one face per
        // pixel is all it needs, and keeping the far one means the volume still
        // draws with the camera inside it. Culling neither marches every pixel
        // twice and blends the result over itself.
        descriptor.primitive.cull_mode = Some(Face::Front);

        // Those far faces are usually buried in the floor, which a depth test
        // would throw the whole shaft away for. The shader ends its march at
        // the depth prepass instead, which occludes the beam per sample rather
        // than all at once.
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_compare = Some(CompareFunction::Always);
        }
        Ok(())
    }
}

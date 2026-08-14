//! Volumetric beams: one screen pass that marches every cone at once.
//!
//! A cone used to be its own mesh with its own material, so a pixel the whole rig
//! pointed through was shaded, marched and blended once per fixture. Instead the
//! beams go into one buffer, get binned into screen tiles on the cpu, and a single
//! fullscreen pass marches whatever list its tile holds.
//!
//! Occlusion is a ray query against the scene's acceleration structure, so a shaft
//! stops at whatever it hits with no shadow map in between. Without the hardware
//! for it the pass still runs, but nothing blocks the haze.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::core_pipeline::FullscreenShader;
use bevy::core_pipeline::prepass::{DeferredPrepass, DepthPrepass, ViewPrepassTextures};
use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::platform::collections::HashMap;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_asset::RenderAssets;
use bevy::tasks::ComputeTaskPool;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only, texture_2d, texture_2d_array, texture_depth_2d,
    texture_depth_2d_multisampled, texture_storage_2d, uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BlendComponent,
    BlendFactor, BlendOperation, BlendState, CachedComputePipelineId, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, ComputePassDescriptor, ComputePipelineDescriptor, Extent3d,
    FragmentState, MultisampleState, PipelineCache, RenderPassDescriptor, RenderPipelineDescriptor,
    AddressMode, FilterMode, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
    ShaderType, SpecializedComputePipeline,
    SpecializedComputePipelines, SpecializedRenderPipeline, SpecializedRenderPipelines,
    StorageBuffer, StorageTextureAccess, TextureDataOrder, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureViewDescriptor, TextureViewDimension,
    UniformBuffer,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{CachedTexture, GpuImage, TextureCache};
use bevy::render::view::{ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy::render::{Extract, ExtractSchedule, GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;
use bevy::solari::scene::{RaytracingMesh3d, RaytracingSceneBindings};

use crate::prelude::*;

/// Pixels on a side of a tile in the beam list.
pub const TILE: u32 = 64;

/// Beams one tile can hold. A tile crossed by more cones than this drops
/// whichever were binned last.
pub const TILE_CAPACITY: u32 = 63;

/// Beams the pass can carry at once.
pub const MAX_BEAMS: usize = 2048;

/// Side of a gobo in the packed array. Wheel art is authored at 1k, and the beam
/// samples it through a mip chain, so nothing is gained by holding more.
const GOBO_SIZE: u32 = 1024;

/// Points taken around each end of a cone when bounding it on screen.
const RIM: usize = 24;

pub struct BeamPlugin;

impl Plugin for BeamPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "beam.wgsl");
        embedded_asset!(app, "composite.wgsl");

        let gobos = app.world_mut().resource_mut::<Assets<Image>>().add(gobo_array(&[]));
        app.insert_resource(Volumetrics {
            gobos,
            layers: HashMap::default(),
            volume: Volume::default(),
            list: Vec::new(),
            bins: Vec::new(),
        });

        app.init_resource::<Proxies>().add_systems(PostUpdate, (traceable, prepare_views));

        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        render
            .init_gpu_resource::<SpecializedComputePipelines<BeamPipeline>>()
            .init_gpu_resource::<SpecializedRenderPipelines<CompositePipeline>>()
            .init_resource::<BeamFrame>()
            .init_resource::<BeamBuffers>()
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(ExtractSchedule, extract_beams)
            .add_systems(
                Render,
                (
                    prepare_buffers.in_set(RenderSystems::PrepareResources),
                    prepare_textures.in_set(RenderSystems::PrepareResources),
                    prepare_pipeline.in_set(RenderSystems::Prepare),
                ),
            )
            // After the main pass, so the depth buffer holds the room, and before
            // bloom, so a shaft glows the way the lens face does.
            .add_systems(Core3d, beam_pass.in_set(Core3dSystems::EarlyPostProcess));
    }
}

/// Mirrors `Beam` in beam.wgsl.
///
/// A wheel is only ever on a slot between one move and the next, so the cone
/// carries two of everything a wheel picks and the edge dividing them.
#[derive(Clone, Default, ShaderType)]
pub struct Beam {
    /// rgb: linear sRGB at a luminance of 1. a: peak intensity in candela.
    pub color: Vec4,
    /// The same, for the far side of the split.
    pub color2: Vec4,
    /// xyz: cone apex in world space. w: cosine of the inner angle.
    pub apex: Vec4,
    /// xyz: unit vector down the beam. w: cosine of the outer angle.
    pub axis: Vec4,
    /// xyz: the gobo's u axis. w: axial distance from the apex to the lens.
    pub right: Vec4,
    /// xyz: the gobo's v axis. w: axial distance from the apex to the far end.
    pub up: Vec4,
    /// xy: cos/sin of the gobo's rotation. z: layer in the gobo array, 0 for an
    /// open gate. w: copies the prism makes, negative for a linear prism and 0
    /// or 1 for none.
    pub gobo: Vec4,
    /// xy: unit axis the wheel's slots travel along, in the gobo's frame.
    /// z: the edge between them across the aperture in radii, +1 for all of the
    /// near side. w: the far side's layer in the gobo array.
    pub split: Vec4,
    /// xy: where the first prism copy sits across the aperture, in radii.
    /// zw: how to get from one copy to the next, added along for a linear prism
    /// and turned about the centre for a circular one.
    pub prism: Vec4,
}

/// Mirrors `Volume` in beam.wgsl.
#[derive(Clone, Default, ShaderType)]
pub struct Volume {
    /// xy: tiles across and down. z: pixels on a tile's side. w: beams a tile holds.
    pub grid: UVec4,
    /// x: scattering coefficient of the air, per metre. y: how far a beam
    /// bleeds past its cone, in cone radii. z: what that bleed is worth at the
    /// rim.
    pub air: Vec4,
}

/// What the beam pass draws this frame, filled by [`publish`].
#[derive(Resource)]
pub struct Volumetrics {
    /// The packed gobo array. Layer 0 is white, which is an open gate.
    pub gobos: Handle<Image>,
    layers: HashMap<AssetId<Image>, u32>,
    volume: Volume,
    /// Reused between frames so the per-frame binning allocates nothing.
    pub(super) list: Vec<Beam>,
    bins: Vec<u32>,
}

impl Volumetrics {
    /// Layer holding `image` in the gobo array, or 0 for an open gate.
    pub fn layer(&self, image: Option<&Handle<Image>>) -> f32 {
        image.and_then(|h| self.layers.get(&h.id())).copied().unwrap_or(0) as f32
    }
}

/// Hand the frame's beams to the pass, binned into the screen tiles each one can
/// cover. A cone that straddles the camera plane has no screen bounds, so it goes
/// into every tile.
pub(super) fn publish(volumetrics: &mut Volumetrics, clip: Mat4, size: UVec2, air: Vec4) {
    let across = size.x.div_ceil(TILE).max(1);
    let down = size.y.div_ceil(TILE).max(1);
    let stride = (TILE_CAPACITY + 1) as usize;

    volumetrics.volume.grid = UVec4::new(across, down, TILE, TILE_CAPACITY);
    volumetrics.volume.air = air;

    // Matches `bleed` in beam.wgsl: how far past its rim a beam still reaches.
    let bleed = match air.z > 0.0 {
        true => 4.0 * air.y,
        false => 0.0,
    };

    let mut bins = std::mem::take(&mut volumetrics.bins);
    bins.clear();
    bins.resize(across as usize * down as usize * stride, 0);
    for (index, beam) in volumetrics.list.iter().enumerate() {
        let (lo, hi) = match screen_bounds(beam, clip, size.as_vec2(), bleed) {
            Bounds::Screen(lo, hi) => (lo, hi),
            Bounds::Everywhere => (Vec2::ZERO, size.as_vec2()),
            Bounds::Behind => continue,
        };
        let x0 = (lo.x / TILE as f32).floor().max(0.0) as u32;
        let y0 = (lo.y / TILE as f32).floor().max(0.0) as u32;
        let x1 = (hi.x / TILE as f32).floor().min(across as f32 - 1.0) as u32;
        let y1 = (hi.y / TILE as f32).floor().min(down as f32 - 1.0) as u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let slot = (y * across + x) as usize * stride;
                let count = bins[slot];
                if count < TILE_CAPACITY {
                    bins[slot] = count + 1;
                    bins[slot + 1 + count as usize] = index as u32;
                }
            }
        }
    }
    volumetrics.bins = bins;
}

/// Where a cone can land on screen.
enum Bounds {
    Screen(Vec2, Vec2),
    /// It straddles the camera plane, so a projection says nothing about it.
    Everywhere,
    /// Entirely behind the camera.
    Behind,
}

/// Pixel bounds of a cone. It lies inside the hull of its two end discs, so the
/// corners of the projected rims bound it.
fn screen_bounds(beam: &Beam, clip: Mat4, size: Vec2, bleed: f32) -> Bounds {
    let (apex, axis) = (beam.apex.truncate(), beam.axis.truncate());
    let (right, up) = (beam.right.truncate(), beam.up.truncate());
    let cos = beam.axis.w.max(1e-4);
    // Wide enough to hold every prism copy, which sit outside the cone the
    // fixture alone makes, and the bleed that hangs past those.
    let spread = Vec2::new(beam.prism.x, beam.prism.y).length();
    let tan = (1.0 - cos * cos).max(0.0).sqrt() / cos * (1.0 + spread + bleed);

    let (mut lo, mut hi) = (Vec2::MAX, Vec2::MIN);
    let mut behind = 0;
    for depth in [beam.right.w, beam.up.w] {
        let centre = apex + axis * depth;
        let radius = depth * tan;
        for i in 0..RIM {
            let a = i as f32 / RIM as f32 * std::f32::consts::TAU;
            let point = centre + (right * a.cos() + up * a.sin()) * radius;
            let clipped = clip * point.extend(1.0);
            if clipped.w <= 1e-4 {
                behind += 1;
                continue;
            }
            let ndc = clipped.truncate() / clipped.w;
            let pixel = Vec2::new((ndc.x + 1.0) * 0.5 * size.x, (1.0 - ndc.y) * 0.5 * size.y);
            lo = lo.min(pixel);
            hi = hi.max(pixel);
        }
    }

    match behind {
        0 => {
            // The rim points inscribe an ellipse, so the bound misses by the
            // sagitta of one of their chords.
            let slack = (hi - lo) * (1.0 - (std::f32::consts::PI / RIM as f32).cos()) + 1.0;
            Bounds::Screen(lo - slack, hi + slack)
        }
        n if n == 2 * RIM => Bounds::Behind,
        _ => Bounds::Everywhere,
    }
}

/// Build the gobo array once the library's images have all landed. Gobos arrive
/// without a mip chain, which leaves the beam aliasing into noise against full
/// resolution pixels.
///
/// Packing costs about as much as a whole second of frames, so it waits for the
/// last image rather than redoing every layer each time one arrives.
pub(super) fn pack_gobos(
    library: Res<GdtfLibrary>,
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    mut volumetrics: ResMut<Volumetrics>,
) {
    let ours = |id: &AssetId<Image>| {
        library.types.values().any(|ty| ty.images.values().any(|h| h.id() == *id))
    };
    let landed = events.read().any(|e| matches!(e, AssetEvent::Added { id } if ours(id)));
    if !landed {
        return;
    }

    // Sorted so a layer's index does not depend on the order they loaded in.
    let mut ids: Vec<AssetId<Image>> =
        library.types.values().flat_map(|ty| ty.images.values().map(|h| h.id())).collect();
    ids.sort();
    ids.dedup();

    let sources: Vec<&Image> = ids.iter().filter_map(|id| images.get(*id)).collect();
    if sources.len() < ids.len() {
        return;
    }

    let packed = gobo_array(&sources);
    volumetrics.layers = ids.iter().enumerate().map(|(i, id)| (*id, i as u32 + 1)).collect();
    if let Some(mut slot) = images.get_mut(&volumetrics.gobos) {
        *slot = packed;
    }
}

/// Pack gobos into one array texture with a mip chain each. Layer 0 is white, so
/// an open gate can index the array without a branch.
fn gobo_array(sources: &[&Image]) -> Image {
    let mips = GOBO_SIZE.ilog2() + 1;
    // A layer is tens of millions of float ops, and they do not depend on each
    // other, so they are built side by side.
    let layers = ComputeTaskPool::get().scope(|scope| {
        scope.spawn(async { chain(&vec![1.0; (GOBO_SIZE * GOBO_SIZE * 4) as usize]) });
        for source in sources {
            scope.spawn(async move { chain(&resample(source)) });
        }
    });
    let data = layers.concat();

    Image {
        data: Some(data),
        // Each layer holds its whole chain, which is how they are built.
        data_order: TextureDataOrder::LayerMajor,
        texture_descriptor: TextureDescriptor {
            label: None,
            size: Extent3d {
                width: GOBO_SIZE,
                height: GOBO_SIZE,
                depth_or_array_layers: sources.len() as u32 + 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        sampler: ImageSampler::Descriptor(ImageSamplerDescriptor {
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        }),
        texture_view_descriptor: Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..default()
        }),
        asset_usage: RenderAssetUsages::RENDER_WORLD,
        copy_on_resize: false,
    }
}

/// One layer's mip chain, halving in linear light and writing out sRGB.
/// Averaging a light mask in sRGB drifts the mostly-black gobos brighter at
/// every level.
fn chain(linear: &[f32]) -> Vec<u8> {
    let (mut width, mut height) = (GOBO_SIZE as usize, GOBO_SIZE as usize);
    let mut level = linear.to_vec();
    let mut data = Vec::new();
    loop {
        data.extend(level.iter().map(|v| (linear_to_srgb(*v) * 255.0).round() as u8));
        if width == 1 && height == 1 {
            return data;
        }
        let (half_w, half_h) = ((width / 2).max(1), (height / 2).max(1));
        let mut next = vec![0.0; half_w * half_h * 4];
        for y in 0..half_h {
            for x in 0..half_w {
                for c in 0..4 {
                    let mut sum = 0.0;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(width - 1);
                        let sy = (y * 2 + dy).min(height - 1);
                        sum += level[(sy * width + sx) * 4 + c];
                    }
                    next[(y * half_w + x) * 4 + c] = sum / 4.0;
                }
            }
        }
        level = next;
        (width, height) = (half_w, half_h);
    }
}

/// Box-filter an image to the array's size, in linear light.
fn resample(image: &Image) -> Vec<f32> {
    let size = GOBO_SIZE as usize;
    let mut out = vec![0.0; size * size * 4];
    let (Some(data), width, height) =
        (image.data.as_ref(), image.width() as usize, image.height() as usize)
    else {
        return out;
    };
    let srgb = image.texture_descriptor.format == TextureFormat::Rgba8UnormSrgb;
    // Four decodes per output texel, so table it rather than calling powf 4M times.
    let decode: [f32; 256] = std::array::from_fn(|i| match srgb {
        true => srgb_to_linear(i as f32 / 255.0),
        false => i as f32 / 255.0,
    });
    if data.len() < width * height * 4 || width == 0 || height == 0 {
        return out;
    }

    for y in 0..size {
        let (sy0, sy1) = (y * height / size, ((y + 1) * height / size).max(y * height / size + 1));
        for x in 0..size {
            let (sx0, sx1) = (x * width / size, ((x + 1) * width / size).max(x * width / size + 1));
            let mut sum = [0.0f32; 4];
            for sy in sy0..sy1.min(height) {
                for sx in sx0..sx1.min(width) {
                    for (c, channel) in sum.iter_mut().enumerate() {
                        // Alpha is not a colour, so it never went through a curve.
                        let v = data[(sy * width + sx) * 4 + c];
                        *channel += match c < 3 {
                            true => decode[v as usize],
                            false => v as f32 / 255.0,
                        };
                    }
                }
            }
            let count = ((sy1.min(height) - sy0) * (sx1.min(width) - sx0)).max(1) as f32;
            for (c, channel) in sum.iter().enumerate() {
                out[(y * size + x) * 4 + c] = channel / count;
            }
        }
    }
    out
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

/// What every 3d view needs for the beams to be drawn at all, set here rather
/// than asked of each app: the fixtures are shaded by this pass alone, so a
/// view missing any of it would be lit by nothing.
///
/// The march reads the depth the main pass left, to stop each shaft at whatever
/// it lands on. Bevy asks for a depth texture it can only render to, so this
/// widens it rather than paying for a whole prepass to get a second copy. The
/// gbuffer is where the surfaces the beams land on get their albedo and normal,
/// and bevy only fills it at one sample, so multisampling has to go.
fn prepare_views(
    mut cmds: Commands,
    mut cameras: Query<(Entity, &mut Camera3d, &mut Msaa, Has<DeferredPrepass>)>,
) {
    for (entity, mut camera, mut msaa, deferred) in &mut cameras {
        let usage = TextureUsages::from(camera.depth_texture_usages);
        // Each of these only where it is not already so: a view written to every
        // frame counts as changed every frame, and its pipelines specialize
        // again for it.
        if !usage.contains(TextureUsages::TEXTURE_BINDING) {
            camera.depth_texture_usages = (usage | TextureUsages::TEXTURE_BINDING).into();
        }
        // Bevy makes Msaa a required component of every camera, so this is
        // always overriding a default rather than filling a gap.
        msaa.set_if_neq(Msaa::Off);
        if !deferred {
            cmds.entity(entity).insert((DepthPrepass, DeferredPrepass));
        }
    }
}

/// Raytracing stand-ins, keyed by the mesh each was made from.
#[derive(Resource, Default)]
struct Proxies(HashMap<AssetId<Mesh>, Handle<Mesh>>);

/// Everything a beam can be blocked by has to be in the acceleration structure,
/// which is opt-in per mesh. Fixtures are left out of it for the same reason
/// they cast no shadow: the beam starts inside the head, so its own housing
/// would block all of it.
fn traceable(
    mut cmds: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut proxies: ResMut<Proxies>,
    solid: Query<
        (Entity, &Mesh3d),
        (With<MeshMaterial3d<StandardMaterial>>, Without<RaytracingMesh3d>, Without<NotShadowCaster>),
    >,
    lit: Query<Entity, (With<RaytracingMesh3d>, With<NotShadowCaster>)>,
) {
    for (entity, mesh) in &solid {
        let handle = match proxies.0.get(&mesh.0.id()) {
            Some(handle) => Some(handle.clone()),
            None => {
                let Some(source) = meshes.get(&mesh.0) else { continue };
                let built = match traceable_as_is(source) {
                    true => Some(mesh.0.clone()),
                    false => proxy(source).map(|m| meshes.add(m)),
                };
                if let Some(handle) = &built {
                    proxies.0.insert(mesh.0.id(), handle.clone());
                }
                built
            }
        };
        if let Some(handle) = handle {
            cmds.entity(entity).insert(RaytracingMesh3d(handle));
        }
    }
    // A part only picks up NotShadowCaster once its scene lands, which can be
    // after it was taken for a solid.
    for entity in &lit {
        cmds.entity(entity).remove::<RaytracingMesh3d>();
    }
}

/// Whether a mesh is already in the shape an acceleration structure is built
/// from: a triangle list carrying exactly position, normal, uv and tangent,
/// indexed 32 bit. Anything else is silently left out of the structure.
fn traceable_as_is(mesh: &Mesh) -> bool {
    mesh.primitive_topology() == PrimitiveTopology::TriangleList
        && matches!(mesh.indices(), Some(Indices::U32(_)))
        && mesh.attributes().map(|(a, _)| a.id).eq([
            Mesh::ATTRIBUTE_POSITION.id,
            Mesh::ATTRIBUTE_NORMAL.id,
            Mesh::ATTRIBUTE_UV_0.id,
            Mesh::ATTRIBUTE_TANGENT.id,
        ])
}

/// A copy of a mesh trimmed to that shape. Only the positions and the indices
/// decide where a ray lands, so the rest is filled in rather than derived.
fn proxy(mesh: &Mesh) -> Option<Mesh> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.clone();
    let count = positions.len();
    let indices = match mesh.indices()? {
        Indices::U32(indices) => indices.clone(),
        Indices::U16(indices) => indices.iter().map(|i| *i as u32).collect(),
    };

    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    out.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(normals) if normals.len() == count => normals.clone(),
            _ => VertexAttributeValues::Float32x3(vec![[0.0, 1.0, 0.0]; count]),
        },
    );
    out.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(uvs) if uvs.len() == count => uvs.clone(),
            _ => VertexAttributeValues::Float32x2(vec![[0.0, 0.0]; count]),
        },
    );
    out.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; count]);
    out.insert_indices(Indices::U32(indices));
    Some(out)
}

/// Bindings the march reads. Group 0 belongs to the raytracing scene, which
/// declares itself there, and which is why this is a compute pass: solari's
/// bindings are visible to compute shaders only.
fn march_layout(msaa: bool) -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        match msaa {
            true => "beam_march_layout_msaa",
            false => "beam_march_layout",
        },
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<ViewUniform>(true),
                uniform_buffer::<Volume>(false),
                storage_buffer_read_only::<Beam>(false),
                storage_buffer_read_only::<u32>(false),
                texture_2d_array(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                match msaa {
                    true => texture_depth_2d_multisampled(),
                    false => texture_depth_2d(),
                },
                texture_storage_2d(BEAM_FORMAT, StorageTextureAccess::WriteOnly),
                texture_2d(TextureSampleType::Uint),
            ),
        ),
    )
}

/// What the composite adds into the frame.
fn composite_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "beam_composite_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_2d(TextureSampleType::Float { filterable: true }),
            ),
        ),
    )
}

fn init_pipeline(
    mut cmds: Commands,
    device: Res<RenderDevice>,
    fullscreen: Res<FullscreenShader>,
    assets: Res<AssetServer>,
    scene: Option<Res<RaytracingSceneBindings>>,
) {
    if scene.is_none() {
        warn!("no raytracing support: beams will not be drawn");
    }
    cmds.insert_resource(BeamPipeline {
        scene: scene.map(|s| s.bind_group_layout.clone()),
        shader: assets.load("embedded://lib/gdtf/beam.wgsl"),
    });
    cmds.insert_resource(CompositePipeline {
        fullscreen: fullscreen.clone(),
        shader: assets.load("embedded://lib/gdtf/composite.wgsl"),
        sampler: device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            ..default()
        }),
    });
}

#[derive(Resource, Default)]
struct BeamFrame {
    volume: Volume,
    beams: Vec<Beam>,
    tiles: Vec<u32>,
    gobos: Handle<Image>,
}

#[derive(Resource, Default)]
struct BeamBuffers {
    volume: UniformBuffer<Volume>,
    beams: StorageBuffer<Vec<Beam>>,
    tiles: StorageBuffer<Vec<u32>>,
}

#[derive(Resource)]
struct BeamPipeline {
    /// The acceleration structure's own layout, absent when the gpu cannot trace.
    scene: Option<BindGroupLayoutDescriptor>,
    shader: Handle<Shader>,
}

#[derive(Resource)]
struct CompositePipeline {
    fullscreen: FullscreenShader,
    shader: Handle<Shader>,
    /// Smooths the march back up to the frame's resolution.
    sampler: Sampler,
}

/// The pass's pipelines for one view.
#[derive(Component)]
struct BeamPipelineIds {
    march: CachedComputePipelineId,
    surfaces: CachedComputePipelineId,
    composite: CachedRenderPipelineId,
}

/// Where the two compute passes write, before they are added into the frame.
/// The haze is marched coarse and filtered back up; what the beams land on is
/// shaded at full resolution, since a gobo thrown on a wall has edges.
#[derive(Component)]
struct BeamTexture {
    beams: CachedTexture,
    surfaces: CachedTexture,
}

/// One of the two jobs the beam shader does, for a view.
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
struct MarchKey {
    /// Whether the depth buffer this view reads is multisampled.
    msaa: bool,
    /// Lighting the surfaces the frame drew, rather than the haze between them.
    surfaces: bool,
}

impl SpecializedComputePipeline for BeamPipeline {
    type Key = MarchKey;

    fn specialize(&self, key: MarchKey) -> ComputePipelineDescriptor {
        let (label, entry) = match key.surfaces {
            true => ("beam_surfaces", "light_surfaces"),
            false => ("beam_march", "march_beams"),
        };
        ComputePipelineDescriptor {
            label: Some(label.into()),
            layout: vec![self.scene.clone().unwrap_or_default(), march_layout(key.msaa)],
            shader: self.shader.clone(),
            shader_defs: match key.msaa {
                true => vec!["MULTISAMPLED".into()],
                false => vec![],
            },
            entry_point: Some(entry.into()),
            ..default()
        }
    }
}

impl SpecializedRenderPipeline for CompositePipeline {
    type Key = BeamKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("beam_composite".into()),
            layout: vec![composite_layout()],
            vertex: self.fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs: vec![],
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: key.format,
                    // Haze adds to what is behind it without hiding any of it.
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::One,
                            dst_factor: BlendFactor::One,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent::REPLACE,
                    }),
                    write_mask: ColorWrites::COLOR,
                })],
            }),
            multisample: MultisampleState { count: key.samples, ..default() },
            ..default()
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone)]
struct BeamKey {
    format: TextureFormat,
    samples: u32,
}

fn extract_beams(mut frame: ResMut<BeamFrame>, volumetrics: Extract<Res<Volumetrics>>) {
    frame.volume = volumetrics.volume.clone();
    frame.beams.clear();
    frame.beams.extend_from_slice(&volumetrics.list);
    frame.tiles.clear();
    frame.tiles.extend_from_slice(&volumetrics.bins);
    frame.gobos = volumetrics.gobos.clone();
}

fn prepare_buffers(
    mut buffers: ResMut<BeamBuffers>,
    frame: Res<BeamFrame>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    buffers.volume.set(frame.volume.clone());
    // A runtime-sized array binding still has to be one element wide.
    buffers.beams.set(match frame.beams.is_empty() {
        true => vec![Beam::default()],
        false => frame.beams.clone(),
    });
    buffers.tiles.set(match frame.tiles.is_empty() {
        true => vec![0u32; 4],
        false => frame.tiles.clone(),
    });
    buffers.volume.write_buffer(&device, &queue);
    buffers.beams.write_buffer(&device, &queue);
    buffers.tiles.write_buffer(&device, &queue);
}

/// The march writes here first: a compute shader cannot blend into the frame.
const BEAM_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// How much coarser than the frame the beams are marched. Shafts are smooth
/// next to the geometry around them, and every pixel dropped is a pixel's worth
/// of occlusion rays not traced, which is what the pass actually costs.
pub const BEAM_SCALE: u32 = 2;

fn prepare_textures(
    mut cmds: Commands,
    mut textures: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ExtractedCamera)>,
) {
    for (entity, camera) in &views {
        let Some(size) = camera.physical_viewport_size else { continue };
        let mut gather = |label, scale: u32| {
            textures.get(
                &device,
                TextureDescriptor {
                    label: Some(label),
                    size: Extent3d {
                        width: size.x.div_ceil(scale).max(1),
                        height: size.y.div_ceil(scale).max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: BEAM_FORMAT,
                    usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            )
        };
        cmds.entity(entity).insert(BeamTexture {
            beams: gather("beams", BEAM_SCALE),
            surfaces: gather("beam_surfaces", 1),
        });
    }
}

fn prepare_pipeline(
    mut cmds: Commands,
    cache: Res<PipelineCache>,
    mut marches: ResMut<SpecializedComputePipelines<BeamPipeline>>,
    mut composites: ResMut<SpecializedRenderPipelines<CompositePipeline>>,
    march: Res<BeamPipeline>,
    composite: Res<CompositePipeline>,
    views: Query<(Entity, &ViewTarget, &Msaa)>,
) {
    for (entity, target, msaa) in &views {
        let key = BeamKey { format: target.main_texture_format(), samples: msaa.samples() };
        let job = |surfaces| MarchKey { msaa: msaa.samples() > 1, surfaces };
        cmds.entity(entity).insert(BeamPipelineIds {
            march: marches.specialize(&cache, &march, job(false)),
            surfaces: marches.specialize(&cache, &march, job(true)),
            composite: composites.specialize(&cache, &composite, key),
        });
    }
}

/// Pixels one workgroup covers on a side.
const WORKGROUP: u32 = 8;

fn beam_pass(
    view: ViewQuery<(
        &ViewTarget,
        &ViewUniformOffset,
        &ViewDepthTexture,
        &BeamTexture,
        &BeamPipelineIds,
        &Msaa,
        &ViewPrepassTextures,
    )>,
    cache: Res<PipelineCache>,
    composite: Res<CompositePipeline>,
    buffers: Res<BeamBuffers>,
    frame: Res<BeamFrame>,
    images: Res<RenderAssets<GpuImage>>,
    views: Res<ViewUniforms>,
    scene: Option<Res<RaytracingSceneBindings>>,
    mut ctx: RenderContext,
) {
    let (target, offset, depth, textures, pipelines, msaa, prepass) = view.into_inner();
    if frame.beams.is_empty() {
        return;
    }

    // Where the surfaces get their albedo and normal. Without it the fixtures
    // light nothing, since they no longer go through bevy's own lights.
    let Some(gbuffer) = prepass.deferred.as_ref().map(|t| &t.texture.default_view) else {
        return;
    };

    let (
        Some(march),
        Some(surfaces),
        Some(composite_pipeline),
        Some(view_binding),
        Some(gobos),
        Some(volume),
        Some(beams),
        Some(tiles),
    ) = (
        cache.get_compute_pipeline(pipelines.march),
        cache.get_compute_pipeline(pipelines.surfaces),
        cache.get_render_pipeline(pipelines.composite),
        views.uniforms.binding(),
        images.get(&frame.gobos),
        buffers.volume.binding(),
        buffers.beams.binding(),
        buffers.tiles.binding(),
    )
    else {
        return;
    };

    // Occlusion is the ray query, so there is nothing to draw until the
    // acceleration structure is up.
    let Some(scene) = scene.as_ref().and_then(|s| s.bind_group.as_ref()) else {
        return;
    };

    let device = ctx.render_device().clone();
    let layout = cache.get_bind_group_layout(&march_layout(msaa.samples() > 1));
    // The two passes differ only in where they write, so they differ only in
    // the one binding.
    let bind = |label, output: &CachedTexture| {
        device.create_bind_group(
            label,
            &layout,
            &BindGroupEntries::sequential((
                view_binding.clone(),
                volume.clone(),
                beams.clone(),
                tiles.clone(),
                &gobos.texture_view,
                &gobos.sampler,
                depth.view(),
                &output.default_view,
                gbuffer,
            )),
        )
    };
    let march_bind_group = bind("beam_march_bind_group", &textures.beams);
    let surface_bind_group = bind("beam_surfaces_bind_group", &textures.surfaces);
    let composite_bind_group = device.create_bind_group(
        "beam_composite_bind_group",
        &cache.get_bind_group_layout(&composite_layout()),
        &BindGroupEntries::sequential((
            &textures.beams.default_view,
            &composite.sampler,
            &textures.surfaces.default_view,
        )),
    );

    let encoder = ctx.command_encoder();
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("beam_march"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, scene, &[]);
        // Both textures are pooled between frames, so every pixel of each has
        // to be written or it shows whatever last used it.
        let size = textures.beams.texture.size();
        pass.set_pipeline(march);
        pass.set_bind_group(1, &march_bind_group, &[offset.offset]);
        pass.dispatch_workgroups(size.width.div_ceil(WORKGROUP), size.height.div_ceil(WORKGROUP), 1);

        let size = textures.surfaces.texture.size();
        pass.set_pipeline(surfaces);
        pass.set_bind_group(1, &surface_bind_group, &[offset.offset]);
        pass.dispatch_workgroups(size.width.div_ceil(WORKGROUP), size.height.div_ceil(WORKGROUP), 1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("beam_composite"),
            color_attachments: &[Some(target.get_color_attachment())],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(composite_pipeline);
        pass.set_bind_group(0, &composite_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

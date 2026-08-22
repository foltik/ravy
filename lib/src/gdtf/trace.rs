//! The acceleration structure the beams trace occlusion against.
//!
//! Solari's scene was here before, but its bindings carry every vertex, index
//! and material buffer through binding arrays, which metal has no support for.
//! The beam pass only ever asks whether a ray reaches the lens, so this holds
//! the structure alone: a BLAS per mesh, one TLAS over them, one binding. Ray
//! queries are the whole of what it needs, which metal has.

use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::render::mesh::RenderMesh;
use bevy::render::mesh::allocator::{MeshAllocator, MeshAllocatorSettings, allocate_and_free_meshes};
use bevy::render::render_asset::{ExtractedAssets, prepare_assets};
use bevy::render::render_resource::binding_types::acceleration_structure;
use bevy::render::render_resource::{
    AccelerationStructureFlags, AccelerationStructureGeometryFlags,
    AccelerationStructureUpdateMode, BindGroup, BindGroupEntries, BindGroupLayoutDescriptor,
    BindGroupLayoutEntries, Blas, BlasBuildEntry, BlasGeometries, BlasGeometrySizeDescriptors,
    BlasTriangleGeometry, BlasTriangleGeometrySizeDescriptor, BufferUsages,
    CommandEncoderDescriptor, CreateBlasDescriptor, CreateTlasDescriptor, IndexFormat,
    PipelineCache, ShaderStages, TlasInstance,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::settings::WgpuFeatures;
use bevy::render::sync_world::{RenderEntity, SyncToRenderWorld};
use bevy::render::{Extract, ExtractSchedule, GpuResourceAppExt, Render, RenderApp, RenderSystems};

use crate::prelude::*;

pub struct TraceScenePlugin;

impl Plugin for TraceScenePlugin {
    fn build(&self, app: &mut App) {
        bevy::shader::load_shader_library!(app, "trace.wgsl");
    }

    fn finish(&self, app: &mut App) {
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        let features = render.world().resource::<RenderDevice>().features();
        // Nothing is registered without the hardware, so the bindings resource
        // being present is itself the support check downstream.
        if !features.contains(WgpuFeatures::EXPERIMENTAL_RAY_QUERY) {
            return;
        }

        // The meshes' own buffers are what the BLASes are built from.
        render.world_mut().resource_mut::<MeshAllocatorSettings>().extra_buffer_usages |=
            BufferUsages::BLAS_INPUT;

        render
            .init_gpu_resource::<BlasManager>()
            .insert_resource(TraceSceneBindings::new())
            .add_systems(ExtractSchedule, (extract_meshes, extract_removals))
            .add_systems(
                Render,
                (
                    prepare_blas
                        .in_set(RenderSystems::PrepareAssets)
                        .before(prepare_assets::<RenderMesh>)
                        .after(allocate_and_free_meshes),
                    prepare_scene_bindings.in_set(RenderSystems::PrepareBindGroups),
                ),
            );
    }
}

/// Puts a mesh in the acceleration structure. The mesh has to be a triangle
/// list carrying exactly position, normal, uv and tangent, indexed 32 bit;
/// anything else is silently left out.
#[derive(Component, Clone, Deref)]
#[require(SyncToRenderWorld)]
pub struct TraceMesh(pub Handle<Mesh>);

/// The structure's bind group, group 0 of the beam pass. The group is empty
/// until something traceable lands, and the resource itself is absent when the
/// gpu cannot trace at all.
#[derive(Resource)]
pub struct TraceSceneBindings {
    pub bind_group: Option<BindGroup>,
    pub bind_group_layout: BindGroupLayoutDescriptor,
}

impl TraceSceneBindings {
    fn new() -> Self {
        Self {
            bind_group: None,
            bind_group_layout: BindGroupLayoutDescriptor::new(
                "trace_scene_layout",
                &BindGroupLayoutEntries::single(ShaderStages::COMPUTE, acceleration_structure()),
            ),
        }
    }
}

/// One BLAS per traceable mesh, dropped and rebuilt when the mesh changes.
#[derive(Resource, Default)]
struct BlasManager(HashMap<AssetId<Mesh>, Blas>);

fn extract_meshes(
    meshes: Extract<Query<(RenderEntity, &TraceMesh, &GlobalTransform)>>,
    mut cmds: Commands,
) {
    for (entity, mesh, transform) in &meshes {
        cmds.entity(entity).insert((mesh.clone(), *transform));
    }
}

/// The extract above only ever inserts, so a `TraceMesh` removed in the main
/// world — a part `NotShadowCaster` reached a frame after it spawned — would
/// stay in the structure forever, frozen at its last transform, silently
/// occluding every beam that crosses it.
fn extract_removals(
    mut removed: Extract<RemovedComponents<TraceMesh>>,
    entities: Extract<Query<&RenderEntity>>,
    mut cmds: Commands,
) {
    for entity in removed.read() {
        if let Ok(render) = entities.get(entity) {
            cmds.entity(render.id()).remove::<TraceMesh>();
        }
    }
}

/// Whether a mesh is in the shape [`prepare_blas`] builds from, which is the
/// shape `traceable` in beam.rs hands over.
fn traceable_shape(mesh: &Mesh) -> bool {
    mesh.enable_raytracing
        && mesh.primitive_topology() == PrimitiveTopology::TriangleList
        && matches!(mesh.indices(), Some(Indices::U32(_)))
        && mesh.attributes().map(|(a, _)| a.id).eq([
            Mesh::ATTRIBUTE_POSITION.id,
            Mesh::ATTRIBUTE_NORMAL.id,
            Mesh::ATTRIBUTE_UV_0.id,
            Mesh::ATTRIBUTE_TANGENT.id,
        ])
}

fn prepare_blas(
    mut manager: ResMut<BlasManager>,
    extracted: Res<ExtractedAssets<RenderMesh>>,
    allocator: Res<MeshAllocator>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    for id in extracted.removed.iter().chain(extracted.modified.iter()) {
        manager.0.remove(id);
    }
    if extracted.extracted.is_empty() {
        return;
    }

    let resources: Vec<_> = extracted
        .extracted
        .iter()
        .filter(|(_, mesh)| traceable_shape(mesh))
        .filter_map(|(id, _)| {
            let vertices = allocator.mesh_vertex_slice(id)?;
            let indices = allocator.mesh_index_slice(id)?;
            let size = BlasTriangleGeometrySizeDescriptor {
                vertex_format: Mesh::ATTRIBUTE_POSITION.format,
                vertex_count: vertices.range.len() as u32,
                index_format: Some(IndexFormat::Uint32),
                index_count: Some(indices.range.len() as u32),
                flags: AccelerationStructureGeometryFlags::OPAQUE,
            };
            let blas = device.wgpu_device().create_blas(
                &CreateBlasDescriptor {
                    label: Some(&id.to_string()),
                    flags: AccelerationStructureFlags::PREFER_FAST_TRACE,
                    update_mode: AccelerationStructureUpdateMode::Build,
                },
                BlasGeometrySizeDescriptors::Triangles { descriptors: vec![size.clone()] },
            );
            manager.0.insert(*id, blas);
            Some((*id, vertices, indices, size))
        })
        .collect();
    if resources.is_empty() {
        return;
    }

    let entries: Vec<_> = resources
        .iter()
        .map(|(id, vertices, indices, size)| BlasBuildEntry {
            blas: &manager.0[id],
            geometry: BlasGeometries::TriangleGeometries(vec![BlasTriangleGeometry {
                size,
                vertex_buffer: vertices.buffer,
                first_vertex: vertices.range.start,
                // Position, normal, uv and tangent, which the shape check pinned.
                vertex_stride: 48,
                index_buffer: Some(indices.buffer),
                first_index: Some(indices.range.start),
                transform_buffer: None,
                transform_buffer_offset: None,
            }]),
        })
        .collect();

    let mut encoder = device
        .create_command_encoder(&CommandEncoderDescriptor { label: Some("build_trace_blas") });
    encoder.build_acceleration_structures(&entries, &[]);
    queue.submit([encoder.finish()]);
}

/// Rebuilt whole every frame: the rig is a few dozen instances, and a rebuild
/// at that size is cheaper than tracking which of them moved.
fn prepare_scene_bindings(
    instances: Query<(&TraceMesh, &GlobalTransform)>,
    manager: Res<BlasManager>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
    mut bindings: ResMut<TraceSceneBindings>,
) {
    bindings.bind_group = None;
    if instances.is_empty() {
        return;
    }

    let mut tlas = device.wgpu_device().create_tlas(&CreateTlasDescriptor {
        label: Some("trace_scene_tlas"),
        flags: AccelerationStructureFlags::PREFER_FAST_TRACE,
        update_mode: AccelerationStructureUpdateMode::Build,
        max_instances: instances.iter().len() as u32,
    });

    let mut count = 0;
    for (mesh, transform) in &instances {
        let Some(blas) = manager.0.get(&mesh.id()) else { continue };
        let transform: [f32; 12] =
            transform.to_matrix().transpose().to_cols_array()[..12].try_into().unwrap();
        *tlas.get_mut_single(count).unwrap() =
            Some(TlasInstance::new(blas, transform, Default::default(), 0xFF));
        count += 1;
    }
    if count == 0 {
        return;
    }

    let mut encoder = device
        .create_command_encoder(&CommandEncoderDescriptor { label: Some("build_trace_tlas") });
    encoder.build_acceleration_structures(&[], [&tlas]);
    queue.submit([encoder.finish()]);

    bindings.bind_group = Some(device.create_bind_group(
        "trace_scene_bind_group",
        &cache.get_bind_group_layout(&bindings.bind_group_layout),
        &BindGroupEntries::single(tlas.as_binding()),
    ));
}

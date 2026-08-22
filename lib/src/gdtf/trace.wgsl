// The acceleration structure the beams trace occlusion against, and the one
// query ever made of it.

enable wgpu_ray_query;

#define_import_path ravy::trace

@group(0) @binding(0) var tlas: acceleration_structure;

const RAY_T_MIN = 0.001f;
const RAY_NO_CULL = 0xFFu;

fn trace_ray(ray_origin: vec3<f32>, ray_direction: vec3<f32>, ray_t_min: f32, ray_t_max: f32, ray_flag: u32) -> RayIntersection {
    let ray = RayDesc(ray_flag, RAY_NO_CULL, ray_t_min, ray_t_max, ray_origin, ray_direction);
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, ray);
    rayQueryProceed(&rq);
    return rayQueryGetCommittedIntersection(&rq);
}

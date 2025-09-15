use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;

use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::math::primitives::{ConicalFrustum, Cuboid, Cylinder, Sphere};
use egui_plot::{Line, Plot, PlotPoints}; // <- plotting
use lib::prelude::*; // assumes this exports bevy, bevy_egui, etc.

//
// ---------- Constants (fixture dims) ----------
//

const BASE_SIZE: Vec3 = Vec3::new(1.6, 0.25, 1.6);
const HEAD_LEN: f32 = 0.8;
const HEAD_RAD: f32 = 0.32;
const LENS_RAD: f32 = HEAD_RAD * 0.95;
const LENS_Z: f32 = HEAD_LEN * 0.6;
const FRUSTUM_LEN: f32 = 60.0;
const HEAD_Y: f32 = BASE_SIZE.y + 0.9;

//
// ---------- App wiring ----------
//

/// Tool for simulating different DMX fixtures
#[derive(argh::FromArgs)]
struct Args {
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() -> Result {
    let args: Args = argh::from_env();
    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, apply_pattern)
        .add_systems(
            Update,
            (
                spawn_fixture_if_needed,
                advance_test_queue,
                drive_motion,
                apply_pose_to_scene,
                send_dmx_if_enabled,
            ),
        )
        .add_systems(EguiPrimaryContextPass, draw_ui)
        .insert_resource(E131::new("10.16.4.1")?)
        .insert_resource(SimState::default())
        .insert_resource(TestQueue::default())
        .run();
    Ok(())
}

fn apply_pattern(mut sim: ResMut<SimState>, t: Res<Time>) {
    let t = t.elapsed_secs() / 4.0;

    let t_pitch = t.phase(1.0, 0.25).square(1.0, 0.5);
    let t_yaw = t.negsquare(1.0, 0.5);
    let pitch = 0.1 + 0.25 * t_pitch;
    let yaw = 0.5 + 0.08 * t_yaw;

    sim.target_pitch_norm = pitch;
    sim.target_yaw_norm = yaw - 0.25 / 1.5;
}

//
// ---------- Data model & helpers ----------
//

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FixtureKind {
    Beam,
}
impl FixtureKind {
    const ALL: &'static [FixtureKind] = &[FixtureKind::Beam];
    fn as_str(&self) -> &'static str {
        match self {
            FixtureKind::Beam => "Beam (Moving Head)",
        }
    }
}

#[derive(Clone, Copy)]
struct MotionParams {
    v_max: f32,            // deg/s (cruise limit)
    a_max: f32,            // deg/s^2 (used for BOTH accel & decel)
    j_max: f32,            // deg/s^3 (caps change in accel/vel per unit time)
    k_small: f32,          // (deg/s) per deg of initial delta (for small moves)
    small_thresh_deg: f32, // deg (<= uses linear-constant mode if not locked)
    snap_pos: f32,         // deg (position snap window)
    snap_vel: f32,         // deg/s (velocity snap window)
    // NEW: weaker braking while reversing (0..1). 1.0 = no reduction.
    reverse_brake_scale: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MoveMode {
    Linear,    // constant velocity based on *initial* delta (no damping)
    Trapezoid, // accel → cruise → decel (with jerk limit + reverse brake)
}

#[derive(Clone, Copy)]
struct AxisState {
    mode: MoveMode,
    lock_trapezoid: bool, // once set, stay in trapezoid until snap
    lin_v_cmd: f32,       // signed constant speed for LinearConst mode
    lin_initialized: bool,
    last_target_deg: f32, // to detect target changes while in LinearConst
}
impl Default for AxisState {
    fn default() -> Self {
        Self {
            mode: MoveMode::Linear,
            lock_trapezoid: false,
            lin_v_cmd: 0.0,
            lin_initialized: false,
            last_target_deg: f32::MAX,
        }
    }
}

#[derive(Resource)]
struct SimState {
    // UI state
    selected: FixtureKind,
    dmx_enabled: bool,
    // channels are 1-based in the UI to match DMX habits (you chose direct indices in send_dmx)
    yaw_ch: u16,          // 1..=512
    pitch_ch: u16,        // 1..=512
    dimmer_ch: u16,       // master dimmer channel (255 always)
    rgbw_start: u16,      // start of R,G,B,W consecutive block
    color_rgbw: [f32; 4], // 0..=1 (we'll use RGB only; W ignored)

    // Normalized motion targets (0..1)
    target_yaw_norm: f32,   // 0..1 (0 = forward, 1 = rotate end-start degrees)
    target_pitch_norm: f32, // 0..1 (0 = forward, 1 = up/back)

    // Mapping 0..1 → degrees (configurable)
    yaw_start_deg: f32,
    yaw_end_deg: f32,
    pitch_start_deg: f32,
    pitch_end_deg: f32,

    // Internal: current pose & velocities (deg/s)
    current_yaw_deg: f32,
    current_pitch_deg: f32,
    yaw_vel: f32,
    pitch_vel: f32,

    // Beam visuals
    beam_outer_deg: f32, // UI-controlled outer cone angle in degrees

    // Motion profile params (per axis)
    yaw_v_max: f32,
    yaw_a_max: f32,
    yaw_j_max: f32,
    yaw_k_small: f32,          // slope for small-move speed vs initial delta
    yaw_small_thresh_deg: f32, // threshold to choose linear vs trapezoid
    yaw_snap_pos: f32,
    yaw_snap_vel: f32,
    // NEW:
    yaw_reverse_brake_scale: f32, // 0..1

    pitch_v_max: f32,
    pitch_a_max: f32,
    pitch_j_max: f32,
    pitch_k_small: f32, // slope for small-move speed vs initial delta
    pitch_small_thresh_deg: f32,
    pitch_snap_pos: f32,
    pitch_snap_vel: f32,
    // NEW:
    pitch_reverse_brake_scale: f32, // 0..1

    // Per-axis commanded state (mode & lock)
    yaw_cmd: AxisState,
    pitch_cmd: AxisState,

    // Plot axis selection
    plot_yaw: bool,

    // Internal: spawned marker
    spawned_kind: Option<FixtureKind>,
}

impl Default for SimState {
    fn default() -> Self {
        Self {
            selected: FixtureKind::Beam,
            dmx_enabled: false,

            // >>> Your defaults <<<
            yaw_ch: 82,
            pitch_ch: 83,
            dimmer_ch: 85,
            rgbw_start: 87,

            color_rgbw: [1.0, 1.0, 1.0, 0.0], // white by default (W ignored/sent as 0)

            target_yaw_norm: 0.0,
            target_pitch_norm: 0.0,

            yaw_start_deg: 0.0,
            yaw_end_deg: 540.0,
            pitch_start_deg: 0.0,
            pitch_end_deg: 180.0,

            current_yaw_deg: 0.0,
            current_pitch_deg: 0.0,
            yaw_vel: 0.0,
            pitch_vel: 0.0,

            beam_outer_deg: 20.0,

            // Baseline params
            yaw_v_max: 300.0,
            yaw_a_max: 800.0,
            yaw_j_max: 50_000.0,
            yaw_k_small: 1.5,
            yaw_small_thresh_deg: 80.0,
            yaw_snap_pos: 0.5,
            yaw_snap_vel: 3.0,
            yaw_reverse_brake_scale: 1.4, // NEW (weaker braking while reversing)

            pitch_v_max: 320.0,
            pitch_a_max: 1600.0,
            pitch_j_max: 70_000.0,
            pitch_k_small: 5.0,
            pitch_small_thresh_deg: 15.0,
            pitch_snap_pos: 0.3,
            pitch_snap_vel: 2.0,
            pitch_reverse_brake_scale: 1.25, // NEW

            yaw_cmd: AxisState::default(),
            pitch_cmd: AxisState::default(),

            plot_yaw: true,

            spawned_kind: None,
        }
    }
}

// Helpers
#[inline]
fn map01_to_deg(n: f32, start: f32, end: f32) -> f32 {
    start + n.clamp(0.0, 1.0) * (end - start)
}
#[inline]
fn dmx_byte_clamped(v: f32) -> u8 {
    v.clamp(0.0, 255.0).round() as u8
}

// Trapezoid step (a_max for accel & decel) + jerk limit + reverse brake + clean snap
fn step_trapezoid(pos: f32, vel: f32, target: f32, p: MotionParams, dt: f32) -> (f32, f32, bool) {
    let d0 = target - pos;
    if d0.abs() <= p.snap_pos && vel.abs() <= p.snap_vel {
        return (target, 0.0, true);
    }

    let mut x = pos;
    let mut v = vel;

    // Distance & direction to target
    let d = target - x;
    let ad = d.abs();
    let dir = if ad > 0.0 { d.signum() } else { 0.0 };

    // Stop-distance logic
    let a_full = p.a_max.max(1e-3);
    let stop_dist = v.abs() * v.abs() / (2.0 * a_full);
    let must_brake = stop_dist >= ad;
    let mut a_target = if must_brake { -dir * a_full } else { dir * a_full };

    // NEW: weaker braking while reversing (mimics inertia & driver limits)
    if v * dir < 0.0 {
        let s = p.reverse_brake_scale.clamp(0.0, 1.0);
        a_target = a_target.clamp(-s * a_full, s * a_full);
    }

    // --- JERK LIMIT (stateless approximation) ---
    // dv_wish = a_target * dt ; cap |dv| <= j_max * dt^2
    let dv_wish = a_target * dt;
    let dv_cap = p.j_max * dt * dt;
    let dv = dv_wish.clamp(-dv_cap, dv_cap);
    v += dv;

    // Clamp to v_max
    v = v.clamp(-p.v_max, p.v_max);

    // Also ensure we can still stop in time with remaining distance
    let v_allow = (2.0 * a_full * ad).sqrt();
    if v.abs() > v_allow {
        v = v.signum() * v_allow;
    }

    // Integrate position
    let x_next = x + v * dt;

    // Crossed the target? snap
    if (target - x).signum() != (target - x_next).signum() {
        return (target, 0.0, true);
    }

    x = x_next;

    // Final snap window
    if (target - x).abs() <= p.snap_pos && v.abs() <= p.snap_vel {
        return (target, 0.0, true);
    }

    (x, v, false)
}

// Linear-constant step for small moves: constant v_cmd (no end damping), snap on overshoot or snap_pos
fn step_linear_const(
    pos: f32,
    v_cmd: f32, // signed constant speed decided at mode entry (or when target changes)
    target: f32,
    snap_pos: f32,
    dt: f32,
) -> (f32, f32, bool) {
    let d = target - pos;
    if d.abs() <= snap_pos {
        return (target, 0.0, true);
    }

    let x_next = pos + v_cmd * dt;

    // overshoot detection
    if (target - pos).signum() != (target - x_next).signum() {
        return (target, 0.0, true);
    }

    (x_next, v_cmd, false)
}

//
// ---------- Test queue (for button sequences) ----------
//

#[derive(Clone, Copy)]
enum AxisKind {
    Yaw,
    Pitch,
}

#[derive(Clone, Copy)]
struct SeqStep {
    delay: f32,       // seconds remaining until apply
    axis: AxisKind,   // which axis
    target_norm: f32, // 0..1
}

#[derive(Resource, Default)]
struct TestQueue {
    steps: VecDeque<SeqStep>,
}

fn push_step(q: &mut TestQueue, delay: f32, axis: AxisKind, target_norm: f32) {
    q.steps
        .push_back(SeqStep { delay, axis, target_norm: target_norm.clamp(0.0, 1.0) });
}

fn advance_test_queue(time: Res<Time>, mut q: ResMut<TestQueue>, mut state: ResMut<SimState>) {
    if let Some(mut step) = q.steps.pop_front() {
        let dt = time.delta_secs();
        if step.delay > dt {
            step.delay -= dt;
            q.steps.push_front(step);
        } else {
            match step.axis {
                AxisKind::Yaw => state.target_yaw_norm = step.target_norm,
                AxisKind::Pitch => state.target_pitch_norm = step.target_norm,
            }
        }
    }
}

//
// ---------- Scene setup ----------
//

#[derive(Component)]
struct BeamRoot;
#[derive(Component)]
struct BeamHead;
#[derive(Component)]
struct BeamSpot;
#[derive(Component)]
struct BeamLensMat(Handle<StandardMaterial>);
#[derive(Component)]
struct BeamFrustum;
#[derive(Component)]
struct BeamFrustumMesh(Handle<Mesh>);
#[derive(Component)]
struct BeamFrustumMat(Handle<StandardMaterial>);

fn setup(mut commands: Commands) {
    // Camera with a nice angle & HDR + Bloom (no fog)
    commands.spawn((
        Camera3d::default(),
        Camera { hdr: true, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        Bloom::NATURAL,
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
        Transform::from_xyz(8.0, 6.0, 12.0).looking_at(Vec3::new(0.0, 4.0, 0.0), Vec3::Y),
    ));

    commands.insert_resource(AmbientLight { color: Color::WHITE, brightness: 200.0, ..Default::default() });
}

//
// ---------- Fixture spawning ----------
//

fn spawn_fixture_if_needed(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<SimState>,
    q_existing: Query<Entity, With<BeamRoot>>,
) {
    if state.spawned_kind == Some(state.selected) {
        return;
    }

    for e in &q_existing {
        commands.entity(e).despawn();
    }

    match state.selected {
        FixtureKind::Beam => {
            let matte = mats.add(StandardMaterial {
                base_color: Color::srgb(0.06, 0.06, 0.07),
                perceptual_roughness: 0.9,
                metallic: 0.15,
                ..default()
            });

            // Base
            let base = commands
                .spawn((
                    BeamRoot,
                    Visibility::default(),
                    Transform::from_xyz(0.0, BASE_SIZE.y * 0.5, 0.0),
                    Mesh3d(meshes.add(Mesh::from(Cuboid::from_size(BASE_SIZE)))),
                    MeshMaterial3d(matte.clone()),
                ))
                .id();

            // Head pivot
            let head = commands
                .spawn((BeamHead, Transform::from_xyz(0.0, HEAD_Y, 0.0), Visibility::default()))
                .id();
            commands.entity(base).add_child(head);

            // Head visuals
            commands.entity(head).with_children(|hc| {
                // Cylinder housing points along +Z: rotate +90° about +X
                hc.spawn((
                    Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)).with_translation(Vec3::new(
                        0.0,
                        0.0,
                        HEAD_LEN * 0.25,
                    )),
                    Mesh3d(meshes.add(Mesh::from(Cylinder::new(HEAD_RAD * 2.0, HEAD_LEN)))),
                    MeshMaterial3d(matte.clone()),
                ));

                // Emissive lens
                let lens_mat = mats.add(StandardMaterial {
                    base_color: Color::srgb(0.2, 0.2, 0.2),
                    emissive: Color::WHITE.into(),
                    emissive_exposure_weight: 0.0,
                    ..default()
                });
                hc.spawn((
                    BeamLensMat(lens_mat.clone()),
                    Transform::from_translation(Vec3::new(0.0, 0.0, LENS_Z)),
                    Mesh3d(meshes.add(Mesh::from(Sphere { radius: LENS_RAD }))),
                    MeshMaterial3d(lens_mat),
                ));

                // Real spotlight at the lens, facing +Z
                hc.spawn((
                    BeamSpot,
                    Transform::from_translation(Vec3::new(0.0, 0.0, LENS_Z)).looking_to(Vec3::Z, Vec3::Y),
                    SpotLight {
                        intensity: 300_000.0,
                        color: Color::WHITE,
                        range: FRUSTUM_LEN,
                        inner_angle: 0.6,
                        outer_angle: 20.0_f32.to_radians(),
                        radius: 0.015,
                        shadows_enabled: true,
                        ..default()
                    },
                ));

                // Additive FRUSTUM beam (tip at lens, expands forward)
                let outer = 20.0_f32.clamp(2.0, 60.0).to_radians();
                let end_r = (LENS_RAD + FRUSTUM_LEN * (outer * 0.5).max(0.0001).tan()).max(LENS_RAD + 0.001);

                let beam_mat = mats.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 1.0, 1.0, 0.18),
                    unlit: true,
                    alpha_mode: AlphaMode::Add,
                    ..default()
                });

                let fr_mesh = meshes.add(Mesh::from(ConicalFrustum {
                    height: FRUSTUM_LEN,
                    radius_top: LENS_RAD, // near end = lens side
                    radius_bottom: end_r, // far end
                    ..Default::default()
                }));

                // Aim +Y → +Z and flip about center; center at LENS_Z - len/2
                hc.spawn((
                    BeamFrustum,
                    BeamFrustumMesh(fr_mesh.clone()),
                    BeamFrustumMat(beam_mat.clone()),
                    Transform::from_xyz(0.0, 0.0, LENS_Z - FRUSTUM_LEN * 0.5).with_rotation(
                        Quat::from_rotation_y(std::f32::consts::PI) * // flip about center
                            Quat::from_rotation_x(FRAC_PI_2), // +Y → +Z
                    ),
                    Mesh3d(fr_mesh),
                    MeshMaterial3d(beam_mat),
                ));
            });

            state.spawned_kind = Some(FixtureKind::Beam);
        }
    }
}

//
// ---------- Motion model & animation ----------
//

fn drive_motion(time: Res<Time>, mut state: ResMut<SimState>) {
    let dt = time.delta_secs();

    // Map normalized 0..1 sliders to degrees
    let tgt_yaw_deg = map01_to_deg(state.target_yaw_norm, state.yaw_start_deg, state.yaw_end_deg);
    let tgt_pitch_deg = map01_to_deg(state.target_pitch_norm, state.pitch_start_deg, state.pitch_end_deg);

    // Build per-axis params (raw)
    let yaw_p = MotionParams {
        v_max: state.yaw_v_max,
        a_max: state.yaw_a_max,
        j_max: state.yaw_j_max,
        k_small: state.yaw_k_small,
        small_thresh_deg: state.yaw_small_thresh_deg,
        snap_pos: state.yaw_snap_pos,
        snap_vel: state.yaw_snap_vel,
        reverse_brake_scale: state.yaw_reverse_brake_scale, // NEW
    };
    let pitch_p = MotionParams {
        v_max: state.pitch_v_max,
        a_max: state.pitch_a_max,
        j_max: state.pitch_j_max,
        k_small: state.pitch_k_small,
        small_thresh_deg: state.pitch_small_thresh_deg,
        snap_pos: state.pitch_snap_pos,
        snap_vel: state.pitch_snap_vel,
        reverse_brake_scale: state.pitch_reverse_brake_scale, // NEW
    };

    // ---- Yaw: choose mode from CURRENT delta; if big, lock trapezoid until snap
    let yaw_delta_now = (tgt_yaw_deg - state.current_yaw_deg).abs();
    if state.yaw_cmd.lock_trapezoid {
        state.yaw_cmd.mode = MoveMode::Trapezoid;
    } else if yaw_delta_now <= yaw_p.small_thresh_deg {
        // Enter or continue LinearConst. Only set v_cmd on entry or target change.
        if !state.yaw_cmd.lin_initialized
            || (state.yaw_cmd.last_target_deg - tgt_yaw_deg).abs() > f32::EPSILON
        {
            let v_mag = (yaw_p.k_small * yaw_delta_now).min(yaw_p.v_max);
            let dir = if yaw_delta_now > 0.0 {
                (tgt_yaw_deg - state.current_yaw_deg).signum()
            } else {
                0.0
            };
            state.yaw_cmd.lin_v_cmd = dir * v_mag;
            state.yaw_cmd.lin_initialized = true;
            state.yaw_cmd.last_target_deg = tgt_yaw_deg;
        }
        state.yaw_cmd.mode = MoveMode::Linear;
    } else {
        state.yaw_cmd.lock_trapezoid = true;
        state.yaw_cmd.mode = MoveMode::Trapezoid;
    }

    let (ny, nvy_tmp, yaw_snapped) = match state.yaw_cmd.mode {
        MoveMode::Linear => {
            step_linear_const(state.current_yaw_deg, state.yaw_cmd.lin_v_cmd, tgt_yaw_deg, yaw_p.snap_pos, dt)
        }
        MoveMode::Trapezoid => step_trapezoid(state.current_yaw_deg, state.yaw_vel, tgt_yaw_deg, yaw_p, dt),
    };
    if yaw_snapped && matches!(state.yaw_cmd.mode, MoveMode::Trapezoid) {
        state.yaw_cmd.lock_trapezoid = false; // unlock for next command
        state.yaw_cmd.lin_initialized = false; // force recompute next time we go LinearConst
    }
    let nvy = if matches!(state.yaw_cmd.mode, MoveMode::Linear) {
        state.yaw_cmd.lin_v_cmd
    } else {
        nvy_tmp
    };

    // ---- Pitch: same logic
    let pitch_delta_now = (tgt_pitch_deg - state.current_pitch_deg).abs();
    if state.pitch_cmd.lock_trapezoid {
        state.pitch_cmd.mode = MoveMode::Trapezoid;
    } else if pitch_delta_now <= pitch_p.small_thresh_deg {
        if !state.pitch_cmd.lin_initialized
            || (state.pitch_cmd.last_target_deg - tgt_pitch_deg).abs() > f32::EPSILON
        {
            let v_mag = (pitch_p.k_small * pitch_delta_now).min(pitch_p.v_max);
            let dir = if pitch_delta_now > 0.0 {
                (tgt_pitch_deg - state.current_pitch_deg).signum()
            } else {
                0.0
            };
            state.pitch_cmd.lin_v_cmd = dir * v_mag;
            state.pitch_cmd.lin_initialized = true;
            state.pitch_cmd.last_target_deg = tgt_pitch_deg;
        }
        state.pitch_cmd.mode = MoveMode::Linear;
    } else {
        state.pitch_cmd.lock_trapezoid = true;
        state.pitch_cmd.mode = MoveMode::Trapezoid;
    }

    let (np, nvp_tmp, pitch_snapped) = match state.pitch_cmd.mode {
        MoveMode::Linear => step_linear_const(
            state.current_pitch_deg,
            state.pitch_cmd.lin_v_cmd,
            tgt_pitch_deg,
            pitch_p.snap_pos,
            dt,
        ),
        MoveMode::Trapezoid => {
            step_trapezoid(state.current_pitch_deg, state.pitch_vel, tgt_pitch_deg, pitch_p, dt)
        }
    };
    if pitch_snapped && matches!(state.pitch_cmd.mode, MoveMode::Trapezoid) {
        state.pitch_cmd.lock_trapezoid = false;
        state.pitch_cmd.lin_initialized = false;
    }
    let nvp = if matches!(state.pitch_cmd.mode, MoveMode::Linear) {
        state.pitch_cmd.lin_v_cmd
    } else {
        nvp_tmp
    };

    state.current_yaw_deg = ny;
    state.current_pitch_deg = np;
    state.yaw_vel = nvy;
    state.pitch_vel = nvp;
}

fn apply_pose_to_scene(
    mut sets: ParamSet<(
        Query<&mut Transform, With<BeamRoot>>,
        Query<&mut Transform, With<BeamHead>>,
        Query<&mut SpotLight, With<BeamSpot>>,
        Query<(&BeamFrustumMesh, &BeamFrustumMat, &mut Transform), With<BeamFrustum>>,
    )>,
    lens_q: Query<&BeamLensMat>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    state: Res<SimState>,
) {
    // Root (yaw around +Y)
    if let Ok(mut t_root) = sets.p0().single_mut() {
        t_root.rotation = Quat::from_rotation_y(-state.current_yaw_deg.to_radians());
    }

    // Head (pitch: 0 = forward, + = up/back → rotate negative around +X)
    if let Ok(mut t_head) = sets.p1().single_mut() {
        t_head.rotation = Quat::from_rotation_x(-state.current_pitch_deg.to_radians());
    }

    // Spotlight angles & color
    let outer = state.beam_outer_deg.clamp(2.0, 60.0).to_radians();
    let eps = 0.5_f32.to_radians();
    let inner = (outer * 0.6).min(outer - eps).max(0.0);

    if let Ok(mut spot) = sets.p2().single_mut() {
        let [r, g, b, _w_unused] = state.color_rgbw;
        spot.color = Color::srgb(r, g, b);
        spot.outer_angle = outer;
        spot.inner_angle = inner;
        spot.range = FRUSTUM_LEN;
    }

    // Lens emissive tint (bloom)
    if let Ok(BeamLensMat(h)) = lens_q.single() {
        if let Some(m) = mats.get_mut(h) {
            let [r, g, b, _w_unused] = state.color_rgbw;
            m.emissive = Color::srgb(r * 6.0, g * 6.0, b * 6.0).into();
        }
    }

    // Frustum: recolor + rebuild mesh if needed; keep center where we want it
    if let Ok((BeamFrustumMesh(mesh_h), BeamFrustumMat(mat_h), mut t)) = sets.p3().single_mut() {
        if let Some(m) = mats.get_mut(mat_h) {
            let [r, g, b, _w_unused] = state.color_rgbw;
            m.base_color = Color::srgba(r, g, b, 0.18);
        }

        if let Some(m) = meshes.get_mut(mesh_h) {
            let end_r = (LENS_RAD + FRUSTUM_LEN * (outer * 0.5).max(0.0001).tan()).max(LENS_RAD + 0.001);
            *m = Mesh::from(ConicalFrustum {
                height: FRUSTUM_LEN,
                radius_top: LENS_RAD,
                radius_bottom: end_r,
                ..Default::default()
            });
        }

        // Keep midpoint so the tip sits at the lens (with our spawn rotation)
        t.translation.z = LENS_Z + FRUSTUM_LEN * 0.5;
    }
}

//
// ---------- DMX output ----------
//

fn send_dmx_if_enabled(mut e131: ResMut<E131>, state: Res<SimState>) {
    if !state.dmx_enabled {
        return;
    }

    let mut universe = [0u8; 512];

    // Use normalized sliders directly for DMX (0..1 → 0..255)
    let yaw_idx = state.yaw_ch as usize;
    let pitch_idx = state.pitch_ch as usize;
    if yaw_idx < 512 {
        universe[yaw_idx] = dmx_byte_clamped(state.target_yaw_norm * 255.0);
    }
    if pitch_idx < 512 {
        universe[pitch_idx] = dmx_byte_clamped(state.target_pitch_norm * 255.0);
    }

    // Master dimmer at full (255)
    let dim_idx = state.dimmer_ch as usize;
    if dim_idx < 512 {
        universe[dim_idx] = 255;
    }

    // RGB only (W ignored/sent as 0)
    let base = state.rgbw_start as usize;
    let [r, g, b, _w_unused] = state.color_rgbw;
    let mut push = |i: usize, v: f32| {
        if i < 512 {
            universe[i] = dmx_byte_clamped(v * 255.0);
        }
    };
    push(base + 0, r); // R
    push(base + 1, g); // G
    push(base + 2, b); // B
    if base + 3 < 512 {
        universe[base + 3] = 0; // W forced to 0
    }

    let _ = e131.send(&mut universe);
}

//
// ---------- UI ----------
//

fn draw_ui(mut egui_ctx: EguiContexts, mut state: ResMut<SimState>, mut tests: ResMut<TestQueue>) -> Result {
    let ctx = egui_ctx.ctx_mut()?;
    egui::Window::new("DMX Fixture Simulator").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Fixture:");
            egui::ComboBox::from_label("")
                .selected_text(state.selected.as_str())
                .show_ui(ui, |cb| {
                    for k in FixtureKind::ALL {
                        cb.selectable_value(&mut state.selected, *k, k.as_str());
                    }
                });
        });

        ui.separator();

        ui.heading("Motion (normalized)");
        ui.horizontal(|ui| {
            ui.label("Pitch (0..1):");
            ui.add(egui::Slider::new(&mut state.target_pitch_norm, 0.0..=1.0));
            ui.label("Yaw (0..1):");
            ui.add(egui::Slider::new(&mut state.target_yaw_norm, 0.0..=1.0));
        });

        ui.collapsing("Angle mapping (0..1 → degrees)", |ui| {
            ui.horizontal(|ui| {
                ui.label("Pitch start");
                ui.add(egui::DragValue::new(&mut state.pitch_start_deg).speed(0.5));
                ui.label("Pitch end");
                ui.add(egui::DragValue::new(&mut state.pitch_end_deg).speed(0.5));
            });
            ui.horizontal(|ui| {
                ui.label("Yaw start");
                ui.add(egui::DragValue::new(&mut state.yaw_start_deg).speed(0.5));
                ui.label("Yaw end");
                ui.add(egui::DragValue::new(&mut state.yaw_end_deg).speed(0.5));
            });
            ui.label("Ref: 0 pitch = forward; 1 = up/back. 0 yaw = forward; 1 = rotate end-start degrees.");
        });

        ui.separator();

        ui.heading("Motion profile — RAW params (deg/s, deg/s², deg/s³)");
        ui.columns(2, |cols| {
            // Pitch column
            cols[0].label("Pitch (linear-constant for small moves; trapezoid for large)");
            cols[0].horizontal(|ui| {
                ui.label("v_max");
                ui.add(egui::DragValue::new(&mut state.pitch_v_max).range(10.0..=5000.0).speed(10.0));
                ui.label("a_max");
                ui.add(egui::DragValue::new(&mut state.pitch_a_max).range(10.0..=20000.0).speed(50.0));
                ui.label("j_max");
                ui.add(
                    egui::DragValue::new(&mut state.pitch_j_max)
                        .range(1_000.0..=100_000.0)
                        .speed(50.0),
                );
            });
            cols[0].horizontal(|ui| {
                ui.label("reverse_brake");
                ui.add(
                    egui::DragValue::new(&mut state.pitch_reverse_brake_scale)
                        .range(0.2..=5.0)
                        .speed(0.01),
                );
            });
            cols[0].horizontal(|ui| {
                ui.label("k_small");
                ui.add(egui::DragValue::new(&mut state.pitch_k_small).range(0.01..=50.0).speed(0.1));
            });
            cols[0].horizontal(|ui| {
                ui.label("small_thresh (°)");
                ui.add(
                    egui::DragValue::new(&mut state.pitch_small_thresh_deg)
                        .range(0.0..=90.0)
                        .speed(1.0),
                );
                ui.label("snap_pos");
                ui.add(egui::DragValue::new(&mut state.pitch_snap_pos).range(0.0..=5.0).speed(0.05));
                ui.label("snap_vel");
                ui.add(egui::DragValue::new(&mut state.pitch_snap_vel).range(0.0..=20.0).speed(0.1));
            });

            // Yaw column
            cols[1].label("Yaw (linear-constant for small moves; trapezoid for large)");
            cols[1].horizontal(|ui| {
                ui.label("v_max");
                ui.add(egui::DragValue::new(&mut state.yaw_v_max).range(10.0..=5000.0).speed(10.0));
                ui.label("a_max");
                ui.add(egui::DragValue::new(&mut state.yaw_a_max).range(10.0..=20000.0).speed(50.0));
                ui.label("j_max");
                ui.add(
                    egui::DragValue::new(&mut state.yaw_j_max)
                        .range(1_000.0..=100_000.0)
                        .speed(50.0),
                );
            });
            cols[1].horizontal(|ui| {
                ui.label("reverse_brake");
                ui.add(
                    egui::DragValue::new(&mut state.yaw_reverse_brake_scale)
                        .range(0.2..=5.0)
                        .speed(0.01),
                );
            });
            cols[1].horizontal(|ui| {
                ui.label("k_small");
                ui.add(egui::DragValue::new(&mut state.yaw_k_small).range(0.01..=50.0).speed(0.1));
            });
            cols[1].horizontal(|ui| {
                ui.label("small_thresh (°)");
                ui.add(
                    egui::DragValue::new(&mut state.yaw_small_thresh_deg)
                        .range(0.0..=180.0)
                        .speed(1.0),
                );
                ui.label("snap_pos");
                ui.add(egui::DragValue::new(&mut state.yaw_snap_pos).range(0.0..=5.0).speed(0.05));
                ui.label("snap_vel");
                ui.add(egui::DragValue::new(&mut state.yaw_snap_vel).range(0.0..=20.0).speed(0.1));
            });
        });

        ui.separator();

        // -------- Profile plot (0 -> limit) ----------
        ui.heading("Profile preview (synthetic 0 → limit)");
        ui.horizontal(|ui| {
            ui.radio_value(&mut state.plot_yaw, true, "Plot Yaw");
            ui.radio_value(&mut state.plot_yaw, false, "Plot Pitch");
        });

        let (p, target_deg) = if state.plot_yaw {
            (
                MotionParams {
                    v_max: state.yaw_v_max,
                    a_max: state.yaw_a_max,
                    j_max: state.yaw_j_max,
                    k_small: state.yaw_k_small,
                    small_thresh_deg: state.yaw_small_thresh_deg,
                    snap_pos: state.yaw_snap_pos,
                    snap_vel: state.yaw_snap_vel,
                    reverse_brake_scale: state.yaw_reverse_brake_scale,
                },
                (state.yaw_end_deg - state.yaw_start_deg).abs().max(1.0),
            )
        } else {
            (
                MotionParams {
                    v_max: state.pitch_v_max,
                    a_max: state.pitch_a_max,
                    j_max: state.pitch_j_max,
                    k_small: state.pitch_k_small,
                    small_thresh_deg: state.pitch_small_thresh_deg,
                    snap_pos: state.pitch_snap_pos,
                    snap_vel: state.pitch_snap_vel,
                    reverse_brake_scale: state.pitch_reverse_brake_scale,
                },
                (state.pitch_end_deg - state.pitch_start_deg).abs().max(1.0),
            )
        };

        // simulate 0→target using trapezoid (no reversal here, just shows jerk ramp)
        let mut t = 0.0f32;
        let dt = 1.0 / 240.0;
        let mut x = 0.0f32;
        let mut v = 0.0f32;

        let mut pts_pos: Vec<[f64; 2]> = Vec::with_capacity(4096);
        let mut pts_vel: Vec<[f64; 2]> = Vec::with_capacity(4096);
        let mut pts_acc: Vec<[f64; 2]> = Vec::with_capacity(4096);
        let mut pts_jerk: Vec<[f64; 2]> = Vec::with_capacity(4096);

        let mut a_prev = 0.0f32;

        for _ in 0..20_000 {
            // emulate a single step but also compute effective a & jerk
            // re-run the internal bits from step_trapezoid to derive dv (accel estimate)
            let d = target_deg - x;
            let ad = d.abs();
            let dir = if ad > 0.0 { d.signum() } else { 0.0 };

            let a_full = p.a_max.max(1e-3);
            let stop_dist = v.abs() * v.abs() / (2.0 * a_full);
            let must_brake = stop_dist >= ad;
            let mut a_target = if must_brake { -dir * a_full } else { dir * a_full };

            // reverse brake wouldn't trigger in this forward-only sim (v*dir ≥ 0), but keep for completeness:
            if v * dir < 0.0 {
                let s = p.reverse_brake_scale.clamp(0.0, 1.0);
                a_target = a_target.clamp(-s * a_full, s * a_full);
            }

            let dv_wish = a_target * dt;
            let dv_cap = p.j_max * dt * dt;
            let dv = dv_wish.clamp(-dv_cap, dv_cap);

            let v_next = (v + dv).clamp(-p.v_max, p.v_max);

            let v_allow = (2.0 * a_full * ad).sqrt();
            let v_next = if v_next.abs() > v_allow { v_next.signum() * v_allow } else { v_next };

            let x_next = x + v_next * dt;

            // effective acceleration & jerk
            let a_eff = (v_next - v) / dt;
            let j_eff = (a_eff - a_prev) / dt;
            a_prev = a_eff;

            pts_pos.push([t as f64, x as f64]);
            pts_vel.push([t as f64, v as f64]);
            pts_acc.push([t as f64, a_eff as f64]);
            pts_jerk.push([t as f64, j_eff as f64]);

            x = x_next;
            v = v_next;
            t += dt;

            if (target_deg - x).abs() <= p.snap_pos && v.abs() <= p.snap_vel {
                // push last sample at the snap
                pts_pos.push([t as f64, target_deg as f64]);
                pts_vel.push([t as f64, 0.0]);
                pts_acc.push([t as f64, 0.0]);
                pts_jerk.push([t as f64, 0.0]);
                break;
            }

            if t > 10.0 {
                break; // safety
            }
        }

        let line_pos = Line::new("pos (deg)", PlotPoints::from(pts_pos));
        let line_vel = Line::new("vel (deg/s)", PlotPoints::from(pts_vel));
        let line_acc = Line::new("acc (deg/s²)", PlotPoints::from(pts_acc));
        let line_jerk = Line::new("jerk (deg/s³)", PlotPoints::from(pts_jerk));

        Plot::new("motion_plot")
            .height(220.0)
            .legend(egui_plot::Legend::default())
            // .allow_zoom(Vec2 { x: true.into(), y: true.into() })
            // .allow_scroll(Vec2 { x: true.into(), y: true.into() })
            .show(ui, |plot_ui| {
                plot_ui.line(line_pos);
                plot_ui.line(line_vel);
                plot_ui.line(line_acc);
                plot_ui.line(line_jerk);
            });

        ui.separator();

        ui.heading("Beam");
        ui.add(egui::Slider::new(&mut state.beam_outer_deg, 2.0..=60.0).text("Outer angle (°)"));

        ui.separator();

        ui.heading("Color (RGB only)");
        let mut rgb = [
            (state.color_rgbw[0] * 255.0) as u8,
            (state.color_rgbw[1] * 255.0) as u8,
            (state.color_rgbw[2] * 255.0) as u8,
        ];
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            state.color_rgbw[0] = rgb[0] as f32 / 255.0;
            state.color_rgbw[1] = rgb[1] as f32 / 255.0;
            state.color_rgbw[2] = rgb[2] as f32 / 255.0;
            state.color_rgbw[3] = 0.0; // ensure W is ignored
        }

        ui.separator();

        // ------------ Test buttons -------------
        let yaw_deg_range = (state.yaw_end_deg - state.yaw_start_deg).abs().max(1.0);
        let pitch_deg_range = (state.pitch_end_deg - state.pitch_start_deg).abs().max(1.0);
        let yaw_small_norm = (state.yaw_small_thresh_deg / yaw_deg_range).clamp(0.0, 1.0);
        let pitch_small_norm = (state.pitch_small_thresh_deg / pitch_deg_range).clamp(0.0, 1.0);

        ui.heading("Test Cases — Yaw");
        ui.horizontal(|ui| {
            if ui.button("to 0").clicked() {
                state.target_yaw_norm = 0.0;
            }
            if ui.button("to 1").clicked() {
                state.target_yaw_norm = 1.0;
            }

            if ui.button("+ small*0.25").clicked() {
                state.target_yaw_norm = (state.target_yaw_norm + 0.25 * yaw_small_norm).clamp(0.0, 1.0);
            }
            if ui.button("+ small*0.5").clicked() {
                state.target_yaw_norm = (state.target_yaw_norm + 0.5 * yaw_small_norm).clamp(0.0, 1.0);
            }
            if ui.button("+ small*0.75").clicked() {
                state.target_yaw_norm = (state.target_yaw_norm + 0.75 * yaw_small_norm).clamp(0.0, 1.0);
            }

            if ui.button("+ small").clicked() {
                state.target_yaw_norm = (state.target_yaw_norm + yaw_small_norm).clamp(0.0, 1.0);
            }
            if ui.button("+ small ×1.1").clicked() {
                state.target_yaw_norm = (state.target_yaw_norm + 1.1 * yaw_small_norm).clamp(0.0, 1.0);
            }

            if ui.button("0 → 0.8").clicked() {
                state.target_yaw_norm = 0.0;
                state.target_yaw_norm = 0.8;
            }
            if ui.button("0 → 0.8 → 0.8+small").clicked() {
                state.target_yaw_norm = 0.0;
                push_step(&mut tests, 0.05, AxisKind::Yaw, 0.8);
                push_step(&mut tests, 0.10, AxisKind::Yaw, (0.8 + yaw_small_norm).clamp(0.0, 1.0));
            }
        });

        ui.heading("Test Cases — Pitch");
        ui.horizontal(|ui| {
            if ui.button("to 0").clicked() {
                state.target_pitch_norm = 0.0;
            }
            if ui.button("to 1").clicked() {
                state.target_pitch_norm = 1.0;
            }

            if ui.button("+ small*0.25").clicked() {
                state.target_pitch_norm = (state.target_pitch_norm + 0.25 * pitch_small_norm).clamp(0.0, 1.0);
            }
            if ui.button("+ small*0.5").clicked() {
                state.target_pitch_norm = (state.target_pitch_norm + 0.5 * pitch_small_norm).clamp(0.0, 1.0);
            }
            if ui.button("+ small*0.75").clicked() {
                state.target_pitch_norm = (state.target_pitch_norm + 0.75 * pitch_small_norm).clamp(0.0, 1.0);
            }

            if ui.button("+ small").clicked() {
                state.target_pitch_norm = (state.target_pitch_norm + pitch_small_norm).clamp(0.0, 1.0);
            }
            if ui.button("+ small ×1.1").clicked() {
                state.target_pitch_norm = (state.target_pitch_norm + 1.1 * pitch_small_norm).clamp(0.0, 1.0);
            }

            if ui.button("0 → 0.8").clicked() {
                state.target_pitch_norm = 0.0;
                state.target_pitch_norm = 0.8;
            }
            if ui.button("0 → 0.8 → 0.8+small").clicked() {
                state.target_pitch_norm = 0.0;
                push_step(&mut tests, 0.05, AxisKind::Pitch, 0.8);
                push_step(&mut tests, 0.10, AxisKind::Pitch, (0.8 + pitch_small_norm).clamp(0.0, 1.0));
            }
        });

        ui.separator();

        ui.heading("DMX / E1.31");
        ui.checkbox(&mut state.dmx_enabled, "Enable DMX sending (E1.31)");
        ui.horizontal(|ui| {
            ui.label("Yaw ch:");
            ui.add(egui::DragValue::new(&mut state.yaw_ch).range(1..=512));
            ui.label("Pitch ch:");
            ui.add(egui::DragValue::new(&mut state.pitch_ch).range(1..=512));
            ui.label("Dimmer ch:");
            ui.add(egui::DragValue::new(&mut state.dimmer_ch).range(1..=512));
        });
        ui.horizontal(|ui| {
            ui.label("RGBW start ch:");
            ui.add(egui::DragValue::new(&mut state.rgbw_start).range(1..=509));
            ui.label("(uses 4 consecutive slots; W forced to 0)");
        });
    });
    Ok(())
}

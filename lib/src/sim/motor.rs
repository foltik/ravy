use crate::prelude::*;

pub struct MotorDynamics {
    /// Minimum angle in rad.
    θ_min: f32,
    /// Maximum angle in rad.
    θ_max: f32,

    /// Maximum velocity in rad/s
    v_max: f32,
    /// Maximum acceleration in rad/s^2
    a_max: f32,
    /// Maximum jerk in rad/s^3
    j_max: f32,

    /// Threshold in rad below which motion curve becomes linear
    linear_threshold: f32,
    /// Speed in rad/s per rad for linear motions
    linear_gain: f32,
}

#[derive(Component)]
pub struct Motor {
    axis: Axis,
    dynamics: MotorDynamics,
    motion: Option<Motion>,

    /// angle (rad)
    pub θ: f32,
    /// velocity (rad/s)
    pub v: f32,
    /// acceleration (rad/s^2)
    pub a: f32,
    /// jerk (rad/s^3) — stored as ±j_max (or 0 at steady accel)
    pub j: f32,
}

enum Motion {
    /// Constant velocity rotation
    Linear { θ: f32 },
    /// Jerk-limited S-curve rotation
    SCurve { θ: f32 },
}

impl Motor {
    pub fn new(axis: Axis, dynamics: MotorDynamics) -> Self {
        Self { axis, dynamics, motion: None, θ: 0.0, v: 0.0, a: 0.0, j: 0.0 }
    }

    /// Set a new target rotation in 0..1
    pub fn rotate(&mut self, fr: f32) {
        let MotorDynamics { θ_min, θ_max, v_max, linear_threshold, linear_gain, .. } = self.dynamics;

        let θ = fr.clamp(0.0, 1.0).lerp(θ_min..θ_max);

        let dist = θ - self.θ;
        if dist.abs() <= linear_threshold {
            self.v = (linear_gain * dist).clamp(-v_max, v_max);
            self.motion = Some(Motion::Linear { θ });
        } else {
            self.motion = Some(Motion::SCurve { θ });
        }
    }

    /// Step the motor simulation, returning the current angle.
    pub fn step(&mut self, dt: f32) -> f32 {
        /// Angle error in rad under which to snap to the target
        const SNAP_ANGLE: f32 = 0.005;

        let MotorDynamics { v_max, a_max, j_max, .. } = self.dynamics;

        match self.motion {
            Some(Motion::Linear { θ }) => {
                // Integrate angle.
                let dist_abs = (θ - self.θ).abs();
                let dθ = self.v * dt;
                self.θ += dθ;
                self.a = 0.0;
                self.j = 0.0;

                let snap = dist_abs <= SNAP_ANGLE;
                let overshoot = dθ.abs() > dist_abs;
                if snap || overshoot {
                    self.stop(θ);
                }
            }
            Some(Motion::SCurve { θ }) => {
                let dist = θ - self.θ;
                let dist_abs = dist.abs();
                let dir = if dist != 0.0 { dist.signum() } else { 0.0 };

                // Integrate acceleration.
                // - If we can stop in time accelerate at a_max, otherwise decelerate at -a_max
                // - Limit jerk to j_max
                let stop_dist = self.v.powi(2) / (2.0 * a_max);
                let must_brake = stop_dist >= dist_abs;
                let a = if must_brake { -dir * a_max } else { dir * a_max };
                let da = a - self.a;
                self.j = (da / dt).clamp(-j_max, j_max);
                self.a = (self.a + self.j * dt).clamp(-a_max, a_max);

                // Integrate velocity.
                // - Limit velocity such that it's still possible to stop within the remaining dist
                // - Limit velocity to v_max
                let v_stop = (2.0 * a_max * dist_abs).sqrt();
                let v_max = v_max.min(v_stop);
                self.v = (self.v + self.a * dt).clamp(-v_max, v_max);

                // Integrate angle.
                let dθ = self.v * dt;
                self.θ += dθ;

                let snap = dist_abs <= SNAP_ANGLE;
                let overshoot = dθ.abs() > dist_abs;
                if snap || overshoot {
                    self.stop(θ);
                }
            }
            _ => {}
        }

        self.θ
    }

    fn stop(&mut self, θ: f32) {
        self.θ = θ;
        self.v = 0.0;
        self.a = 0.0;
        self.j = 0.0;
        self.motion = None;
    }
}

fn simulate_motors(mut motors: Query<(&mut Motor, &mut Transform)>, time: Res<Time>) {
    let dt = time.delta_secs();

    for (mut motor, mut transform) in motors.iter_mut() {
        let θ = motor.step(dt);

        let (mut x, mut y, mut z) = transform.rotation.to_euler(EulerRot::XYZ);
        match motor.axis {
            Axis::X => x = θ,
            Axis::Y => y = θ,
            Axis::Z => z = θ,
        }
        transform.rotation = Quat::from_euler(EulerRot::XYZ, x, y, z);
    }
}

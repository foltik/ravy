use crate::prelude::*;

#[derive(Reflect, Component)]
pub struct Motor {
    axis: Axis,
    dynamics: MotorDynamics,
    motion: Option<Motion>,

    /// angle (deg)
    pub θ: f32,
    /// velocity (deg/s)
    pub v: f32,
    /// acceleration (deg/s^2)
    pub a: f32,
    /// jerk (deg/s^3) — stored as ±j_max (or 0 at steady accel)
    pub j: f32,
}

#[derive(Reflect, Clone, Copy)]
pub struct MotorDynamics {
    /// Minimum angle in deg.
    pub θ_min: f32,
    /// Maximum angle in deg.
    pub θ_max: f32,

    /// Maximum velocity in deg/s
    pub v_max: f32,
    /// Maximum acceleration in deg/s^2
    pub a_max: f32,
    /// Maximum jerk in deg/s^3
    pub j_max: f32,

    /// Threshold in deg below which motion curve becomes linear
    pub linear_threshold: f32,
    /// Speed in deg/s per deg for linear motions
    pub linear_gain: f32,
}

#[derive(Reflect, Debug)]
enum Motion {
    /// Constant velocity rotation
    Linear { θ_start: f32, θ_end: f32 },
    /// Jerk-limited S-curve rotation
    SCurve { θ: f32 },
}

impl Motor {
    pub fn new(axis: Axis, dynamics: MotorDynamics) -> Self {
        Self { axis, dynamics, motion: None, θ: 0.0, v: 0.0, a: 0.0, j: 0.0 }
    }

    /// Set a new target rotation in 0..1
    pub fn rotate(&mut self, fr: f32) {
        let MotorDynamics { θ_min, θ_max, linear_threshold, .. } = self.dynamics;

        let θ = fr.clamp(0.0, 1.0).lerp(θ_min..θ_max);

        let dist = θ - self.θ;
        if dist.abs() <= linear_threshold {
            self.motion = Some(match self.motion {
                // New linear motion
                None => Motion::Linear { θ_start: self.θ, θ_end: θ },
                // New target, compute velocity
                Some(Motion::Linear { θ_end: old_θ_end, .. }) if θ != old_θ_end => {
                    Motion::Linear { θ_start: self.θ, θ_end: θ }
                }
                // Same Linear target, don't change anything
                Some(Motion::Linear { θ_start, θ_end }) => Motion::Linear { θ_start, θ_end },
                // Stay in SCurve until it's finished even if θ is below the threshold
                Some(Motion::SCurve { .. }) => Motion::SCurve { θ },
            });
        } else {
            // SCurve recomputes continuously, just adjust to the new target
            self.motion = Some(Motion::SCurve { θ });
        }
    }

    /// Step the motor simulation, returning the current angle.
    pub fn step(&mut self, dt: f32) -> f32 {
        /// Angle error in deg under which to snap to the target
        const SNAP_ANGLE: f32 = 0.005;
        let MotorDynamics { v_max, a_max, j_max, linear_gain, .. } = self.dynamics;

        match self.motion {
            Some(Motion::Linear { θ_start, θ_end }) => {
                // Calculate the constant velocity to use
                let orig_dist_abs = (θ_end - θ_start).abs();
                self.j = 0.0;
                self.a = 0.0;
                self.v = (linear_gain * orig_dist_abs).clamp(-v_max, v_max);

                // Integrate angle
                let dist_abs = (θ_end - self.θ).abs();
                let dθ = self.v * dt;
                self.θ += dθ;

                // Check when we've arrived or have overshot
                let snap = dist_abs <= SNAP_ANGLE;
                let overshoot = dθ.abs() > dist_abs;
                if snap || overshoot {
                    self.stop(θ_end);
                }
            }
            Some(Motion::SCurve { θ }) => {
                let dist = θ - self.θ;
                let dist_abs = dist.abs();
                let dir = if dist != 0.0 { dist.signum() } else { 0.0 };

                // Integrate acceleration
                // - If we can stop in time accelerate at a_max, otherwise decelerate at -a_max
                // - Limit jerk to j_max
                let stop_dist = self.v.powi(2) / (2.0 * a_max);
                let must_brake = stop_dist >= dist_abs;
                let a = if must_brake { -dir * a_max } else { dir * a_max };
                let da = a - self.a;
                self.j = (da / dt).clamp(-j_max, j_max);
                self.a = (self.a + self.j * dt).clamp(-a_max, a_max);

                // Integrate velocity
                // - Limit velocity such that it's still possible to stop within the remaining dist
                // - Limit velocity to v_max
                let v_stop = (2.0 * a_max * dist_abs).sqrt();
                let v_lim = v_max.min(v_stop);
                self.v = (self.v + self.a * dt).clamp(-v_lim, v_lim);

                // Integrate angle
                let dθ = self.v * dt;
                self.θ += dθ;

                // Check when we've arrived or have overshot
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

#[derive(Reflect, Component)]
pub struct MotorZero(Quat);

/// Record zero positions
pub fn zero(mut cmds: Commands, motors: Query<(Entity, &Transform), (With<Motor>, Without<MotorZero>)>) {
    for (entity, transform) in motors {
        cmds.entity(entity).insert(MotorZero(transform.rotation));
    }
}

/// Step the simulation
pub fn simulate(mut motors: Query<(&mut Motor, &MotorZero, &mut Transform)>, time: Res<Time>) {
    let dt = time.delta_secs();

    for (mut motor, zero, mut transform) in motors.iter_mut() {
        let θ = motor.step(dt).to_radians();
        let rotation = match motor.axis {
            Axis::X => Quat::from_rotation_x(θ),
            Axis::Y => Quat::from_rotation_y(θ),
            Axis::Z => Quat::from_rotation_z(θ),
        };
        transform.rotation = zero.0 * rotation;
    }
}

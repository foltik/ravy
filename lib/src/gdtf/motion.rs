//! Pan, tilt and zoom travel, rather than teleporting to whatever the DMX says.
//!
//! A yoke ramps up, cruises, and brakes, and it takes a different path for a
//! nudge than for a full sweep: short moves run at a slow constant speed with no
//! ramp at all, which is why a fixture creeps through small corrections and
//! slams through big ones. Ported from dmxsim.

use std::collections::VecDeque;

use egui_plot::{HLine, Legend, Line, LineStyle, Plot, PlotPoints};
use crate::prelude::*;

/// Per-axis limits, in degrees: of rotation for pan and tilt, of field angle
/// for zoom.
#[derive(Clone, Copy)]
pub struct Params {
    /// Cruise speed. 0 is unlimited.
    pub v_max: f32,
    /// Acceleration, used for both ramping up and braking. 0 is unlimited.
    pub a_max: f32,
    /// Cap on how fast the acceleration itself may change. 0 is unlimited.
    pub j_max: f32,
    /// Speed per degree of travel, for moves too short to ramp.
    pub k_small: f32,
    /// Travel below which the axis runs at a constant speed instead.
    pub small: f32,
    /// Position window inside which the axis counts as arrived.
    pub snap_pos: f32,
    /// Speed below which it is allowed to count as arrived.
    pub snap_vel: f32,
    /// Fraction of the brake available while still travelling the wrong way.
    pub reverse_brake: f32,
    /// Time constant in seconds of the firmware's input smoothing, easing the
    /// latched packets before the profiler chases them. 0 disables.
    pub smooth: f32,
}

impl Params {
    pub fn pan() -> Self {
        Self {
            v_max: 300.0,
            a_max: 800.0,
            j_max: 50_000.0,
            k_small: 1.5,
            small: 80.0,
            snap_pos: 0.5,
            snap_vel: 3.0,
            reverse_brake: 1.0,
            smooth: 0.0,
        }
    }

    pub fn tilt() -> Self {
        Self {
            v_max: 320.0,
            a_max: 1600.0,
            j_max: 70_000.0,
            k_small: 5.0,
            small: 15.0,
            snap_pos: 0.3,
            snap_vel: 2.0,
            reverse_brake: 1.0,
            smooth: 0.0,
        }
    }

    /// Zoom carriage. A lead screw, so it ramps for any move rather than
    /// creeping, and it reaches its speed fast enough that neither ramp is
    /// worth a figure. A fixture with its own measurement overrides this
    /// through [`crate::gdtf::GdtfDevice`].
    pub fn zoom() -> Self {
        Self {
            v_max: 100.0,
            a_max: 0.0,
            j_max: 0.0,
            k_small: 1.5,
            small: 0.0,
            snap_pos: 0.2,
            snap_vel: 1.0,
            reverse_brake: 1.0,
            smooth: 0.0,
        }
    }

    /// Replace the unlimited entries with rates that finish inside one frame,
    /// which is as instant as a frame-stepped profile gets. Each falls out of
    /// the limit above it: cross the whole gap, reach cruise, or command the
    /// full acceleration, in a single step.
    fn resolve(mut self, gap: f32, dt: f32) -> Self {
        if self.v_max <= 0.0 {
            self.v_max = (gap.abs() / dt).max(1e-3);
        }
        if self.a_max <= 0.0 {
            self.a_max = self.v_max / dt;
        }
        if self.j_max <= 0.0 {
            self.j_max = self.a_max / dt;
        }
        self
    }
}

/// Live state of one axis.
#[derive(Default)]
pub struct Axis {
    /// Where the axis is now, in degrees.
    pub pos: f32,
    /// How fast it is going, in degrees per second.
    pub vel: f32,
    /// Acceleration the last step actually applied, for the plot.
    pub acc: f32,
    /// Jerk the last step actually applied, for the plot.
    pub jerk: f32,
    /// Target and speed of a short move, fixed when the move was given.
    glide: Option<(f32, f32)>,
    /// Brake latch. Hysteresis, so the decision cannot chatter frame to frame
    /// and spray the acceleration with sign flips.
    decel: bool,
    /// A long move stays ramped until it lands, rather than dropping into the
    /// short-move path as the gap closes.
    ramping: bool,
    /// Cleared until the first target arrives, so a fixture starts where it is
    /// aimed instead of sweeping in from zero.
    homed: bool,
}

impl Axis {
    /// Advance one frame towards `target`, and report where the axis now is.
    pub fn step(&mut self, target: f32, p: Params, dt: f32) -> f32 {
        if !self.homed || dt <= 0.0 {
            self.homed = true;
            self.pos = target;
            (self.vel, self.acc, self.jerk) = (0.0, 0.0, 0.0);
            return self.pos;
        }
        let p = p.resolve(target - self.pos, dt);

        self.ramping |= (target - self.pos).abs() > p.small;
        if self.ramping {
            if self.ramp(target, p, dt) {
                self.ramping = false;
                self.glide = None;
            }
            return self.pos;
        }

        // Speed comes from the gap at the moment the move was given and is then
        // held, so a nudge crawls the whole way instead of easing out.
        let speed = match self.glide {
            Some((was, speed)) if was == target => speed,
            _ => {
                let gap = target - self.pos;
                let speed = gap.signum() * (p.k_small * gap.abs()).min(p.v_max);
                self.glide = Some((target, speed));
                speed
            }
        };
        let next = self.pos + speed * dt;
        let landed = (target - self.pos).abs() <= p.snap_pos
            || (target - self.pos).signum() != (target - next).signum();
        match landed {
            true => self.land(target),
            // A creep is quasi-static: constant speed, nothing to accelerate.
            false => (self.pos, self.vel, self.acc, self.jerk) = (next, speed, 0.0, 0.0),
        }
        self.pos
    }

    /// Accelerate, cruise and brake. True once the axis lands.
    fn ramp(&mut self, target: f32, p: Params, dt: f32) -> bool {
        let gap = target - self.pos;
        if gap.abs() <= p.snap_pos && self.vel.abs() <= p.snap_vel {
            self.land(target);
            return true;
        }

        let (dir, a_max) = (gap.signum(), p.a_max.max(1e-3));
        // Brake once the distance left is all the distance needed to stop in,
        // plus what slewing the acceleration over will cover. Latched with a
        // hysteresis band: right on the boundary the raw comparison flips
        // every frame.
        let swing = (self.acc * dir + a_max) / p.j_max.max(1.0);
        let stop = self.vel * self.vel / (2.0 * a_max) + self.vel.abs() * swing * 0.5;
        if stop >= gap.abs() {
            self.decel = true;
        } else if stop < 0.7 * gap.abs() {
            self.decel = false;
        }

        let mut wanted = match self.decel {
            // Exactly the deceleration that lands on the target, so the brake
            // is one smooth curve rather than a rail plus a per-frame speed
            // clamp whose corrections read as noise.
            true => -dir * (self.vel * self.vel / (2.0 * gap.abs().max(1e-4))).min(2.0 * a_max),
            false => dir * a_max,
        };
        // Nothing left to gain once the cruise speed is reached.
        if !self.decel && self.vel * dir >= p.v_max {
            wanted = 0.0;
        }
        // Already travelling the wrong way, so the brake has inertia to fight.
        if self.vel * dir < 0.0 {
            let limit = p.reverse_brake.clamp(0.0, 1.0) * a_max;
            wanted = wanted.clamp(-limit, limit);
        }

        // Acceleration is state, slewed toward the command at j_max. The plot
        // reads these variables directly; nothing is differentiated back out
        // of the velocity.
        self.jerk = ((wanted - self.acc) / dt).clamp(-p.j_max, p.j_max);
        self.acc += self.jerk * dt;

        let unclamped = self.vel + self.acc * dt;
        self.vel = unclamped.clamp(-p.v_max, p.v_max);
        if self.vel != unclamped {
            // The clamp did the work; keep the state telling the truth.
            self.acc = (self.vel - unclamped + self.acc * dt) / dt;
        }

        let next = self.pos + self.vel * dt;
        if (target - self.pos).signum() != (target - next).signum() {
            self.land(target);
            return true;
        }
        self.pos = next;
        let landed = (target - self.pos).abs() <= p.snap_pos && self.vel.abs() <= p.snap_vel;
        if landed {
            self.land(target);
        }
        landed
    }

    fn land(&mut self, target: f32) {
        (self.pos, self.vel, self.acc, self.jerk) = (target, 0.0, 0.0, 0.0);
        self.decel = false;
    }
}

/// Seconds of history the realtime plot keeps, and the widest it can show.
const KEEP: f64 = 30.0;

/// Interval between DMX packets on the wire, ~44 a second.
const PACKET: f64 = 1.0 / 44.0;

/// Seconds the plot keeps rolling after the axis settles, so the landing ends
/// up on screen rather than at the very edge.
const HOLD: f64 = 1.0;

/// How far past its limit a normalized series may show before clipping, so a
/// snap spike cannot crush the rest of the chart.
const CLIP: f32 = 1.5;

/// One motion sample, timed on the trace's own clock.
struct Sample {
    t: f64,
    dmx: f32,
    pos: f32,
    vel: f32,
    acc: f32,
    jerk: f32,
}

/// The input stage of a fixture: packets latch the target at the wire rate,
/// and optional firmware smoothing eases the steps before the profiler
/// chases them. This is what the sim's axis actually gets to see, which is
/// why a scrubbed slider desyncs like the real head does.
#[derive(Default)]
pub struct Command {
    /// Value the last packet latched.
    latched: f32,
    /// Real time `latched` last changed.
    stamped: f64,
    /// Smoothing state chasing `latched`; what the axis chases in turn.
    smoothed: f32,
    /// Smoothing velocity, for the critically damped chase.
    rate: f32,
    started: bool,
}

impl Command {
    /// Advance the input stage and return the target to hand the axis.
    pub fn step(&mut self, now: f64, target: f32, smooth: f32, dt: f32) -> f32 {
        if !self.started {
            (self.started, self.latched, self.smoothed, self.stamped) = (true, target, target, now);
        }
        // A fresh change lands instantly; a stream of changes is paced to the
        // packet rate.
        if target != self.latched && now - self.stamped >= PACKET {
            (self.latched, self.stamped) = (target, now);
        }
        if smooth <= 0.0 || dt <= 0.0 {
            (self.smoothed, self.rate) = (self.latched, 0.0);
            return self.smoothed;
        }
        // Critically damped chase: eases in and out, never overshoots. The
        // frequency is capped against the frame time so the integration
        // cannot blow up at tiny time constants.
        let w = 1.0 / smooth.max(2.0 * dt);
        self.rate += (w * w * (self.latched - self.smoothed) - 2.0 * w * self.rate) * dt;
        self.smoothed += self.rate * dt;
        // Snap once converged, so downstream equality checks can settle too.
        if (self.latched - self.smoothed).abs() < 1e-3 && self.rate.abs() < 1e-2 {
            (self.smoothed, self.rate) = (self.latched, 0.0);
        }
        self.smoothed
    }

    /// What the wire is carrying, for display.
    pub fn wire(&self) -> f32 {
        self.latched
    }
}

/// Record of the move one axis is making, feeding the realtime plot.
#[derive(Default)]
pub struct Trace {
    /// Samples, oldest first. Cleared when a new move starts after a settle,
    /// so the buffer is only ever the move being made.
    samples: VecDeque<Sample>,
    /// Real time of the previous push.
    real: f64,
    /// Real time the axis last had anywhere to be but where it is.
    active: f64,
    /// Plot clock: runs while the axis moves, pauses once it settles so a
    /// landed move stays on screen for study instead of scrolling away.
    clock: f64,
    started: bool,
}

impl Trace {
    /// Record a frame: `dmx` is the value on the wire, already paced.
    pub fn push(&mut self, now: f64, dmx: f32, axis: &Axis) {
        if !self.started {
            (self.started, self.real) = (true, now);
        }
        if axis.vel != 0.0 || axis.pos != dmx {
            // A move begun after a settle starts a fresh trace.
            if now - self.active > HOLD {
                self.samples.clear();
                self.clock = 0.0;
            }
            self.active = now;
        }
        let dt = (now - self.real) as f32;
        self.real = now;
        if dt <= 0.0 || now - self.active > HOLD {
            return;
        }
        self.clock += dt as f64;

        self.samples.push_back(Sample {
            t: self.clock,
            dmx,
            pos: axis.pos,
            vel: axis.vel,
            acc: axis.acc,
            jerk: axis.jerk,
        });
        while self.samples.front().is_some_and(|s| s.t < self.clock - KEEP) {
            self.samples.pop_front();
        }
    }

    /// One sampled value against seconds-ago on the trace's clock, thinned to
    /// roughly `budget` points. A chart a few hundred pixels wide holds half a
    /// minute of frames, and tessellating every one of them is what makes the
    /// whole app stutter. Each bucket keeps its extremes, so a one-frame spike
    /// still shows.
    /// Largest magnitude the trace holds, for scaling a curve with no limit.
    fn peak(&self, pick: impl Fn(&Sample) -> f32) -> f32 {
        self.samples.iter().map(|s| pick(s).abs()).fold(0.0, f32::max)
    }

    fn line(&self, name: &str, budget: usize, pick: impl Fn(&Sample) -> f32) -> Line<'static> {
        let stride = self.samples.len().div_ceil(budget.max(1)).max(1);
        let at = |s: &Sample| [s.t - self.clock, pick(s) as f64];
        let mut points: Vec<[f64; 2]> = Vec::with_capacity(2 * self.samples.len().div_ceil(stride));
        for start in (0..self.samples.len()).step_by(stride) {
            let end = (start + stride).min(self.samples.len());
            let mut window = self.samples.range(start..end).map(&at);
            let Some(first) = window.next() else { continue };
            let (mut low, mut high) = (first, first);
            for point in window {
                if point[1] < low[1] {
                    low = point;
                }
                if point[1] > high[1] {
                    high = point;
                }
            }
            // Back into time order, so the trace still reads left to right.
            let (low, high) = match low[0] <= high[0] {
                true => (low, high),
                false => (high, low),
            };
            points.push(low);
            if high != low {
                points.push(high);
            }
        }
        Line::new(name.to_owned(), PlotPoints::from(points))
    }
}

/// Which series the charts show, and how far back they may look.
pub struct View {
    pub vel: bool,
    pub accel: bool,
    pub jerk: bool,
    /// Cap on the seconds on screen. The x axis fits the current move and the
    /// right edge stays locked to now; this only zooms in closer.
    pub window: f64,
}

impl Default for View {
    fn default() -> Self {
        Self { vel: true, accel: true, jerk: true, window: KEEP }
    }
}

/// Gray guides: a solid zero line, dotted lines at the normalized limits.
fn guide(y: f64) -> HLine {
    let style = match y == 0.0 {
        true => LineStyle::Solid,
        false => LineStyle::dotted_dense(),
    };
    HLine::new(String::new(), y)
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60)))
        .style(style)
}

/// Realtime scope, one chart per axis. Everything is normalized onto one y
/// scale: position against the move's biggest excursion, the derivatives
/// against their own limits, so 1 is "at the limit" and the dotted guides sit
/// there. The x axis fits the move being made, right edge locked to now.
pub fn plot(ui: &mut egui::Ui, view: &mut View, axes: &[(&str, &Trace, Params)]) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut view.vel, "Vel");
        ui.checkbox(&mut view.accel, "Accel");
        ui.checkbox(&mut view.jerk, "Jerk");
        ui.separator();
        ui.spacing_mut().slider_width = 160.0;
        ui.add(
            egui::Slider::new(&mut view.window, 1.0..=KEEP)
                .logarithmic(true)
                .text("window (s)"),
        );
    });

    // Charts split whatever height is left, so resizing the window scales them.
    let height = (ui.available_height() / axes.len().max(1) as f32 - 30.0).max(150.0);
    // Two points per pixel is as much detail as a chart can show.
    let budget = (ui.available_width() as usize).max(64) * 2;
    for (name, trace, p) in axes {
        ui.strong(*name);
        let norm = trace
            .samples
            .iter()
            .map(|s| s.dmx.abs().max(s.pos.abs()))
            .fold(1e-3f32, f32::max);
        Plot::new(format!("motion_{name}"))
            .height(height)
            .legend(Legend::default())
            .show_grid(false)
            .show_axes(false)
            .show_x(false)
            .show_y(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_drag(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .include_y(1.05)
            .include_y(-1.05)
            .show(ui, |plot| {
                // Fit the whole move, unless the slider zooms in closer.
                let span = trace.samples.front().map_or(0.5, |s| trace.clock - s.t).max(0.5);
                plot.set_plot_bounds_x(-span.min(view.window)..=0.0);
                for y in [0.0, 1.0, -1.0] {
                    plot.hline(guide(y));
                }
                // A couple of pixels in plot units, to stagger the derivative
                // lines so exactly overlapping curves all stay visible.
                let eps = 2.1 / height * 2.0;
                let clip = |v: f32| v.clamp(-CLIP, CLIP);
                // An unlimited axis has no rail to measure against, so its own
                // peak stands in and the shape still reads.
                let scale = |limit: f32, pick: fn(&Sample) -> f32| match limit > 0.0 {
                    true => limit,
                    false => trace.peak(pick).max(1e-6),
                };
                let (vel, accel, jerk) = (
                    scale(p.v_max, |s| s.vel),
                    scale(p.a_max, |s| s.acc),
                    scale(p.j_max, |s| s.jerk),
                );
                let fade = |c: egui::Color32| c.gamma_multiply(0.7);
                plot.line(trace.line("DMX", budget, |s| s.dmx / norm).color(fade(egui::Color32::GRAY)));
                plot.line(trace.line("Pos", budget, |s| s.pos / norm).color(fade(egui::Color32::GREEN)));
                if view.vel {
                    plot.line(
                        trace
                            .line("Vel", budget, |s| clip(s.vel / vel))
                            .color(fade(egui::Color32::YELLOW)),
                    );
                }
                if view.accel {
                    plot.line(
                        trace
                            .line("Accel", budget, |s| clip(s.acc / accel) + eps)
                            .color(fade(egui::Color32::ORANGE)),
                    );
                }
                if view.jerk {
                    plot.line(
                        trace
                            .line("Jerk", budget, |s| clip(s.jerk / jerk) + 2.0 * eps)
                            .color(fade(egui::Color32::RED)),
                    );
                }
            });
    }
}

/// Tuning section for one axis: a labelled column per limit. `unit` is what
/// the axis travels in, "°" or "%".
pub fn tune(ui: &mut egui::Ui, name: &str, unit: &str, p: &mut Params) {
    ui.strong(name);
    let labels = [
        format!("speed ({unit}/s)"),
        format!("accel ({unit}/s²)"),
        format!("jerk ({unit}/s³)"),
        format!("creep ({unit}/s per {unit})"),
        format!("creep below ({unit})"),
        format!("arrive ({unit})"),
        format!("arrive ({unit}/s)"),
        "reverse brake".into(),
        "smooth (s)".into(),
    ];
    let fields = [
        // Each starts at 0, which is unlimited.
        (&mut p.v_max, 0.0..=5000.0, 10.0),
        (&mut p.a_max, 0.0..=20000.0, 50.0),
        (&mut p.j_max, 0.0..=500_000.0, 500.0),
        (&mut p.k_small, 0.01..=50.0, 0.1),
        (&mut p.small, 0.0..=180.0, 1.0),
        (&mut p.snap_pos, 0.0..=5.0, 0.05),
        (&mut p.snap_vel, 0.0..=20.0, 0.1),
        (&mut p.reverse_brake, 0.0..=1.0, 0.05),
        (&mut p.smooth, 0.0..=0.5, 0.005),
    ];
    egui::Grid::new(format!("tune_{name}"))
        .num_columns(fields.len())
        .show(ui, |ui| {
            for label in &labels {
                ui.label(label);
            }
            ui.end_row();
            for (value, range, speed) in fields {
                ui.add(egui::DragValue::new(value).range(range).speed(speed));
            }
            ui.end_row();
        });
}

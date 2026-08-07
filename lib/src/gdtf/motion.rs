//! Pan, tilt and zoom travel, rather than teleporting to whatever the DMX says.
//!
//! A yoke ramps up, cruises and brakes, all three of speed, acceleration and
//! jerk bounded. Rather than nudge the axis a frame at a time and hope, each new
//! target is answered with the whole move planned end to end: a handful of
//! constant-jerk pieces whose durations are solved for, and which are then
//! evaluated as the cubics they are. Nothing is integrated, so the path does not
//! depend on the frame rate and cannot drift, ring or stall part way.

use std::collections::VecDeque;

use egui_plot::{HLine, Legend, Line, LineStyle, Plot, PlotPoints};
use crate::prelude::*;

/// Per-axis limits, in degrees: of rotation for pan and tilt, of field angle
/// for zoom.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Params {
    /// Cruise speed. 0 is unlimited.
    pub v_max: f32,
    /// Acceleration, used for both ramping up and braking. 0 is unlimited.
    pub a_max: f32,
    /// Cap on how fast the acceleration itself may change. 0 is unlimited.
    pub j_max: f32,
    /// Cruise speed per degree of travel, for moves too short to be worth
    /// winding all the way up.
    pub k_small: f32,
    /// Travel below which `k_small` sets the cruise speed instead of `v_max`.
    pub small: f32,
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

/// Where an axis is, and how it is moving, at one instant.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct State {
    pos: f32,
    vel: f32,
    acc: f32,
}

impl State {
    /// Where holding `jerk` for `secs` puts the axis. This is the whole of the
    /// physics; everything else here only works out which jerk to hold, and for
    /// how long.
    fn after(self, jerk: f32, secs: f32) -> Self {
        let (t, t2, t3) = (secs, secs * secs, secs * secs * secs);
        Self {
            pos: self.pos + self.vel * t + self.acc * t2 / 2.0 + jerk * t3 / 6.0,
            vel: self.vel + self.acc * t + jerk * t2 / 2.0,
            acc: self.acc + jerk * t,
        }
    }
}

/// One constant-jerk piece of a plan.
#[derive(Clone, Copy, Default)]
struct Leg {
    jerk: f32,
    secs: f32,
}

/// Where `legs` leave the axis.
fn walk(from: State, legs: &[Leg]) -> State {
    legs.iter().fold(from, |at, leg| at.after(leg.jerk, leg.secs))
}

/// The three legs that bring the axis to `vel` with its acceleration back at
/// zero, as fast as the acceleration and jerk limits allow: slew the
/// acceleration towards a peak, hold it there if the limit caps it, slew it
/// back down.
///
/// The peak follows from the fact that the two slews alone change the velocity
/// by `(2*peak^2 - acc^2) / 2*jerk`, which inverts directly. Which side of zero
/// it lands is decided by comparing the change asked for against the change that
/// merely unwinding the current acceleration would cause anyway.
fn to_vel(from: State, vel: f32, p: Params) -> [Leg; 3] {
    let (acc, dv) = (from.acc, vel - from.vel);
    let unwound = acc * acc.abs() / (2.0 * p.j_max);
    let dir = match dv >= unwound {
        true => 1.0,
        false => -1.0,
    };
    let peak = (dir * (acc * acc / 2.0 + dir * p.j_max * dv).max(0.0).sqrt()).clamp(-p.a_max, p.a_max);

    let up = Leg { jerk: dir * p.j_max, secs: ((peak - acc) / (dir * p.j_max)).max(0.0) };
    let held = from.after(up.jerk, up.secs).acc;
    let down = Leg { jerk: -held.signum() * p.j_max, secs: held.abs() / p.j_max };
    // Whatever velocity the two slews leave short is covered holding the peak.
    let short = vel - walk(from, &[up, down]).vel;
    let hold = match held.abs() > 1e-6 {
        true => Leg { jerk: 0.0, secs: (short / held).max(0.0) },
        false => Leg::default(),
    };
    [up, hold, down]
}

/// A whole move, as constant-jerk legs, plus where it is meant to end so the
/// last step can land on that rather than on the rounding the legs picked up.
struct Plan {
    legs: [Leg; 7],
    target: f32,
}

impl Plan {
    /// The move from `from` to a standstill at `target`: wind up to some peak
    /// speed, hold it, brake.
    ///
    /// Every leg is closed form once that peak speed is known, and the distance
    /// the legs cover varies smoothly with it, so the peak that lands exactly on
    /// the target is found by bisection, to the last bit of an `f32`.
    fn to(from: State, target: f32, p: Params) -> Self {
        let reach = |peak: f32| {
            let at = walk(from, &to_vel(from, peak, p));
            walk(at, &to_vel(at, 0.0, p)).pos - from.pos
        };
        let want = target - from.pos;
        // Speed the axis keeps if it does nothing but unwind the acceleration it
        // already has. Peaks past it mean winding up, peaks short of it mean
        // shedding speed, and the two have to be searched apart: in between sit
        // the peaks describing one brake taken in two bites, which covers a
        // different distance than either whole and would hand the search a second
        // answer to find. Where the two searches meet they can pick routes that
        // differ by a fraction of a DMX step, which `the_branch_seam_is_finer_
        // than_the_wire` holds to that.
        let free =
            (from.vel + from.acc * from.acc.abs() / (2.0 * p.j_max)).clamp(-p.v_max, p.v_max);
        // Past the speed limit there is no faster peak to be had, so whatever
        // distance is still owed gets covered coasting.
        let (peak, coast) = match (want >= reach(free), reach(-p.v_max), reach(p.v_max)) {
            (true, _, far) if want >= far => (p.v_max, (want - far) / p.v_max),
            (false, near, _) if want <= near => (-p.v_max, (near - want) / p.v_max),
            (true, ..) => (bisect(reach, want, free, p.v_max), 0.0),
            (false, ..) => (bisect(reach, want, -p.v_max, free), 0.0),
        };

        let up = to_vel(from, peak, p);
        let at = walk(from, &up);
        let down = to_vel(at, 0.0, p);
        let cruise = Leg { jerk: 0.0, secs: coast };
        Self { legs: [up[0], up[1], up[2], cruise, down[0], down[1], down[2]], target }
    }

    /// Where the plan has the axis `secs` after it started.
    fn at(&self, from: State, secs: f32) -> State {
        let mut left = secs;
        let mut at = from;
        for leg in &self.legs {
            let ran = leg.secs.min(left);
            at = at.after(leg.jerk, ran);
            left -= ran;
            if left <= 0.0 {
                return at;
            }
        }
        // The plan is spent, so the move is over.
        State { pos: self.target, vel: 0.0, acc: 0.0 }
    }
}

/// The one peak speed in `lo..hi` whose `reach` is `want`. 32 halvings take any
/// bracket a fixture could have below what an `f32` can tell apart.
fn bisect(reach: impl Fn(f32) -> f32, want: f32, mut lo: f32, mut hi: f32) -> f32 {
    for _ in 0..32 {
        let mid = 0.5 * (lo + hi);
        match reach(mid) < want {
            true => lo = mid,
            false => hi = mid,
        }
    }
    0.5 * (lo + hi)
}

/// Cruise speed for one move. A head does not wind all the way up to answer a
/// nudge: travel under `small` degrees runs at `k_small` per degree of it. Read
/// once, when the move is planned, so the cap cannot shrink along with the
/// closing gap and leave the axis crawling in forever.
fn creep(gap: f32, p: Params) -> Params {
    match p.small > 0.0 && gap.abs() < p.small {
        true => Params { v_max: (p.k_small * gap.abs()).clamp(1e-3, p.v_max), ..p },
        false => p,
    }
}

/// The move in hand: what was asked for, the plan answering it, and how far in
/// the axis is. Held rather than replanned every frame, so the axis rides one
/// exact curve instead of stitching together a fresh curve each step.
struct Move {
    ask: (f32, Params),
    plan: Plan,
    from: State,
    elapsed: f32,
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
    going: Option<Move>,
    /// Cleared until the first target arrives, so a fixture starts where it is
    /// aimed instead of sweeping in from zero.
    homed: bool,
}

impl Axis {
    /// Advance one frame towards `target`, and report where the axis now is.
    pub fn step(&mut self, target: f32, p: Params, dt: f32) -> f32 {
        if !self.homed || dt <= 0.0 {
            self.homed = true;
            (self.pos, self.vel, self.acc, self.jerk) = (target, 0.0, 0.0, 0.0);
            self.going = None;
            return self.pos;
        }
        let ask = (target, p.resolve(target - self.pos, dt));
        // Replanning mid-move would give back the same curve, so only a change
        // of ask is worth the work.
        if self.going.as_ref().is_none_or(|going| going.ask != ask) {
            let from = State { pos: self.pos, vel: self.vel, acc: self.acc };
            let p = creep(target - from.pos, ask.1);
            self.going = Some(Move { ask, plan: Plan::to(from, target, p), from, elapsed: 0.0 });
        }

        let going = self.going.as_mut().expect("just planned");
        going.elapsed += dt;
        let at = going.plan.at(going.from, going.elapsed);
        // Averaged over the frame, which is all a frame-sampled plot can say.
        self.jerk = (at.acc - self.acc) / dt;
        (self.pos, self.vel, self.acc) = (at.pos, at.vel, at.acc);
        self.pos
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

    /// Seconds of x axis to show, with the right edge at now.
    fn span(&self, view: &View) -> f64 {
        match view.scroll {
            true => view.window,
            // Fit the move, but never past what the buffer holds.
            false => {
                let held = self.samples.front().map_or(0.5, |s| self.clock - s.t).max(0.5);
                held.min(view.window)
            }
        }
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
    /// Hold the x axis at `window` instead of sizing it to the move. A fixture
    /// driven continuously never settles, so the move has no end and the fitted
    /// axis stretches out to the whole buffer, shrinking the trace as it goes.
    /// Scrolling keeps the scale put and lets the trace run off the left.
    pub scroll: bool,
    /// Seconds on screen. Fitted, this is only a cap and the axis may show less;
    /// scrolling, it is the width.
    pub window: f64,
}

impl Default for View {
    fn default() -> Self {
        Self { vel: true, accel: true, jerk: true, scroll: false, window: KEEP }
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
        ui.selectable_value(&mut view.scroll, false, "Settle")
            .on_hover_text("Size the x axis to the move, and hold it once the axis lands");
        ui.selectable_value(&mut view.scroll, true, "Scroll")
            .on_hover_text("Hold the x axis at the window width, so continuous motion cannot stretch it");
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
                plot.set_plot_bounds_x(-trace.span(view)..=0.0);
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
        "smooth (s)".into(),
    ];
    let fields = [
        // Each starts at 0, which is unlimited.
        (&mut p.v_max, 0.0..=5000.0, 10.0),
        (&mut p.a_max, 0.0..=20000.0, 50.0),
        (&mut p.j_max, 0.0..=500_000.0, 500.0),
        (&mut p.k_small, 0.01..=50.0, 0.1),
        (&mut p.small, 0.0..=180.0, 1.0),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Pan's limits with the creep path off, which is how a fixture following a
    /// path is driven.
    fn limits() -> Params {
        Params { small: 0.0, ..Params::pan() }
    }

    /// A snapshot per frame of an axis driven to `target` from rest.
    fn sweep(p: Params, target: f32, dt: f32, secs: f32) -> Vec<Axis> {
        let mut axis = Axis::default();
        axis.step(0.0, p, dt);
        (0..(secs / dt) as usize)
            .map(|_| {
                axis.step(target, p, dt);
                Axis { pos: axis.pos, vel: axis.vel, acc: axis.acc, jerk: axis.jerk, ..default() }
            })
            .collect()
    }

    /// The target a chase follows, at frame `i`.
    fn wave(amp: f32, hz: f32, i: usize) -> f32 {
        amp * (std::f32::consts::TAU * hz * i as f32 / 44.0).sin()
    }

    /// A snapshot per frame of an axis chasing a sine, which is what a pattern
    /// looks like: the target never stops moving and never repeats.
    fn chase(p: Params, amp: f32, hz: f32, laps: f32) -> Vec<Axis> {
        let mut axis = Axis::default();
        (0..(44.0 * laps / hz) as usize)
            .map(|i| {
                axis.step(wave(amp, hz, i), p, 1.0 / 44.0);
                Axis { pos: axis.pos, vel: axis.vel, acc: axis.acc, jerk: axis.jerk, ..default() }
            })
            .collect()
    }

    /// Starts that make a plan work for it: rolling, and braking hard the wrong
    /// way.
    fn awkward() -> [State; 5] {
        [
            State::default(),
            State { pos: 0.0, vel: 120.0, acc: 0.0 },
            State { pos: 0.0, vel: -40.0, acc: 600.0 },
            State { pos: 0.0, vel: 90.0, acc: -700.0 },
            State { pos: 0.0, vel: -250.0, acc: -800.0 },
        ]
    }

    /// A packet that barely moves the target must barely move the axis. This is
    /// the property the old profiler broke and the one that matters: its brake
    /// latch and arrival window were both cliffs, so a hair of target either side
    /// of one produced wholly different motion, which is what the judder was.
    ///
    /// It is not enough to know the search behaves, because two peak speeds can
    /// describe the same curve carved up differently. So this looks at the motion
    /// itself, from starts that put the plan through every branch it has.
    #[test]
    fn a_hair_more_target_is_a_hair_more_motion() {
        let (p, dt) = (limits(), 1.0_f32 / 44.0);
        // A hundredth of a degree is well under one step of a 16-bit channel
        // across any pan or tilt range, so no real packet can ask for less.
        let target = |i: i32| i as f32 / 100.0 - 60.0;
        for from in awkward() {
            let mut last = Plan::to(from, target(0), p).at(from, dt);
            for i in 1..=12_000 {
                let at = Plan::to(from, target(i), p).at(from, dt);
                let (pos, acc) = ((at.pos - last.pos).abs(), (at.acc - last.acc).abs());
                // Loose on purpose: a move short enough to finish inside one
                // frame really is this sensitive, so the bound that means
                // something is being an order under the half degree the old
                // profiler used to snap across, not being tight in absolute
                // terms. The exact seam is measured below.
                assert!(
                    pos <= 0.05 && acc <= p.j_max * dt,
                    "{from:?} at target {}: position jumped {pos} and accel {acc}",
                    target(i)
                );
                last = at;
            }
        }
    }

    /// Winding up and shedding speed are searched separately, so where the two
    /// meet the plan can switch between routes that cover the same distance by
    /// slightly different means. That seam has to stay finer than one step of a
    /// 16-bit channel, or it would be a real jolt rather than a rounding.
    #[test]
    fn the_branch_seam_is_finer_than_the_wire() {
        let (p, dt) = (limits(), 1.0_f32 / 44.0);
        // Pan is the coarsest axis on the fixture: 540 degrees over 16 bits.
        let step = 540.0 / 65535.0;
        for from in awkward() {
            let free =
                (from.vel + from.acc * from.acc.abs() / (2.0 * p.j_max)).clamp(-p.v_max, p.v_max);
            let at = walk(from, &to_vel(from, free, p));
            let seam = walk(at, &to_vel(at, 0.0, p)).pos;

            let side = |target: f32| Plan::to(from, target, p).at(from, dt);
            let (before, after) = (side(seam - 1e-5), side(seam + 1e-5));
            let jump = (after.pos - before.pos).abs();
            assert!(jump < step, "{from:?}: the seam at {seam} jumps {jump}, over {step}");
        }
    }

    /// And the plan the search builds has to actually arrive, from any start and
    /// for any distance, including ones too short to stop in without overshooting
    /// and coming back.
    #[test]
    fn every_plan_lands_on_its_target() {
        let p = limits();
        for from in awkward() {
            for target in [-300.0, -1.0, 0.0, 0.05, 2.0, 45.0, 400.0] {
                let plan = Plan::to(from, target, p);
                let secs: f32 = plan.legs.iter().map(|leg| leg.secs).sum();
                assert!(secs.is_finite() && secs < 60.0, "{from:?} to {target}: {secs}s");
                let end = walk(from, &plan.legs);
                assert!(
                    (end.pos - target).abs() < 1e-2 && end.vel.abs() < 1e-2,
                    "{from:?} to {target} ended {end:?}"
                );
            }
        }
    }

    /// The point of planning rather than integrating: the frame rate picks the
    /// sampling, not the path. A tenth of the step must land on the same curve.
    #[test]
    fn the_frame_rate_does_not_change_the_path() {
        let (p, dt) = (limits(), 1.0_f32 / 44.0);
        let coarse = sweep(p, 90.0, dt, 1.0);
        let fine = sweep(p, 90.0, dt / 10.0, 1.0);
        for (i, at) in coarse.iter().enumerate() {
            // Same instant, ten times the samples to get there.
            let Some(same) = fine.get((i + 1) * 10 - 1) else { break };
            assert!((at.pos - same.pos).abs() < 1e-3, "frame {i}: {} against {}", at.pos, same.pos);
        }
    }

    /// No limit may be broken, on a big move or while chasing a path. The old
    /// profiler overshot its speed limit by half again while chasing.
    #[test]
    fn the_limits_hold() {
        let p = limits();
        for run in [sweep(p, 200.0, 1.0 / 44.0, 3.0), chase(p, 10.0, 0.5, 4.0)] {
            for at in &run {
                assert!(at.vel.abs() <= p.v_max * 1.001, "speed {}", at.vel);
                assert!(at.acc.abs() <= p.a_max * 1.001, "accel {}", at.acc);
                assert!(at.jerk.abs() <= p.j_max * 1.001, "jerk {}", at.jerk);
            }
        }
    }

    /// Chasing a path must be one unbroken glide. The old profiler stopped dead
    /// on over half its frames, teleporting onto the target and starting again,
    /// which is what made the simulated head judder.
    #[test]
    fn chasing_a_path_never_stalls_or_chatters() {
        let run = chase(limits(), 10.0, 0.5, 4.0);
        let stalls = run.iter().skip(2).filter(|at| at.vel == 0.0).count();
        assert_eq!(stalls, 0, "the axis stopped dead on {stalls} of {} frames", run.len());
        // One sign change per half lap is the turn; anything more is chatter.
        let turns = run.windows(2).filter(|w| w[0].vel * w[1].vel < 0.0).count();
        assert!(turns <= 8, "velocity changed sign {turns} times over four laps");
    }

    /// Tightening one axis must warp the path smoothly, which is the whole
    /// reason for simulating: a slower axis lags further behind, so a circle
    /// leans into an ellipse instead of falling apart.
    #[test]
    fn less_acceleration_lags_further_behind() {
        let mut worst = 0.0_f32;
        for a_max in [1600.0, 800.0, 400.0, 200.0, 100.0] {
            let run = chase(Params { a_max, ..limits() }, 10.0, 0.5, 4.0);
            // Compare the settled laps against the target that drove them.
            let lag = run
                .iter()
                .enumerate()
                .skip(run.len() / 2)
                .map(|(i, at)| (wave(10.0, 0.5, i) - at.pos).abs())
                .fold(0.0_f32, f32::max);
            assert!(lag > worst, "a_max {a_max} lagged {lag:.3}, no worse than {worst:.3}");
            worst = lag;
        }
    }

    /// A move ends on the target exactly, with nothing left over, and stays put.
    #[test]
    fn a_move_lands_and_stays() {
        for target in [90.0, -37.5, 0.25] {
            let run = sweep(limits(), target, 1.0 / 44.0, 5.0);
            let last = run.last().expect("frames");
            assert_eq!((last.pos, last.vel, last.acc), (target, 0.0, 0.0));
        }
    }

    /// Creep caps the speed of a nudge without changing how it is shaped, so the
    /// acceleration limit still applies where it used to be bypassed entirely.
    #[test]
    fn creep_only_caps_the_speed() {
        let p = Params { small: 80.0, k_small: 1.5, ..Params::pan() };
        let peak = |p| sweep(p, 10.0, 1.0 / 44.0, 5.0).iter().map(|at| at.vel.abs()).fold(0.0, f32::max);
        let crept = peak(p);
        assert!((crept - 15.0).abs() < 0.5, "creep ran at {crept}, not 1.5 per degree of 10");
        assert!(peak(Params { a_max: 20.0, ..p }) < crept, "the accel limit missed the creep path");
    }

    /// A trace fed `secs` of unbroken motion at 20 Hz, which is what a fixture
    /// driven round a path looks like: it never settles.
    fn moving(secs: f64) -> Trace {
        let mut trace = Trace::default();
        let axis = Axis { vel: 10.0, ..Axis::default() };
        let mut t = 0.0;
        while t < secs {
            t += 0.05;
            trace.push(t, 1.0, &axis);
        }
        trace
    }

    /// Fitted, the axis sizes itself to the move, so continuous motion stretches
    /// it until the buffer runs out. Scrolling has to hold the width instead.
    #[test]
    fn scroll_holds_the_window_while_fit_grows() {
        let fit = View { window: 20.0, scroll: false, ..default() };
        let scroll = View { window: 20.0, scroll: true, ..default() };

        let (short, long) = (moving(4.0), moving(12.0));
        assert!(short.span(&fit) < 5.0, "fit showed {}", short.span(&fit));
        assert!(long.span(&fit) > 11.0, "fit did not grow: {}", long.span(&fit));
        assert_eq!(short.span(&scroll), 20.0);
        assert_eq!(long.span(&scroll), 20.0);
    }

    /// Neither mode may show more than the buffer holds, or the trace would end
    /// in dead space.
    #[test]
    fn fit_never_outruns_the_buffer() {
        let view = View { window: KEEP, scroll: false, ..default() };
        assert!(moving(2.0 * KEEP).span(&view) <= KEEP);
    }
}

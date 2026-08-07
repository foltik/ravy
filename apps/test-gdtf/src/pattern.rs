//! Drive pan and tilt around a closed path and compare where the beam actually
//! lands against where it was aimed.
//!
//! The shape is drawn on a flat surface a set throw away, square to whatever the
//! panel is aimed at, and turned into pan and tilt by inverse kinematics. It is
//! not swept in pan and tilt degrees, because a yoke is a spherical mechanism:
//! tilt is the angle off the pan axis and pan is the azimuth about it. Sweeping
//! the two as if they were the x and y of a plane draws a figure eight through
//! the middle, since tilt passing through zero puts the beam on the axis and
//! then swings it out the opposite side. Worse, pan's effect on the spot scales
//! with the sine of tilt, so no fixed ratio between the two can straighten it.
//!
//! Creep is switched off throughout: see `ramped`. Taking the surface normal
//! from the centre of aim rather than from the pan
//! axis is what keeps the shape undistorted whether it is thrown at a ceiling
//! overhead or a wall straight ahead. Everything the panel plots is back in
//! surface metres, which is what the eye sees.
//!
//! Two modes. Raw sends the aim straight out, which is what a console does, and
//! the head lags and rounds the shape off. Computed solves for the commands
//! whose *output* is the shape: it runs the same profile model in a loop,
//! correcting the command by whatever error the last run left. That only works
//! where the shape fits inside the axis limits, so the panel also reports how
//! far outside them it is and how fast it could run before the error passes
//! what the user will accept.

use egui_plot::{Line, LineStyle, Plot, PlotPoints, Points};
use lib::gdtf::motion;
use lib::prelude::*;

/// DMX packets a second, which is the rate commands can change at.
const RATE: f32 = 44.0;

/// Profile steps per packet. The fixture's own motion tick runs at 1 kHz, so
/// this is about one step per firmware tick.
const SUB: usize = 23;

/// Laps to simulate. The head starts where it is aimed, so the first lap is a
/// transient; only the last one is kept.
const LAPS: usize = 3;

/// Correction passes. The loop converges fast, and each one costs a full run.
const PASSES: usize = 8;

/// How much of the error to fold into the command each pass. Below 1 so a shape
/// that saturates the axis cannot oscillate.
const GAIN: f32 = 0.45;

/// Circular smoothing passes over each correction before it is applied. The
/// head cannot reproduce detail finer than its own ramp, so learning that
/// detail feeds back and the loop diverges within a few passes; blurring the
/// correction keeps only what the axis can actually follow.
const BLUR: usize = 3;

/// Cap on samples per lap, so a slow shape cannot make the solve crawl.
const MAX_STEPS: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Square,
    Triangle,
}

impl Shape {
    const ALL: [(Shape, &'static str); 3] =
        [(Shape::Circle, "Circle"), (Shape::Square, "Square"), (Shape::Triangle, "Triangle")];

    /// Point on the shape at `u` in 0..1, on a unit circle or its inscribed
    /// polygon, so every shape covers the same extent.
    fn at(self, u: f32) -> Vec2 {
        let u = u.rem_euclid(1.0);
        match self {
            Shape::Circle => {
                let a = u * std::f32::consts::TAU;
                Vec2::new(a.cos(), a.sin())
            }
            Shape::Square => polygon(4, u),
            Shape::Triangle => polygon(3, u),
        }
    }
}

/// Constant-speed walk around an `n`-gon inscribed in the unit circle.
fn polygon(n: usize, u: f32) -> Vec2 {
    let edge = u * n as f32;
    let i = edge.floor() as usize % n;
    let f = edge.fract();
    let corner = |k: usize| {
        let a = (k as f32 / n as f32) * std::f32::consts::TAU;
        Vec2::new(a.cos(), a.sin())
    };
    corner(i).lerp(corner((i + 1) % n), f)
}

/// Beam direction for pan and tilt in degrees, in a frame whose +z is the pan
/// axis. Tilt is the angle off that axis, pan the azimuth about it.
fn dir(aim: Vec2) -> Vec3 {
    let (p, t) = (aim.x.to_radians(), aim.y.to_radians());
    Vec3::new(t.sin() * p.cos(), t.sin() * p.sin(), t.cos())
}

fn wrap(x: f32) -> f32 {
    x - 360.0 * (x / 360.0).round()
}

/// Pan and tilt that point along `d`, taking whichever of the two that do lies
/// nearer `near`. Any direction is reached either by tilting one way or by
/// panning half a turn and tilting the other, and picking the wrong one throws
/// the axis to the far end of its travel.
fn aim_along(d: Vec3, near: Vec2) -> Vec2 {
    let tilt = d.truncate().length().atan2(d.z).to_degrees();
    let pan = d.y.atan2(d.x).to_degrees();
    let cost = |a: Vec2| wrap(a.x - near.x).abs() + (a.y - near.y).abs();
    let (a, b) = (Vec2::new(pan, tilt), Vec2::new(wrap(pan + 180.0), -tilt));
    match cost(a) <= cost(b) {
        true => a,
        false => b,
    }
}

/// Orthonormal frame of the surface. Its normal is the centre of aim, so the
/// shape is undistorted there however the fixture is pointed; `u` runs level
/// across the surface and `v` up it.
fn frame(centre: Vec2) -> (Vec3, Vec3, Vec3) {
    let n = dir(centre).normalize_or(Vec3::X);
    let across = Vec3::Z.cross(n);
    // Aimed straight along the pan axis there is no level direction to find, so
    // any perpendicular will do.
    let u = match across.length() > 1e-4 {
        true => across.normalize(),
        false => Vec3::X,
    };
    (n, u, n.cross(u).normalize())
}

/// Keep a pan sequence continuous. Azimuth is cut at half a turn, and a shape
/// crossing that cut would otherwise read as a 360 degree slam.
fn unwrap_pan(aims: &mut [Vec2]) {
    let mut turns = 0.0;
    for i in 1..aims.len() {
        let step = (aims[i].x + turns) - aims[i - 1].x;
        turns -= 360.0 * (step / 360.0).round();
        aims[i].x += turns;
    }
}

/// One axis of the fixture as the solver sees it: its limits and how far it can
/// travel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Axis {
    pub params: motion::Params,
    /// Full travel in degrees, for quantising the command to 16-bit DMX.
    pub range: f32,
}

pub struct Pattern {
    pub shape: Shape,
    /// Laps a second.
    pub speed: f32,
    /// Error the user is willing to accept, in metres on the surface.
    pub allowance: f32,
    /// Solve for the commands rather than sending the aim as-is.
    pub computed: bool,
    /// Distance to the surface, in metres, measured square to it.
    pub throw: f32,
    /// Radius of the shape on the surface, in metres.
    pub size: f32,
    /// Pan and tilt the middle of the shape sits at, in degrees. The surface is
    /// square to this, so the shape is undistorted wherever it is aimed.
    pub centre: Vec2,
    /// Drive the fixture's pan and tilt channels from the shape.
    pub running: bool,
    /// How far round the lap the fixture is, in 0..1.
    phase: f32,
    /// When the phase was last moved. egui lays a panel out more than once per
    /// frame, so advancing by a delta per call would run the lap fast.
    clock: Option<f64>,
    solved: Option<Solved>,
    /// Inputs the cached solve was made from, so it is only redone on a change.
    stamp: Option<Stamp>,
}

/// Everything a solve depends on. Floats are compared by bits: the values come
/// straight from sliders, so an exact match means nothing moved.
#[derive(PartialEq)]
struct Stamp {
    shape: Shape,
    speed: u32,
    allowance: u32,
    computed: bool,
    throw: u32,
    size: u32,
    centre: [u32; 2],
    /// Held whole rather than field by field, so a limit the panel does not know
    /// about yet still invalidates the solve when the Tuning section moves it.
    axes: [Axis; 2],
}

impl Stamp {
    /// Whether two solves face the same limits, differing only in how fast the
    /// shape runs. Speed is what the feasible-speed search sweeps, so a change
    /// to it alone is not worth another search.
    fn same_shape(&self, other: &Stamp) -> bool {
        (self.shape, self.allowance, self.computed, self.throw, self.size, self.centre, self.axes)
            == (
                other.shape,
                other.allowance,
                other.computed,
                other.throw,
                other.size,
                other.centre,
                other.axes,
            )
    }
}

struct Solved {
    /// All three in metres on the surface, which is what the eye sees.
    target: Vec<Vec2>,
    command: Vec<Vec2>,
    actual: Vec<Vec2>,
    /// Pan and tilt the fixture is sent, for driving it.
    aims: Vec<Vec2>,
    /// Worst and typical distance on the surface between aim and landing.
    peak: f32,
    rms: f32,
    /// Fraction of each axis limit the shape asks for, worst over the lap.
    load: [Vec3; 2],
    /// Fastest the shape runs inside `allowance`, in laps a second.
    limit: f32,
    /// Span of pan and of tilt the shape needs, in degrees.
    sweep: Vec2,
    /// Range of each axis the shape covers, for reading against the wire.
    span: [(f32, f32); 2],
    /// Closest the beam comes to the pan axis, in degrees. Near zero the azimuth
    /// is ill conditioned and pan has to slew violently.
    pole: f32,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            shape: Shape::Circle,
            speed: 0.5,
            allowance: 0.05,
            computed: false,
            // Straight ahead and level, a metre off the wall, so the circle
            // lands square on a surface right in front of the fixture and can be
            // eyeballed against the real head without any throw distortion.
            throw: 1.0,
            size: 0.24,
            centre: Vec2::new(0.0, -86.495),
            running: false,
            phase: 0.0,
            clock: None,
            solved: None,
            stamp: None,
        }
    }
}

impl Pattern {
    /// Samples one lap, given how long a lap takes.
    fn steps(speed: f32) -> usize {
        ((RATE / speed.max(1e-3)).round() as usize).clamp(24, MAX_STEPS)
    }

    /// The shape on the surface, in metres from the middle of it.
    fn surface(&self, steps: usize) -> Vec<Vec2> {
        (0..steps).map(|i| self.size * self.shape.at(i as f32 / steps as f32)).collect()
    }

    /// The pan and tilt that trace it, continuous across the azimuth cut.
    fn aims(&self, surface: &[Vec2]) -> Vec<Vec2> {
        let (n, u, v) = frame(self.centre);
        let mut aims: Vec<Vec2> = surface
            .iter()
            .map(|off| {
                let at = self.throw * n + off.x * u + off.y * v;
                aim_along(at.normalize_or(n), self.centre)
            })
            .collect();
        unwrap_pan(&mut aims);
        aims
    }

    /// Run the profile model over `command`, repeated for `LAPS`, and report
    /// where the head was at each packet of the final lap. Pan and tilt degrees
    /// throughout: this is the axis, not the surface.
    fn run(command: &[Vec2], axes: [Axis; 2], speed: f32) -> Vec<Vec2> {
        let axes = ramped(axes);
        let steps = command.len();
        let frame = 1.0 / (steps as f32 * speed.max(1e-3));
        let dt = frame / SUB as f32;

        let mut travel = [motion::Axis::default(), motion::Axis::default()];
        let mut input = [motion::Command::default(), motion::Command::default()];
        let mut now = 0.0_f64;
        let mut out = Vec::with_capacity(steps);

        for f in 0..steps * LAPS {
            let want = command[f % steps];
            for _ in 0..SUB {
                for a in 0..2 {
                    let p = axes[a].params;
                    let target = input[a].step(now, want[a], p.smooth, dt);
                    travel[a].step(target, p, dt);
                }
                now += dt as f64;
            }
            if f >= steps * (LAPS - 1) {
                out.push(Vec2::new(travel[0].pos, travel[1].pos));
            }
        }
        out
    }

    /// Snap a command to the 16-bit step the wire can actually carry.
    fn quantise(v: Vec2, axes: [Axis; 2]) -> Vec2 {
        let one = |x: f32, a: Axis| {
            let step = a.range / 65535.0;
            match step > 0.0 {
                true => (x / step).round() * step,
                false => x,
            }
        };
        Vec2::new(one(v.x, axes[0]), one(v.y, axes[1]))
    }

    /// Packets the head trails the command by, found by sliding the travel back
    /// over the target until they line up. Correcting a command by the error
    /// measured at the *same* packet fights this lag instead of removing it, so
    /// the loop barely converges without it.
    fn lag(target: &[Vec2], actual: &[Vec2]) -> usize {
        let n = target.len();
        let cost = |shift: usize| {
            (0..n).map(|i| target[i].distance_squared(actual[(i + shift) % n])).sum::<f32>()
        };
        (0..n / 2).min_by(|&a, &b| cost(a).total_cmp(&cost(b))).unwrap_or(0)
    }

    /// Correct the command until the head's travel lands on the target, and
    /// return the best command found with what it achieved. In pan and tilt
    /// degrees, since that is the space the profile model lives in.
    fn solve(target: &[Vec2], axes: [Axis; 2], speed: f32) -> (Vec<Vec2>, Vec<Vec2>) {
        let n = target.len();
        let mut command: Vec<Vec2> = target.to_vec();
        let mut actual = Self::run(&command, axes, speed);
        let lag = Self::lag(target, &actual);
        let mut best = (command.clone(), actual.clone());
        let mut best_peak = peak(target, &actual);

        for _ in 0..PASSES {
            // A command lands `lag` packets later, so it answers for the error
            // there, not the one under it now.
            let mut fix: Vec<Vec2> =
                (0..n).map(|i| target[(i + lag) % n] - actual[(i + lag) % n]).collect();
            blur(&mut fix);
            for i in 0..n {
                command[i] = Self::quantise(command[i] + GAIN * fix[i], axes);
            }
            actual = Self::run(&command, axes, speed);
            let got = peak(target, &actual);
            if got < best_peak {
                (best, best_peak) = ((command.clone(), actual.clone()), got);
            }
        }
        best
    }

    /// Fraction of each axis's speed, acceleration and jerk limit the shape asks
    /// for, from the pan and tilt the wire carries.
    fn load(aims: &[Vec2], axes: [Axis; 2], speed: f32) -> [Vec3; 2] {
        let n = aims.len();
        let dt = 1.0 / (n as f32 * speed.max(1e-3));
        let mut out = [Vec3::ZERO; 2];
        for a in 0..2 {
            let p = axes[a].params;
            let x = |i: usize| aims[i.rem_euclid(n)][a];
            let (mut v, mut acc, mut jerk) = (0.0f32, 0.0f32, 0.0f32);
            for i in 0..n {
                let (m2, m1, c) = (x((i + n - 2) % n), x((i + n - 1) % n), x(i));
                let (p1, p2) = (x((i + 1) % n), x((i + 2) % n));
                v = v.max(((p1 - m1) / (2.0 * dt)).abs());
                acc = acc.max(((p1 - 2.0 * c + m1) / (dt * dt)).abs());
                jerk = jerk.max(((p2 - 2.0 * p1 + 2.0 * m1 - m2) / (2.0 * dt * dt * dt)).abs());
            }
            let over = |got: f32, limit: f32| match limit > 0.0 {
                true => got / limit,
                false => 0.0,
            };
            out[a] = Vec3::new(over(v, p.v_max), over(acc, p.a_max), over(jerk, p.j_max));
        }
        out
    }

    /// Where the beam lands on the surface, for a run's worth of pan and tilt.
    fn landed(&self, aims: &[Vec2]) -> Vec<Vec2> {
        let (n, u, v) = frame(self.centre);
        aims.iter()
            .map(|&a| {
                let d = dir(a);
                // Pointed away from the surface there is no crossing, so cap the
                // projection: the point flies off screen, which is the truth.
                let at = d * (self.throw / d.dot(n).max(1e-2));
                Vec2::new(at.dot(u), at.dot(v))
            })
            .collect()
    }

    /// Fastest the shape runs while staying inside `allowance`, found by
    /// bisection because the model's snap and creep make the error step rather
    /// than slide. Measured on the surface, like everything the panel reports.
    fn fastest(&self, axes: [Axis; 2]) -> f32 {
        let fits = |speed: f32| {
            let steps = Self::steps(speed);
            let surface = self.surface(steps);
            let aims = self.aims(&surface);
            let actual = match self.computed {
                true => Self::solve(&aims, axes, speed).1,
                false => Self::run(&aims, axes, speed),
            };
            peak(&surface, &self.landed(&actual)) <= self.allowance
        };
        let (mut lo, mut hi) = (0.02_f32, 4.0_f32);
        if fits(hi) {
            return hi;
        }
        if !fits(lo) {
            return 0.0;
        }
        for _ in 0..7 {
            let mid = (lo * hi).sqrt();
            match fits(mid) {
                true => lo = mid,
                false => hi = mid,
            }
        }
        lo
    }

    fn refresh(&mut self, axes: [Axis; 2]) {
        let bits = |x: f32| x.to_bits();
        let stamp = Stamp {
            shape: self.shape,
            speed: bits(self.speed),
            allowance: bits(self.allowance),
            computed: self.computed,
            throw: bits(self.throw),
            size: bits(self.size),
            centre: [bits(self.centre.x), bits(self.centre.y)],
            axes,
        };
        if self.stamp.as_ref() == Some(&stamp) {
            return;
        }
        let held = self.stamp.as_ref().is_some_and(|was| was.same_shape(&stamp));
        let limit = match held {
            true => self.solved.as_ref().map_or(0.0, |s| s.limit),
            false => self.fastest(axes),
        };

        let steps = Self::steps(self.speed);
        let surface = self.surface(steps);
        let target = self.aims(&surface);
        let (aims, actual) = match self.computed {
            true => Self::solve(&target, axes, self.speed),
            false => {
                let aims: Vec<Vec2> = target.iter().map(|&v| Self::quantise(v, axes)).collect();
                let actual = Self::run(&aims, axes, self.speed);
                (aims, actual)
            }
        };

        let landed = self.landed(&actual);
        let span: [(f32, f32); 2] = std::array::from_fn(|a| {
            let vals = target.iter().map(|t| t[a]);
            (vals.clone().fold(f32::MAX, f32::min), vals.fold(f32::MIN, f32::max))
        });
        self.solved = Some(Solved {
            peak: peak(&surface, &landed),
            rms: rms(&surface, &landed),
            load: Self::load(&target, axes, self.speed),
            limit,
            sweep: Vec2::new(span[0].1 - span[0].0, span[1].1 - span[1].0),
            span,
            pole: target.iter().map(|t| t.y.abs()).fold(f32::MAX, f32::min),
            command: self.landed(&aims),
            actual: landed,
            target: surface,
            aims,
        });
        self.stamp = Some(stamp);
    }
}

/// Strip the creep path. Below `small` degrees of travel the profile abandons
/// its ramp and crawls at a constant speed, which is a fair model of a fixture
/// answering one nudge but wrong for following a path: every step between
/// packets is short by definition, so creep would drive the whole lap, and the
/// jump in behaviour at the threshold is a discontinuity the solve cannot see
/// past. The panel always ramps.
fn ramped(mut axes: [Axis; 2]) -> [Axis; 2] {
    for axis in &mut axes {
        axis.params.small = 0.0;
    }
    axes
}

/// Three-tap circular moving average, applied `BLUR` times. Symmetric, so it
/// softens the correction without shifting it in time.
fn blur(v: &mut [Vec2]) {
    let n = v.len();
    if n < 3 {
        return;
    }
    for _ in 0..BLUR {
        let src = v.to_vec();
        for i in 0..n {
            v[i] = (src[(i + n - 1) % n] + 2.0 * src[i] + src[(i + 1) % n]) * 0.25;
        }
    }
}

fn peak(target: &[Vec2], actual: &[Vec2]) -> f32 {
    target.iter().zip(actual).map(|(t, a)| t.distance(*a)).fold(0.0, f32::max)
}

fn rms(target: &[Vec2], actual: &[Vec2]) -> f32 {
    let n = target.len().max(1) as f32;
    (target.iter().zip(actual).map(|(t, a)| t.distance_squared(*a)).sum::<f32>() / n).sqrt()
}

/// Close a lap so the plotted trace has no gap between its last and first point.
fn loop_points(points: &[Vec2]) -> PlotPoints<'static> {
    let mut out: Vec<[f64; 2]> = points.iter().map(|p| [p.x as f64, p.y as f64]).collect();
    if let Some(&first) = out.first() {
        out.push(first);
    }
    PlotPoints::from(out)
}

/// Draw the panel and, while it is running, report the pan and tilt the fixture
/// should be sent this frame, in the degrees its channels use.
pub fn draw(ui: &mut egui::Ui, pattern: &mut Pattern, axes: [Axis; 2], now: f64) -> Option<Vec2> {
    ui.horizontal(|ui| {
        let run = egui::Button::new(match pattern.running {
            true => "\u{25a0} Stop",
            false => "\u{25b6} Run",
        })
        .fill(match pattern.running {
            true => egui::Color32::from_rgb(70, 120, 70),
            false => ui.visuals().widgets.inactive.bg_fill,
        });
        if ui.add(run).on_hover_text("Drive the fixture's pan and tilt from the shape").clicked() {
            pattern.running = !pattern.running;
            (pattern.phase, pattern.clock) = (0.0, None);
        }
        ui.separator();
        for (shape, name) in Shape::ALL {
            ui.selectable_value(&mut pattern.shape, shape, name);
        }
        ui.separator();
        ui.selectable_value(&mut pattern.computed, false, "Raw")
            .on_hover_text("Send the aim straight out, as a console would");
        ui.selectable_value(&mut pattern.computed, true, "Computed")
            .on_hover_text("Solve for the commands whose output is the shape");
    });

    pattern.refresh(axes);
    let Some(solved) = &pattern.solved else { return None };
    let limit = solved.limit;

    // Commands are piecewise constant between packets, so hold each one rather
    // than sliding between them: that is what goes out on the wire.
    let steps = solved.aims.len();
    let at = (pattern.phase * steps as f32).floor() as usize;
    let sending = pattern.running.then(|| solved.aims[at.min(steps - 1)]);
    let dot = pattern.running.then(|| solved.command[at.min(steps - 1)]);
    if pattern.running {
        let dt = (now - pattern.clock.unwrap_or(now)).max(0.0) as f32;
        pattern.phase = (pattern.phase + pattern.speed * dt).rem_euclid(1.0);
        pattern.clock = Some(now);
    }

    ui.scope(|ui| {
        // Fixed, not derived from the available width: a slider sized from the
        // space it is given makes the content wider than the space, which grows an
        // auto-sizing window a little more every frame.
        ui.spacing_mut().slider_width = 200.0;
        let speed = ui.add(
            egui::Slider::new(&mut pattern.speed, 0.02..=4.0)
                .logarithmic(true)
                .custom_formatter(|v, _| format!("{v:.2}"))
                .text("laps/s"),
        );
        // egui has no tick on a slider, so mark the feasible limit by hand.
        if limit > 0.0 {
            let rect = speed.rect;
            // Matches the slider's own logarithmic mapping.
            let frac = (limit / 0.02).ln() / (4.0f32 / 0.02).ln();
            let pad = ui.spacing().slider_rail_height.max(8.0);
            let x = rect.left() + pad + (rect.width() - 2.0 * pad) * frac.clamp(0.0, 1.0);
            ui.painter().vline(
                x,
                rect.top() + 2.0..=rect.bottom() - 2.0,
                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(120, 200, 120)),
            );
        }
        ui.add(
            egui::Slider::new(&mut pattern.allowance, 0.002..=0.5)
                .logarithmic(true)
                .custom_formatter(|v, _| format!("{:.1}", v * 100.0))
                .text("allowed error (cm)"),
        );
        ui.add(egui::Slider::new(&mut pattern.size, 0.02..=5.0).text("radius (m)"));
        ui.add(egui::Slider::new(&mut pattern.throw, 0.3..=30.0).text("throw (m)"));
    });
    ui.horizontal(|ui| {
        ui.label("aimed at");
        ui.add(egui::DragValue::new(&mut pattern.centre.x).speed(0.5).suffix("\u{b0} pan"));
        ui.add(egui::DragValue::new(&mut pattern.centre.y).speed(0.5).suffix("\u{b0} tilt"));
        ui.label(format!(
            "\u{b1}{:.2}\u{b0} radius",
            (pattern.size / pattern.throw.max(1e-3)).atan().to_degrees()
        ));
    });
    ui.label(format!(
        "sweeps pan {:.2}..{:.2}\u{b0} ({:.2}\u{b0}), tilt {:.2}..{:.2}\u{b0} ({:.2}\u{b0})",
        solved.span[0].0,
        solved.span[0].1,
        solved.sweep.x,
        solved.span[1].0,
        solved.span[1].1,
        solved.sweep.y,
    ));

    let over = solved.load.iter().flat_map(|l| [l.x, l.y, l.z]).fold(0.0, f32::max);
    let warn = egui::Color32::from_rgb(230, 170, 90);
    let good = egui::Color32::from_rgb(120, 200, 120);
    ui.horizontal(|ui| {
        match over <= 1.0 {
            true => ui.colored_label(good, "feasible"),
            false => ui.colored_label(warn, format!("over limits {over:.1}\u{d7}")),
        };
        ui.separator();
        ui.label(format!(
            "error {:.1} cm peak / {:.1} cm rms",
            solved.peak * 100.0,
            solved.rms * 100.0
        ));
        ui.separator();
        match limit > 0.0 {
            true => ui.label(format!("fits allowance up to {limit:.2} laps/s")),
            false => ui.label("never fits allowance"),
        };
    });
    // Near the pan axis the azimuth is ill conditioned: a small move of the spot
    // needs a huge move of pan, and through it the shape folds into a figure
    // eight. Say so rather than leaving the trace unexplained.
    if solved.pole < 5.0 {
        ui.colored_label(
            warn,
            format!(
                "comes within {:.1}\u{b0} of the pan axis, where pan has to slew to \
                 keep up. Aim further off the axis.",
                solved.pole
            ),
        );
    }
    for (i, name) in ["pan", "tilt"].iter().enumerate() {
        if solved.sweep[i] > axes[i].range {
            ui.colored_label(
                warn,
                format!(
                    "needs {:.0}\u{b0} of {name} but the axis has {:.0}\u{b0}",
                    solved.sweep[i], axes[i].range
                ),
            );
        }
    }
    egui::Grid::new("pattern_load").num_columns(4).show(ui, |ui| {
        ui.label("");
        for h in ["speed", "accel", "jerk"] {
            ui.label(h);
        }
        ui.end_row();
        for (name, load) in ["Pan", "Tilt"].iter().zip(&solved.load) {
            ui.label(*name);
            for v in [load.x, load.y, load.z] {
                let colour = match v > 1.0 {
                    true => warn,
                    false => ui.visuals().text_color(),
                };
                ui.colored_label(colour, format!("{:.0}%", v * 100.0));
            }
            ui.end_row();
        }
    });

    let height = ui.available_height().max(220.0);
    Plot::new("pattern_xy")
        .height(height)
        .data_aspect(1.0)
        .show_grid(true)
        .show_axes(true)
        .allow_scroll(false)
        .show(ui, |plot| {
            plot.line(
                Line::new("Command", loop_points(&solved.command))
                    .color(egui::Color32::from_gray(150))
                    .style(LineStyle::dotted_dense()),
            );
            plot.line(
                Line::new("Target", loop_points(&solved.target))
                    .color(egui::Color32::from_gray(85))
                    .width(2.0_f32),
            );
            plot.line(
                Line::new("Actual", loop_points(&solved.actual))
                    .color(egui::Color32::from_rgb(80, 200, 90))
                    .width(1.5_f32),
            );
            // The middle of the shape, which is also where the surface is square
            // to the beam.
            plot.points(
                Points::new("Aim", PlotPoints::from(vec![[0.0, 0.0]]))
                    .radius(3.0_f32)
                    .shape(egui_plot::MarkerShape::Cross)
                    .color(egui::Color32::from_gray(110)),
            );
            // Where on the lap the fixture is right now, so the panel can be
            // read against the beam itself.
            if let Some(at) = dot {
                plot.points(
                    Points::new("Now", PlotPoints::from(vec![[at.x as f64, at.y as f64]]))
                        .radius(4.0_f32)
                        .color(egui::Color32::from_rgb(240, 240, 240)),
                );
            }
        });

    sending
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outcast's fitted pan and tilt, the fixture this panel was built for.
    fn outcast() -> [Axis; 2] {
        [
            Axis {
                params: motion::Params {
                    v_max: 500.0,
                    a_max: 800.0,
                    j_max: 50_000.0,
                    small: 0.0,
                    ..motion::Params::pan()
                },
                range: 540.0,
            },
            Axis {
                params: motion::Params {
                    v_max: 470.0,
                    a_max: 1500.0,
                    j_max: 70_000.0,
                    small: 0.0,
                    ..motion::Params::tilt()
                },
                range: 260.0,
            },
        ]
    }

    fn dish(speed: f32) -> Pattern {
        Pattern { speed, ..default() }
    }

    /// Aiming and landing must be exact inverses, or the shape the fixture draws
    /// is not the shape the panel plots.
    #[test]
    fn surface_and_aim_invert() {
        for centre in [Vec2::new(0.0, -86.495), Vec2::new(37.0, 20.0), Vec2::new(-120.0, 60.0)] {
            let pattern = Pattern { centre, throw: 2.0, size: 0.4, ..default() };
            let surface = pattern.surface(64);
            let back = pattern.landed(&pattern.aims(&surface));
            assert!(peak(&surface, &back) < 1e-4, "{centre:?} round trip off by {}", peak(&surface, &back));
        }
    }

    /// A circle on the surface must stay a circle: every point the same distance
    /// from the middle. This is what the pan/tilt sweep got wrong.
    #[test]
    fn circle_stays_round() {
        let pattern = dish(0.5);
        let landed = pattern.landed(&pattern.aims(&pattern.surface(96)));
        let radii = landed.iter().map(|p| p.length());
        let (lo, hi) = (
            radii.clone().fold(f32::MAX, f32::min),
            radii.fold(f32::MIN, f32::max),
        );
        assert!(hi - lo < 1e-4, "radius wandered {lo:.5}..{hi:.5} m");
        assert!((hi - pattern.size).abs() < 1e-4, "radius {hi:.5} not {:.5}", pattern.size);
    }

    /// The bug this was rewritten for: sweeping a circle in pan/tilt degrees
    /// about tilt zero puts the beam on the pan axis twice a lap, which is the
    /// figure eight seen on the ceiling. Aiming off the axis must not.
    #[test]
    fn shape_keeps_off_the_pan_axis() {
        let naive: Vec<Vec2> = (0..64).map(|i| 20.0 * Shape::Circle.at(i as f32 / 64.0)).collect();
        let closest = |aims: &[Vec2]| aims.iter().map(|a| a.y.abs()).fold(f32::MAX, f32::min);
        assert!(closest(&naive) < 1.0, "a pan/tilt sweep should reach the axis");

        let pattern = dish(0.5);
        let aims = pattern.aims(&pattern.surface(64));
        assert!(closest(&aims) > 70.0, "surface shape strayed to {:.1} deg", closest(&aims));
    }

    /// The requested setup: level and square to the wall, so the circle sits
    /// centred on the aim and reaches exactly as far up and down as its radius
    /// subtends at the throw. The radius itself is left free to tune.
    #[test]
    fn default_matches_the_asked_for_aim() {
        let pattern = dish(0.5);
        let aims = pattern.aims(&pattern.surface(256));
        let tilts = aims.iter().map(|a| a.y);
        let (lo, hi) = (tilts.clone().fold(f32::MAX, f32::min), tilts.fold(f32::MIN, f32::max));
        let half = (pattern.size / pattern.throw).atan().to_degrees();
        assert!((lo - (pattern.centre.y - half)).abs() < 0.1, "tilt low {lo:.2}, half {half:.2}");
        assert!((hi - (pattern.centre.y + half)).abs() < 0.1, "tilt high {hi:.2}, half {half:.2}");
        // Near the equator azimuth and polar angle are locally isotropic, so a
        // round shape needs about as much pan as tilt.
        let pans = aims.iter().map(|a| a.x);
        let sweep = pans.clone().fold(f32::MIN, f32::max) - pans.fold(f32::MAX, f32::min);
        assert!((sweep - (hi - lo)).abs() < 1.5, "pan sweep {sweep:.2} vs tilt {:.2}", hi - lo);
    }

    /// Pan must come out continuous even where the shape crosses the back of the
    /// fixture, or the axis would be asked for a 360 degree slam mid-lap.
    #[test]
    fn pan_unwraps_across_the_cut() {
        let across = Pattern { centre: Vec2::new(180.0, 40.0), ..default() };
        let aims = across.aims(&across.surface(128));
        let worst = aims.windows(2).map(|w| (w[1].x - w[0].x).abs()).fold(0.0, f32::max);
        assert!(worst < 30.0, "pan jumped {worst:.1} deg between packets");
    }

    /// Sending the shape straight out leaves the beam trailing it.
    #[test]
    fn raw_lags() {
        let (axes, pattern) = (outcast(), dish(0.8));
        let surface = pattern.surface(Pattern::steps(0.8));
        let aims = pattern.aims(&surface);
        let landed = pattern.landed(&Pattern::run(&aims, axes, 0.8));
        assert!(peak(&surface, &landed) > 0.02, "expected visible lag");
    }

    /// Solving for the command puts the beam on the shape instead.
    #[test]
    fn computed_tracks() {
        let (axes, pattern) = (outcast(), dish(0.8));
        let surface = pattern.surface(Pattern::steps(0.8));
        let aims = pattern.aims(&surface);
        let raw = peak(&surface, &pattern.landed(&Pattern::run(&aims, axes, 0.8)));
        let solved =
            peak(&surface, &pattern.landed(&Pattern::solve(&aims, axes, 0.8).1));
        assert!(solved < raw / 4.0, "solved {solved:.4} vs raw {raw:.4}");
    }

    /// Without a blurred correction this loop diverges spectacularly, reaching
    /// hundreds of degrees within ten passes because it learns detail the head
    /// cannot reproduce. Blurred, it is bounded but not monotone: the profile's
    /// arrive window is a discontinuity, so passes wander inside a band rather
    /// than settling. Hence the solver keeping its best pass, and hence this
    /// testing the best rather than the last.
    #[test]
    fn solve_does_not_diverge() {
        let (axes, pattern) = (outcast(), dish(0.5));
        let surface = pattern.surface(Pattern::steps(0.5));
        let target = pattern.aims(&surface);
        let n = target.len();
        let mut command = target.clone();
        let mut actual = Pattern::run(&command, axes, 0.5);
        let lag = Pattern::lag(&target, &actual);
        let first = peak(&target, &actual);
        let mut best = first;

        for pass in 0..3 * PASSES {
            let mut fix: Vec<Vec2> =
                (0..n).map(|i| target[(i + lag) % n] - actual[(i + lag) % n]).collect();
            blur(&mut fix);
            for i in 0..n {
                command[i] = Pattern::quantise(command[i] + GAIN * fix[i], axes);
            }
            actual = Pattern::run(&command, axes, 0.5);
            let got = peak(&target, &actual);
            assert!(got <= first, "pass {pass} went backwards: {got:.4} vs {first:.4}");
            best = best.min(got);
        }
        assert!(best < first / 4.0, "best {best:.4} vs first {first:.4}");
    }

    /// The cache key used to hold only speed, acceleration, jerk and smoothing,
    /// so moving any other limit in the Tuning section left the stale solve on
    /// screen. It must record the whole of each axis, including the creep limits
    /// the panel deliberately ignores: keeping the key complete costs nothing and
    /// means a limit that starts mattering cannot go stale unnoticed.
    #[test]
    fn cache_key_records_every_limit() {
        let axes = outcast();
        for tweak in [
            |p: &mut motion::Params| p.v_max = 200.0,
            |p: &mut motion::Params| p.a_max = 300.0,
            |p: &mut motion::Params| p.j_max = 9_000.0,
            |p: &mut motion::Params| p.smooth = 0.05,
            |p: &mut motion::Params| p.k_small = 0.2,
            |p: &mut motion::Params| p.small = 90.0,
        ] {
            let mut moved = axes;
            moved.iter_mut().for_each(|a| tweak(&mut a.params));
            assert!(moved != axes, "the tweak changed nothing");

            let mut pattern = Pattern { computed: true, ..default() };
            pattern.refresh(axes);
            pattern.refresh(moved);
            let stamp = pattern.stamp.as_ref().unwrap();
            assert_eq!(stamp.axes, moved, "a limit moved but the cache kept the old key");
        }
    }

    /// And a limit that binds must actually land in a different solve. The
    /// values matter: this shape only asks for about 32 deg/s, so capping speed
    /// at 200 changes nothing and proves nothing.
    #[test]
    fn tuning_a_limit_that_binds_redoes_the_solve() {
        let axes = outcast();
        let solve = |axes: [Axis; 2]| {
            let mut pattern = Pattern { computed: true, ..default() };
            pattern.refresh(axes);
            pattern.solved.as_ref().unwrap().actual.clone()
        };
        let before = solve(axes);
        for tweak in [
            |p: &mut motion::Params| p.v_max = 25.0,
            |p: &mut motion::Params| p.a_max = 60.0,
            |p: &mut motion::Params| p.j_max = 500.0,
            |p: &mut motion::Params| p.smooth = 0.05,
        ] {
            let mut moved = axes;
            moved.iter_mut().for_each(|a| tweak(&mut a.params));
            assert!(solve(moved) != before, "a limit that binds left the solve unchanged");
        }
    }

    /// Creep must never drive the lap, however the fixture is tuned: below its
    /// threshold the profile crawls instead of ramping, which would take over
    /// the whole path and stall the solve.
    #[test]
    fn creep_is_ignored() {
        let axes = outcast();
        let mut creeping = axes;
        creeping.iter_mut().for_each(|a| {
            a.params.small = 180.0;
            a.params.k_small = 0.5;
        });
        let pattern = dish(0.5);
        let aims = pattern.aims(&pattern.surface(Pattern::steps(0.5)));
        assert_eq!(
            Pattern::run(&aims, axes, 0.5),
            Pattern::run(&aims, creeping, 0.5),
            "creep limits leaked into the run"
        );
    }
}

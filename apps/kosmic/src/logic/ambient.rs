//! Ambient: the sun runs the rig. Daytime parks the movers lens-down out of
//! the dust, golden hour swings them up and brings the beams to full just
//! before sunset, and from sundown a loop of low energy patterns carries the
//! night.

use std::time::{SystemTime, UNIX_EPOCH};

use lib::prelude::*;

use super::look::{Gobo, Pose};
use super::{State, Swatch};
use crate::blackout::Blackout;
use crate::dmx::Patch;
use crate::home::Home;
use crate::lights::{Lights, Wheels};
use crate::rig::Trim;
use crate::sim::Movers;

/// Black Rock City wall clock, PDT.
const UTC_OFFSET: i64 = -7 * 3600;

/// Local (sunrise, sunset) in hours, one row per day from `DAY0` on. NOAA
/// solar times for the Man site, 40.786 N 119.204 W.
const DAY0: i64 = 20692; // 2026-08-27, in days since the epoch
#[rustfmt::skip]
const SUN: [(f32, f32); 14] = [
    (6.32, 19.61), (6.33, 19.59), (6.35, 19.56), (6.37, 19.53), // aug 27-30
    (6.38, 19.51), (6.40, 19.48), (6.42, 19.45), (6.43, 19.43), // aug 31 - sep 3
    (6.45, 19.40), (6.46, 19.37), (6.48, 19.34), (6.50, 19.32), // sep 4-7
    (6.51, 19.29), (6.53, 19.26),                               // sep 8-9
];

/// Hours before sunset the rig starts waking up.
const RAMP: f32 = 1.5;

/// The pace the patterns run at; phase here is decoration, not a beat.
pub const BPM: f32 = 30.0;

/// Seconds each night pattern holds, and the dip either side of a swap.
const HOLD: f32 = 240.0;
const FADE: f32 = 5.0;

/// Ceiling for the night show: passively interesting, not a set.
const LEVEL: f32 = 0.5;

/// The two edge colorados are aimed at the seahorses: what they run at flat
/// out, and the colour they hold.
const SEAHORSE: Rgbw = Rgbw::HOUSE;
const SEAHORSE_LEVEL: f32 = 0.6;

/// Golden hour's colour.
const DUSK: Swatch = Swatch::HOUSE;

enum Sky {
    Day,
    /// How far through the pre-sunset ramp, `0..1`.
    Dusk(f32),
    Night,
}

/// The panel can pin one look instead of following the sun.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Force {
    /// Follow the sun.
    Auto,
    Day,
    /// Dusk held at the panel's ramp position.
    Dusk,
    /// One night pattern, by index, with no swap fades.
    Pattern(usize),
}

fn clock() -> (i64, f32) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let local = secs + UTC_OFFSET;
    (local.div_euclid(86400), local.rem_euclid(86400) as f32 / 3600.0)
}

fn sun(day: i64) -> (f32, f32) {
    SUN[(day - DAY0).clamp(0, SUN.len() as i64 - 1) as usize]
}

/// The wall clock hour, for the panel's time slider.
pub fn hour_now() -> f32 {
    clock().1
}

/// The hour a pinned dusk position stands for, for the same slider.
pub fn dusk_hour(fr: f32) -> f32 {
    let (day, _) = clock();
    let (_, sunset) = sun(day);
    sunset - RAMP * (1.0 - fr)
}

fn sky(over: Option<f32>) -> Sky {
    let (day, hour) = clock();
    let hour = over.unwrap_or(hour);
    let (sunrise, sunset) = sun(day);
    match hour {
        h if h < sunrise || h >= sunset => Sky::Night,
        h if h >= sunset - RAMP => Sky::Dusk((h - (sunset - RAMP)) / RAMP),
        _ => Sky::Day,
    }
}

/// The seahorse pair's slow fade: `-1..1` over ~8 seconds.
fn breathe(t: f32) -> f32 {
    (t / 8.0).fsin(1.0) * 2.0 - 1.0
}

pub fn render(
    s: &State,
    l: &mut Lights,
    movers: &Movers,
    patch: &Patch,
    blackout: &Blackout,
    home: &Home,
    trim: &Trim,
    dt: f32,
    wheels: &mut Wheels,
    universe: &mut Universe,
) {
    l.reset();
    // The hydro's defaults are lit (open shutter, full dimmer): ambient
    // starts every frame dark and turns on only what it means to.
    for h in &mut l.hydros {
        h.alpha = 0.0;
    }
    match s.ambient_force {
        Force::Day => day(l, home),
        Force::Dusk => dusk(s, l, home, s.ambient_fr),
        Force::Pattern(i) => night(s, l, home, Some(i)),
        Force::Auto => match sky(s.ambient_override.then_some(s.ambient_hour)) {
            Sky::Day => day(l, home),
            Sky::Dusk(fr) => dusk(s, l, home, fr),
            Sky::Night => night(s, l, home, None),
        },
    }

    for o in &mut l.outcasts {
        o.ring = o.ring * (trim.ring * trim.global);
        o.center = o.center * (trim.beam * trim.global);
    }
    for h in &mut l.hydros {
        h.alpha *= trim.hydro * trim.global;
    }
    for p in &mut l.pars {
        p.color = p.color * (trim.colorado * trim.global);
    }

    wheels.mask(&mut l.hydros, dt);
    l.encode(patch, blackout, home, movers, universe);
}

/// Ambient aims are poses off the rest, mirrored: every mover rests pointing
/// straight up, so pitch 0 is the zenith, +90 straight out at the crowd, and
/// negative pitch leans back over the stage. Positive yaw swings each of a
/// pair outward to its own side, so one number mirrors left and right.
///
/// The heads sit a few feet off the ground: the crowd side is faces and the
/// neighbouring camp, so a lit beam's forward pitch stops well above eye
/// level. The park is forward-down for the dust, reached dark.
const SKY_MAX: f32 = 35.0;

/// Lens forward and down, out of the dust.
const PARK: Pose = Pose { pitch: 135.0, yaw: 0.0 };

/// Aim mover `i` `pitch`/`yaw` degrees off its rest, mirrored, the forward
/// tip capped at [`SKY_MAX`].
fn point(l: &mut Lights, home: &Home, i: usize, pitch: f32, yaw: f32) {
    let pose = Pose { pitch: pitch.min(SKY_MAX), yaw };
    let (p, y) = pose.aim(&home.movers[i], i, true);
    match i {
        0 | 1 => (l.outcasts[i].pitch, l.outcasts[i].yaw) = (p, y),
        _ => (l.hydros[i - 2].pitch, l.hydros[i - 2].yaw) = (p, y),
    }
}

/// Parked lens-down out of the dust, dark but for the seahorses.
fn day(l: &mut Lights, home: &Home) {
    for i in 0..4 {
        let (p, y) = PARK.aim(&home.movers[i], i, true);
        match i {
            0 | 1 => (l.outcasts[i].pitch, l.outcasts[i].yaw) = (p, y),
            _ => (l.hydros[i - 2].pitch, l.hydros[i - 2].yaw) = (p, y),
        }
    }
    for p in &mut l.pars[..2] {
        p.color = SEAHORSE * SEAHORSE_LEVEL;
    }
}

/// The rig wakes: the beams swing up out of the forward park, and the
/// families join one after another — centre pars, outer pars, hydros, then
/// the outcasts — each only once its beam is well clear of faces, all full
/// just before the sun is really gone. Aimed past [`point`]'s cap on
/// purpose: the low half of the swing carries no light.
fn dusk(s: &State, l: &mut Lights, home: &Home, fr: f32) {
    let pitch = fr.inout_quad().lerp(PARK.pitch..0.0);
    // Nothing lights below 35 degrees above the horizon.
    let lift = ((55.0 - pitch) / 30.0).clamp(0.0, 1.0);
    let join = |from: f32| ((fr - from) / 0.15).clamp(0.0, 1.0).in_quad();
    let bright = (fr / 0.9).min(1.0).in_quad();
    for i in 0..4 {
        let (p, y) = Pose { pitch, yaw: 0.0 }.aim(&home.movers[i], i, true);
        let gain = lift
            * bright
            * match i {
                0 | 1 => join(0.75),
                _ => join(0.55),
            };
        match i {
            0 | 1 => {
                let o = &mut l.outcasts[i];
                (o.pitch, o.yaw) = (p, y);
                o.ring = DUSK.rgbw * gain;
                o.center = o.ring;
            }
            _ => {
                let h = &mut l.hydros[i - 2];
                (h.pitch, h.yaw) = (p, y);
                h.alpha = gain;
                h.color = DUSK.one;
                h.color2 = DUSK.two;
                h.color_mask = true;
                h.focus = 0.5;
            }
        }
    }

    let sea = SEAHORSE_LEVEL + fr * 0.3 * breathe(s.t);
    for p in &mut l.pars[..2] {
        p.color = SEAHORSE * sea;
    }
    for (i, p) in l.pars.iter_mut().enumerate().skip(2) {
        let from = match i {
            3 | 4 => 0.2,
            _ => 0.4,
        };
        p.color = DUSK.rgbw * (bright * join(from) * 0.3);
    }
}

/// What each night pattern paints the seahorse pair, so the edges never
/// clash with the rest of the show.
const NIGHT_SEA: [Swatch; 5] =
    [Swatch::HOUSE, Swatch::BLUE, Swatch::ORANGE, Swatch::MINT, Swatch::LAVENDER];

/// From sundown: fixed patterns held for minutes each, crossed through dark,
/// the whole show breathing slowly under its ceiling. A forced pattern holds
/// with no swap fades.
fn night(s: &State, l: &mut Lights, home: &Home, force: Option<usize>) {
    let (idx, fade) = match force {
        Some(i) => (i, 1.0),
        None => {
            let slot_t = s.t % HOLD;
            let fade = (slot_t / FADE).min((HOLD - slot_t) / FADE).clamp(0.0, 1.0);
            ((s.t / HOLD) as usize % 5, fade)
        }
    };
    let level = LEVEL * fade * (s.t / 12.0).fsin(1.0).lerp(0.55..1.0);
    match idx {
        0 => waves(s, l, home, level),
        1 => circles(s, l, home, level),
        2 => backstage(s, l, home, level),
        3 => crossing(s, l, home, level),
        _ => orbit(s, l, home, level),
    }

    // The seahorse pair holds its fade all night, in the pattern's colour.
    let sea = SEAHORSE_LEVEL + 0.3 * breathe(s.t);
    for p in &mut l.pars[..2] {
        p.color = NIGHT_SEA[idx.min(4)].rgbw * sea;
    }
}

/// One colour on everything: beams through the movers, a drifting shimmer on
/// the pars.
fn fill(s: &State, l: &mut Lights, swatch: Swatch, level: f32, zoom: f32) {
    for o in &mut l.outcasts {
        o.ring = swatch.rgbw * level;
        o.center = o.ring;
        o.zoom = zoom;
    }
    for h in &mut l.hydros {
        let Rgbw(r, g, b, w) = swatch.rgbw;
        h.alpha = r.max(g).max(b).max(w) * swatch.spot as u8 as f32 * level;
        h.color = swatch.one;
        h.color2 = swatch.two;
        h.color_mask = true;
        h.focus = 0.5;
        h.zoom = zoom;
    }
    for (i, p) in l.pars.iter_mut().enumerate() {
        p.color = swatch.rgbw * (level * 0.6 * (s.t / 10.0 + i as f32 * 0.17).fsin(1.0).lerp(0.4..1.0));
    }
}

/// The colorados left to right as they hang, the side-edge pair at the ends.
const PAR_ORDER: [usize; 6] = [0, 2, 3, 4, 5, 1];

/// The beamwashes ride plainly up and down through the zenith while the
/// hydros lean diagonally out to their sides, drawing circles in the air
/// that rise and fall in step — but mostly one family at a time, the
/// outcasts surfacing as the hydros rest. The colorado zooms pulse in a
/// wave down the line.
fn waves(s: &State, l: &mut Lights, home: &Home, level: f32) {
    let (rise, swirl) = ((s.t / 24.0).ssin(1.0), (s.t / 24.0).scos(1.0));
    for i in 0..2 {
        point(l, home, i, 30.0 * rise, 0.0);
    }
    for i in 2..4 {
        point(l, home, i, 30.0 * rise, 25.0 + 12.0 * swirl);
    }
    fill(s, l, Swatch::HOUSE, level * 0.5, 1.0);
    let trade = (s.t / 80.0).fsin(1.0);
    for o in &mut l.outcasts {
        o.ring = o.ring * trade;
        o.center = o.center * trade;
    }
    for h in &mut l.hydros {
        h.alpha *= 1.0 - 0.75 * trade;
    }
    for (at, i) in PAR_ORDER.iter().enumerate() {
        l.pars[*i].zoom = (s.t / 6.0 + at as f32 * 0.15).fsin(1.0);
    }
}

/// Mirrored circles drawn round the zenith, each pair a quarter turn apart,
/// the pairs slowly trading who carries the light.
fn circles(s: &State, l: &mut Lights, home: &Home, level: f32) {
    for i in 0..4 {
        let ph = s.t / 20.0 + (i / 2) as f32 * 0.25;
        point(l, home, i, 18.0 * ph.ssin(1.0), 20.0 + 14.0 * ph.scos(1.0));
    }
    fill(s, l, Swatch::BLUE, level * 0.5, 0.0);
    let trade = (s.t / 100.0).fsin(1.0);
    for o in &mut l.outcasts {
        o.ring = o.ring * (1.0 - 0.75 * trade);
        o.center = o.center * (1.0 - 0.75 * trade);
    }
    for h in &mut l.hydros {
        h.alpha *= trade.max(0.2);
    }
}

/// Beams crossed overhead, swaying wide apart and back over each other.
fn crossing(s: &State, l: &mut Lights, home: &Home, level: f32) {
    let sway = (s.t / 30.0).fsin(1.0);
    for i in 0..4 {
        point(l, home, i, 12.0 + 10.0 * (s.t / 18.0).fsin(1.0), sway.lerp(-40.0..25.0));
    }
    fill(s, l, Swatch::MINT, level, 0.4);
}

/// Slow circles just off the zenith, winding a couple of turns forward then
/// smoothly back, each head a little behind the last: the reversal is a
/// turn, never a rewind.
fn orbit(s: &State, l: &mut Lights, home: &Home, level: f32) {
    for i in 0..4 {
        let wind = (s.t / 90.0).ssin(1.0) * 2.2 + i as f32 * 0.1;
        point(l, home, i, 12.0 + 6.0 * wind.scos(1.0), 18.0 * wind.ssin(1.0));
    }
    fill(s, l, Swatch::LAVENDER, level * 0.6, 0.2);
}

/// The outcasts keep a slow rock through the zenith while the hydros lean
/// back over the stage and grind a gobo round a figure of eight on it.
fn backstage(s: &State, l: &mut Lights, home: &Home, level: f32) {
    for o in &mut l.outcasts {
        o.ring = Swatch::ORANGE.rgbw * (level * 0.35);
        o.center = o.ring;
    }
    for i in 0..2 {
        let t = (s.t / 24.0 + i as f32 * 0.5).tri(1.0);
        point(l, home, i, (t - 0.5) * 30.0, 0.0);
    }
    for i in 2..4 {
        let t = s.t / 16.0;
        point(l, home, i, -60.0 + 12.0 * (t * 2.0).ssin(1.0), 10.0 + 14.0 * t.ssin(1.0));
        let h = &mut l.hydros[i - 2];
        h.alpha = level * 0.7;
        h.color = Swatch::ORANGE.one;
        h.color2 = Swatch::ORANGE.two;
        h.color_mask = true;
        h.gobo = Gobo::Lines;
        h.gobo_rot = 200;
        h.focus = 0.4;
    }
    for (i, p) in l.pars.iter_mut().enumerate() {
        p.color =
            Swatch::ORANGE.rgbw * (level * 0.3 * (s.t / 10.0 + i as f32 * 0.17).fsin(1.0).lerp(0.4..1.0));
    }
}

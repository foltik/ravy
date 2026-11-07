//! Where each mover rests: the aim it returns to with nothing to do, and what
//! the keyed patterns are written relative to.
//!
//! Pan and tilt snap to quarter turns and zoom to one end of its travel, since
//! a rest angle anywhere else leaves the axle uneven room to work in.

use lib::gdtf::GdtfDevice;
use lib::lights::fixture::{HydroSpot, OutcastBeamwash};
use lib::prelude::*;

use crate::dmx::{MOVERS, travel};

/// Saved rest aims, one `<fixture> <pitch> <yaw> <zoom> <flip pitch> <flip yaw>`
/// per line, the flips written as 0 or 1.
const HOME_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/home.txt");

/// The only angles a mover is allowed to rest at.
pub const SNAP: f32 = 90.0;

/// Where one mover rests, and which way round it works from there. Pitch and
/// yaw are degrees off the middle of the axle's travel, zoom the fraction the
/// lens sits at.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Rest {
    pub pitch: f32,
    pub yaw: f32,
    pub zoom: f32,
    /// Whether each axle is turned round, so a pair of movers either side of
    /// centre stage works as a mirror image rather than in parallel. Pitch
    /// first, then yaw.
    pub flip: [bool; 2],
}

impl Rest {
    /// What the fixture type itself declares, which is where it rests until the
    /// rig says otherwise.
    fn of<D: GdtfDevice>(travel: [f32; 2]) -> Self {
        let [yaw, pitch, zoom] = D::HOME;
        Self { pitch, yaw, zoom, flip: [false; 2] }.snapped(travel)
    }

    /// The nearest rest the axles will hold.
    pub fn snapped(self, travel: [f32; 2]) -> Self {
        Self {
            pitch: quarter(self.pitch, travel[1]),
            yaw: quarter(self.yaw, travel[0]),
            zoom: self.zoom.clamp(0.0, 1.0).round(),
            ..self
        }
    }

    /// Which way each axle runs: -1 where it is turned round, 1 where it is not.
    pub fn sign(&self) -> (f32, f32) {
        let sign = |flipped| match flipped {
            true => -1.0,
            false => 1.0,
        };
        (sign(self.flip[0]), sign(self.flip[1]))
    }

    /// The rest aim in degrees, as the axles actually reach it. A flipped axle
    /// is turned round whole, so the aim it comes to rest at is the other side
    /// of the middle of its travel.
    pub fn aim(&self) -> (f32, f32) {
        let (up, round) = self.sign();
        (up * self.pitch, round * self.yaw)
    }
}

/// The nearest quarter turn that is still inside an axle's travel.
pub fn quarter(angle: f32, travel: f32) -> f32 {
    let limit = (travel.abs() / 2.0 / SNAP).floor() * SNAP;
    ((angle / SNAP).round() * SNAP).clamp(-limit, limit)
}

#[derive(Resource)]
pub struct Home {
    pub movers: [Rest; MOVERS.len()],
}

impl Default for Home {
    fn default() -> Self {
        let mut home = Self { movers: declared() };
        home.reload();
        home
    }
}

/// The rest aims the mover types declare, in patch slot order.
fn declared() -> [Rest; MOVERS.len()] {
    [
        Rest::of::<OutcastBeamwash>(travel(0)),
        Rest::of::<OutcastBeamwash>(travel(1)),
        Rest::of::<HydroSpot>(travel(2)),
        Rest::of::<HydroSpot>(travel(3)),
    ]
}

impl Home {
    /// Drop back to what is on disk, leaving whatever it doesn't name at what
    /// the fixture declares.
    pub fn reload(&mut self) {
        self.movers = declared();
        let Ok(text) = std::fs::read_to_string(HOME_FILE) else {
            return;
        };
        for line in text.lines() {
            let mut field = line.split_whitespace();
            let Some(slot) = field.next().and_then(|n| MOVERS.iter().position(|m| *m == n)) else {
                continue;
            };
            let numbers: Vec<f32> = field.filter_map(|f| f.parse().ok()).collect();
            let &[pitch, yaw, zoom, pitch_flip, yaw_flip] = numbers.as_slice() else {
                warn!("home: {line} is not five numbers");
                continue;
            };
            let flip = [pitch_flip != 0.0, yaw_flip != 0.0];
            self.movers[slot] = Rest { pitch, yaw, zoom, flip }.snapped(travel(slot));
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let body: String = MOVERS
            .iter()
            .zip(&self.movers)
            .map(|(name, at)| {
                let [pitch, yaw] = at.flip.map(|flipped| flipped as u8);
                format!("{name} {} {} {} {pitch} {yaw}\n", at.pitch, at.yaw, at.zoom)
            })
            .collect();
        std::fs::write(HOME_FILE, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only quarter turns, and never one the axle cannot reach: tilt travels
    /// 260 or 270, so a quarter turn either side of the middle is as far as it
    /// goes. A reversed axle travels just as far.
    #[test]
    fn a_rest_is_a_quarter_turn_inside_the_travel() {
        assert_eq!(quarter(0.0, 260.0), 0.0);
        assert_eq!(quarter(50.0, 260.0), 90.0);
        assert_eq!(quarter(-130.0, 260.0), -90.0);
        assert_eq!(quarter(400.0, 540.0), 270.0);
        assert_eq!(quarter(-100.0, -540.0), -90.0);
    }

    /// Zoom rests at one end of its travel or the other.
    #[test]
    fn a_rest_zoom_is_either_end() {
        let at = |zoom| Rest { pitch: 0.0, yaw: 0.0, zoom, flip: [false; 2] };
        assert_eq!(at(0.6).snapped(travel(0)).zoom, 1.0);
        assert_eq!(at(0.4).snapped(travel(0)).zoom, 0.0);
    }
}

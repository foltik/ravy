use lib::lights::fixture::{SaberSpot, StealthBeam};
use lib::prelude::*;

use crate::{DiscoBall, FixtureChannel, FixtureIndex};

pub struct Lights<'a> {
    #[allow(unused)]
    x: Range,
    #[allow(unused)]
    z: Range,

    beams: Vec<Light<'a, StealthBeam>>,
    spots: Vec<Light<'a, SaberSpot>>,

    #[allow(unused)]
    disco: Vec3,
}

impl<'a> Lights<'a> {
    pub fn new<'w, 's>(
        beams_q: Query<'w, 's, (&mut StealthBeam, &Transform, &FixtureChannel, &FixtureIndex)>,
        spots_q: Query<'w, 's, (&mut SaberSpot, &Transform, &FixtureChannel, &FixtureIndex)>,
        disco_q: Query<'w, 's, &Transform, With<DiscoBall>>,
    ) -> Option<Lights<'a>>
    where
        'w: 'a,
    {
        let mut x = Range { lo: -f32::MIN, hi: f32::MAX };
        let mut z = Range { lo: -f32::MIN, hi: f32::MAX };

        let disco = disco_q.single().ok()?.translation;

        let mut beams = vec![];
        for (beam, transform, chan, idx) in beams_q {
            beams.push(Light {
                light: beam,
                channel: chan.0,
                row: idx.row,
                col: idx.col,
                i: idx.i,
                x: transform.translation.x,
                z: transform.translation.z,
                transform,
            });
            x.lo = x.lo.min(transform.translation.x);
            x.hi = x.hi.min(transform.translation.x);
            z.lo = z.lo.min(transform.translation.z);
            z.hi = z.hi.min(transform.translation.z);
        }

        let mut spots = vec![];
        for (spot, transform, chan, idx) in spots_q {
            spots.push(Light {
                light: spot,
                channel: chan.0,
                row: idx.row,
                col: idx.col,
                i: idx.i,
                x: transform.translation.x,
                z: transform.translation.z,
                transform,
            });
            x.lo = x.lo.min(transform.translation.x);
            x.hi = x.hi.min(transform.translation.x);
            z.lo = z.lo.min(transform.translation.z);
            z.hi = z.hi.min(transform.translation.z);
        }

        for beam in &mut beams {
            beam.x = beam.x.ilerp(x);
            beam.z = beam.z.ilerp(z);
        }
        for spot in &mut spots {
            spot.x = spot.x.ilerp(x);
            spot.z = spot.z.ilerp(z);
        }

        Some(Self { x, z, beams, spots, disco })
    }

    pub fn reset(&mut self) {
        self.for_each_spot(|spot, _i, _fr| *spot = Default::default());
        self.for_each_beam(|beam, _i, _fr| *beam = Default::default());
    }

    /// Spots one color, beams another
    pub fn split(&mut self, col0: Rgbw, col1: Rgbw) {
        self.for_each_spot(|spot, _i, _fr| spot.color = col0);
        self.for_each_beam(|beam, _i, _fr| beam.color = col1);
    }

    /// Apply a function to the color of each light
    pub fn map_colors(&mut self, mut f: impl FnMut(Rgbw) -> Rgbw) {
        self.for_each_spot(|spot, _i, _fr| spot.color = f(spot.color));
        self.for_each_beam(|beam, _i, _fr| beam.color = f(beam.color));
    }

    // Iterate through the lights, with additional index and fr (from 0 to 1) parameters.
    pub fn for_each_spot(&mut self, f: impl FnMut(&mut SaberSpot, usize, f32)) {
        Self::for_each(&mut self.spots, f);
    }
    pub fn for_each_beam(&mut self, f: impl FnMut(&mut StealthBeam, usize, f32)) {
        Self::for_each(&mut self.beams, f);
    }

    fn for_each<T>(slice: &mut [Light<T>], mut f: impl FnMut(&mut T, usize, f32)) {
        let n = slice.len();
        slice.iter_mut().enumerate().for_each(|(i, t)| f(t, i, i as f32 / n as f32));
    }

    pub fn send(&self, e131: &mut E131) {
        let mut dmx = vec![0; 232];

        for Light { light, channel, .. } in &self.beams {
            light.encode(&mut dmx[*channel..]);
        }
        for Light { light, channel, .. } in &self.spots {
            light.encode(&mut dmx[*channel..]);
        }

        e131.send(&dmx);
    }
}

#[allow(unused)]
pub struct Light<'a, T> {
    light: Mut<'a, T>,

    channel: usize,
    row: usize,
    col: usize,
    i: usize,

    x: f32,
    z: f32,

    transform: &'a Transform,
}

impl<'a, T> std::ops::Deref for Light<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.light
    }
}
impl<'a, T> std::ops::DerefMut for Light<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.light
    }
}

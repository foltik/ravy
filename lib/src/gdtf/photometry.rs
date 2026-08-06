//! Real photometric units for the emitters, so brightness and colour follow the
//! fixture rather than a look.
//!
//! Most of it comes out of the GDTF: emitter chromaticities and their relative
//! luminous intensity, the beam and field angles, the luminous flux. Where a
//! manufacturer's file disagreed with their own lab report, the file was
//! corrected rather than overridden here.
//!
//! What the format cannot hold is how any of that moves with zoom, and it moves
//! a lot: the Outcast makes 380 kcd zoomed in and 12 kcd out. `MEASURED` is the
//! lab's own zoom sweep, which has nowhere else to go.

use std::collections::HashMap;
use std::f32::consts::TAU;

use crate::prelude::*;

use super::file::Gdtf;

/// Beam profile, in the form bevy attenuates a spot light with: it squares a
/// linear ramp in cos(theta) between the inner and outer angles. Solving that
/// for half intensity at the beam angle and a tenth at the field angle puts the
/// rendered falloff on the two angles the manufacturer publishes.
#[derive(Clone, Copy)]
pub struct Profile {
    pub cos_inner: f32,
    pub cos_outer: f32,
}

impl Default for Profile {
    fn default() -> Self {
        Self::new(20.0, 25.0)
    }
}

impl Profile {
    /// From the full beam and field angles, in degrees.
    pub fn new(beam: f32, field: f32) -> Self {
        let field = field.clamp(0.02, 179.0);
        let beam = beam.clamp(0.01, field);
        let (hot, edge) = ((beam.to_radians() / 2.0).cos(), (field.to_radians() / 2.0).cos());
        // Ramp coefficients: 1/sqrt(2) of the ramp squares to a half, 1/sqrt(10)
        // to a tenth, and the two constraints fix its ends.
        let span = (hot - edge).max(1e-7);
        Self {
            cos_inner: (hot + 0.74934 * span).min(1.0),
            cos_outer: (hot - 1.80899 * span).max(-1.0),
        }
    }

    /// Half angle to the hot core, in radians.
    pub fn inner(&self) -> f32 {
        self.cos_inner.acos()
    }

    /// Half angle to the outer edge, in radians.
    pub fn outer(&self) -> f32 {
        self.cos_outer.acos()
    }

    /// Solid angle the squared ramp integrates to. Peak intensity in candela is
    /// the luminous flux divided by this.
    pub fn solid_angle(&self) -> f32 {
        TAU * ((self.cos_inner - self.cos_outer) / 3.0 + 1.0 - self.cos_inner)
    }
}

/// A fixture's zoom sweep, sampled fully in, half way, and fully out.
struct Measured {
    /// Full angle to half intensity, in degrees.
    beam: [f32; 3],
    /// Full angle to a tenth of it.
    field: [f32; 3],
    /// On-axis intensity in candela, with every emitter at full.
    ///
    /// Anchoring on this rather than on flux is what keeps the light the
    /// fixture throws down its axis exact. The cost lands in the outer falloff,
    /// where bevy's squared ramp cannot follow a real optic anyway: the hydro's
    /// is a top hat, flat to 75% out to 7 degrees and then a cliff.
    peak: [f32; 3],
}

/// Sources: ADJ's photometric reports for the hydro, Chauvet's IES files for
/// both others, all at full power with every emitter on.
const MEASURED: &[(&str, Measured)] = &[
    (
        "Hydro Spot 1",
        Measured {
            beam: [11.2, 16.0, 18.7],
            field: [13.1, 18.6, 21.7],
            peak: [160638.0, 131459.0, 96745.0],
        },
    ),
    (
        "COLORado 1 Solo",
        Measured {
            beam: [5.15, 24.83, 48.76],
            field: [9.20, 33.61, 59.54],
            peak: [65458.0, 5172.0, 1352.0],
        },
    ),
    (
        "Rogue Outcast 1 BeamWash",
        Measured {
            beam: [3.88, 15.74, 36.00],
            field: [5.88, 21.22, 49.77],
            peak: [380274.0, 44350.0, 12368.0],
        },
    ),
];

/// One LED colour.
#[derive(Clone, Copy)]
pub struct Led {
    /// Linear sRGB at a luminance of 1, so scaling it by lumens stays photometric.
    pub rgb: Vec3,
    /// Luminous intensity relative to the fixture's other emitters.
    pub weight: f32,
}

pub struct Photometry {
    emitters: HashMap<String, Led>,
    /// Summed weight of the mixing emitters, which is what "everything at full"
    /// adds up to before any derating.
    pub full: f32,
    /// What the fixture actually reaches with everything at full, as a fraction
    /// of `full`. LEDs lose a little when they all run at once.
    derate: f32,
    measured: Option<&'static Measured>,
}

impl Photometry {
    pub fn new(gdtf: &Gdtf) -> Self {
        let emitters: HashMap<String, Led> = gdtf
            .emitters
            .iter()
            .map(|e| {
                let led = Led { rgb: xy_to_rgb(e.color.x, e.color.y), weight: e.intensity };
                (e.name.to_lowercase(), led)
            })
            .collect();

        let mixed: f32 = ["red", "green", "blue", "white"]
            .iter()
            .filter_map(|n| emitters.get(*n))
            .map(|l| l.weight)
            .sum();
        let all_on = emitters
            .iter()
            .find(|(name, _)| name.contains("all on"))
            .map(|(_, led)| led.weight)
            .filter(|w| *w > 0.0);

        // Nominal weights: no emitter data, so fall back to primaries that add
        // up to white.
        let full = match mixed > 0.0 {
            true => mixed,
            false => 2.0,
        };
        Self {
            emitters,
            full,
            derate: all_on.map_or(1.0, |w| (w / full).clamp(0.1, 1.0)),
            measured: MEASURED.iter().find(|(name, _)| gdtf.name.contains(name)).map(|(_, m)| m),
        }
    }

    /// Output relative to every emitter at full, from a summed emitter weight.
    /// Ramps the derating in so one emitter alone lands on its own measured
    /// figure and all of them together land on the fixture's "all on" one.
    pub fn output(&self, weight: f32) -> f32 {
        let share = weight / self.full;
        share * (1.0 - share * (1.0 - self.derate)) / self.derate
    }

    /// The LED a colour-mixing attribute drives. Without emitter data, sRGB's
    /// own primaries stand in, weighted so all four at full come out white.
    pub fn led(&self, attribute: &str) -> Led {
        let (name, xy, weight) = match attribute {
            "ColorAdd_R" => ("red", (0.64, 0.33), 0.2126),
            "ColorAdd_G" => ("green", (0.30, 0.60), 0.7152),
            "ColorAdd_B" => ("blue", (0.15, 0.06), 0.0722),
            _ => ("white", (0.3127, 0.3290), 1.0),
        };
        match self.emitters.get(name) {
            Some(led) => *led,
            None => Led { rgb: xy_to_rgb(xy.0, xy.1), weight },
        }
    }

    /// Source colour of a fixture that has no colour mixing.
    pub fn source(&self) -> Vec3 {
        match self.emitters.len() {
            1 => self.emitters.values().next().unwrap().rgb,
            _ => self.emitters.get("white").map_or(Vec3::ONE, |led| led.rgb),
        }
    }

    /// Beam angle, field angle and on-axis intensity at a zoom position, 0
    /// fully in and 1 fully out. Without a sweep, the GDTF's own angles and
    /// flux stand in; they describe the wide end.
    pub fn beam(&self, zoom: f32, nominal: (f32, f32, f32)) -> (f32, f32, f32) {
        let Some(m) = self.measured else {
            let (beam, field, flux) = nominal;
            return (beam, field, flux / Profile::new(beam, field).solid_angle());
        };
        let travel = zoom.clamp(0.0, 1.0) * 2.0;
        let (i, f) = ((travel as usize).min(1), travel - (travel as usize).min(1) as f32);
        let between = |v: [f32; 3]| v[i] + (v[i + 1] - v[i]) * f;
        // Intensity spans more than an order of magnitude across the travel,
        // so it interpolates geometrically where the angles do so linearly.
        let peak = m.peak[i] * (m.peak[i + 1] / m.peak[i]).powf(f);
        (between(m.beam), between(m.field), peak)
    }

    pub fn is_measured(&self) -> bool {
        self.measured.is_some()
    }
}

/// CIE xy chromaticity to linear sRGB at a luminance of 1. Out-of-gamut
/// primaries clip to the nearest displayable hue and keep their luminance,
/// which matters for the deep blue LEDs these fixtures use.
pub fn xy_to_rgb(x: f32, y: f32) -> Vec3 {
    if y <= 1e-4 {
        return Vec3::ONE;
    }
    let (big_x, big_z) = (x / y, (1.0 - x - y) / y);
    let rgb = Vec3::new(
        3.2406 * big_x - 1.5372 - 0.4986 * big_z,
        -0.9689 * big_x + 1.8758 + 0.0415 * big_z,
        0.0557 * big_x - 0.2040 + 1.0570 * big_z,
    )
    .max(Vec3::ZERO);

    let luminance = rgb.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    match luminance > 1e-4 {
        true => rgb / luminance,
        false => Vec3::ONE,
    }
}

/// Luminance of a linear sRGB triple.
pub fn luminance(rgb: Vec3) -> f32 {
    rgb.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

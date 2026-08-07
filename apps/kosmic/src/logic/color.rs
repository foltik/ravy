//! Colour the rig can actually make.
//!
//! The Outcasts and the pars mix rgbw, so they land on whatever colour a look
//! asks for. The Hydros have no mixing at all: their colour is two dichroic
//! wheels, and every wheel stops between its slots as well as on them, showing
//! two filters side by side across the gate. So a colour of the show is an
//! rgbw and the wheel positions that stand for it.

pub use lib::lights::fixture::{HydroSpotWheel1 as Wheel1, HydroSpotWheel2 as Wheel2};
use lib::prelude::*;

///////////////////////// SWATCH /////////////////////////

/// A colour of the show: what the mixing fixtures render, and where the
/// Hydros' two wheels sit to stand for it. Both wheels are in the beam, so a
/// slot on each stacks two filters and passes almost nothing; a swatch picks
/// one wheel and leaves the other open.
#[derive(Clone, Copy, Debug)]
pub struct Swatch {
    pub rgbw: Rgbw,
    pub one: Wheel1,
    pub two: Wheel2,
    /// Whether the Hydros show this colour at all. A colour they cannot follow
    /// is better sat out than approximated.
    pub spot: bool,
}

impl Swatch {
    const fn one(rgbw: Rgbw, one: Wheel1) -> Self {
        Self { rgbw, one, two: Wheel2::Open, spot: true }
    }

    const fn two(rgbw: Rgbw, two: Wheel2) -> Self {
        Self { rgbw, one: Wheel1::Open, two, spot: true }
    }

    /// The same colour with both wheels parked open, for looks that change
    /// faster than a wheel can travel: the mixing fixtures still take it, the
    /// Hydros stay white rather than chase it.
    pub const fn open(self) -> Self {
        Self { one: Wheel1::Open, two: Wheel2::Open, ..self }
    }

    /// The same colour with the Hydros out of it entirely.
    pub const fn dark(self) -> Self {
        Self { spot: false, ..self.open() }
    }

    /// The closest the wheels can get to a colour the show computed rather than
    /// named. Whichever wheel comes nearer does the work and the other stays
    /// open, since both in the beam stack two filters and pass almost nothing.
    ///
    /// Only whole slots: a half step is two filters side by side across the
    /// gate, which is a split beam rather than the colour between them.
    pub fn nearest(rgbw: Rgbw) -> Self {
        let want = unit(rgbw.into());
        let best = |chart: &[Rgb; 9], dark: usize| {
            (0..9)
                .filter(|slot| *slot != dark)
                .map(|slot| (slot, distance(want, unit(chart[slot]))))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("the wheel has slots")
        };
        let (one, d1) = best(&WHEEL1, CONGO);
        let (two, d2) = best(&WHEEL2, UV);
        match d1 <= d2 {
            true => Self::one(rgbw, Wheel1::ALL[one * 2]),
            false => Self::two(rgbw, Wheel2::ALL[two * 2]),
        }
    }
}

///////////////////////// WHEEL COLOUR /////////////////////////

/// What each slot of colour wheel 1 puts on the wall, in wheel order.
#[rustfmt::skip]
const WHEEL1: [Rgb; 9] = [
    Rgb::WHITE, Rgb::RED, Rgb::BLUE, Rgb::LIME, Rgb::HOUSE, Rgb::ORANGE,
    Rgb(0.85, 0.9, 1.0), Rgb(1.0, 0.92, 0.8), Rgb(0.25, 0.0, 1.0),
];

/// The same for colour wheel 2.
#[rustfmt::skip]
const WHEEL2: [Rgb; 9] = [
    Rgb::WHITE, Rgb::MAGENTA, Rgb::CYAN, Rgb::PEA, Rgb::YELLOW,
    Rgb(0.7, 0.6, 1.0), Rgb::PINK, Rgb(0.35, 0.0, 0.9), Rgb(1.0, 0.78, 0.5),
];

/// The two that pass almost nothing. A match landing on either reads as a
/// fault rather than a colour, so nothing computed is sent there.
const CONGO: usize = 8;
const UV: usize = 7;

/// Scaled so the brightest channel is full: a dichroic passes what it passes,
/// and how hard the beam is driven through it is the dimmer's business.
fn unit(Rgb(r, g, b): Rgb) -> Rgb {
    match r.max(g).max(b) {
        max if max > 0.0 => Rgb(r / max, g / max, b / max),
        _ => Rgb::WHITE,
    }
}

fn distance(Rgb(r0, g0, b0): Rgb, Rgb(r1, g1, b1): Rgb) -> f32 {
    (r0 - r1).powi(2) + (g0 - g1).powi(2) + (b0 - b1).powi(2)
}

// Dichroics, not a mixer: each colour is the slot that carries it, or the half
// step between two slots where neither wheel has one. Violet is the gap: the
// only deep purple is the congo, which passes almost nothing.
#[rustfmt::skip]
impl Swatch {
    pub const WHITE:   Self = Self::one(Rgbw::WHITE,   Wheel1::Open);
    pub const RGBW:    Self = Self::one(Rgbw::RGBW,    Wheel1::Open);
    pub const HOUSE:   Self = Self::one(Rgbw::HOUSE,   Wheel1::Amber);
    pub const RED:     Self = Self::one(Rgbw::RED,     Wheel1::Red);
    pub const ORANGE:  Self = Self::one(Rgbw::ORANGE,  Wheel1::Orange);
    pub const YELLOW:  Self = Self::two(Rgbw::YELLOW,  Wheel2::Yellow);
    pub const PEA:     Self = Self::two(Rgbw::PEA,     Wheel2::Pea);
    pub const LIME:    Self = Self::one(Rgbw::LIME,    Wheel1::Green);
    pub const MINT:    Self = Self::two(Rgbw::MINT,    Wheel2::CyanPea);
    pub const CYAN:    Self = Self::two(Rgbw::CYAN,    Wheel2::Cyan);
    pub const BLUE:    Self = Self::one(Rgbw::BLUE,    Wheel1::Blue);
    pub const VIOLET:  Self = Self::two(Rgbw::VIOLET,  Wheel2::Magenta);
    pub const MAGENTA: Self = Self::two(Rgbw::MAGENTA, Wheel2::Magenta);
    pub const PINK:    Self = Self::two(Rgbw::PINK,    Wheel2::Pink);
    pub const UV:      Self = Self::two(Rgbw::VIOLET,  Wheel2::Uv);
    pub const CONGO:   Self = Self::one(Rgbw::VIOLET,  Wheel1::Congo);
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lib::gdtf::{Gdtf, GdtfDevice};
    use lib::lights::fixture::HydroSpot;

    use super::*;

    /// Every position has to land where the fixture's own chart says, half
    /// steps included: a byte a few off puts the wheel on the wrong colour,
    /// which nothing in the show would report.
    #[test]
    fn positions_land_on_their_slot() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fixtures/hydro_spot.gdtf");
        let gdtf = Gdtf::open(Path::new(path)).expect("the hydro is in the app's assets");
        let mode = gdtf.modes.iter().find(|m| m.name == HydroSpot::MODE).expect("22 CH mode");

        let check = |attribute: &str, bytes: Vec<u8>| {
            let channel =
                mode.channels.iter().find(|c| c.attribute == attribute).expect("colour channel");
            for (pos, byte) in bytes.into_iter().enumerate() {
                let (_, slot, offset) =
                    channel.slot(byte as u32).expect("a position selects a slot");
                assert_eq!(slot, pos / 2, "{attribute} position {pos}");
                assert_eq!(offset, if pos % 2 == 0 { 0.0 } else { 0.5 }, "{attribute} position {pos}");
            }
        };
        check("Color1", Wheel1::ALL.iter().map(|w| w.byte()).collect());
        check("Color2", Wheel2::ALL.iter().map(|w| w.byte()).collect());
    }

    /// The enums are named for what the fixture shows and the file is not, so
    /// what has to line up between them is the order.
    #[test]
    fn the_slots_are_in_chart_order() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fixtures/hydro_spot.gdtf");
        let gdtf = Gdtf::open(Path::new(path)).expect("the hydro is in the app's assets");
        let named = |wheel: &str, slot: usize| gdtf.wheels[wheel][slot].name.trim().to_string();

        assert_eq!(named("Color Wheel 1", Wheel1::Red as usize / 2), "Red");
        assert_eq!(named("Color Wheel 1", Wheel1::Amber as usize / 2), "Amber");
        assert_eq!(named("Color Wheel 2", Wheel2::Magenta as usize / 2), "Magenta");
        assert_eq!(named("Color Wheel 2", Wheel2::Yellow as usize / 2), "Yellow");

        // Where the file's name is not what the slot actually shows.
        assert_eq!(named("Color Wheel 1", Wheel1::Congo as usize / 2), "UV");
        assert_eq!(named("Color Wheel 2", Wheel2::Cyan as usize / 2), "Blue");
        assert_eq!(named("Color Wheel 2", Wheel2::Pea as usize / 2), "Green");
        assert_eq!(named("Color Wheel 2", Wheel2::Cold as usize / 2), "Light Purple");

        // A half step turns off one slot and onto the next.
        assert_eq!(named("Color Wheel 2", Wheel2::CyanPea as usize / 2), "Blue");
        assert_eq!(named("Color Wheel 2", Wheel2::CyanPea as usize / 2 + 1), "Green");
    }
}

#[cfg(test)]
mod nearest {
    use super::*;

    /// Both wheels sit in the same beam, so a match that engages two of them
    /// stacks the filters and passes almost nothing.
    #[test]
    fn only_one_wheel_ever_leaves_open() {
        for step in 0..64 {
            let hue = step as f32 / 64.0;
            let at = Swatch::nearest(Rgb::hsv(hue, 1.0, 1.0).into());
            assert!(
                at.one == Wheel1::Open || at.two == Wheel2::Open,
                "hue {hue} engaged {:?} and {:?}",
                at.one,
                at.two
            );
        }
    }

    /// Whole slots only: a half step shows two filters side by side rather
    /// than the colour between them.
    #[test]
    fn a_match_lands_on_a_slot() {
        for step in 0..64 {
            let at = Swatch::nearest(Rgb::hsv(step as f32 / 64.0, 1.0, 1.0).into());
            assert_eq!(at.one as usize % 2, 0);
            assert_eq!(at.two as usize % 2, 0);
        }
    }

    /// The named colours are the hand-picked half of the same map, so what is
    /// computed has to agree with them wherever a whole slot carries it.
    #[test]
    fn the_named_colours_map_to_themselves() {
        #[rustfmt::skip]
        let named = [
            Swatch::WHITE, Swatch::RED, Swatch::ORANGE, Swatch::YELLOW, Swatch::PEA,
            Swatch::LIME, Swatch::CYAN, Swatch::BLUE, Swatch::MAGENTA, Swatch::PINK,
            Swatch::HOUSE,
        ];
        for at in named {
            let near = Swatch::nearest(at.rgbw);
            assert_eq!((near.one, near.two), (at.one, at.two), "{:?}", at.rgbw);
        }
    }

    /// Nothing computed is sent to a filter that passes almost nothing.
    #[test]
    fn the_dark_filters_are_never_chosen() {
        for step in 0..64 {
            let at = Swatch::nearest(Rgb::hsv(step as f32 / 64.0, 1.0, 1.0).into());
            assert_ne!(at.one, Wheel1::Congo);
            assert_ne!(at.two, Wheel2::Uv);
        }
    }
}


// None are bound while the grid runs the mslive layout.
#![allow(dead_code)]

use lib::prelude::*;

use super::look::{Energy, Look, Movement};

///////////////////////// SPECIALS /////////////////////////

/// How a special activates.
#[derive(Clone, Copy, Debug)]
pub enum Fire {
    /// Active while the button is held.
    Hold,
    /// Fires for `pd` beats, then pops.
    OneShot { pd: Pd },
    /// Sticks until pressed again.
    Latch,
}

/// A one-off gesture: a partial look stacked on top of the base look.
#[derive(Clone, Copy)]
pub struct Special {
    pub name: &'static str,
    pub fire: Fire,
    pub color: Rgbw,
    pub look: Look,
}

/// An active special, keyed by the button that fired it.
pub struct Active {
    pub xy: (i8, i8),
    pub t0: f32,
    pub special: Special,
}

///////////////////////// THE SPECIALS /////////////////////////

#[rustfmt::skip]
pub const BLACKOUT: Special = Special { name: "blackout", fire: Fire::Hold, color: Rgbw::RED,
    look: Look { energy: Some(Energy::Off), ..Look::NONE } };

#[rustfmt::skip]
pub const WHITE_STROBE: Special = Special { name: "white strobe", fire: Fire::Hold, color: Rgbw::WHITE,
    look: Look { energy: Some(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }), color: Some(Rgbw::WHITE), ..Look::NONE } };

#[rustfmt::skip]
pub const RISER: Special = Special { name: "riser", fire: Fire::OneShot { pd: Pd(8, 1) }, color: Rgbw::MINT,
    look: Look { movement: Some(Movement::RaisingBeams), ..Look::NONE } };

#[rustfmt::skip]
pub const WHIRL: Special = Special { name: "whirl", fire: Fire::OneShot { pd: Pd(16, 1) }, color: Rgbw::CYAN,
    look: Look { movement: Some(Movement::Whirl), ..Look::NONE } };

#[rustfmt::skip]
pub const DJ: Special = Special { name: "dj", fire: Fire::Latch, color: Rgbw::YELLOW,
    look: Look { movement: Some(Movement::Center), ..Look::NONE } };

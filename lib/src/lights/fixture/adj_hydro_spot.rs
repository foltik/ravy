//! ADJ Hydro Spot 1
//!
//! Extended 22CH mode: 16-bit pan/tilt/dimmer/zoom, two color wheels, gobo
//! wheel, two prisms, motorized focus, and two frosts.
//!
//! https://www.adj.com/products/hydro-spot-1

use crate::gdtf::{GdtfDevice, WHEEL_SPEED, motion};
use crate::prelude::*;

#[derive(Clone, Copy, Debug, Component)]
pub struct HydroSpot {
    /// Tilt, `0..1` across 270°.
    pub pitch: f32,
    /// Pan, `0..1` across 540° or 630° (fixture menu setting).
    pub yaw: f32,
    /// Pan/tilt speed, `0..1` = fastest to slowest.
    pub speed: f32,

    pub color: HydroSpotWheel1,
    pub color2: HydroSpotWheel2,
    pub gobo: HydroSpotGobo,
    /// Hold the dimmer down while a colour wheel is turning, so a change is a
    /// cut rather than the gate sweeping past everything in between. Ours, not
    /// the fixture's: it has no such setting, so [`HydroSpotWheels`] does it on
    /// the way to the wire.
    pub color_mask: bool,
    /// The same for the gobo wheel: hold the dimmer down while it turns, so a
    /// change is a cut rather than the gate sweeping past every gobo in
    /// between. Off by default, since the sweep is usually worth seeing.
    pub gobo_mask: bool,
    /// How hard the gobo shakes in the gate, `0..1` slow to fast. Zero holds
    /// it still, and the open gate has nothing to shake.
    pub gobo_shake: f32,
    /// Raw gobo rotation byte: 0-127 indexing, then cw/ccw spin.
    pub gobo_rot: u8,

    /// Raw shutter byte. See [`Self::SHUTTER_OPEN`], [`Self::strobe`],
    /// [`Self::pulse`], [`Self::random_strobe`].
    pub shutter: u8,
    /// Dimmer.
    pub alpha: f32,

    pub prism: HydroSpotPrism,
    /// Prism rotation: clockwise at -1, still at 0, counter-clockwise at 1,
    /// fastest at either end.
    pub prism_rot: f32,
    /// Focus, `0..1`.
    pub focus: f32,
    /// Zoom, `0..1` = narrow to wide.
    pub zoom: f32,
    /// Heavy frost, `0..1`.
    pub frost: f32,
    /// Medium frost, `0..1`.
    pub frost2: f32,

    /// Raw dimmer mode byte: 0-20 unit default, then Standard/Stage/TV/
    /// Architectural/Theater/Stage 2, 141-160 dim speed fast to slow.
    pub dimmer_mode: u8,
    /// Raw dim curve byte: 0-20 square, 21-40 linear, 41-60 inverse square,
    /// 61-80 S curve.
    pub dim_curve: u8,
}

/// How long each wheel still has to travel, so the output can be held down
/// while it does. Changing slot swings the wheel past everything in between,
/// and the fixture will not blank itself for it.
///
/// Fed the values on their way to the wire, so what it reports covers the rig
/// and the sim alike.
#[derive(Clone, Copy, Debug, Default)]
pub struct HydroSpotWheels {
    /// Seconds left on each wheel: the two colour wheels, then the gobo wheel.
    left: [f32; 3],
    /// Where each was last sent, in slots.
    at: [f32; 3],
}

impl HydroSpotWheels {
    /// Slots on each wheel. Both colour wheels carry the same chart.
    const SLOTS: [f32; 3] = [9.0, 9.0, 7.0];

    /// Moves no longer than this are left alone. Neighbouring slots only sweep
    /// the edge between them past the gate, which is what the beam shows for a
    /// half step anyway, and the palette flashes between them faster than a cut
    /// would read.
    const ADJACENT: f32 = 1.0;

    /// Follow where the wheels are being sent, let `dt` pass, and hold the
    /// dimmer down while one is crossing to a slot that is not its neighbour.
    pub fn mask(&mut self, light: &mut HydroSpot, dt: f32) {
        // Colour positions count the half steps between slots, so a slot is two
        // apart; the gobo wheel has no half steps.
        let sent = [
            light.color as usize as f32 / 2.0,
            light.color2 as usize as f32 / 2.0,
            light.gobo as usize as f32,
        ];
        for (i, left) in self.left.iter_mut().enumerate() {
            let slots = Self::SLOTS[i];
            if sent[i] != self.at[i] {
                // Whichever way round is shorter, at the speed a wheel turns.
                let apart = (sent[i] - self.at[i]).rem_euclid(slots);
                let apart = apart.min(slots - apart);
                *left = match apart > Self::ADJACENT {
                    true => apart * (360.0 / slots) / WHEEL_SPEED,
                    false => 0.0,
                };
                self.at[i] = sent[i];
            }
            *left = (*left - dt).max(0.0);
        }
        let masked = (light.color_mask && self.left[0..2].iter().any(|&left| left > 0.0))
            || (light.gobo_mask && self.left[2] > 0.0);
        if masked {
            light.alpha = 0.0;
        }
    }
}

/// First DMX value of each position on a colour wheel. Both wheels use the
/// same chart, and rotation takes over at 128.
const CHART: [u8; 19] =
    [0, 10, 17, 24, 31, 38, 45, 52, 59, 66, 73, 80, 87, 94, 101, 108, 115, 122, 128];

/// Middle of a position's range, which is the value least likely to land on a
/// neighbour.
const fn wheel_byte(pos: usize) -> u8 {
    (CHART[pos] + CHART[pos + 1] - 1) / 2
}

/// Colour wheel 1, in wheel order: each slot, then the half step where the
/// gate shows it alongside the next one. Named for what the fixture puts on
/// the wall, which is not always what its chart calls the slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum HydroSpotWheel1 {
    #[default]
    Open,
    OpenRed,
    Red,
    RedBlue,
    Blue,
    BlueGreen,
    Green,
    GreenAmber,
    Amber,
    AmberOrange,
    Orange,
    OrangeCold,
    /// Chart: High CRI.
    Cold,
    ColdWarm,
    /// Chart: CTB 5600K.
    Warm,
    WarmCongo,
    /// Deep blue. Chart: UV.
    Congo,
    CongoOpen,
}

/// Colour wheel 2, in wheel order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum HydroSpotWheel2 {
    #[default]
    Open,
    OpenMagenta,
    Magenta,
    MagentaCyan,
    /// Chart: Blue.
    Cyan,
    CyanPea,
    /// Chart: Green.
    Pea,
    PeaYellow,
    Yellow,
    YellowCold,
    /// Chart: Light Purple.
    Cold,
    ColdPink,
    Pink,
    PinkUv,
    Uv,
    UvWarm,
    /// Chart: CTO 3200K.
    Warm,
    WarmOpen,
}

#[rustfmt::skip]
impl HydroSpotWheel1 {
    /// Every position, in wheel order.
    pub const ALL: [Self; 18] = [
        Self::Open, Self::OpenRed, Self::Red, Self::RedBlue, Self::Blue, Self::BlueGreen,
        Self::Green, Self::GreenAmber, Self::Amber, Self::AmberOrange, Self::Orange,
        Self::OrangeCold, Self::Cold, Self::ColdWarm, Self::Warm, Self::WarmCongo, Self::Congo,
        Self::CongoOpen,
    ];

    pub fn byte(self) -> u8 {
        wheel_byte(self as usize)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Open        => "open",
            Self::OpenRed     => "open/red",
            Self::Red         => "red",
            Self::RedBlue     => "red/blue",
            Self::Blue        => "blue",
            Self::BlueGreen   => "blue/green",
            Self::Green       => "green",
            Self::GreenAmber  => "green/amber",
            Self::Amber       => "amber",
            Self::AmberOrange => "amber/orange",
            Self::Orange      => "orange",
            Self::OrangeCold  => "orange/cold",
            Self::Cold        => "cold",
            Self::ColdWarm    => "cold/warm",
            Self::Warm        => "warm",
            Self::WarmCongo   => "warm/congo",
            Self::Congo       => "congo",
            Self::CongoOpen   => "congo/open",
        }
    }
}

#[rustfmt::skip]
impl HydroSpotWheel2 {
    /// Every position, in wheel order.
    pub const ALL: [Self; 18] = [
        Self::Open, Self::OpenMagenta, Self::Magenta, Self::MagentaCyan, Self::Cyan, Self::CyanPea,
        Self::Pea, Self::PeaYellow, Self::Yellow, Self::YellowCold, Self::Cold, Self::ColdPink,
        Self::Pink, Self::PinkUv, Self::Uv, Self::UvWarm, Self::Warm, Self::WarmOpen,
    ];

    pub fn byte(self) -> u8 {
        wheel_byte(self as usize)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Open         => "open",
            Self::OpenMagenta  => "open/magenta",
            Self::Magenta      => "magenta",
            Self::MagentaCyan  => "magenta/cyan",
            Self::Cyan         => "cyan",
            Self::CyanPea      => "cyan/pea",
            Self::Pea          => "pea",
            Self::PeaYellow    => "pea/yellow",
            Self::Yellow       => "yellow",
            Self::YellowCold   => "yellow/cold",
            Self::Cold         => "cold",
            Self::ColdPink     => "cold/pink",
            Self::Pink         => "pink",
            Self::PinkUv       => "pink/uv",
            Self::Uv           => "uv",
            Self::UvWarm       => "uv/warm",
            Self::Warm         => "warm",
            Self::WarmOpen     => "warm/open",
        }
    }
}

/// The gobos, each of which the fixture can also shake in the gate.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HydroSpotGobo {
    #[default]
    Open,
    Dots,
    Tris,
    Lines,
    Flower,
    Stars,
    Swirl,
}

#[rustfmt::skip]
impl HydroSpotGobo {
    /// Every gobo, in wheel order.
    pub const ALL: [Self; 7] = [
        Self::Open, Self::Dots, Self::Tris, Self::Lines, Self::Flower, Self::Stars, Self::Swirl,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Open   => "open",
            Self::Dots   => "dots",
            Self::Tris   => "tris",
            Self::Lines  => "lines",
            Self::Flower => "flower",
            Self::Stars  => "stars",
            Self::Swirl  => "swirl",
        }
    }

    pub fn byte(self) -> u8 {
        match self {
            Self::Open   => 0,
            Self::Dots   => 5,
            Self::Tris   => 20,
            Self::Lines  => 35,
            Self::Flower => 50,
            Self::Stars  => 65,
            Self::Swirl  => 80,
        }
    }

    /// Chart byte for the same gobo shaking, `0..1` slow to fast.
    pub fn shaking(self, fr: f32) -> u8 {
        let (from, to) = match self {
            Self::Open   => return self.byte(),
            Self::Dots   => (95, 109),
            Self::Tris   => (110, 124),
            Self::Lines  => (125, 139),
            Self::Flower => (140, 154),
            Self::Stars  => (155, 169),
            Self::Swirl  => (170, 189),
        };
        from + (fr.clamp(0.0, 1.0) * (to - from) as f32) as u8
    }
}

/// The prisms. The chart's macro half, which pairs them with gobos, is not
/// reachable from here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HydroSpotPrism {
    #[default]
    Off,
    /// 5 facet, in a line.
    Linear,
    /// 6 facet, in a ring.
    Circular,
}

impl HydroSpotPrism {
    pub fn byte(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Linear => 6,
            Self::Circular => 67,
        }
    }
}

impl HydroSpot {
    /// Shutter chart: closed.
    pub const SHUTTER_CLOSED: u8 = 0;
    /// Shutter chart: open, no strobe.
    pub const SHUTTER_OPEN: u8 = 255;

    /// Shutter chart byte for a hardware strobe, `0..1` slow to fast.
    pub fn strobe(fr: f32) -> u8 {
        64 + (fr.clamp(0.0, 1.0) * 31.0) as u8
    }
    /// Shutter chart byte for the pulse effect, `0..1` slow to fast.
    pub fn pulse(fr: f32) -> u8 {
        128 + (fr.clamp(0.0, 1.0) * 31.0) as u8
    }
    /// Shutter chart byte for a random strobe, `0..1` slow to fast.
    pub fn random_strobe(fr: f32) -> u8 {
        192 + (fr.clamp(0.0, 1.0) * 31.0) as u8
    }
}

/// A rotation chart byte: 129-191 clockwise fast to slow, 193-255
/// counter-clockwise slow to fast, and 0 for a prism that is not turning.
fn spin(fr: f32) -> u8 {
    let fr = fr.clamp(-1.0, 1.0);
    if fr == 0.0 {
        return 0;
    }
    match fr < 0.0 {
        true => 191 - (-fr * 62.0) as u8,
        false => 193 + (fr * 62.0) as u8,
    }
}

impl DmxDevice for HydroSpot {
    fn channels(&self) -> usize {
        22
    }

    fn encode(&self, dmx: &mut [u8]) {
        let dmx = &mut dmx[0..self.channels()];
        dmx.fill(0);

        dmx[0..2].copy_from_slice(&self.yaw.coarse_fine()); // 1/2   pan + fine
        dmx[2..4].copy_from_slice(&self.pitch.coarse_fine()); // 3/4   tilt + fine
        dmx[4] = self.color.byte(); //   5
        dmx[5] = self.color2.byte(); //  6
        dmx[6] = match self.gobo_shake > 0.0 {
            true => self.gobo.shaking(self.gobo_shake), // 7
            false => self.gobo.byte(),
        };
        dmx[7] = self.gobo_rot; //       8
        dmx[8] = self.shutter; //        9
        dmx[9..11].copy_from_slice(&self.alpha.coarse_fine()); // 10/11 dimmer + fine
        dmx[11] = self.prism.byte(); //  12
        dmx[12] = spin(self.prism_rot); // 13
        dmx[13] = self.focus.byte(); //  14
        dmx[14..16].copy_from_slice(&self.zoom.coarse_fine()); // 15/16 zoom + fine
        dmx[16] = self.frost.byte(); //  17
        dmx[17] = self.frost2.byte(); // 18
        dmx[18] = self.dimmer_mode; //   19
        dmx[19] = self.dim_curve; //     20
        // 21 pan/tilt speed: values >= 226 are blackout modes, stay in 0-225
        dmx[20] = (self.speed.clamp(0.0, 1.0) * 225.0) as u8;
        // 22 special functions stays 0 = no function: fan control, resets,
        // and refresh rate overrides live here, deliberately not exposed
    }
}

impl Default for HydroSpot {
    fn default() -> Self {
        Self {
            pitch: 0.5,
            yaw: 0.0,
            speed: 0.0,
            color: HydroSpotWheel1::Open,
            color2: HydroSpotWheel2::Open,
            gobo: HydroSpotGobo::Open,
            color_mask: true,
            gobo_mask: false,
            gobo_shake: 0.0,
            gobo_rot: 0,
            shutter: Self::SHUTTER_OPEN,
            alpha: 1.0,
            prism: HydroSpotPrism::Off,
            prism_rot: 0.0,
            focus: 0.5,
            zoom: 0.5,
            frost: 0.0,
            frost2: 0.0,
            dimmer_mode: 0,
            dim_curve: 30, // linear
        }
    }
}

impl GdtfDevice for HydroSpot {
    const KIND: &'static str = "HydroSpot";
    const MODE: &'static str = "22 CH";

    fn motion() -> [motion::Params; 3] {
        [
            motion::Params { v_max: 380.0, a_max: 700.0, j_max: 40_000.0, ..motion::Params::pan() },
            motion::Params { v_max: 450.0, a_max: 600.0, j_max: 70_000.0, ..motion::Params::tilt() },
            motion::Params { v_max: 100.0, a_max: 0.0, j_max: 0.0, ..motion::Params::zoom() },
        ]
    }
}

impl RdmDevice for HydroSpot {
    const MANUFACTURER: u16 = 0x1900;
    const MODEL: u16 = 0x1581;
    /// "Extended 22". The others are "Basic 17" and "Standard 20".
    const PERSONALITY: u8 = 3;
}

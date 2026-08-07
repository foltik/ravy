//! What the Rogue Outcast 1 BeamWash's own firmware does with its LED ring and
//! its shutter, read off the firmware and replayed here.
//!
//! None of this is general. The frame tables, the rate tables and the shutter
//! chart are this one fixture's, and another maker numbers its ranges
//! differently, so everything here is gated on [`matches`] and nothing else
//! ever runs it.

use super::file::Gdtf;

/// The fixture all of this was read off.
const RDM: (u16, u16) = (0x21a4, 0x0731);

/// Whether this is that fixture.
pub fn matches(gdtf: &Gdtf) -> bool {
    gdtf.rdm.as_ref().is_some_and(|rdm| (rdm.manufacturer, rdm.model) == RDM)
}

///////////////////////// RING /////////////////////////

/// Frames of one macro. Bit `n` is segment `n`.
type Frames = &'static [u16];

const ROTATE_SMALL: Frames =
    &[0x007, 0x00e, 0x01c, 0x038, 0x070, 0x0e0, 0x1c0, 0x380, 0x700, 0xe00, 0xc01, 0x803];

const ROTATE_BIG: Frames =
    &[0x01f, 0x03e, 0x07c, 0x0f8, 0x1f0, 0x3e0, 0x7c0, 0xf80, 0xf01, 0xe03, 0xc07, 0x80f];

const ROTATE_QUADRANT: Frames = &[0x007, 0x038, 0x1c0, 0xe00];

/// The fixture stores the second half as a copy of the first; kept as it is,
/// since it changes nothing on screen.
const ROTATE_TWO: Frames =
    &[0x041, 0x082, 0x104, 0x208, 0x410, 0x820, 0x041, 0x082, 0x104, 0x208, 0x410, 0x820];

const ROTATE_THREE: Frames = &[0x111, 0x222, 0x444, 0x888];

const ROTATE_FOUR: Frames = &[0x249, 0x492, 0x924];

const ROTATE_SIX: Frames = &[0x333, 0xccc];

const ROTATE_TWELVE: Frames = &[0x555, 0xaaa];

const FLIP_ZIGZAG: Frames = &[0x038, 0xe00, 0x007, 0x1c0];

/// The last frame repeats the second, so the cycle is odd and the two blocks
/// swap which side leads on every pass.
const FLIP_ROTATE: Frames =
    &[0x007, 0x1c0, 0x00e, 0x380, 0x01c, 0x700, 0x038, 0xe00, 0x070, 0xc01, 0x0e0, 0x803, 0x1c0];

const FLIP_RANDOM: Frames = &[0x003, 0x0c0, 0x00c, 0x300, 0x030, 0xc00];

const BOUNCE_Y: Frames = &[0x00c, 0x012, 0x021, 0x840, 0x480, 0x300, 0x480, 0x840, 0x021, 0x012];

const BOUNCE_X: Frames = &[0xc03, 0x606, 0x30c, 0x198, 0x0f0, 0x198, 0x30c, 0x606];

/// Ticks a frame is held for, indexed by `min(v, 255 - v) >> 3`.
const RATE: [u32; 16] = [0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 36, 50, 60, 70];

/// Seconds per tick of [`RATE`]. The table is the fixture's; this scale is
/// fitted, since the counter it feeds was never traced.
const MACRO_TICK: f32 = 0.012;

/// Frames a macro channel value selects, or None where the ring is left alone.
/// The fixture numbers its macros from 1 in blocks of five.
fn frames(value: u32) -> Option<Frames> {
    match value.checked_sub(1)? / 5 {
        4 => Some(ROTATE_SMALL),
        6 => Some(ROTATE_BIG),
        8 => Some(BOUNCE_Y),
        9 => Some(BOUNCE_X),
        12 => Some(FLIP_RANDOM),
        13 => Some(ROTATE_QUADRANT),
        14 => Some(FLIP_ROTATE),
        16 => Some(FLIP_ZIGZAG),
        17 => Some(ROTATE_TWO),
        18 => Some(ROTATE_THREE),
        19 => Some(ROTATE_FOUR),
        20 => Some(ROTATE_TWELVE),
        21 => Some(ROTATE_SIX),
        _ => None,
    }
}

/// How long a frame is held, or None where the macro stands still.
fn interval(speed: u32) -> Option<f32> {
    let speed = speed.min(u8::MAX as u32);
    (speed != 128).then(|| RATE[(speed.min(255 - speed) >> 3) as usize].max(1) as f32 * MACRO_TICK)
}

/// Which segment of this fixture's ring a beam is, from the geometry names
/// above it.
pub fn segment(gdtf: &Gdtf, chain: &[String]) -> Option<usize> {
    if !matches(gdtf) {
        return None;
    }
    let ring = chain.iter().position(|name| name == "Ring")?;
    let name = chain.get(ring + 1)?;
    let digits = name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    name[digits..].parse::<usize>().ok()?.checked_sub(1)
}

/// Where the LED ring macro has got to.
#[derive(Default)]
pub struct Ring {
    step: usize,
    clock: f32,
    /// Segments lit right now, or None while no macro is running.
    mask: Option<u16>,
}

impl Ring {
    /// Walk the macro on by `dt`, from the raw macro and speed channel values.
    pub fn step(&mut self, macro_value: u32, speed: u32, dt: f32) {
        let Some(frames) = frames(macro_value) else {
            *self = Self::default();
            return;
        };
        if let Some(interval) = interval(speed) {
            self.clock += dt;
            let steps = (self.clock / interval) as usize;
            self.clock -= steps as f32 * interval;
            self.step += steps;
        }
        self.step %= frames.len();
        // Past the still point the fixture walks the list backwards.
        let at = match speed > 128 {
            true => frames.len() - 1 - self.step,
            false => self.step,
        };
        self.mask = Some(frames[at]);
    }

    /// Whether a segment is lit, or None where the ring is left alone.
    pub fn lit(&self, segment: usize) -> Option<bool> {
        Some(self.mask? >> segment & 1 == 1)
    }
}

///////////////////////// SHUTTER /////////////////////////

/// Chart values at or below this hold the shutter shut, and at or below [`OPEN`]
/// hold it open.
const CLOSED: u32 = 19;
const OPEN: u32 = 24;
/// Last value whose timing was traced. The pulse and random modes above it are
/// left open rather than guessed at.
const STROBE: u32 = 69;

/// Ticks the gap lasts, indexed by `(v - 24) / 3`.
const DARK: [u32; 16] =
    [20, 30, 50, 80, 120, 180, 250, 400, 600, 900, 1300, 1800, 2000, 4000, 5000, 6000];

/// Ticks the flash lasts, which steps up once past [`WIDE`].
const LIT: [u32; 2] = [80, 120];
const WIDE: u32 = 50;

/// Seconds a tick lasts. The fixture counts them off a 2 kHz timer.
const SHUTTER_TICK: f32 = 0.0005;

/// How long the flash and the gap after it last, or None where the value is not
/// one of the rates we know.
fn phases(value: u32) -> Option<(f32, f32)> {
    let (lit, dark) = (OPEN < value && value <= STROBE)
        .then(|| (LIT[(value >= WIDE) as usize], DARK[((value - OPEN) / 3) as usize]))?;
    Some((lit as f32 * SHUTTER_TICK, dark as f32 * SHUTTER_TICK))
}

/// Where a shutter has got to. The flash is a fixed width and only the gap
/// after it grows, so the fast end of the chart is nearly continuous and the
/// slow end is a brief blip a long way apart.
#[derive(Default)]
pub struct Strobe {
    /// Chart value being run, so a change can restart the flash.
    value: u32,
    clock: f32,
    /// Whether light is passing, or None where the value is not one we model.
    open: Option<bool>,
}

impl Strobe {
    /// Walk the shutter on by `dt` from the raw chart value.
    pub fn step(&mut self, value: u32, dt: f32) {
        if value != self.value {
            // The fixture restarts the phase whenever the chart value moves.
            *self = Self { value, ..Self::default() };
        }
        let Some((lit, dark)) = phases(value) else {
            self.open = match value {
                v if v <= CLOSED => Some(false),
                v if v <= OPEN => Some(true),
                _ => None,
            };
            return;
        };
        // It comes up dark, so every flash is a whole one.
        let mut open = self.open == Some(true);
        self.clock += dt;
        loop {
            let hold = match open {
                true => lit,
                false => dark,
            };
            if self.clock < hold {
                break;
            }
            self.clock -= hold;
            open = !open;
        }
        self.open = Some(open);
    }

    /// Whether light is passing, or None where the chart value is one whose
    /// timing we don't know.
    pub fn open(&self) -> Option<bool> {
        self.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every value the fixture type hands us, against the macro it selects.
    const CHART: [(u32, Frames); 13] = [
        (21, ROTATE_SMALL),
        (31, ROTATE_BIG),
        (41, BOUNCE_Y),
        (46, BOUNCE_X),
        (61, FLIP_RANDOM),
        (66, ROTATE_QUADRANT),
        (71, FLIP_ROTATE),
        (81, FLIP_ZIGZAG),
        (86, ROTATE_TWO),
        (91, ROTATE_THREE),
        (96, ROTATE_FOUR),
        (101, ROTATE_TWELVE),
        (106, ROTATE_SIX),
    ];

    /// Flashes per second at a chart value, for reading the chart back.
    fn hz(value: u32) -> f32 {
        let (lit, dark) = phases(value).unwrap();
        1.0 / (lit + dark)
    }

    #[test]
    fn all_of_this_belongs_to_one_fixture() {
        use crate::dmx::RdmDevice;
        use crate::lights::fixture::OutcastBeamwash;
        assert_eq!(RDM, (OutcastBeamwash::MANUFACTURER, OutcastBeamwash::MODEL));
    }

    #[test]
    fn macros_are_picked_by_their_dmx_value() {
        for (value, expected) in CHART {
            assert_eq!(frames(value), Some(expected), "dmx {value}");
        }
        assert!(frames(0).is_none());
        // A macro occupies five values, and the ones we don't model do nothing.
        assert_eq!(frames(35), Some(ROTATE_BIG));
        assert!(frames(36).is_none());
        assert!(frames(216).is_none());
    }

    /// Picking a pattern has to land on a macro we model, or the ring would sit
    /// there doing nothing.
    #[test]
    fn every_named_pattern_is_modelled() {
        use crate::lights::fixture::OutcastBeamwashRingPattern as Pattern;
        for pattern in Pattern::ALL {
            let value = pattern.byte() as u32;
            let modelled = frames(value).is_some();
            assert_eq!(modelled, pattern != Pattern::Off, "{} is dmx {value}", pattern.name());
        }
    }

    #[test]
    fn every_frame_lights_something_inside_the_ring() {
        for (value, frames) in CHART {
            for frame in frames {
                assert_ne!(*frame, 0, "dmx {value} has a blank frame");
                assert_eq!(frame >> 12, 0, "dmx {value} lights past segment 11");
            }
        }
    }

    #[test]
    fn speed_runs_out_from_the_still_point() {
        assert_eq!(interval(128), None);
        assert!(interval(0).unwrap() < interval(127).unwrap());
        assert_eq!(interval(0), interval(255));
        assert_eq!(interval(127), interval(129));
    }

    #[test]
    fn a_macro_only_ever_shows_its_own_frames() {
        let mut ring = Ring::default();
        for i in 0..50 {
            ring.step(31, 0, 0.05 * i as f32);
            assert!(ROTATE_BIG.contains(&ring.mask.unwrap()));
        }
    }

    #[test]
    fn the_still_point_holds_a_frame() {
        let mut ring = Ring::default();
        ring.step(31, 0, 1.0);
        let held = ring.mask;
        for _ in 0..5 {
            ring.step(31, 128, 1.0);
            assert_eq!(ring.mask, held);
        }
    }

    #[test]
    fn past_the_still_point_it_runs_backwards() {
        let dt = interval(0).unwrap() * 1.5;
        let (mut forward, mut back) = (Ring::default(), Ring::default());
        forward.step(31, 0, dt);
        back.step(31, 255, dt);
        assert_eq!(forward.mask, Some(ROTATE_BIG[1]));
        assert_eq!(back.mask, Some(ROTATE_BIG[ROTATE_BIG.len() - 2]));
    }

    #[test]
    fn a_macro_that_is_not_modelled_leaves_the_ring_alone() {
        let mut ring = Ring::default();
        ring.step(31, 0, 1.0);
        assert!(ring.lit(0).is_some());
        ring.step(0, 0, 1.0);
        assert_eq!(ring.lit(0), None);
    }

    #[test]
    fn the_shutter_ranges_come_before_the_strobe() {
        let mut shutter = Strobe::default();
        shutter.step(0, 0.0);
        assert_eq!(shutter.open(), Some(false));
        shutter.step(19, 0.0);
        assert_eq!(shutter.open(), Some(false));
        shutter.step(20, 0.0);
        assert_eq!(shutter.open(), Some(true));
        shutter.step(24, 0.0);
        assert_eq!(shutter.open(), Some(true));
        // Above the traced range we have no timing, so nothing is claimed.
        shutter.step(70, 0.0);
        assert_eq!(shutter.open(), None);
        shutter.step(255, 0.0);
        assert_eq!(shutter.open(), None);
    }

    #[test]
    fn the_strobe_runs_fast_to_slow() {
        assert!((hz(25) - 20.0).abs() < 0.01);
        assert!((hz(69) - 0.327).abs() < 0.01);
        for value in 26..=STROBE {
            assert!(hz(value) <= hz(value - 1), "dmx {value} is not slower than {}", value - 1);
        }
    }

    #[test]
    fn a_flash_is_a_fixed_width_with_a_growing_gap() {
        let (lit, dark) = phases(25).unwrap();
        assert!(lit > dark, "the fast end is mostly lit");
        let (lit, dark) = phases(69).unwrap();
        assert!(dark > lit * 10.0, "the slow end is a blip");
    }

    #[test]
    fn it_comes_up_dark_and_then_flashes() {
        let (lit, dark) = phases(69).unwrap();
        let mut shutter = Strobe::default();
        shutter.step(69, dark * 0.5);
        assert_eq!(shutter.open(), Some(false));
        shutter.step(69, dark * 0.5);
        assert_eq!(shutter.open(), Some(true));
        shutter.step(69, lit);
        assert_eq!(shutter.open(), Some(false));
    }

    #[test]
    fn moving_the_chart_value_restarts_the_flash() {
        let (_, dark) = phases(69).unwrap();
        let mut shutter = Strobe::default();
        shutter.step(69, dark);
        assert_eq!(shutter.open(), Some(true));
        shutter.step(66, 0.0);
        assert_eq!(shutter.open(), Some(false));
    }
}

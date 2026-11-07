//! Keeping the movers out of people's eyes: a patch of the sphere around each
//! mover in which the beam is held down to a set fraction of what it was told,
//! with a fade either side of it back to full.
//!
//! The patch is written where the light actually goes, and so is the beam it is
//! measured against: the aim comes from the head the sim has built out of the
//! fixture's own geometry, not from the pan and tilt it was sent. A head whose
//! yoke rests turned, as the Outcast's does a quarter turn, does not point where
//! its DMX alone would say, and a window written against the DMX lands somewhere
//! else in the room. Reading the head also means the window bites while the beam
//! is really crossing someone, rather than the instant the command to move away
//! goes out.
//!
//! It is applied where the show and the debug panel encode their fixtures, so
//! the wire and the sim both carry the faded level.

use lib::lights::fixture::{HydroSpot, OutcastBeamwash};
use lib::prelude::*;

use crate::dmx::MOVERS;
use crate::home::Rest;

/// Saved windows, one fixture per line: its name and then seven numbers per
/// window, `<pitch> <pitch arc> <pitch fade> <bearing> <bearing arc>
/// <bearing fade> <level>`. A line carrying one window's worth is a file from
/// when a mover only had one, and fills the first.
const BLACKOUT_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/blackout.txt");

/// Windows each mover has, so one can hold the crowd off and another the booth.
pub const WINDOWS: usize = 2;
/// Numbers one of them is written as.
const FIELDS: usize = 7;

/// Fold an angle onto -180..180.
pub fn wrap(degrees: f32) -> f32 {
    (degrees + 180.0).fmod(360.0) - 180.0
}

/// A beam's direction in the fixture's own frame, as how far it is tipped off
/// straight down, `0..180`, and which way that tip points, `-180..180`.
///
/// -y is the beam with both axles centred, pitch swings it about x and the
/// bearing carries that round y. The bearing means nothing at either pole, where
/// the tip alone says where the beam is.
pub fn polar(aim: Vec3) -> (f32, f32) {
    let tip = (-aim.y).clamp(-1.0, 1.0).acos().to_degrees();
    (tip, f32::atan2(-aim.x, -aim.z).to_degrees())
}

/// The way back: the direction a tip and bearing stand for.
pub fn direction(tip: f32, bearing: f32) -> Vec3 {
    let turn = Quat::from_rotation_y(bearing.to_radians()) * Quat::from_rotation_x(tip.to_radians());
    turn * Vec3::NEG_Y
}

/// A bearing round the fixture, in the frame the window is written in: round
/// from where the head rests rather than from the middle of its pan travel, and
/// mirrored where the yaw axle is flipped.
///
/// The middle of the travel is an arbitrary direction that depends on how the
/// fixture was hung, and two heads resting a quarter turn apart put the same
/// window a quarter turn apart in the room. Home is the one direction the rig
/// is set against, so a mirrored pair takes the same window and moving home
/// carries the window round with it.
pub fn homed(rest: &Rest, bearing: f32) -> f32 {
    wrap(rest.sign().1 * (bearing - rest.aim().1))
}

/// The way back, for the drawn volume and the panel readout.
pub fn unhomed(rest: &Rest, bearing: f32) -> f32 {
    wrap(rest.sign().1 * bearing + rest.aim().1)
}

/// One axis of the window: an arc holding the level outright, and a fade either
/// side of it.
#[derive(Clone, Copy, PartialEq)]
pub struct Arc {
    /// Middle of the window, in degrees.
    pub at: f32,
    /// How wide it holds the level, with nothing given back to the fade.
    pub width: f32,
    /// Arc either side of that over which it eases between full and the level.
    pub fade: f32,
}

impl Arc {
    /// Half of what holds the level outright.
    pub fn held(&self) -> f32 {
        self.width / 2.0
    }

    /// Half of everything the window touches on this axis, the fade included.
    pub fn reach(&self) -> f32 {
        self.held() + self.fade
    }

    /// 1 anywhere across the width, easing to 0 over the fade either side.
    ///
    /// Smoothstepped rather than ramped straight, so the beam neither steps as
    /// it crosses into the window nor picks up a visible corner where the fade
    /// meets full.
    fn depth(&self, angle: f32) -> f32 {
        let off = wrap(angle - self.at).abs();
        let held = self.held();
        if off <= held {
            return 1.0;
        }
        if self.fade <= 0.0 {
            return 0.0;
        }
        let fr = (1.0 - (off - held) / self.fade).clamp(0.0, 1.0);
        fr * fr * (3.0 - 2.0 * fr)
    }
}

/// The window one mover fades in.
#[derive(Clone, Copy, PartialEq)]
pub struct Zone {
    /// How far the window is tipped off straight down, signed the way the tilt
    /// knob is and free to run past either pole: a window taken over straight
    /// up carries on down the far side of the fixture.
    pub pitch: Arc,
    /// Which way round the fixture the window faces.
    pub yaw: Arc,
    /// What the window is worth, as a fraction of the commanded brightness. 1
    /// leaves the fixture alone, 0 blacks it out. Held across the whole of both
    /// arcs, so it does not depend on how wide they are.
    pub level: f32,
    /// Draw the window as a volume in the sim.
    pub show: bool,
    /// Draw the fade around it as well, in a lighter shade.
    pub show_fade: bool,
}

impl Default for Zone {
    fn default() -> Self {
        Self {
            pitch: Arc { at: 90.0, width: 40.0, fade: 30.0 },
            yaw: Arc { at: 0.0, width: 60.0, fade: 30.0 },
            level: 1.0,
            show: false,
            show_fade: false,
        }
    }
}

impl Zone {
    /// What the commanded brightness is worth for a head pointing `aim`, which
    /// is a direction in the fixture's own frame. The beam has to be inside the
    /// window on both counts for it to bite, so the shallower of the two
    /// governs.
    ///
    /// A direction is `(tip, bearing)` and equally `(-tip, bearing + 180)`, and
    /// both spellings are tried. That is what carries a window over the top: a
    /// beam past straight up is tipped less than 180 again from the far side,
    /// and only the second spelling puts it where the window was set.
    pub fn gain(&self, rest: &Rest, aim: Vec3) -> f32 {
        let (tip, bearing) = polar(aim);
        let bearing = homed(rest, bearing);
        let near = self.pitch.depth(tip).min(self.yaw.depth(bearing));
        let far = self.pitch.depth(-tip).min(self.yaw.depth(bearing + 180.0));
        fade(self.level, near.max(far))
    }
}

/// Fade a commanded brightness by `depth` into a window worth `level`.
///
/// Perceived lightness goes roughly as the cube root of what a fixture puts
/// out, so a fade that reads as even has to be taken there and back. Ramped
/// straight down instead, most of the fade is still blinding and the whole drop
/// happens in its last few degrees.
fn fade(level: f32, depth: f32) -> f32 {
    let lightness = depth.clamp(0.0, 1.0).lerp(1.0..level.clamp(0.0, 1.0).cbrt());
    lightness * lightness * lightness
}

/// The slot a saved line names and the windows it carries, or nothing if it
/// names no fixture the rig has or does not carry whole windows of numbers.
fn parse(line: &str) -> Option<(usize, Vec<Zone>)> {
    let mut field = line.split_whitespace();
    let slot = field.next().and_then(|n| MOVERS.iter().position(|m| *m == n))?;
    let numbers: Vec<f32> = field.filter_map(|f| f.parse().ok()).collect();
    if numbers.is_empty() || numbers.len() % FIELDS != 0 || numbers.len() / FIELDS > WINDOWS {
        warn!("blackout: {line} is not seven numbers per window");
        return None;
    }
    let windows = numbers
        .chunks_exact(FIELDS)
        .map(|w| Zone {
            pitch: Arc { at: w[0], width: w[1], fade: w[2] },
            yaw: Arc { at: w[3], width: w[4], fade: w[5] },
            level: w[6],
            ..default()
        })
        .collect();
    Some((slot, windows))
}

#[derive(Resource)]
pub struct Blackout {
    pub movers: [[Zone; WINDOWS]; MOVERS.len()],
}

impl Default for Blackout {
    fn default() -> Self {
        let mut blackout = Self { movers: [[Zone::default(); WINDOWS]; MOVERS.len()] };
        blackout.reload();
        blackout
    }
}

impl Blackout {
    /// What a mover's windows are worth where its head is pointing. Nothing
    /// until the head is built and the sim can say where that is.
    ///
    /// The darkest of them governs rather than all of them together, so each one
    /// is worth exactly the level it was set to wherever it reaches, whatever
    /// else it happens to overlap.
    fn gain(&self, slot: usize, rest: &Rest, aim: Option<Vec3>) -> f32 {
        match (self.movers.get(slot), aim) {
            (Some(zones), Some(aim)) => {
                zones.iter().map(|zone| zone.gain(rest, aim)).fold(1.0, f32::min)
            }
            _ => 1.0,
        }
    }

    /// Fade a wash's dimmers by what its window is worth where it is aimed.
    pub fn wash(&self, slot: usize, rest: &Rest, aim: Option<Vec3>, light: &mut OutcastBeamwash) {
        let gain = self.gain(slot, rest, aim);
        light.ring_alpha *= gain;
        light.center_alpha *= gain;
    }

    /// Fade a spot's dimmer by what its window is worth where it is aimed.
    pub fn spot(&self, slot: usize, rest: &Rest, aim: Option<Vec3>, light: &mut HydroSpot) {
        light.alpha *= self.gain(slot, rest, aim);
    }

    /// Drop back to what is on disk, defaulting whatever it doesn't name.
    pub fn reload(&mut self) {
        self.movers = [[Zone::default(); WINDOWS]; MOVERS.len()];
        let Ok(text) = std::fs::read_to_string(BLACKOUT_FILE) else {
            return;
        };
        for line in text.lines() {
            let Some((slot, windows)) = parse(line) else { continue };
            // A line naming fewer windows than a mover has leaves the rest at
            // the default, which is a window that does nothing.
            for (zone, from) in self.movers[slot].iter_mut().zip(windows) {
                *zone = Zone { show: zone.show, show_fade: zone.show_fade, ..from };
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let body: String = MOVERS
            .iter()
            .zip(&self.movers)
            .map(|(name, zones)| {
                let windows: String = zones
                    .iter()
                    .map(|zone| {
                        let (up, round) = (zone.pitch, zone.yaw);
                        format!(
                            " {} {} {} {} {} {} {}",
                            up.at, up.width, up.fade, round.at, round.width, round.fade, zone.level
                        )
                    })
                    .collect();
                format!("{name}{windows}\n")
            })
            .collect();
        std::fs::write(BLACKOUT_FILE, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx::Patch;

    /// A head resting in the middle of its travel, where a bearing round the
    /// fixture and a bearing off home are the same number.
    const HOME: Rest = Rest { pitch: 0.0, yaw: 0.0, zoom: 0.0, flip: [false; 2] };

    fn close(got: f32, want: f32) {
        assert!((got - want).abs() < 1e-4, "{got} != {want}");
    }

    /// A window facing straight ahead, level across 60 degrees of bearing with
    /// 30 more either side to fade over. Its pitch stops short of both poles, so
    /// what it covers is the bearing alone.
    fn ahead(level: f32) -> Zone {
        Zone {
            pitch: Arc { at: 90.0, width: 120.0, fade: 0.0 },
            yaw: Arc { at: 0.0, width: 60.0, fade: 30.0 },
            level,
            ..default()
        }
    }

    /// The windows are indexed by patch slot, so their names have to be the
    /// slots the patch counts first.
    #[test]
    fn the_movers_are_the_first_slots() {
        let patch = Patch::default();
        let names: Vec<&str> = patch.slots.iter().take(MOVERS.len()).map(|s| s.name.as_str()).collect();
        assert_eq!(names, MOVERS);
    }

    /// A tip and bearing name a direction and a direction names them back. The
    /// volume the sim draws is built out of one and the fade out of the other,
    /// so a window is only ever where it is drawn if the two agree.
    #[test]
    fn a_direction_and_its_polar_are_the_same_thing() {
        // Within a hundredth of a degree: the tip comes back through an arc
        // cosine, which is worth little of its own precision near either pole.
        let close = |got: f32, want: f32| assert!((got - want).abs() < 0.01, "{got} != {want}");
        for tip in [1.0, 30.0, 90.0, 150.0, 179.0] {
            for bearing in [-179.0, -90.0, 0.0, 45.0, 179.0] {
                let (back, round) = polar(direction(tip, bearing));
                close(back, tip);
                close(wrap(round - bearing), 0.0);
            }
        }
        // Tipping past straight down is the same as tipping the other way from
        // half a turn round, which is how a head reaches a bearing twice over.
        for tip in [10.0, 80.0] {
            let (near, far) = (direction(-tip, 30.0), direction(tip, 30.0 + 180.0));
            assert!(near.distance(far) < 1e-5, "{near} is not {far}");
        }
    }

    /// The level is the level everywhere across the arc, however wide the arc
    /// is. Making it depend on the width is what made a wide window useless:
    /// only its very middle was ever held down.
    #[test]
    fn the_level_holds_across_the_whole_arc() {
        let zone = ahead(0.2);
        let gain = |bearing| zone.gain(&HOME, direction(90.0, bearing));
        for bearing in [-30.0, -20.0, 0.0, 20.0, 30.0] {
            close(gain(bearing), 0.2);
        }
        // A wider arc holds the same level, rather than spreading it thinner.
        let wide = Zone { yaw: Arc { at: 0.0, width: 200.0, fade: 30.0 }, ..zone };
        close(wide.gain(&HOME, direction(90.0, 80.0)), 0.2);
    }

    /// Outside the arc it eases back to full over the fade, and is untouched
    /// past that.
    #[test]
    fn the_fade_carries_it_back_to_full() {
        let zone = ahead(0.2);
        let gain = |bearing| zone.gain(&HOME, direction(90.0, bearing));
        close(gain(60.0), 1.0);
        close(gain(90.0), 1.0);
        // Half way out the fade sits between the two, and nowhere near either.
        let half = gain(45.0);
        assert!(half > 0.3 && half < 0.85, "half way out the fade left {half}");
        // And it only ever climbs on the way out.
        let mut last = 0.0;
        for step in 0..=60 {
            let at = gain(30.0 + step as f32 / 2.0);
            assert!(at >= last - 1e-6, "the fade fell back at {step}");
            last = at;
        }
    }

    /// With no fade the window is a hard edge, which is the whole arc and
    /// nothing beyond it.
    #[test]
    fn no_fade_is_a_hard_edge() {
        let zone = Zone { yaw: Arc { at: 0.0, width: 60.0, fade: 0.0 }, ..ahead(0.0) };
        let gain = |bearing| zone.gain(&HOME, direction(90.0, bearing));
        close(gain(29.0), 0.0);
        close(gain(31.0), 1.0);
    }

    /// A pitch set below straight down is the same patch as one above it half a
    /// turn round, so the panel can show pitch on the tilt knob's signed scale.
    #[test]
    fn a_pitch_below_the_aim_is_the_same_patch() {
        let above = Zone {
            pitch: Arc { at: 60.0, width: 40.0, fade: 0.0 },
            yaw: Arc { at: 90.0, width: 60.0, fade: 0.0 },
            level: 0.0,
            ..default()
        };
        let below = Zone {
            pitch: Arc { at: -60.0, ..above.pitch },
            yaw: Arc { at: -90.0, ..above.yaw },
            ..above
        };
        for tip in [-90.0, -60.0, 0.0, 60.0, 90.0] {
            for bearing in [-180.0, -90.0, 0.0, 90.0] {
                let aim = direction(tip, bearing);
                close(below.gain(&HOME, aim), above.gain(&HOME, aim));
            }
        }
    }

    /// A window reaching past straight down carries on up the far side of the
    /// fixture, which is the swing the tilt axle makes through zero.
    #[test]
    fn the_window_carries_through_straight_down() {
        // 40 either side of a pitch of 20, so 20 of it is past straight down.
        let zone = Zone {
            pitch: Arc { at: 20.0, width: 80.0, fade: 0.0 },
            yaw: Arc { at: 0.0, width: 60.0, fade: 0.0 },
            level: 0.0,
            ..default()
        };
        let gain = |tip, bearing| zone.gain(&HOME, direction(tip, bearing));
        close(gain(0.0, 0.0), 0.0);
        close(gain(50.0, 0.0), 0.0);
        // Tipped 10 the far way round, which is 10 past straight down.
        close(gain(10.0, 180.0), 0.0);
        // And no further than the window reaches, either side of it.
        close(gain(30.0, 180.0), 1.0);
        close(gain(70.0, 0.0), 1.0);
    }

    /// The same over straight up, which is what a window stopping dead at the
    /// pole got wrong: past it the beam is tipped less than 180 again, from a
    /// bearing half a turn away.
    #[test]
    fn the_window_carries_over_straight_up() {
        // 50 either side of a pitch of 150, so 20 of it is past straight up.
        let zone = Zone {
            pitch: Arc { at: 150.0, width: 100.0, fade: 0.0 },
            yaw: Arc { at: 0.0, width: 60.0, fade: 0.0 },
            level: 0.0,
            ..default()
        };
        let gain = |tip, bearing| zone.gain(&HOME, direction(tip, bearing));
        close(gain(150.0, 0.0), 0.0);
        close(gain(180.0, 0.0), 0.0);
        // Tipped 170 the far way round, which is 10 past straight up.
        close(gain(170.0, 180.0), 0.0);
        close(gain(120.0, 180.0), 1.0);
        close(gain(95.0, 0.0), 1.0);
    }

    /// The window is written round where the head rests, not round the middle of
    /// its pan travel. Two heads hung the same way but resting a quarter turn
    /// apart took the same window a quarter turn apart in the room, which is
    /// what put the fade nowhere near where it was set.
    #[test]
    fn the_window_is_written_off_home() {
        let zone = Zone { yaw: Arc { at: 40.0, width: 20.0, fade: 0.0 }, ..ahead(0.0) };
        let rested = Rest { yaw: -90.0, ..HOME };
        let gain = |rest: &Rest, bearing| zone.gain(rest, direction(90.0, bearing));
        // 40 round from the middle for a head that rests there, and 40 round
        // from -90 for one that rests at -90.
        close(gain(&HOME, 40.0), 0.0);
        close(gain(&rested, -50.0), 0.0);
        close(gain(&rested, 40.0), 1.0);
    }

    /// And a flipped axle mirrors it, so a pair of heads either side of centre
    /// stage takes the same window rather than one written backwards.
    #[test]
    fn a_flipped_axle_mirrors_the_window() {
        let zone = Zone { yaw: Arc { at: 40.0, width: 20.0, fade: 0.0 }, ..ahead(0.0) };
        // The rig's own pair: both rest a quarter turn off the middle, one of
        // them by turning its yaw axle round instead.
        let (left, right) =
            (Rest { yaw: -90.0, ..HOME }, Rest { yaw: -90.0, flip: [false, true], ..HOME });
        assert_eq!((left.aim().1, right.aim().1), (-90.0, 90.0));
        let gain = |rest: &Rest, bearing| zone.gain(rest, direction(90.0, bearing));
        // 40 one way from home on one head, 40 the other way on the other.
        close(gain(&left, -50.0), 0.0);
        close(gain(&right, 50.0), 0.0);
        close(gain(&right, 130.0), 1.0);
    }

    /// An empty window is what an unconfigured mover has, and it must not dim.
    #[test]
    fn an_empty_window_does_nothing() {
        let zone = Zone { yaw: Arc { at: 0.0, width: 0.0, fade: 0.0 }, level: 0.0, ..default() };
        close(zone.gain(&HOME, Vec3::NEG_Y), 1.0);
    }

    /// A line saved when a mover had one window still loads, into the first of
    /// the two it has now.
    #[test]
    fn a_saved_line_loads_however_many_windows_it_names() {
        let (slot, one) = parse("HydroSpot.0 60 20 20 45 20 20 0").unwrap();
        assert_eq!(slot, 2);
        assert_eq!(one.len(), 1);
        close(one[0].yaw.at, 45.0);
        close(one[0].level, 0.0);

        let (_, both) = parse("HydroSpot.0 60 20 20 45 20 20 0 10 30 5 -90 40 10 0.5").unwrap();
        assert_eq!(both.len(), 2);
        close(both[1].pitch.at, 10.0);
        close(both[1].yaw.at, -90.0);
        close(both[1].level, 0.5);

        // Part of a window is not a window, and neither is a fixture the rig
        // hasn't got.
        assert!(parse("HydroSpot.0 1 2 3").is_none());
        assert!(parse("HydroSpot.9 1 2 3 4 5 6 7").is_none());
    }

    /// The two are separate windows, not one that adds up: each is worth what it
    /// was set to wherever it reaches, and where they overlap the darker wins.
    #[test]
    fn the_darkest_window_governs() {
        let facing = |bearing, level| Zone {
            yaw: Arc { at: bearing, width: 60.0, fade: 0.0 },
            level,
            ..ahead(level)
        };
        let blackout =
            Blackout { movers: [[facing(0.0, 0.25), facing(40.0, 0.5)]; MOVERS.len()] };
        let gain = |bearing| blackout.gain(0, &HOME, Some(direction(90.0, bearing)));
        // Only the second, only the first, and both at once.
        close(gain(60.0), 0.5);
        close(gain(-20.0), 0.25);
        close(gain(20.0), 0.25);
        close(gain(120.0), 1.0);
    }

    /// And a head the sim has not built yet is not somewhere to be faded.
    #[test]
    fn a_head_that_has_not_been_built_is_left_alone() {
        let shut = Zone { level: 0.0, ..default() };
        let blackout = Blackout { movers: [[shut; WINDOWS]; MOVERS.len()] };
        let mut light = HydroSpot { alpha: 1.0, ..default() };
        blackout.spot(0, &HOME, None, &mut light);
        close(light.alpha, 1.0);
    }
}

//! The settings that belong to the venue rather than to the show: how hard each
//! family of fixture is driven, how bright the room is, and how much haze is in
//! the air. Set once when the rig is up and saved together.

use lib::prelude::*;

use crate::ledwall::{
    BallParams, BoxesParams, CubeParams, Fx, KaleidoParams, RingsParams, ScreenParams,
    SpiralParams, WormholeParams,
};
use crate::sim::Room;

/// Saved settings, one `<name> <value>` per line.
const RIG_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/rig.txt");

/// Brightness multipliers, on top of whatever the show and the master fader
/// asked for. One per fixture family, since they are nowhere near each other in
/// output, plus a global for pulling the whole rig down.
#[derive(Resource, Clone, Copy)]
pub struct Trim {
    pub global: f32,
    pub par: f32,
    pub beam: f32,
    pub bigbeam: f32,
    pub spider: f32,
    pub bar: f32,
    pub strobe: f32,
    /// The hut side lights (both bars and par 6): the dim level they hold
    /// through every broad look.
    pub hut: f32,
}

impl Default for Trim {
    fn default() -> Self {
        // The BigBeams run half out of the box: they always were far brighter
        // than the rest of the rig.
        Self {
            global: 1.0,
            par: 1.0,
            beam: 1.0,
            bigbeam: 0.5,
            spider: 1.0,
            bar: 1.0,
            strobe: 1.0,
            hut: 0.3,
        }
    }
}

/// Everything the Save/Reload buttons carry, borrowed mutably in one place.
pub struct Rig<'a> {
    pub room: &'a mut Room,
    pub haze: &'a mut Haze,
    pub iso: &'a mut ScreenParams,
    pub cube: &'a mut CubeParams,
    pub spiral: &'a mut SpiralParams,
    pub ball: &'a mut BallParams,
    pub worm: &'a mut WormholeParams,
    pub kal: &'a mut KaleidoParams,
    pub rings: &'a mut RingsParams,
    pub boxes: &'a mut BoxesParams,
    pub fx: &'a mut Fx,
}

impl Trim {
    /// Every saved field, by the name it is written under.
    fn fields<'a>(&'a mut self, rig: Rig<'a>) -> Vec<(&'static str, &'a mut f32)> {
        let (i, c) = (&mut *rig.iso, &mut *rig.cube);
        let (sp, ba, wo) = (&mut *rig.spiral, &mut *rig.ball, &mut *rig.worm);
        let (ka, ri, bx) = (&mut *rig.kal, &mut *rig.rings, &mut *rig.boxes);
        macro_rules! fx {
            ($tag:literal, $p:expr) => {
                [
                    (concat!("fx.", $tag, ".trim"), &mut $p.trim),
                    (concat!("fx.", $tag, ".glitch"), &mut $p.glitch),
                    (concat!("fx.", $tag, ".vhs"), &mut $p.vhs),
                    (concat!("fx.", $tag, ".shake"), &mut $p.shake),
                    (concat!("fx.", $tag, ".mega"), &mut $p.mega),
                    (concat!("fx.", $tag, ".pause"), &mut $p.pause),
                    (concat!("fx.", $tag, ".edge"), &mut $p.edge),
                    (concat!("fx.", $tag, ".invert"), &mut $p.invert),
                    (concat!("fx.", $tag, ".red"), &mut $p.red),
                    (concat!("fx.", $tag, ".flash"), &mut $p.flash),
                ]
            };
        }
        let mut fields: Vec<(&'static str, &'a mut f32)> = vec![
            ("global", &mut self.global),
            ("par", &mut self.par),
            ("beam", &mut self.beam),
            ("bigbeam", &mut self.bigbeam),
            ("spider", &mut self.spider),
            ("bar", &mut self.bar),
            ("strobe", &mut self.strobe),
            ("hut", &mut self.hut),
            ("room", &mut rig.room.0),
            ("haze", &mut rig.haze.0),
            ("iso.drift", &mut i.drift),
            ("iso.kick", &mut i.kick),
            ("iso.snap", &mut i.snap),
            ("iso.wave", &mut i.wave),
            ("iso.cam", &mut i.cam),
            ("iso.fade", &mut i.fade),
            ("iso.lines", &mut i.lines),
            ("iso.thick", &mut i.thick),
            ("iso.pump", &mut i.pump),
            ("iso.sway", &mut i.sway),
            ("iso.settle", &mut i.settle),
            ("cube.spin", &mut c.spin),
            ("cube.surge", &mut c.surge),
            ("cube.pump", &mut c.pump),
            ("cube.mega", &mut c.mega),
            ("cube.wire", &mut c.wire),
            ("spiral.swirl", &mut sp.swirl),
            ("spiral.speed", &mut sp.speed),
            ("spiral.spokes", &mut sp.spokes),
            ("spiral.cutoff", &mut sp.cutoff),
            ("spiral.amount", &mut sp.amount),
            ("spiral.pulse", &mut sp.pulse),
            ("spiral.width", &mut sp.width),
            ("spiral.every", &mut sp.every),
            ("ball.size", &mut ba.size),
            ("ball.checks", &mut ba.checks),
            ("ball.spin", &mut ba.spin),
            ("ball.boost", &mut ba.boost),
            ("ball.brake", &mut ba.brake),
            ("ball.every", &mut ba.every),
            ("worm.speed", &mut wo.speed),
            ("worm.surge", &mut wo.surge),
            ("worm.warp", &mut wo.warp),
            ("worm.pulse", &mut wo.pulse),
            ("worm.every", &mut wo.every),
            ("kal.speed", &mut ka.speed),
            ("kal.surge", &mut ka.surge),
            ("kal.zoom", &mut ka.zoom),
            ("kal.breathe", &mut ka.breathe),
            ("kal.sharp", &mut ka.sharp),
            ("kal.fade", &mut ka.fade),
            ("kal.iters", &mut ka.iters),
            ("kal.every", &mut ka.every),
            ("rings.reach", &mut ri.reach),
            ("rings.width", &mut ri.width),
            ("rings.life", &mut ri.life),
            ("rings.glow", &mut ri.glow),
            ("rings.snap", &mut ri.snap),
            ("rings.wob", &mut ri.wob),
            ("rings.pump", &mut ri.pump),
            ("rings.every", &mut ri.every),
            ("boxes.speed", &mut bx.speed),
            ("boxes.surge", &mut bx.surge),
            ("boxes.twist", &mut bx.twist),
            ("boxes.density", &mut bx.density),
            ("boxes.thick", &mut bx.thick),
            ("boxes.cutoff", &mut bx.cutoff),
            ("boxes.every", &mut bx.every),
        ];
        fields.extend(fx!("iso", rig.fx.isolines));
        fields.extend(fx!("cube", rig.fx.cuberoom));
        fields.extend(fx!("spiral", rig.fx.spiral));
        fields.extend(fx!("ball", rig.fx.ball));
        fields.extend(fx!("worm", rig.fx.wormhole));
        fields.extend(fx!("kal", rig.fx.kaleido));
        fields.extend(fx!("rings", rig.fx.rings));
        fields.extend(fx!("boxes", rig.fx.boxes));
        fields
    }

    /// Drop back to what is on disk, leaving whatever it doesn't name at the
    /// default.
    pub fn reload(&mut self, rig: Rig) {
        *self = Self::default();
        (*rig.room, *rig.haze) = (Room::default(), Haze::default());
        (*rig.iso, *rig.cube, *rig.fx) =
            (ScreenParams::default(), CubeParams::default(), Fx::default());
        (*rig.spiral, *rig.ball) = (SpiralParams::default(), BallParams::default());
        *rig.worm = WormholeParams::default();
        (*rig.kal, *rig.rings, *rig.boxes) =
            (KaleidoParams::default(), RingsParams::default(), BoxesParams::default());
        let Ok(text) = std::fs::read_to_string(RIG_FILE) else {
            return;
        };
        let mut fields = self.fields(rig);
        for line in text.lines() {
            let Some((name, value)) = line.split_once(char::is_whitespace) else { continue };
            let Some(value) = value.trim().parse::<f32>().ok() else {
                warn!("rig: {line} is not a number");
                continue;
            };
            match fields.iter_mut().find(|(n, _)| *n == name) {
                Some((_, at)) => **at = value,
                None => warn!("rig: no setting called {name}"),
            }
        }
    }

    pub fn save(&self, rig: Rig) -> std::io::Result<()> {
        let mut trim = *self;
        let (mut room, mut haze) = (Room(rig.room.0), Haze(rig.haze.0));
        let (mut iso, mut cube, mut fx) = (rig.iso.clone(), rig.cube.clone(), rig.fx.clone());
        let (mut spiral, mut ball) = (rig.spiral.clone(), rig.ball.clone());
        let mut worm = rig.worm.clone();
        let (mut kal, mut rings, mut boxes) =
            (rig.kal.clone(), rig.rings.clone(), rig.boxes.clone());
        let copy = Rig {
            room: &mut room,
            haze: &mut haze,
            iso: &mut iso,
            cube: &mut cube,
            spiral: &mut spiral,
            ball: &mut ball,
            worm: &mut worm,
            kal: &mut kal,
            rings: &mut rings,
            boxes: &mut boxes,
            fx: &mut fx,
        };
        let body: String =
            trim.fields(copy).iter().map(|(n, v)| format!("{n} {v}\n")).collect();
        std::fs::write(RIG_FILE, body)
    }
}

/// Startup: pull whatever was saved.
pub fn load(
    mut trim: ResMut<Trim>,
    mut room: ResMut<Room>,
    mut haze: ResMut<Haze>,
    mut iso: ResMut<ScreenParams>,
    mut cube: ResMut<CubeParams>,
    mut fx: ResMut<Fx>,
    patterns: (
        ResMut<SpiralParams>,
        ResMut<BallParams>,
        ResMut<WormholeParams>,
        ResMut<KaleidoParams>,
        ResMut<RingsParams>,
        ResMut<BoxesParams>,
    ),
) {
    let (mut spiral, mut ball, mut worm, mut kal, mut rings, mut boxes) = patterns;
    trim.reload(Rig {
        room: &mut room,
        haze: &mut haze,
        iso: &mut iso,
        cube: &mut cube,
        spiral: &mut spiral,
        ball: &mut ball,
        worm: &mut worm,
        kal: &mut kal,
        rings: &mut rings,
        boxes: &mut boxes,
        fx: &mut fx,
    });
}

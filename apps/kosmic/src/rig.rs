//! The settings that belong to the venue rather than to the show: how hard each
//! family of fixture is driven, how bright the room is, and how much haze is in
//! the air. They are set once when the rig is up and saved together.

use lib::prelude::*;

use crate::sim::Room;

/// Saved settings, one `<name> <value>` per line.
const RIG_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/rig.txt");

/// Brightness multipliers, on top of whatever the show and the master fader
/// asked for. One per fixture family, since they are nowhere near each other in
/// output, plus a global for pulling the whole rig down.
#[derive(Resource, Clone, Copy)]
pub struct Trim {
    pub global: f32,
    /// The Outcast's ring and its centre beam, which are far apart in output and
    /// are the two halves of the same fixture.
    pub ring: f32,
    pub beam: f32,
    pub hydro: f32,
    pub colorado: f32,
}

impl Default for Trim {
    fn default() -> Self {
        Self { global: 1.0, ring: 1.0, beam: 1.0, hydro: 1.0, colorado: 1.0 }
    }
}

impl Trim {
    /// Every saved field, by the name it is written under.
    fn fields<'a>(&'a mut self, room: &'a mut Room, haze: &'a mut Haze) -> [(&'static str, &'a mut f32); 7] {
        [
            ("global", &mut self.global),
            ("outcast_ring", &mut self.ring),
            ("outcast_beam", &mut self.beam),
            ("hydro", &mut self.hydro),
            ("colorado", &mut self.colorado),
            ("room", &mut room.0),
            ("haze", &mut haze.0),
        ]
    }

    /// Drop back to what is on disk, leaving whatever it doesn't name at the
    /// default.
    pub fn reload(&mut self, room: &mut Room, haze: &mut Haze) {
        (*self, *room, *haze) = (Self::default(), Room::default(), Haze::default());
        let Ok(text) = std::fs::read_to_string(RIG_FILE) else {
            return;
        };
        let mut fields = self.fields(room, haze);
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

    pub fn save(&self, room: &Room, haze: &Haze) -> std::io::Result<()> {
        let (mut trim, mut room, mut haze) = (*self, Room(room.0), Haze(haze.0));
        let body: String =
            trim.fields(&mut room, &mut haze).iter().map(|(n, v)| format!("{n} {v}\n")).collect();
        std::fs::write(RIG_FILE, body)
    }
}

/// Pull the saved settings in at startup, so the rig comes up how it was left.
pub fn load(mut trim: ResMut<Trim>, mut room: ResMut<Room>, mut haze: ResMut<Haze>) {
    trim.reload(&mut room, &mut haze);
}

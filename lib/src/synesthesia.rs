use std::net::SocketAddr;

use crate::prelude::*;

#[derive(Resource)]
pub struct Synesthesia {
    osc: Osc,
    syn_addr: SocketAddr,
}

impl Synesthesia {
    pub fn new(listen_addr: &str, syn_addr: &str) -> Result<Self> {
        let osc = Osc::new(listen_addr.parse()?)?;
        let syn_addr = syn_addr.parse()?;

        Ok(Self { osc, syn_addr })
    }

    pub fn send(&mut self, addr: String, args: Vec<OscType>) {
        self.osc.send(&self.syn_addr, addr, args);
    }

    pub fn set_slider(&mut self, i: usize, value: f32) {
        self.send(format!("/controls/global/slider/{i}"), vec![OscType::Float(value)]);
    }
    pub fn set_knob(&mut self, i: usize, value: f32) {
        self.send(format!("/controls/global/knob/{i}"), vec![OscType::Float(value)]);
    }
    pub fn set_toggle(&mut self, i: usize, value: f32) {
        self.send(format!("/controls/global/toggle/{i}"), vec![OscType::Float(value)]);
    }
    pub fn set_bang(&mut self, i: usize, value: f32) {
        self.send(format!("/controls/global/bang/{i}"), vec![OscType::Float(value)]);
    }
    pub fn set_xy(&mut self, i: usize, x: f32, y: f32) {
        self.send(format!("/controls/global/xy/{i}"), vec![OscType::Float(x), OscType::Float(y)]);
    }
    pub fn set_color(&mut self, i: usize, color: Rgb) {
        self.send(
            format!("/controls/global/color/{i}"),
            vec![
                OscType::Float(color.0),
                OscType::Float(color.1),
                OscType::Float(color.2),
            ],
        );
    }

    pub fn set_control(&mut self, bank: &str, name: &str, value: f32) {
        self.send(format!("/controls/{bank}/{name}"), vec![OscType::Float(value)]);
    }
    pub fn set_control_color(&mut self, bank: &str, name: &str, color: Rgb) {
        self.send(
            format!("/controls/{bank}/{name}"),
            vec![
                OscType::Float(color.0),
                OscType::Float(color.1),
                OscType::Float(color.2),
            ],
        );
    }

    pub fn launch_scene(&mut self, scene: &str) {
        self.send(format!("/scenes/{scene}"), vec![]);
    }
    pub fn launch_scene_preset(&mut self, scene: &str, preset: &str) {
        self.send(format!("/scenes/{scene}"), vec![OscType::String(preset.into())]);
    }

    pub fn launch_playlist(&mut self, playlist: &str) {
        self.send(format!("/playlist/select"), vec![OscType::String(playlist.into())]);
    }
    pub fn playlist_next(&mut self) {
        self.send("/playlist/next".into(), vec![]);
    }
    pub fn playlist_prev(&mut self) {
        self.send("/playlist/previous".into(), vec![]);
    }

    pub fn launch_media(&mut self, name: &str) {
        self.send("/media/name".into(), vec![OscType::String(name.into())])
    }
}

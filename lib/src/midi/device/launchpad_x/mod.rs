use crate::color::Rgb;
use crate::midi::Midi;
use crate::num::{Byte, Interp};
use std::collections::HashMap;

use super::MidiDevice;

pub mod types;
use types::*;

#[derive(Debug)]
pub struct LaunchpadX {
    mode: Mode,
    /// Cache of last requested output state per position byte.
    /// Stores (output_type, color_bytes) to avoid duplicate MIDI messages.
    cache: std::collections::HashMap<u8, (u8, Vec<u8>)>,
}

impl Default for LaunchpadX {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            cache: HashMap::new(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Input {
    Press(Index, f64),
    Release(Index),

    MonoPressure(f64),
    PolyPressure(Index, f64),

    Up(bool),
    Down(bool),
    Left(bool),
    Right(bool),
    Session(bool),
    Note(bool),
    Custom(bool),
    Capture(bool),

    Volume(bool),
    Pan(bool),
    A(bool),
    B(bool),
    Stop(bool),
    Mute(bool),
    Solo(bool),
    Record(bool),

    Unknown,
}

impl Input {
    pub fn xy(self) -> Option<(i8, i8)> {
        match self {
            Input::Press(i, _) => {
                let c = Coord::from(i);
                Some((c.0, c.1))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Output {
    Light(Pos, PaletteColor),
    Flash(Pos, PaletteColor),
    Pulse(Pos, PaletteColor),
    Off(Pos),
    Rgb(Pos, Rgb),
    Clear,
    ClearColor(Color),
    Batch(Vec<(Pos, Color)>),

    Mode(Mode),
    Brightness(f64),
    Velocity(Velocity),
    Pressure(Pressure, PressureCurve),

    Clock,
}

impl MidiDevice for LaunchpadX {
    type Input = Input;
    type Output = Output;

    fn init(pad: &mut Midi<Self>) {
        pad.send(Output::Mode(Mode::Programmer));
        pad.send(Output::Pressure(Pressure::Polyphonic, PressureCurve::Medium));
        pad.send(Output::Clear);
    }

    fn process_input(&mut self, raw: &[u8]) -> Option<Input> {
        Some(match raw[0] {
            0x90 => match self.mode {
                Mode::Live => Input::Unknown,
                Mode::Programmer => {
                    let i = Index::from_byte(raw[1]);
                    match raw[2] {
                        0 => Input::Release(i),
                        v => Input::Press(i, v.midi_float()),
                    }
                }
            },
            0xB0 => {
                let b = raw[2] == 0x7F;
                match raw[1] {
                    0x5B => Input::Up(b),
                    0x5C => Input::Down(b),
                    0x5D => Input::Left(b),
                    0x5E => Input::Right(b),
                    0x5F => Input::Session(b),
                    0x60 => Input::Note(b),
                    0x61 => Input::Custom(b),
                    0x62 => Input::Capture(b),
                    0x59 => Input::Volume(b),
                    0x4F => Input::Pan(b),
                    0x45 => Input::A(b),
                    0x3B => Input::B(b),
                    0x31 => Input::Stop(b),
                    0x27 => Input::Mute(b),
                    0x1D => Input::Solo(b),
                    0x13 => Input::Record(b),
                    _ => unreachable!(),
                }
            }
            0xD0 => Input::MonoPressure(raw[1].midi_float()),
            0xA0 => Input::PolyPressure(Index::from_byte(raw[1]), raw[2].midi_float()),
            _ => return None,
        })
    }

    fn process_output(&mut self, output: Output) -> Vec<u8> {
        // Helper function to extract position and output type for caching
        fn extract_cache_key(output: &Output) -> Option<(u8, u8)> {
            match output {
                Output::Light(p, _) => Some((p.byte(), 0x90)),
                Output::Flash(p, _) => Some((p.byte(), 0x91)),
                Output::Pulse(p, _) => Some((p.byte(), 0x92)),
                Output::Off(p) => Some((p.byte(), 0x80)),
                Output::Rgb(p, _) => Some((p.byte(), 0xF0)), // SysEx
                _ => None, // Global commands like Mode, Brightness, etc. don't need caching
            }
        }

        // Generate the raw MIDI output
        let midi_output = match output {
            Output::Light(p, col) => vec![0x90, p.byte(), col.byte()],
            Output::Flash(p, col) => vec![0x91, p.byte(), col.byte()],
            Output::Pulse(p, col) => vec![0x92, p.byte(), col.byte()],
            Output::Off(p) => vec![0x80, p.byte(), 0x0],
            Output::Rgb(p, col) => {
                let Rgb(r, g, b) = col;
                vec![
                    0xF0,
                    0x0,
                    0x20,
                    0x29,
                    0x2,
                    0xC,
                    0x3,
                    0x3,
                    p.byte(),
                    r.midi_byte(),
                    g.midi_byte(),
                    b.midi_byte(),
                    0xF7,
                ]
            }
            Output::Clear => {
                // Clear operations reset the cache for all positions
                self.cache.clear();
                
                let mut data = Vec::with_capacity(8 + (81 * 3));
                data.extend_from_slice(&[0xF0, 0x0, 0x20, 0x29, 0x2, 0xC, 0x3]);
                for i in 0..81 {
                    data.extend_from_slice(&[0x0, Index9(i).byte(), 0x0]);
                }
                data.push(0xF7);
                data
            }
            Output::ClearColor(color) => {
                // Clear operations reset the cache for affected positions
                for i in 0..8 {
                    for j in 0..8 {
                        self.cache.remove(&Coord(i, j).byte());
                    }
                }
                
                let mut data = Vec::with_capacity(8 + (64 * 4));
                data.extend_from_slice(&[0xF0, 0x0, 0x20, 0x29, 0x2, 0xC, 0x3]);
                for i in 0..8 {
                    for j in 0..8 {
                        match color {
                            Color::Palette(col) => {
                                data.extend_from_slice(&[0x0, Coord(i, j).byte(), col.byte()]);
                            }
                            Color::Rgb(r, g, b) => {
                                data.extend_from_slice(&[0x3, Coord(i, j).byte(), r.midi_byte(), g.midi_byte(), b.midi_byte()]);
                            }
                        }
                    }
                }
                data.push(0xF7);
                data
            }
            Output::Batch(ref colorspecs) => {
                // Batch operations reset cache for all affected positions
                for (pos, _) in colorspecs {
                    self.cache.remove(&pos.byte());
                }
                
                let mut data = Vec::with_capacity(8 + (81 * 4));
                data.extend_from_slice(&[0xF0, 0x0, 0x20, 0x29, 0x2, 0xC, 0x3]);
                for (pos, color) in colorspecs {
                    match color {
                        Color::Palette(col) => {
                            data.extend_from_slice(&[0x0, pos.byte(), col.byte()]);
                        }
                        Color::Rgb(r, g, b) => {
                            data.extend_from_slice(&[0x3, pos.byte(), r.midi_byte(), g.midi_byte(), b.midi_byte()]);
                        }
                    }
                }
                data.push(0xF7);
                data
            }
            Output::Mode(m) => {
                self.mode = m;
                let mode = match m {
                    Mode::Live => 0,
                    Mode::Programmer => 1,
                };
                vec![0xF0, 0x00, 0x20, 0x29, 0x2, 0x0C, 0x0E, mode, 0xF7]
            }
            Output::Brightness(f) => vec![0xF0, 0x00, 0x20, 0x29, 0x2, 0xC, 0x8, f.midi_byte(), 0xF7],
            Output::Velocity(v) => {
                let curve = match v {
                    Velocity::Low => 0,
                    Velocity::Medium => 1,
                    Velocity::High => 2,
                    Velocity::Fixed(_) => 3,
                };

                let fixed = match v {
                    Velocity::Fixed(v) => v,
                    _ => 0x00,
                };

                vec![0xF0, 0x0, 0x20, 0x29, 0x2, 0xC, 0x04, curve, fixed, 0xF7]
            }
            Output::Pressure(a, t) => {
                let ty = match a {
                    Pressure::Polyphonic => 0,
                    Pressure::Channel => 1,
                    Pressure::Off => 2,
                };

                let thres = match t {
                    PressureCurve::Low => 0,
                    PressureCurve::Medium => 1,
                    PressureCurve::High => 2,
                };

                vec![0xF0, 0x0, 0x20, 0x29, 0x2, 0xC, 0xB, ty, thres, 0xF7]
            }
            Output::Clock => vec![0xF8],
        };

        // Check if we can cache this output and if it's changed
        if let Some((pos_byte, output_type)) = extract_cache_key(&output) {
            let cache_key = (output_type, midi_output.clone());
            
            // Check if this exact output was already sent
            if let Some(cached) = self.cache.get(&pos_byte) {
                if cached == &cache_key {
                    // No change -> return empty vec to avoid duplicate MIDI message
                    return Vec::new();
                }
            }
            
            // Update cache with new state
            self.cache.insert(pos_byte, cache_key);
        }

        midi_output
    }
}

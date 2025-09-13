use super::MidiDevice;
use crate::midi::Midi;

pub mod types;
use types::*;

#[derive(Debug, Default)]
pub struct LaunchControlXL;

#[derive(Copy, Clone, Debug)]
pub enum Input {
    Slider(u8, f32),

    SendA(u8, f32),
    SendB(u8, f32),
    Pan(u8, f32),

    Mode(Mode),
    TrackSelect(bool, bool),
    SendSelect(bool, bool),

    Device(bool),
    Mute(bool),
    Solo(bool),
    Record(bool),

    Focus(u8, bool),
    Control(u8, bool),

    Unknown,
}

#[derive(Clone, Debug)]
pub enum Output {
    SendA(u8, Color, Brightness),
    SendB(u8, Color, Brightness),
    Pan(u8, Color, Brightness),

    Focus(u8, Color, Brightness),
    Control(u8, Color, Brightness),

    TrackSelect(bool, Color, Brightness),
    SendSelect(bool, Color, Brightness),

    Device(Color, Brightness),
    Mute(Color, Brightness),
    Solo(Color, Brightness),
    Record(Color, Brightness),

    Batch(Vec<(Led, Color, Brightness)>),
}

impl Output {
    fn single(&self) -> (Led, Color, Brightness) {
        match self {
            Output::SendA(i, c, b) => (Led::SendA(*i), *c, *b),
            Output::SendB(i, c, b) => (Led::SendB(*i), *c, *b),
            Output::Pan(i, c, b) => (Led::Pan(*i), *c, *b),
            Output::Focus(i, c, b) => (Led::Focus(*i), *c, *b),
            Output::Control(i, c, b) => (Led::Control(*i), *c, *b),
            Output::TrackSelect(b, c, br) => (Led::TrackSelect(*b), *c, *br),
            Output::SendSelect(b, c, br) => (Led::SendSelect(*b), *c, *br),
            Output::Device(c, b) => (Led::Device, *c, *b),
            Output::Mute(c, b) => (Led::Mute, *c, *b),
            Output::Solo(c, b) => (Led::Solo, *c, *b),
            Output::Record(c, b) => (Led::Record, *c, *b),
            Output::Batch(_) => unreachable!(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Led {
    SendA(u8),
    SendB(u8),
    Pan(u8),

    Focus(u8),
    Control(u8),

    TrackSelect(bool),
    SendSelect(bool),

    Device,
    Mute,
    Solo,
    Record,
}

impl Led {
    #[rustfmt::skip]
    fn index(self) -> u8 {
        match self {
            Self::SendA(idx) => idx,
            Self::SendB(idx) => idx + 0x08,
            Self::Pan(idx) => idx + 0x10,
            Self::Focus(idx) => idx + 0x18,
            Self::Control(idx) => idx + 0x20,
            Self::SendSelect(i) => if i { 0x2c } else { 0x2d },
            Self::TrackSelect(i) =>  if i { 0x2e } else { 0x2f },
            Self::Device => 0x28,
            Self::Mute => 0x29,
            Self::Solo => 0x2a,
            Self::Record => 0x2b,
        }
    }
}

fn color_mask(c: Color, b: Brightness) -> u8 {
    let color_mask = match c {
        Color::Red => b.byte(),
        Color::Amber => b.byte() | (b.byte() << 4),
        Color::Green => b.byte() << 4,
    };

    color_mask | 0b1100
}

impl MidiDevice for LaunchControlXL {
    type Input = Input;
    type Output = Output;

    fn init(ctrl: &mut Midi<Self>) {
        use types::*;

        let mut batch = vec![];
        for i in 0..8 {
            batch.push((Led::SendA(i), Color::Red, Brightness::Off));
            batch.push((Led::SendB(i), Color::Red, Brightness::Off));
            batch.push((Led::Pan(i), Color::Red, Brightness::Off));
            batch.push((Led::Focus(i), Color::Red, Brightness::Off));
            batch.push((Led::Control(i), Color::Red, Brightness::Off));
        }
        batch.push((Led::SendSelect(false), Color::Red, Brightness::Off));
        batch.push((Led::SendSelect(true), Color::Red, Brightness::Off));
        batch.push((Led::TrackSelect(false), Color::Red, Brightness::Off));
        batch.push((Led::TrackSelect(true), Color::Red, Brightness::Off));
        batch.push((Led::Device, Color::Red, Brightness::Off));
        batch.push((Led::Mute, Color::Red, Brightness::Off));
        batch.push((Led::Solo, Color::Red, Brightness::Off));
        batch.push((Led::Record, Color::Red, Brightness::Off));
        ctrl.send(Output::Batch(batch));
    }

    fn process_input(&mut self, raw: &[u8]) -> Option<Input> {
        Some(match raw[0] & 0xf0 {
            0xf0 => Input::Mode(match raw[7] {
                0x0 => Mode::User,
                0x8 => Mode::Factory,
                _ => return None,
            }),
            0x90 => match raw[1] {
                0x29..=0x2c => Input::Focus(raw[1] - 0x29, true),
                0x39..=0x3c => Input::Focus(4 + raw[1] - 0x39, true),
                0x49..=0x4c => Input::Control(raw[1] - 0x49, true),
                0x59..=0x5c => Input::Control(4 + raw[1] - 0x59, true),
                0x69 => Input::Device(true),
                0x6a => Input::Mute(true),
                0x6b => Input::Solo(true),
                0x6c => Input::Record(true),
                _ => return None,
            },
            0x80 => match raw[1] {
                0x29..=0x2c => Input::Focus(raw[1] - 0x29, false),
                0x39..=0x3c => Input::Focus(4 + raw[1] - 0x39, false),
                0x49..=0x4c => Input::Control(raw[1] - 0x49, false),
                0x59..=0x5c => Input::Control(4 + raw[1] - 0x59, false),
                0x69 => Input::Device(false),
                0x6a => Input::Mute(false),
                0x6b => Input::Solo(false),
                0x6c => Input::Record(false),
                _ => return None,
            },
            0xb0 => match raw[1] {
                0x0d..=0x14 => Input::SendA(raw[1] - 0x0d, float_diverging(raw[2])),
                0x1d..=0x24 => Input::SendB(raw[1] - 0x1d, float_diverging(raw[2])),
                0x31..=0x38 => Input::Pan(raw[1] - 0x31, float_diverging(raw[2])),
                0x4d..=0x54 => Input::Slider(raw[1] - 0x4d, float(raw[2])),
                0x68..=0x69 => Input::SendSelect(raw[1] == 0x69, raw[2] == 0x7f),
                0x6a..=0x6b => Input::TrackSelect(raw[1] == 0x6b, raw[2] == 0x7f),
                _ => return None,
            },
            _ => return None,
        })
    }

    fn process_output(&mut self, output: Output) -> Vec<u8> {
        let batch = match output {
            Output::Batch(batch) => batch,
            _ => vec![output.single()],
        };

        let mut midi = vec![0xf0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x78, 0x00];
        for (led, color, brightness) in batch {
            // XXX: these buttons seem to only support Amber, and are off when set to Red, etc.
            let color = if matches!(led, Led::Device | Led::Mute | Led::Solo | Led::Record) {
                Color::Amber
            } else {
                color
            };
            midi.push(led.index());
            midi.push(color_mask(color, brightness));
        }
        midi.push(0xf7);

        midi
    }
}

fn float(v: u8) -> f32 {
    (v as f32) / 127.0
}

fn float_diverging(v: u8) -> f32 {
    if v >= 0x40 {
        ((v - 0x40) as f32) / 63.0
    } else {
        -1.0 + ((v as f32) / 64.0)
    }
}

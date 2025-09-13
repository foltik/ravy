// pub mod color;
// pub use color::*;

// pub mod pos;
// pub use pos::*;

// pub mod config;
// pub use config::*;

// pub mod time;
// pub use time::*;

#[derive(Copy, Clone, Debug)]
pub enum Mode {
    User,
    Factory,
}

#[derive(Copy, Clone, Debug)]
pub enum Color {
    Red,
    Amber,
    Green,
}

#[derive(Copy, Clone, Debug)]
pub enum Brightness {
    Off,
    Low,
    Medium,
    High,
}

impl Brightness {
    pub fn byte(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum State {
    Momentary,
    Toggle,
}

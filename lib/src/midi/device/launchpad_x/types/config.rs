#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Live,
    Programmer
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Velocity {
    Low,
    Medium,
    High,
    Fixed(u8)
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Pressure {
    Polyphonic,
    Channel,
    Off
}

#[derive(Copy, Clone, Debug)]
pub enum PressureCurve {
    Low,
    Medium,
    High,
}

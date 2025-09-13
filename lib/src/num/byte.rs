pub trait Byte: Sized {
    /// Convert 0..255 to 0..1f
    fn float(self) -> f64;
    /// Convert 0..127 to 0..1f
    fn midi_float(self) -> f64;
}

impl Byte for u8 {
    fn float(self) -> f64 {
        (self as f64) / 255.0
    }

    fn midi_float(self) -> f64 {
        (self as f64) / 127.0
    }
}

pub trait Byte: Sized {
    /// Convert 0..255 to 0..1f
    fn float(self) -> f32;
    /// Convert 0..127 to 0..1f
    fn midi_float(self) -> f32;
}

impl Byte for u8 {
    fn float(self) -> f32 {
        (self as f32) / 255.0
    }

    fn midi_float(self) -> f32 {
        (self as f32) / 127.0
    }
}

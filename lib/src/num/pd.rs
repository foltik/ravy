#[derive(Clone, Copy, Debug)]
pub struct Pd(pub usize, pub usize);
impl Pd {
    pub fn fr(&self) -> f32 {
        self.0 as f32 / self.1 as f32
    }
    pub fn mul(&self, mul: usize) -> Self {
        Self(self.0 * mul, self.1)
    }
    pub fn div(&self, div: usize) -> Self {
        Self(self.0, self.1 * div)
    }
}

impl Default for Pd {
    fn default() -> Self {
        Self(1, 1)
    }
}

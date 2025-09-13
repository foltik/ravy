use crate::prelude::*;

#[derive(Component)]
pub struct Ema {
    value: f32,
    attack_hz: f32,
    decay_hz: f32,
}

impl Ema {
    pub fn new(decay_hz: f32) -> Self {
        Self::new_asymmetric(decay_hz, decay_hz)
    }

    pub fn new_asymmetric(attack_hz: f32, decay_hz: f32) -> Self {
        Self { value: 0.0, attack_hz, decay_hz }
    }

    pub fn update(&mut self, dt: f32, value: f32) {
        let hz = if value > self.value { self.attack_hz } else { self.decay_hz };
        let alpha = 1.0 - (-hz * dt).exp();
        self.value += (value - self.value) * alpha;
    }

    pub fn force(&mut self, value: f32) {
        self.value = value;
    }
}

impl std::ops::Deref for Ema {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

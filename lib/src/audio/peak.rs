use crate::prelude::*;

/// A meter which tracks the maximum peak over a short timeframe with decay.
#[derive(Component)]
pub struct AudioPeakHold {
    /// Currently held peak in 0..1
    peak: f32,
    /// Seconds since peak was updated to a higher value
    age: f32,
    /// How long to hold the peak before it starts falling
    delay: f32,
    /// Linear decay rate once the hold delay has elapsed.
    decay: f32,
}

impl AudioPeakHold {
    pub fn new(delay_sec: f32, decay_hz: f32) -> Self {
        Self { peak: 0.0, age: 0.0, delay: delay_sec, decay: decay_hz }
    }

    /// Currently held peak in 0..1
    pub fn value(&self) -> f32 {
        self.peak
    }
}

impl Default for AudioPeakHold {
    fn default() -> Self {
        const DEFAULT_DELAY_SEC: f32 = 0.5;
        const DEFAULT_DECAY_HZ: f32 = 0.6;
        Self::new(DEFAULT_DELAY_SEC, DEFAULT_DECAY_HZ)
    }
}

impl std::ops::Deref for AudioPeakHold {
    type Target = f32;
    fn deref(&self) -> &Self::Target {
        &self.peak
    }
}

pub fn update(mut holds: Query<&mut AudioPeakHold>, audio: Res<Audio>, time: Res<Time>) {
    let peak = audio.peak();
    let dt = time.delta_secs();

    for mut meter in &mut holds {
        if peak > meter.peak {
            meter.peak = peak;
            meter.age = 0.0;
        } else {
            meter.age += dt;
            if meter.age > meter.delay {
                meter.peak -= meter.decay * dt;
                meter.peak = meter.peak.max(peak);
            }
        }
    }
}

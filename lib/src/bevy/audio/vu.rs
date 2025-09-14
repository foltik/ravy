use crate::prelude::*;

/// A VU-style meter driven by instantaneous RMS.
#[derive(Component)]
pub struct AudioVU {
    /// Current linear value in 0..1 after integration
    value: f32,
    /// Integration time constant in seconds.
    tau: f32,
    /// Digital reference: 0 VU corresponds to this RMS level in dBFS.
    reference: f32,
}

impl AudioVU {
    pub fn new(tau: f32, reference: f32) -> Self {
        Self { value: 0.0, tau, reference }
    }

    /// Current VU value
    #[inline]
    pub fn value(&self) -> f32 {
        linear_to_dbfs(self.value) - self.reference
    }
}

impl Default for AudioVU {
    fn default() -> Self {
        const DEFAULT_TAU: f32 = 0.3;
        const DEFAULT_REF_DBFS: f32 = -14.0;
        Self::new(DEFAULT_TAU, DEFAULT_REF_DBFS)
    }
}

impl std::ops::Deref for AudioVU {
    type Target = f32;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub fn update(mut vus: Query<&mut AudioVU>, audio: Res<Audio>, time: Res<Time>) {
    let rms = audio.rms();
    let dt = time.delta_secs();

    for mut vu in &mut vus {
        let alpha = dt / (vu.tau + dt);
        vu.value += (rms - vu.value) * alpha;
    }
}

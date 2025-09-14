use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Context;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, SampleRate, Stream, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::prelude::*;

const CHANNELS: u16 = 2;
const BUFFER_SZ: u32 = 64;
const SAMPLE_RATE: u32 = 48_000;

/// System to starts/stops AudioStream when the Audio settings change
pub fn reload(mut audio: ResMut<Audio>) {
    // Only run if audio state has changed
    if audio.input == audio.curr_input && audio.output == audio.curr_output {
        return;
    }
    audio.curr_input = audio.input.clone();
    audio.curr_output = audio.output.clone();

    // Cleanup
    audio.rms.store(0.0f32.to_bits(), Ordering::Relaxed);
    audio.peak.store(0.0f32.to_bits(), Ordering::Relaxed);
    if let Some(stream) = audio.stream.take() {
        stream.stop();
    }

    // Create new AudioStream
    if let Some(input) = audio.input.clone() {
        info!("Audio capture started: in={input:?} out={:?}", &audio.output);
        audio.stream = Some(AudioStream::new(&audio, input, audio.output.clone()));
    }
}

#[derive(Resource, Default)]
pub struct Audio {
    rms: Arc<AtomicU32>,
    peak: Arc<AtomicU32>,

    pub input: Option<String>,
    pub output: Option<String>,
    curr_input: Option<String>,
    curr_output: Option<String>,

    stream: Option<AudioStream>,
}

impl Audio {
    /// Audio samples RMS from 0.0 to 1.0
    pub fn rms(&self) -> f32 {
        f32::from_bits(self.rms.load(Ordering::Relaxed))
    }
    /// Audio samples peak from 0.0 to 1.0
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }

    pub fn available_inputs() -> &'static [String] {
        static INPUTS: LazyLock<Vec<String>> = LazyLock::new(|| {
            cpal::default_host()
                .input_devices()
                .map(|it| it.filter_map(|d| d.name().ok()).collect())
                .unwrap_or_default()
        });
        &INPUTS
    }
    pub fn available_outputs() -> &'static [String] {
        static OUTPUTS: LazyLock<Vec<String>> = LazyLock::new(|| {
            cpal::default_host()
                .output_devices()
                .map(|it| it.filter_map(|d| d.name().ok()).collect())
                .unwrap_or_default()
        });
        &OUTPUTS
    }
}

pub struct AudioStream {
    stop: Arc<AtomicBool>,
    _thread: JoinHandle<()>,
}

impl AudioStream {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn new(audio: &Audio, input: String, output: Option<String>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_ = Arc::clone(&stop);

        let rms = Arc::clone(&audio.rms);
        let peak = Arc::clone(&audio.peak);

        let _thread = thread::spawn(move || {
            match Self::stream(input, output, rms, peak) {
                Ok((_input, _output)) => {
                    while !stop_.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
                Err(e) => error!("Audio capture failed: {e}"),
            };
        });

        Self { stop, _thread }
    }

    fn stream(
        input: String,
        output: Option<String>,
        rms: Arc<AtomicU32>,
        peak: Arc<AtomicU32>,
    ) -> Result<(Stream, Option<Stream>)> {
        // Find devices
        let host = cpal::default_host();
        let input = host
            .input_devices()?
            .find(|d| d.name().map_or(false, |n| n == input))
            .with_context(|| format!("no such input device {input:?}"))?;
        let output = match output {
            Some(output) => Some({
                host.output_devices()?
                    .find(|d| d.name().map_or(false, |n| n == output))
                    .with_context(|| format!("no such output device {output:?}"))?
            }),
            None => None,
        };

        // Size the ringbuffer to hold 8 buffers
        let (tx, rx) = RingBuffer::<f32>::new(BUFFER_SZ as usize * CHANNELS as usize * 8);

        // Create streams with a fixed config
        // TODO: Handle different configs / buffer sizes with resampling
        let config = StreamConfig {
            channels: CHANNELS,
            sample_rate: SampleRate(SAMPLE_RATE),
            buffer_size: BufferSize::Fixed(BUFFER_SZ),
        };
        let _input = Self::stream_input(&input, &config, rms, peak, tx)?;
        let _output = match output {
            Some(output) => Some(Self::stream_output(&output, &config, rx)?),
            None => None,
        };

        _input.play()?;
        if let Some(_output) = &_output {
            _output.play()?;
        }

        Ok((_input, _output))
    }

    fn stream_input(
        device: &Device,
        config: &StreamConfig,
        rms: Arc<AtomicU32>,
        peak: Arc<AtomicU32>,
        mut tx: Producer<f32>,
    ) -> Result<Stream> {
        let err_fn = |e| error!("Audio input: {e}");
        let stream = device.build_input_stream(
            config,
            move |data: &[f32], _| {
                // Copy data to ringbuffer
                let mut frames = data.chunks_exact(CHANNELS as usize);
                'outer: for frame in &mut frames {
                    for &s in frame {
                        if tx.push(s).is_err() {
                            break 'outer; // overrun
                        }
                    }
                }

                let mut sum = 0.0;
                let mut max = 0.0f32;
                for &s in data {
                    sum += s * s;
                    let abs = s.abs();
                    if abs > max {
                        max = abs;
                    }
                }
                let avg = (sum / (data.len().max(1) as f32)).sqrt();

                rms.store(avg.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
                peak.store(max.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
            },
            err_fn,
            None,
        )?;
        Ok(stream)
    }

    fn stream_output(device: &Device, config: &StreamConfig, mut rx: Consumer<f32>) -> Result<Stream> {
        let err_fn = |e| error!("Audio output: {e}");
        let stream = device.build_output_stream(
            config,
            move |out: &mut [f32], _| {
                let samples = out.len();

                let mut i = 0usize;
                while i < samples {
                    match rx.pop() {
                        Ok(s) => {
                            out[i] = s;
                            i += 1;
                        }
                        Err(_) => break, // underrun
                    }
                }

                if i < samples {
                    out[i..].fill(0.0);
                }
            },
            err_fn,
            None,
        )?;
        Ok(stream)
    }
}

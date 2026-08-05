//! Minimal v4l2 webcam capture, manual settings, and egui rendering.
//!
//! Opens a UVC webcam (e.g. Logitech C920), locks all automatic controls, and
//! streams grayscale frames from a background thread. Frames are the Y plane of
//! a YUYV capture, which the driver provides for free — no color conversion.
//!
//! ```no_run
//! # use lib::webcam::{Webcam, WebcamConfig, WebcamView};
//! let cam = Webcam::open(WebcamConfig::default())?;
//! // each UI frame:
//! if let Some(frame) = cam.latest() {
//!     // view.show(ui, &frame) to render; cam.set_exposure(n) to adjust.
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::video::capture::Parameters;
use v4l::{Device, FourCC};

use crate::prelude::*;

/// v4l2 control ids (`VIDIOC_S_CTRL`) common to UVC webcams.
pub mod cid {
    pub const WHITE_BALANCE_AUTO: u32 = 0x0098090c;
    pub const GAIN: u32 = 0x00980913;
    pub const AUTO_EXPOSURE: u32 = 0x009a0901;
    pub const EXPOSURE_TIME: u32 = 0x009a0902;
    pub const EXPOSURE_DYNAMIC_FPS: u32 = 0x009a0903;
    pub const FOCUS_ABSOLUTE: u32 = 0x009a090a;
    pub const FOCUS_AUTO: u32 = 0x009a090c;
}

/// How to open a webcam: device index, capture format, and initial manual
/// exposure/gain/focus (autos are always disabled on open).
#[derive(Clone, Copy, Debug)]
pub struct WebcamConfig {
    pub index: usize,
    pub width: usize,
    pub height: usize,
    pub fps: u32,
    /// Initial exposure time in v4l2 units (100µs steps).
    pub exposure: i64,
    /// Initial sensor gain.
    pub gain: i64,
    /// Initial focus (0 = infinity for most UVC lenses).
    pub focus: i64,
}

impl Default for WebcamConfig {
    fn default() -> Self {
        Self { index: 0, width: 640, height: 480, fps: 30, exposure: 250, gain: 64, focus: 0 }
    }
}

/// One grayscale frame: the Y plane of a YUYV capture.
#[derive(Clone)]
pub struct Frame {
    /// Driver (monotonic) timestamp in seconds.
    pub t: f64,
    pub width: usize,
    pub height: usize,
    pub gray: Vec<u8>,
}

/// A running webcam: grayscale frames arrive on a background thread, and
/// control changes are applied between captures.
#[derive(Resource)]
pub struct Webcam {
    frames: Mutex<Receiver<Frame>>,
    ctrls: Sender<(u32, i64)>,
    width: usize,
    height: usize,
}

impl Webcam {
    /// Open the device, lock every auto control, apply the initial manual
    /// values, and start streaming frames from a background thread.
    pub fn open(cfg: WebcamConfig) -> anyhow::Result<Self> {
        let dev = Device::new(cfg.index).context("open webcam")?;

        let mut fmt = dev.format()?;
        fmt.width = cfg.width as u32;
        fmt.height = cfg.height as u32;
        fmt.fourcc = FourCC::new(b"YUYV");
        let fmt = dev.set_format(&fmt)?;
        anyhow::ensure!(fmt.fourcc == FourCC::new(b"YUYV"), "YUYV unsupported, got {}", fmt.fourcc);
        dev.set_params(&Parameters::with_fps(cfg.fps))?;

        // Autos off first: the manual values are inactive until then.
        for (id, v) in [
            (cid::FOCUS_AUTO, 0),
            (cid::WHITE_BALANCE_AUTO, 0),
            (cid::AUTO_EXPOSURE, 1),
            (cid::EXPOSURE_DYNAMIC_FPS, 0),
            (cid::FOCUS_ABSOLUTE, cfg.focus),
            (cid::EXPOSURE_TIME, cfg.exposure),
            (cid::GAIN, cfg.gain),
        ] {
            set_ctrl(&dev, id, v)?;
        }

        let (frame_tx, frame_rx) = channel();
        let (ctrl_tx, ctrl_rx) = channel::<(u32, i64)>();
        let (width, height) = (cfg.width, cfg.height);
        std::thread::spawn(move || stream(dev, width, height, frame_tx, ctrl_rx));

        Ok(Self { frames: Mutex::new(frame_rx), ctrls: ctrl_tx, width, height })
    }

    /// Capture resolution (width, height).
    pub fn dims(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Queue a raw control change, applied before the next capture.
    pub fn set_ctrl(&self, id: u32, value: i64) {
        let _ = self.ctrls.send((id, value));
    }

    /// Set the manual exposure time (v4l2 units, 100µs steps).
    pub fn set_exposure(&self, v: i64) {
        self.set_ctrl(cid::EXPOSURE_TIME, v);
    }

    /// Set the sensor gain.
    pub fn set_gain(&self, v: i64) {
        self.set_ctrl(cid::GAIN, v);
    }

    /// Set the absolute focus (0 = infinity for most UVC lenses).
    pub fn set_focus(&self, v: i64) {
        self.set_ctrl(cid::FOCUS_ABSOLUTE, v);
    }

    /// The newest queued frame, discarding any older ones. `None` if no new
    /// frame has arrived since the last call.
    pub fn latest(&self) -> Option<Frame> {
        self.frames.lock().unwrap().try_iter().last()
    }

    /// Every frame queued since the last call, oldest first. Use this when
    /// per-frame timing matters and dropped frames would corrupt it.
    pub fn drain(&self) -> Vec<Frame> {
        self.frames.lock().unwrap().try_iter().collect()
    }
}

fn set_ctrl(dev: &Device, id: u32, value: i64) -> anyhow::Result<()> {
    use v4l::control::{Control, Value};
    dev.set_control(Control { id, value: Value::Integer(value) })
        .with_context(|| format!("set control {id:#x}"))?;
    Ok(())
}

/// Camera thread: capture frames forever, applying queued control tweaks first.
fn stream(dev: Device, width: usize, height: usize, frames: Sender<Frame>, ctrls: Receiver<(u32, i64)>) {
    let mut stream = match Stream::with_buffers(&dev, Type::VideoCapture, 4) {
        Ok(s) => s,
        Err(e) => return error!("webcam stream: {e}"),
    };
    loop {
        while let Ok((id, v)) = ctrls.try_recv() {
            if let Err(e) = set_ctrl(&dev, id, v) {
                warn!("webcam ctrl: {e}");
            }
        }
        let (buf, meta) = match stream.next() {
            Ok(f) => f,
            Err(e) => return error!("webcam capture: {e}"),
        };
        let t = meta.timestamp.sec as f64 + meta.timestamp.usec as f64 * 1e-6;
        let gray = (0..width * height).map(|i| buf[2 * i]).collect();
        if frames.send(Frame { t, width, height, gray }).is_err() {
            return; // receiver dropped
        }
    }
}

/// Persistent egui texture for displaying grayscale [`Frame`]s.
#[derive(Default)]
pub struct WebcamView {
    tex: Option<egui::TextureHandle>,
}

impl WebcamView {
    /// Upload `frame` and paint it into the available space, preserving aspect
    /// ratio. Returns the on-screen rect and the px-per-pixel scale, so callers
    /// can overlay their own annotations in frame coordinates:
    /// `screen = rect.min + vec2(x, y) * scale`.
    pub fn show(&mut self, ui: &mut egui::Ui, frame: &Frame) -> (egui::Rect, f32) {
        let img = egui::ColorImage::from_gray([frame.width, frame.height], &frame.gray);
        let tex = self
            .tex
            .get_or_insert_with(|| ui.ctx().load_texture("webcam", img.clone(), egui::TextureOptions::LINEAR));
        tex.set(img, egui::TextureOptions::LINEAR);

        let (w, h) = (frame.width as f32, frame.height as f32);
        let scale = (ui.available_width() / w).min(ui.available_height() / h);
        let size = egui::vec2(w, h) * scale;
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        egui::Image::new((tex.id(), size)).paint_at(ui, rect);
        (rect, scale)
    }
}

/// Manual exposure/gain/focus with a slider UI. Holds the current values and
/// pushes changes to the [`Webcam`] as they're edited.
#[derive(Clone, Copy, Debug)]
pub struct WebcamControls {
    pub exposure: i64,
    pub gain: i64,
    pub focus: i64,
}

impl WebcamControls {
    /// Controls matching a config's initial manual values.
    pub fn from_config(cfg: &WebcamConfig) -> Self {
        Self { exposure: cfg.exposure, gain: cfg.gain, focus: cfg.focus }
    }

    /// Push all current values to the device (e.g. once after open).
    pub fn apply(&self, cam: &Webcam) {
        cam.set_exposure(self.exposure);
        cam.set_gain(self.gain);
        cam.set_focus(self.focus);
    }

    /// Draw exposure/gain/focus sliders, applying edits to the device.
    pub fn ui(&mut self, ui: &mut egui::Ui, cam: &Webcam) {
        if ui.add(egui::Slider::new(&mut self.exposure, 3..=2047).text("exposure").logarithmic(true)).changed() {
            cam.set_exposure(self.exposure);
        }
        if ui.add(egui::Slider::new(&mut self.gain, 0..=255).text("gain")).changed() {
            cam.set_gain(self.gain);
        }
        if ui.add(egui::Slider::new(&mut self.focus, 0..=255).text("focus")).changed() {
            cam.set_focus(self.focus);
        }
    }
}

impl Default for WebcamControls {
    fn default() -> Self {
        Self::from_config(&WebcamConfig::default())
    }
}

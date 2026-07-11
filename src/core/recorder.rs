use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

use crate::core::audio::downmix_resample_to_i16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAudio {
    pub samples: Vec<i16>,
    pub sample_rate_hz: u32,
    pub channels: u16,
}

impl RecordedAudio {
    pub fn new(samples: Vec<i16>, sample_rate_hz: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate_hz,
            channels,
        }
    }
}

#[async_trait(?Send)]
pub trait Recorder {
    async fn start(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<RecordedAudio>;
}

pub struct CpalRecorder {
    capture: Option<Capture>,
    selected_microphone: Option<String>,
}

struct Capture {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: cpal::Stream,
    source_rate_hz: u32,
    source_channels: u16,
}

impl CpalRecorder {
    pub fn new() -> Self {
        Self::with_selected_microphone(None)
    }

    pub fn with_audio_config(config: &crate::core::config::AudioConfig) -> Self {
        Self::with_selected_microphone(config.selected_microphone.clone())
    }

    pub fn with_selected_microphone(selected_microphone: Option<String>) -> Self {
        Self {
            capture: None,
            selected_microphone,
        }
    }
}

impl Default for CpalRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Recorder for CpalRecorder {
    async fn start(&mut self) -> Result<()> {
        if self.capture.is_some() {
            return Err(anyhow!("recorder is already running"));
        }

        let host = cpal::default_host();
        let device = select_input_device(&host, self.selected_microphone.as_deref())?;
        let supported = device
            .default_input_config()
            .context("reading default input config")?;
        let source_rate_hz = supported.sample_rate().0;
        let source_channels = supported.channels();
        let config: cpal::StreamConfig = supported.clone().into();
        let samples = Arc::new(Mutex::new(Vec::new()));
        let err_fn = |err| eprintln!("openvoice-desktop audio stream error: {err}");

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => {
                let samples = Arc::clone(&samples);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| push_f32(&samples, data.iter().copied()),
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let samples = Arc::clone(&samples);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        push_f32(&samples, data.iter().map(|s| *s as f32 / i16::MAX as f32))
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let samples = Arc::clone(&samples);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        push_f32(
                            &samples,
                            data.iter()
                                .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                        )
                    },
                    err_fn,
                    None,
                )?
            }
            other => return Err(anyhow!("unsupported input sample format: {other:?}")),
        };

        stream.play()?;
        self.capture = Some(Capture {
            samples,
            stream,
            source_rate_hz,
            source_channels,
        });
        Ok(())
    }

    async fn stop(&mut self) -> Result<RecordedAudio> {
        let capture = self
            .capture
            .take()
            .ok_or_else(|| anyhow!("recorder is not running"))?;
        drop(capture.stream);
        let samples = capture
            .samples
            .lock()
            .map_err(|_| anyhow!("audio sample buffer lock poisoned"))?
            .clone();
        Ok(downmix_resample_to_i16(
            &samples,
            capture.source_rate_hz,
            capture.source_channels,
        ))
    }
}

fn select_input_device(host: &cpal::Host, selected: Option<&str>) -> Result<cpal::Device> {
    if let Some(selected) = selected {
        let selected_device = host
            .input_devices()
            .context("enumerating input devices")?
            .find(|device| device.name().ok().as_deref() == Some(selected));
        if let Some(device) = selected_device {
            return Ok(device);
        }
        eprintln!(
            "openvoice-desktop microphone {:?} unavailable; falling back to system default",
            selected
        );
    }
    host.default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))
}

fn push_f32<I>(samples: &Arc<Mutex<Vec<f32>>>, data: I)
where
    I: IntoIterator<Item = f32>,
{
    if let Ok(mut guard) = samples.lock() {
        guard.extend(data);
    }
}

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{atomic::AtomicU8, Arc, Mutex};

use crate::core::audio::downmix_resample_to_i16;
use crate::core::meter::{CaptureMeter, MicrophoneLevels};

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
    async fn pause(&mut self) -> Result<()> {
        anyhow::bail!("recorder does not support pausing")
    }
    async fn resume(&mut self) -> Result<()> {
        anyhow::bail!("recorder does not support resuming")
    }
    fn level_bucket(&self) -> u8 {
        0
    }
}

pub struct CpalRecorder {
    capture: Option<Capture>,
    selected_microphone: Option<String>,
    levels: MicrophoneLevels,
}

struct Capture {
    samples: Arc<Mutex<CaptureBuffer>>,
    stream: cpal::Stream,
    source_rate_hz: u32,
    source_channels: u16,
}

struct CaptureBuffer {
    meter: CaptureMeter,
    samples: Vec<f32>,
    paused: bool,
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
            levels: MicrophoneLevels::default(),
        }
    }

    fn set_paused(&mut self, paused: bool) -> Result<()> {
        let capture = self
            .capture
            .as_ref()
            .ok_or_else(|| anyhow!("recorder is not running"))?;
        let mut buffer = capture
            .samples
            .lock()
            .map_err(|_| anyhow!("audio sample buffer lock poisoned"))?;
        buffer.paused = paused;
        buffer.meter.reset(&self.levels);
        Ok(())
    }

    pub fn levels(&self) -> MicrophoneLevels {
        self.levels.clone()
    }

    /// Share the capture level with the status snapshot.
    pub fn level_handle(&self) -> Arc<AtomicU8> {
        self.levels.peak_handle()
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
        let samples = Arc::new(Mutex::new(CaptureBuffer {
            samples: Vec::new(),
            paused: false,
            meter: CaptureMeter::new(source_rate_hz, source_channels),
        }));
        self.levels.clear();

        let err_fn = |err| eprintln!("openvoice-desktop audio stream error: {err}");

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => {
                let samples = Arc::clone(&samples);
                let levels = self.levels();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        push_f32(&samples, &levels, data.iter().copied());
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let samples = Arc::clone(&samples);
                let levels = self.levels();
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let values = data.iter().map(|s| *s as f32 / i16::MAX as f32);
                        push_f32(&samples, &levels, values)
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let samples = Arc::clone(&samples);
                let levels = self.levels();
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let values = data
                            .iter()
                            .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0);
                        push_f32(&samples, &levels, values)
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
        let samples = std::mem::take(
            &mut capture
                .samples
                .lock()
                .map_err(|_| anyhow!("audio sample buffer lock poisoned"))?
                .samples,
        );
        self.levels.clear();
        Ok(downmix_resample_to_i16(
            &samples,
            capture.source_rate_hz,
            capture.source_channels,
        ))
    }

    async fn pause(&mut self) -> Result<()> {
        self.set_paused(true)
    }

    async fn resume(&mut self) -> Result<()> {
        self.set_paused(false)
    }

    fn level_bucket(&self) -> u8 {
        self.levels.peak_bucket()
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

fn push_f32<I>(samples: &Arc<Mutex<CaptureBuffer>>, levels: &MicrophoneLevels, data: I)
where
    I: IntoIterator<Item = f32>,
{
    if let Ok(mut guard) = samples.lock() {
        if !guard.paused {
            let buffer = &mut *guard;
            let start = buffer.samples.len();
            buffer.samples.extend(data);
            buffer.meter.observe(&buffer.samples[start..], levels);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_callback_omits_paused_samples_and_keeps_resumed_segments() {
        let samples = Arc::new(Mutex::new(CaptureBuffer {
            samples: Vec::new(),
            paused: false,
            meter: CaptureMeter::new(16000, 1),
        }));
        let level = MicrophoneLevels::default();
        push_f32(&samples, &level, [0.1, 0.2]);
        samples.lock().unwrap().paused = true;
        level.clear();
        for _ in 0..100 {
            push_f32(&samples, &level, [0.9; 128]);
        }
        assert_eq!(level.peak_bucket(), 0);
        assert_eq!(samples.lock().unwrap().samples, [0.1, 0.2]);
        samples.lock().unwrap().paused = false;
        push_f32(&samples, &level, [0.3, 0.4]);
        assert_eq!(samples.lock().unwrap().samples, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(level.peak_bucket(), 4);
        let audio = downmix_resample_to_i16(&samples.lock().unwrap().samples, 16_000, 1);
        let file = tempfile::NamedTempFile::new().unwrap();
        crate::core::audio::write_wav(file.path(), &audio).unwrap();
        let wav = hound::WavReader::open(file.path()).unwrap();
        assert_eq!(wav.duration(), 4);
    }
}

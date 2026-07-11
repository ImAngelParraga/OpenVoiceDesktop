use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::config::{AudioConfig, SoundTheme, TextConfig};
use crate::core::recorder::RecordedAudio;

pub const DEFAULT_SAMPLE_RATE_HZ: u32 = 16_000;
pub const DEFAULT_CHANNELS: u16 = 1;
pub const PCM_BITS_PER_SAMPLE: u16 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioEncodingSettings {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub editable: bool,
}

pub const fn effective_audio_encoding() -> AudioEncodingSettings {
    AudioEncodingSettings {
        sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
        channels: DEFAULT_CHANNELS,
        editable: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavWriteStats {
    pub elapsed: Duration,
    pub bytes: u64,
}

pub fn write_wav(path: &Path, audio: &RecordedAudio) -> Result<WavWriteStats> {
    let start = Instant::now();
    let spec = WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate_hz,
        bits_per_sample: PCM_BITS_PER_SAMPLE,
        sample_format: SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(path, spec).with_context(|| format!("creating {}", path.display()))?;
    for sample in &audio.samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    let bytes = std::fs::metadata(path)?.len();
    Ok(WavWriteStats {
        elapsed: start.elapsed(),
        bytes,
    })
}

pub fn downmix_resample_to_i16(
    input: &[f32],
    source_rate: u32,
    source_channels: u16,
) -> RecordedAudio {
    let source_channels = source_channels.max(1) as usize;
    let frames = input.len() / source_channels;
    if frames == 0 {
        return RecordedAudio::new(Vec::new(), DEFAULT_SAMPLE_RATE_HZ, DEFAULT_CHANNELS);
    }

    let mut mono = Vec::with_capacity(frames);
    for frame in input.chunks_exact(source_channels) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum / source_channels as f32);
    }

    if source_rate == DEFAULT_SAMPLE_RATE_HZ {
        return RecordedAudio::new(
            float_to_i16(&mono),
            DEFAULT_SAMPLE_RATE_HZ,
            DEFAULT_CHANNELS,
        );
    }

    let ratio = source_rate as f64 / DEFAULT_SAMPLE_RATE_HZ as f64;
    let output_len = ((mono.len() as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let source_pos = i as f64 * ratio;
        let left = source_pos.floor() as usize;
        let right = (left + 1).min(mono.len() - 1);
        let t = (source_pos - left as f64) as f32;
        let sample = mono[left] * (1.0 - t) + mono[right] * t;
        output.push(sample.clamp(-1.0, 1.0));
    }

    RecordedAudio::new(
        float_to_i16(&output),
        DEFAULT_SAMPLE_RATE_HZ,
        DEFAULT_CHANNELS,
    )
}

fn float_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelection {
    Selected(String),
    DefaultWithMissingSelected { missing: String },
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub fn enumerate_input_devices() -> Result<Vec<InputDeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let devices = host
        .input_devices()
        .context("enumerating input devices")?
        .filter_map(|device| device.name().ok())
        .map(|name| InputDeviceInfo {
            is_default: default_name.as_deref() == Some(name.as_str()),
            name,
        })
        .collect();
    Ok(devices)
}

pub fn enumerate_output_devices() -> Result<Vec<OutputDeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|device| device.name().ok());
    let devices = host
        .output_devices()
        .context("enumerating output devices")?
        .filter_map(|device| device.name().ok())
        .map(|name| OutputDeviceInfo {
            is_default: default_name.as_deref() == Some(name.as_str()),
            name,
        })
        .collect();
    Ok(devices)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputDeviceResolution {
    Selected {
        name: String,
    },
    Default {
        name: Option<String>,
    },
    DefaultWithMissingSelected {
        missing: String,
        default_name: Option<String>,
    },
}

pub fn resolve_input_device_details(
    config: &AudioConfig,
    available_devices: &[InputDeviceInfo],
) -> InputDeviceResolution {
    let default_name = available_devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.name.clone());
    match &config.selected_microphone {
        Some(selected)
            if available_devices
                .iter()
                .any(|device| &device.name == selected) =>
        {
            InputDeviceResolution::Selected {
                name: selected.clone(),
            }
        }
        Some(selected) => InputDeviceResolution::DefaultWithMissingSelected {
            missing: selected.clone(),
            default_name,
        },
        None => InputDeviceResolution::Default { name: default_name },
    }
}

pub fn resolve_input_device(config: &AudioConfig, available_devices: &[String]) -> DeviceSelection {
    match &config.selected_microphone {
        Some(selected) if available_devices.iter().any(|device| device == selected) => {
            DeviceSelection::Selected(selected.clone())
        }
        Some(selected) => DeviceSelection::DefaultWithMissingSelected {
            missing: selected.clone(),
        },
        None => DeviceSelection::Default,
    }
}

pub fn resolve_output_device(
    config: &AudioConfig,
    available_devices: &[String],
) -> DeviceSelection {
    match &config.selected_output_device {
        Some(selected) if available_devices.iter().any(|device| device == selected) => {
            DeviceSelection::Selected(selected.clone())
        }
        Some(selected) => DeviceSelection::DefaultWithMissingSelected {
            missing: selected.clone(),
        },
        None => DeviceSelection::Default,
    }
}

pub fn resolve_output_device_details(
    config: &AudioConfig,
    available_devices: &[OutputDeviceInfo],
) -> OutputDeviceResolution {
    let default_name = available_devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.name.clone());
    match &config.selected_output_device {
        Some(selected)
            if available_devices
                .iter()
                .any(|device| &device.name == selected) =>
        {
            OutputDeviceResolution::Selected {
                name: selected.clone(),
            }
        }
        Some(selected) => OutputDeviceResolution::DefaultWithMissingSelected {
            missing: selected.clone(),
            default_name,
        },
        None => OutputDeviceResolution::Default { name: default_name },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputDeviceResolution {
    Selected {
        name: String,
    },
    Default {
        name: Option<String>,
    },
    DefaultWithMissingSelected {
        missing: String,
        default_name: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFeedbackEvent {
    Start,
    Stop,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFeedbackPlan {
    pub enabled: bool,
    pub event: AudioFeedbackEvent,
    pub output_device: Option<String>,
    pub volume: f32,
    pub sound_theme: SoundTheme,
    pub custom_sound_path: Option<std::path::PathBuf>,
}

pub fn audio_feedback_plan(
    config: &AudioConfig,
    event: AudioFeedbackEvent,
) -> Option<AudioFeedbackPlan> {
    config.audio_feedback.then(|| AudioFeedbackPlan {
        enabled: true,
        event,
        output_device: config.selected_output_device.clone(),
        volume: config.audio_feedback_volume.clamp(0.0, 1.0),
        sound_theme: config.sound_theme,
        custom_sound_path: match event {
            AudioFeedbackEvent::Start => config.custom_start_sound.clone(),
            AudioFeedbackEvent::Stop => config.custom_stop_sound.clone(),
            AudioFeedbackEvent::Error => config.custom_error_sound.clone(),
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackSound {
    BuiltIn { frequency_hz: u32, duration_ms: u64 },
    Custom(std::path::PathBuf),
}

pub fn feedback_sound(plan: &AudioFeedbackPlan) -> Result<FeedbackSound> {
    if plan.sound_theme == SoundTheme::Custom {
        return plan
            .custom_sound_path
            .clone()
            .map(FeedbackSound::Custom)
            .ok_or_else(|| anyhow!("custom sound theme requires a sound path"));
    }

    let (frequency_hz, duration_ms) = match (plan.sound_theme, plan.event) {
        (SoundTheme::Minimal, AudioFeedbackEvent::Start) => (880, 45),
        (SoundTheme::Minimal, AudioFeedbackEvent::Stop) => (660, 45),
        (SoundTheme::Minimal, AudioFeedbackEvent::Error) => (220, 100),
        (SoundTheme::Default, AudioFeedbackEvent::Start) => (740, 90),
        (SoundTheme::Default, AudioFeedbackEvent::Stop) => (520, 90),
        (SoundTheme::Default, AudioFeedbackEvent::Error) => (180, 180),
        (SoundTheme::Custom, _) => unreachable!(),
    };
    Ok(FeedbackSound::BuiltIn {
        frequency_hz,
        duration_ms,
    })
}

/// Play a short feedback sound through the selected CPAL output device.
/// Platform adapters own calling this function so the core remains agnostic
/// about how feedback is triggered, while tests can replace the adapter.
pub fn play_audio_feedback(plan: &AudioFeedbackPlan) -> Result<()> {
    let host = cpal::default_host();
    let device = select_output_device(&host, plan.output_device.as_deref())?;
    let supported = device
        .default_output_config()
        .context("reading default output config")?;
    let output_rate = supported.sample_rate().0;
    let output_channels = supported.channels().max(1) as usize;
    let source = feedback_samples(plan, output_rate)?;
    if source.is_empty() {
        return Ok(());
    }

    let samples = Arc::new(Mutex::new(source));
    let cursor = Arc::new(Mutex::new(0usize));
    let callback_samples = Arc::clone(&samples);
    let callback_cursor = Arc::clone(&cursor);
    let err_fn = |error| eprintln!("openvoice-desktop audio feedback stream error: {error}");
    let stream_config: cpal::StreamConfig = supported.clone().into();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let callback_samples = Arc::clone(&callback_samples);
            let callback_cursor = Arc::clone(&callback_cursor);
            device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    fill_feedback_buffer(
                        data,
                        output_channels,
                        &callback_samples,
                        &callback_cursor,
                        |sample| sample,
                    )
                },
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let callback_samples = Arc::clone(&callback_samples);
            let callback_cursor = Arc::clone(&callback_cursor);
            device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    fill_feedback_buffer(
                        data,
                        output_channels,
                        &callback_samples,
                        &callback_cursor,
                        |sample| (sample * i16::MAX as f32) as i16,
                    )
                },
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let callback_samples = Arc::clone(&callback_samples);
            let callback_cursor = Arc::clone(&callback_cursor);
            device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _| {
                    fill_feedback_buffer(
                        data,
                        output_channels,
                        &callback_samples,
                        &callback_cursor,
                        |sample| ((sample.clamp(-1.0, 1.0) + 1.0) * 0.5 * u16::MAX as f32) as u16,
                    )
                },
                err_fn,
                None,
            )?
        }
        other => return Err(anyhow!("unsupported output sample format: {other:?}")),
    };

    stream.play()?;
    let duration = {
        let samples = samples
            .lock()
            .map_err(|_| anyhow!("audio feedback buffer lock poisoned"))?;
        Duration::from_secs_f64(samples.len() as f64 / output_rate as f64)
    };
    std::thread::sleep(duration + Duration::from_millis(20));
    drop(stream);
    Ok(())
}

fn select_output_device(host: &cpal::Host, selected: Option<&str>) -> Result<cpal::Device> {
    if let Some(selected) = selected {
        let selected_device = host
            .output_devices()
            .context("enumerating output devices")?
            .find(|device| device.name().ok().as_deref() == Some(selected));
        if let Some(device) = selected_device {
            return Ok(device);
        }
        eprintln!(
            "openvoice-desktop output device {:?} unavailable; falling back to system default",
            selected
        );
    }
    host.default_output_device()
        .ok_or_else(|| anyhow!("no default output device"))
}

fn feedback_samples(plan: &AudioFeedbackPlan, sample_rate: u32) -> Result<Vec<f32>> {
    match feedback_sound(plan)? {
        FeedbackSound::BuiltIn {
            frequency_hz,
            duration_ms,
        } => {
            let length = (sample_rate as u64 * duration_ms / 1_000) as usize;
            let volume = plan.volume.clamp(0.0, 1.0) * 0.25;
            Ok((0..length)
                .map(|index| {
                    let phase = index as f32 * frequency_hz as f32 * std::f32::consts::TAU
                        / sample_rate as f32;
                    let envelope = 1.0 - index as f32 / length.max(1) as f32;
                    phase.sin() * volume * envelope
                })
                .collect())
        }
        FeedbackSound::Custom(path) => load_custom_sound(&path, sample_rate, plan.volume),
    }
}

fn load_custom_sound(path: &std::path::Path, sample_rate: u32, volume: f32) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("opening custom sound {}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let source = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i32>()
            .map(|sample| sample.map(|sample| sample as f32 / i32::MAX as f32))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    let mono: Vec<f32> = source
        .chunks_exact(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
        .collect();
    if spec.sample_rate == sample_rate {
        return Ok(mono
            .into_iter()
            .map(|sample| sample.clamp(-1.0, 1.0) * volume)
            .collect());
    }
    let ratio = spec.sample_rate as f64 / sample_rate as f64;
    let output_len = ((mono.len() as f64) / ratio).ceil() as usize;
    Ok((0..output_len)
        .map(|index| {
            let position = index as f64 * ratio;
            let left = position.floor() as usize;
            let right = (left + 1).min(mono.len().saturating_sub(1));
            let fraction = (position - left as f64) as f32;
            let sample = mono.get(left).copied().unwrap_or_default() * (1.0 - fraction)
                + mono.get(right).copied().unwrap_or_default() * fraction;
            sample.clamp(-1.0, 1.0) * volume
        })
        .collect())
}

fn fill_feedback_buffer<T, F>(
    data: &mut [T],
    channels: usize,
    samples: &Arc<Mutex<Vec<f32>>>,
    cursor: &Arc<Mutex<usize>>,
    convert: F,
) where
    T: Copy,
    F: Fn(f32) -> T,
{
    let mut cursor = match cursor.lock() {
        Ok(cursor) => cursor,
        Err(_) => {
            return;
        }
    };
    let samples = match samples.lock() {
        Ok(samples) => samples,
        Err(_) => return,
    };
    for frame in data.chunks_mut(channels) {
        let sample = samples.get(*cursor).copied().unwrap_or_default();
        *cursor = cursor.saturating_add(1);
        for output in frame {
            *output = convert(sample);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuteRestorePlan {
    pub mute_before_recording: bool,
    pub restore_after_finish: bool,
}

pub fn mute_restore_plan(config: &AudioConfig) -> MuteRestorePlan {
    MuteRestorePlan {
        mute_before_recording: config.mute_while_recording,
        restore_after_finish: config.mute_while_recording,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VadDecision {
    RawRemoteAsr,
    EmptyClip,
    Trim {
        leading_samples: usize,
        trailing_samples: usize,
    },
}

pub fn apply_vad(
    audio: crate::core::recorder::RecordedAudio,
    config: &TextConfig,
) -> Option<crate::core::recorder::RecordedAudio> {
    match vad_decision(&audio.samples, config) {
        VadDecision::RawRemoteAsr => Some(audio),
        VadDecision::EmptyClip => None,
        VadDecision::Trim {
            leading_samples,
            trailing_samples,
        } => {
            let end = audio.samples.len().saturating_sub(trailing_samples);
            Some(crate::core::recorder::RecordedAudio::new(
                audio.samples[leading_samples..end].to_vec(),
                audio.sample_rate_hz,
                audio.channels,
            ))
        }
    }
}

pub fn vad_decision(samples: &[i16], config: &TextConfig) -> VadDecision {
    if !config.vad_enabled {
        return VadDecision::RawRemoteAsr;
    }
    let threshold = 256u16;
    let Some(first_voice) = samples
        .iter()
        .position(|sample| sample.unsigned_abs() > threshold)
    else {
        return VadDecision::EmptyClip;
    };
    if !config.silence_trim_enabled {
        return VadDecision::RawRemoteAsr;
    }
    let last_voice = samples
        .iter()
        .rposition(|sample| sample.unsigned_abs() > threshold)
        .unwrap_or(first_voice);
    VadDecision::Trim {
        leading_samples: first_voice,
        trailing_samples: samples.len().saturating_sub(last_voice + 1),
    }
}

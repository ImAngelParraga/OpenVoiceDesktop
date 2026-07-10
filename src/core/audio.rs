use anyhow::{Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::core::config::{AudioConfig, SoundTheme, TextConfig};
use crate::core::recorder::RecordedAudio;

pub const DEFAULT_SAMPLE_RATE_HZ: u32 = 16_000;
pub const DEFAULT_CHANNELS: u16 = 1;
pub const PCM_BITS_PER_SAMPLE: u16 = 16;

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
    })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingBufferPlan {
    pub pre_ms: u64,
    pub post_ms: u64,
}

pub fn recording_buffer_plan(
    debug_recording_buffer_ms: u64,
    pre_ms: u64,
    post_ms: u64,
) -> RecordingBufferPlan {
    if pre_ms > 0 || post_ms > 0 {
        RecordingBufferPlan { pre_ms, post_ms }
    } else {
        RecordingBufferPlan {
            pre_ms: debug_recording_buffer_ms,
            post_ms: debug_recording_buffer_ms,
        }
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

use anyhow::{Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::path::Path;
use std::time::{Duration, Instant};

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
    let mut writer = WavWriter::create(path, spec).with_context(|| format!("creating {}", path.display()))?;
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

pub fn downmix_resample_to_i16(input: &[f32], source_rate: u32, source_channels: u16) -> RecordedAudio {
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
        return RecordedAudio::new(float_to_i16(&mono), DEFAULT_SAMPLE_RATE_HZ, DEFAULT_CHANNELS);
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

    RecordedAudio::new(float_to_i16(&output), DEFAULT_SAMPLE_RATE_HZ, DEFAULT_CHANNELS)
}

fn float_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

//! Live levels from the microphone stream, independent of the saved recording.

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct MicrophoneLevels {
    peak: Arc<AtomicU8>,
    // One percentage per bar, oldest in the most significant byte.
    history: Arc<AtomicU32>,
}

impl MicrophoneLevels {
    pub(crate) fn with_peak(peak: Arc<AtomicU8>) -> Self {
        Self {
            peak,
            ..Self::default()
        }
    }

    pub(crate) fn peak_handle(&self) -> Arc<AtomicU8> {
        Arc::clone(&self.peak)
    }

    pub fn peak_bucket(&self) -> u8 {
        self.peak.load(Ordering::Relaxed).min(10)
    }

    pub fn waveform(&self) -> [u8; 4] {
        self.history.load(Ordering::Relaxed).to_be_bytes()
    }

    pub(crate) fn clear(&self) {
        self.peak.store(0, Ordering::Relaxed);
        self.history.store(0, Ordering::Relaxed);
    }
}

pub(super) struct CaptureMeter {
    window_samples: usize,
    count: usize,
    energy: f32,
}

impl CaptureMeter {
    pub fn new(rate: u32, channels: u16) -> Self {
        // Each bar represents 50 ms regardless of the device callback size.
        Self {
            window_samples: (rate / 20).max(1) as usize * usize::from(channels.max(1)),
            count: 0,
            energy: 0.0,
        }
    }

    pub fn reset(&mut self, levels: &MicrophoneLevels) {
        self.count = 0;
        self.energy = 0.0;
        levels.clear();
    }

    pub fn observe(&mut self, mut samples: &[f32], levels: &MicrophoneLevels) {
        let mut peak = 0.0_f32;
        while !samples.is_empty() {
            let take = (self.window_samples - self.count).min(samples.len());
            let (chunk, rest) = samples.split_at(take);
            for &sample in chunk {
                peak = peak.max(sample.abs());
                self.energy += sample * sample;
            }
            samples = rest;
            self.count += take;
            if self.count == self.window_samples {
                // RMS on a -60 to 0 dBFS scale keeps quiet voices visible.
                let mean_square = self.energy / self.count as f32;
                let db = 10.0 * mean_square.max(1e-6).log10();
                let percent = ((db + 60.0) * (100.0 / 60.0)).clamp(0.0, 100.0).round() as u32;
                let previous = levels.history.load(Ordering::Relaxed);
                levels
                    .history
                    .store((previous << 8) | percent, Ordering::Relaxed);
                self.count = 0;
                self.energy = 0.0;
            }
        }
        levels
            .peak
            .store(((peak * 10.0).round() as u8).min(10), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_tracks_recent_audio_at_device_rate_across_callback_boundaries() {
        for (rate, channels) in [(16000, 1), (44100, 2), (48000, 2)] {
            let mut meter = CaptureMeter::new(rate, channels);
            let levels = MicrophoneLevels::default();
            let input: Vec<_> = [0.0, 0.01, 0.1, 1.0]
                .into_iter()
                .flat_map(|amplitude| {
                    (0..meter.window_samples)
                        .map(move |i| if i % 2 == 0 { amplitude } else { -amplitude })
                })
                .collect();
            for chunk in input.chunks(127) {
                meter.observe(chunk, &levels);
            }
            assert_eq!(levels.waveform(), [0, 33, 67, 100]);
            meter.observe(&vec![0.; input.len()], &levels);
            assert_eq!(levels.waveform(), [0; 4]);
        }
    }

    #[test]
    fn reset_discards_partial_window_and_previous_bars() {
        let mut meter = CaptureMeter::new(16000, 1);
        let levels = MicrophoneLevels::default();
        meter.observe(&[1.; 1000], &levels);
        assert_eq!(levels.waveform(), [0, 0, 0, 100]);
        meter.reset(&levels);
        meter.observe(&[0.; 800], &levels);
        assert_eq!(levels.waveform(), [0; 4]);
        assert_eq!(levels.peak_bucket(), 0);
    }
}

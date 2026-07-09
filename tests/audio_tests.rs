use openvoice_desktop::core::audio::{
    write_wav, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE_HZ, PCM_BITS_PER_SAMPLE,
};
use openvoice_desktop::core::recorder::RecordedAudio;

#[test]
fn writes_valid_16khz_mono_pcm_wav() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let samples = vec![0_i16; DEFAULT_SAMPLE_RATE_HZ as usize / 10];
    let audio = RecordedAudio::new(samples, DEFAULT_SAMPLE_RATE_HZ, DEFAULT_CHANNELS);

    let stats = write_wav(file.path(), &audio).unwrap();
    assert!(stats.bytes > 44);

    let reader = hound::WavReader::open(file.path()).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, DEFAULT_SAMPLE_RATE_HZ);
    assert_eq!(spec.channels, DEFAULT_CHANNELS);
    assert_eq!(spec.bits_per_sample, PCM_BITS_PER_SAMPLE);
    assert_eq!(reader.duration(), DEFAULT_SAMPLE_RATE_HZ / 10);
}

#[test]
fn ten_seconds_is_about_320kb_plus_header() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let samples = vec![0_i16; DEFAULT_SAMPLE_RATE_HZ as usize * 10];
    let audio = RecordedAudio::new(samples, DEFAULT_SAMPLE_RATE_HZ, DEFAULT_CHANNELS);

    let stats = write_wav(file.path(), &audio).unwrap();
    assert!((319_000..=321_000).contains(&stats.bytes));
}

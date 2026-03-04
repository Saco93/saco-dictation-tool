pub const TARGET_SAMPLE_RATE: u32 = 16_000;
pub const TARGET_CHANNELS: u16 = 1;
pub const FRAME_DURATION_MS: u16 = 20;
pub const MAX_PAYLOAD_BYTES: usize = 1_500_000;

#[must_use]
pub fn frame_size_samples(sample_rate_hz: u32, frame_ms: u16, channels: u16) -> usize {
    ((sample_rate_hz as usize * frame_ms as usize) / 1_000) * channels as usize
}

#[must_use]
pub fn normalize_interleaved_f32_to_pcm16_mono_16khz(
    input: &[f32],
    input_channels: u16,
    input_sample_rate_hz: u32,
) -> Vec<i16> {
    if input.is_empty() || input_channels == 0 || input_sample_rate_hz == 0 {
        return Vec::new();
    }

    let mono = downmix_to_mono(input, input_channels as usize);
    resample_to_16khz(&mono, input_sample_rate_hz)
        .into_iter()
        .map(float_to_i16)
        .collect()
}

fn downmix_to_mono(input: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return input.to_vec();
    }

    input
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn resample_to_16khz(input: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == TARGET_SAMPLE_RATE {
        return input.to_vec();
    }

    let ratio = TARGET_SAMPLE_RATE as f32 / from_rate as f32;
    let out_len = ((input.len() as f32) * ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);

    for out_idx in 0..out_len {
        let src_pos = out_idx as f32 / ratio;
        let src_idx = src_pos.floor() as usize;
        let src_idx = src_idx.min(input.len().saturating_sub(1));
        out.push(input[src_idx]);
    }

    out
}

fn float_to_i16(v: f32) -> i16 {
    let clamped = v.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}

#[cfg(test)]
mod tests {
    use super::{
        TARGET_SAMPLE_RATE, frame_size_samples, normalize_interleaved_f32_to_pcm16_mono_16khz,
    };

    #[test]
    fn frame_size_matches_expected() {
        assert_eq!(frame_size_samples(16_000, 20, 1), 320);
        assert_eq!(frame_size_samples(48_000, 20, 2), 1_920);
    }

    #[test]
    fn normalization_downmixes_and_resamples() {
        let stereo_48k: Vec<f32> = (0..480).flat_map(|_| [0.0_f32, 1.0_f32]).collect();
        let normalized = normalize_interleaved_f32_to_pcm16_mono_16khz(&stereo_48k, 2, 48_000);
        assert!(normalized.len() > 100);
        assert!(normalized.len() < 200);
    }

    #[test]
    fn normalization_keeps_16khz_length_for_mono() {
        let mono_16k = vec![0.1_f32; TARGET_SAMPLE_RATE as usize];
        let normalized =
            normalize_interleaved_f32_to_pcm16_mono_16khz(&mono_16k, 1, TARGET_SAMPLE_RATE);
        assert_eq!(normalized.len(), TARGET_SAMPLE_RATE as usize);
    }
}

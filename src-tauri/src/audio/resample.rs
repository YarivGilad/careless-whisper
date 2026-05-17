use rubato::{FftFixedIn, Resampler};

const TARGET_RATE: u32 = 16_000;

/// Converts multi-channel interleaved samples to mono, then resamples to 16 kHz.
pub fn resample_to_16k(
    samples: Vec<f32>,
    source_rate: u32,
    channels: usize,
) -> Result<Vec<f32>, String> {
    if channels == 0 {
        return Err("Audio stream has zero channels".to_string());
    }

    let mono: Vec<f32> = if channels == 1 {
        samples
    } else {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if source_rate == TARGET_RATE {
        return Ok(mono);
    }

    let chunk_size = 1024;
    let mut resampler =
        FftFixedIn::<f32>::new(source_rate as usize, TARGET_RATE as usize, chunk_size, 2, 1)
            .map_err(|error| format!("Failed to create resampler: {error}"))?;

    let mut output = Vec::new();
    let mut pos = 0;

    while pos < mono.len() {
        let end = (pos + chunk_size).min(mono.len());
        let mut chunk = mono[pos..end].to_vec();
        chunk.resize(chunk_size, 0.0);

        let out = resampler
            .process(&[chunk], None)
            .map_err(|error| format!("Failed to resample audio: {error}"))?;
        output.extend_from_slice(&out[0]);
        pos += chunk_size;
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_zero_channels_errors() {
        let samples = vec![0.0f32; 100];
        let result = resample_to_16k(samples, 16000, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("zero channels"));
    }

    #[test]
    fn test_resample_mono_passthrough_at_16k() {
        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let result = resample_to_16k(samples.clone(), 16000, 1).unwrap();
        assert_eq!(result, samples);
    }

    #[test]
    fn test_resample_stereo_to_mono_averaging() {
        // Stereo interleaved: [L, R, L, R] = [0.2, 0.8, 0.2, 0.8]
        let samples = vec![0.2, 0.8, 0.2, 0.8];
        let result = resample_to_16k(samples, 16000, 2).unwrap();
        assert_eq!(result.len(), 2);
        assert!((result[0] - 0.5).abs() < 1e-6);
        assert!((result[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_resample_44100_to_16000() {
        let num_samples = 44100; // 1 second at 44.1kHz
        let samples: Vec<f32> = (0..num_samples).map(|i| (i as f32 * 0.001).sin()).collect();
        let result = resample_to_16k(samples, 44100, 1).unwrap();
        let expected_len = 16000; // ~1 second at 16kHz
                                  // Allow some tolerance due to resampler padding
        let ratio = result.len() as f64 / expected_len as f64;
        assert!(ratio > 0.9 && ratio < 1.2, "ratio was {}", ratio);
    }

    #[test]
    fn test_resample_48000_to_16000() {
        let num_samples = 48000; // 1 second at 48kHz
        let samples: Vec<f32> = (0..num_samples).map(|i| (i as f32 * 0.001).sin()).collect();
        let result = resample_to_16k(samples, 48000, 1).unwrap();
        let expected_len = 16000;
        let ratio = result.len() as f64 / expected_len as f64;
        assert!(ratio > 0.9 && ratio < 1.2, "ratio was {}", ratio);
    }

    #[test]
    fn test_resample_short_audio_under_chunk_size() {
        // Less than 1024 samples
        let samples: Vec<f32> = (0..500).map(|i| (i as f32 * 0.01).sin()).collect();
        let result = resample_to_16k(samples, 44100, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resample_empty_at_16k() {
        let result = resample_to_16k(vec![], 16000, 1).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_resample_empty_needs_resampling() {
        // Empty input at a rate that requires resampling should not panic
        let result = resample_to_16k(vec![], 44100, 1);
        assert!(result.is_ok());
    }
}

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
    fn test_resample_rejects_zero_channels() {
        let samples = vec![0.0f32; 1024];
        let result = resample_to_16k(samples, 44100, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Audio stream has zero channels");
    }

    #[test]
    fn test_resample_passes_through_at_target_rate() {
        let samples: Vec<f32> = (0..16000).map(|i| (i as f32) * 0.001).collect();
        let result = resample_to_16k(samples.clone(), TARGET_RATE, 1);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.len(), samples.len());
        for (i, (orig, resampled)) in samples.iter().zip(output.iter()).enumerate() {
            assert!(
                (orig - resampled).abs() < f32::EPSILON,
                "mismatch at index {}",
                i
            );
        }
    }

    #[test]
    fn test_resample_stereo_to_mono() {
        let samples: Vec<f32> = (0..2048).map(|i| i as f32).collect();
        let result = resample_to_16k(samples.clone(), TARGET_RATE, 2);
        assert!(result.is_ok());
        let mono = result.unwrap();
        assert_eq!(mono.len(), 1024);
        for (i, sample) in mono.iter().enumerate() {
            let expected = ((i * 2) as f32 + ((i * 2 + 1) as f32)) / 2.0;
            assert!(
                (sample - expected).abs() < f32::EPSILON,
                "mismatch at index {}",
                i
            );
        }
    }

    #[test]
    fn test_resample_four_channels_to_mono() {
        let samples: Vec<f32> = (0..4096).map(|i| i as f32).collect();
        let result = resample_to_16k(samples.clone(), TARGET_RATE, 4);
        assert!(result.is_ok());
        let mono = result.unwrap();
        assert_eq!(mono.len(), 1024);
    }

    #[test]
    fn test_resample_output_length_ratio_44100_to_16k() {
        let samples: Vec<f32> = (0..44100).map(|_| 1.0f32).collect();
        let result = resample_to_16k(samples, 44100, 1);
        assert!(result.is_ok());
        let output = result.unwrap();
        let expected_len = (44100.0 * (TARGET_RATE as f64) / 44100.0) as usize;
        let tolerance = (expected_len as f64 * 0.05) as usize;
        assert!(
            (output.len() as isize - expected_len as isize).unsigned_abs() <= tolerance,
            "output len {} should be near {} (tolerance: {})",
            output.len(),
            expected_len,
            tolerance
        );
    }

    #[test]
    fn test_resample_handles_empty_samples() {
        let samples: Vec<f32> = vec![];
        let result = resample_to_16k(samples, 44100, 1);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn test_resample_preserves_silence() {
        let samples: Vec<f32> = vec![0.0f32; 1024];
        let result = resample_to_16k(samples.clone(), TARGET_RATE, 1);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.iter().all(|s| s.abs() < f32::EPSILON));
    }
}

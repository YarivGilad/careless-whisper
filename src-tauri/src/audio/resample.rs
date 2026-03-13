use rubato::{FftFixedIn, Resampler};

const TARGET_RATE: u32 = 16_000;

/// Converts multi-channel interleaved samples to mono, then resamples to 16 kHz.
pub fn resample_to_16k(samples: Vec<f32>, source_rate: u32, channels: usize) -> Vec<f32> {
    // Mix down to mono
    let mono: Vec<f32> = if channels == 1 {
        samples
    } else {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if source_rate == TARGET_RATE {
        return mono;
    }

    let chunk_size = 1024;
    let mut resampler = FftFixedIn::<f32>::new(
        source_rate as usize,
        TARGET_RATE as usize,
        chunk_size,
        2,
        1,
    )
    .expect("Failed to create resampler");

    let mut output = Vec::new();
    let mut pos = 0;

    while pos < mono.len() {
        let end = (pos + chunk_size).min(mono.len());
        let mut chunk = mono[pos..end].to_vec();
        chunk.resize(chunk_size, 0.0);

        if let Ok(out) = resampler.process(&[chunk], None) {
            output.extend_from_slice(&out[0]);
        }
        pos += chunk_size;
    }

    output
}

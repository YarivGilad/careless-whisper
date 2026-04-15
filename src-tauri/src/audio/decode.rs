use std::{fs::File, path::Path};

use symphonia::core::{
    audio::{AudioBufferRef, SampleBuffer, Signal},
    codecs::DecoderOptions,
    errors::Error as SymphoniaError,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

pub fn decode_audio_file(path: &Path) -> Result<(Vec<f32>, u32, u16), String> {
    let file = File::open(path)
        .map_err(|error| format!("Failed to open audio file '{}': {}", path.display(), error))?;

    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }

    let probe = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| format!("Unsupported or unreadable audio file: {error}"))?;

    let mut format = probe.format;
    let track = format
        .tracks()
        .iter()
        .find(|candidate| candidate.codec_params.sample_rate.is_some())
        .ok_or_else(|| "No decodable audio track found in the selected file".to_string())?;

    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| "Missing sample rate in audio track".to_string())?;
    let channels = track
        .codec_params
        .channels
        .map(|layout| layout.count() as u16)
        .unwrap_or(1); // Default to mono if channel metadata is missing

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| format!("Failed to create audio decoder: {error}"))?;

    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err("Audio stream reset is not supported for this file".to_string())
            }
            Err(error) => return Err(format!("Failed to read audio data: {error}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            Err(SymphoniaError::DecodeError(error)) => {
                log::warn!("[audio-file] skipping undecodable packet: {error}");
                continue;
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err("Audio stream reset is not supported for this file".to_string())
            }
            Err(error) => return Err(format!("Audio decode failed: {error}")),
        };

        append_samples(&mut samples, decoded);
    }

    if samples.is_empty() {
        return Err("The selected file did not contain any decodable audio samples".to_string());
    }

    Ok((samples, sample_rate, channels))
}

fn append_samples(output: &mut Vec<f32>, decoded: AudioBufferRef<'_>) {
    match decoded {
        AudioBufferRef::F32(buffer) => output.extend_from_slice(buffer.chan(0)),
        buffer => {
            let spec = *buffer.spec();
            let duration = buffer.capacity() as u64;
            let mut sample_buffer = SampleBuffer::<f32>::new(duration, spec);
            sample_buffer.copy_interleaved_ref(buffer);
            output.extend_from_slice(sample_buffer.samples());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::resample::resample_to_16k;

    fn generate_wav(path: &std::path::Path, sample_rate: u32, channels: u16, num_samples: usize) {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..num_samples * channels as usize {
            let t = i as f32 / sample_rate as f32;
            let sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin();
            writer
                .write_sample((sample * i16::MAX as f32) as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn test_decode_wav_mono_16k() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mono_16k.wav");
        generate_wav(&path, 16000, 1, 16000);

        let (samples, sample_rate, channels) = decode_audio_file(&path).unwrap();
        assert_eq!(sample_rate, 16000);
        assert_eq!(channels, 1);
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_decode_wav_stereo_44100() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo_44100.wav");
        generate_wav(&path, 44100, 2, 44100);

        let (samples, sample_rate, channels) = decode_audio_file(&path).unwrap();
        assert_eq!(sample_rate, 44100);
        assert_eq!(channels, 2);
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_decode_nonexistent_file() {
        let result = decode_audio_file(Path::new("/tmp/nonexistent_audio_file_xyz.wav"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Failed to open") || err.contains("No such file"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_decode_not_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not_audio.wav");
        std::fs::write(&path, b"not audio").unwrap();

        let result = decode_audio_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_empty_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.wav");
        generate_wav(&path, 16000, 1, 0);

        let result = decode_audio_file(&path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("did not contain any decodable audio"),
            "expected empty audio error"
        );
    }

    #[test]
    fn test_decode_short_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.wav");
        // 100ms at 16kHz = 1600 samples
        generate_wav(&path, 16000, 1, 1600);

        let (samples, sample_rate, channels) = decode_audio_file(&path).unwrap();
        assert_eq!(sample_rate, 16000);
        assert_eq!(channels, 1);
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_pipeline_stereo_to_mono_16k() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo_44100_pipeline.wav");
        let num_samples = 44100; // 1 second
        generate_wav(&path, 44100, 2, num_samples);

        let (samples, sample_rate, channels) = decode_audio_file(&path).unwrap();
        assert_eq!(sample_rate, 44100);
        assert_eq!(channels, 2);

        let resampled = resample_to_16k(samples, sample_rate, channels as usize).unwrap();
        // Should be approximately 1 second at 16kHz
        let ratio = resampled.len() as f64 / 16000.0;
        assert!(
            ratio > 0.8 && ratio < 1.3,
            "expected ~16000 samples, got {}",
            resampled.len()
        );
    }
}

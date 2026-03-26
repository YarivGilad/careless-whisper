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

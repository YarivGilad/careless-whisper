use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct RecordingHandle {
    _stream: cpal::Stream,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    pub channels: u16,
}

// cpal::Stream is not Send by default on macOS; we only use it from a single
// thread so the impl is safe here.
unsafe impl Send for RecordingHandle {}

pub fn start_capture(max_seconds: u32) -> Result<RecordingHandle, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No input device available")?;

    let config = device.default_input_config().map_err(|e| e.to_string())?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let max_samples = (sample_rate as usize) * (max_seconds as usize) * (channels as usize);

    let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let samples_clone = samples.clone();

    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                let mut buf = samples_clone.lock().unwrap();
                if buf.len() < max_samples {
                    buf.extend_from_slice(data);
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )
        .map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;

    Ok(RecordingHandle {
        _stream: stream,
        samples,
        sample_rate,
        channels,
    })
}

pub fn stop_capture(handle: RecordingHandle) -> (Vec<f32>, u32, u16) {
    let sample_rate = handle.sample_rate;
    let channels = handle.channels;
    let samples = handle.samples.lock().unwrap().clone();
    // Dropping handle stops the stream.
    (samples, sample_rate, channels)
}

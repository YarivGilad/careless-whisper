/// System volume control — lowers output volume during recording and restores it after.

#[cfg(target_os = "macos")]
mod platform {
    use std::os::raw::c_void;

    const AUDIO_OBJECT_SYSTEM: u32 = 1;
    const SCOPE_GLOBAL: u32 = 1735159650; // 'glob'
    const SCOPE_OUTPUT: u32 = 1869968496; // 'outp'
    const ELEMENT_MAIN: u32 = 0;
    const PROP_DEFAULT_OUTPUT: u32 = 1682929012; // 'dOut'
    const PROP_VOLUME_SCALAR: u32 = 1987013741; // 'volm'

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyData(
            id: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_data_size: u32,
            qualifier_data: *const c_void,
            data_size: *mut u32,
            data: *mut c_void,
        ) -> i32;

        fn AudioObjectSetPropertyData(
            id: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_data_size: u32,
            qualifier_data: *const c_void,
            data_size: u32,
            data: *const c_void,
        ) -> i32;
    }

    fn default_output_device() -> Result<u32, String> {
        let address = AudioObjectPropertyAddress {
            selector: PROP_DEFAULT_OUTPUT,
            scope: SCOPE_GLOBAL,
            element: ELEMENT_MAIN,
        };
        let mut device_id: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                AUDIO_OBJECT_SYSTEM,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                &mut device_id as *mut u32 as *mut c_void,
            )
        };
        if status != 0 {
            return Err(format!("Failed to get default output device: {}", status));
        }
        Ok(device_id)
    }

    pub fn get_system_volume() -> Result<f32, String> {
        let device = default_output_device()?;
        let address = AudioObjectPropertyAddress {
            selector: PROP_VOLUME_SCALAR,
            scope: SCOPE_OUTPUT,
            element: ELEMENT_MAIN,
        };
        let mut volume: f32 = 0.0;
        let mut size = std::mem::size_of::<f32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                &mut volume as *mut f32 as *mut c_void,
            )
        };
        if status != 0 {
            // Some devices only expose per-channel volume; try channel 1
            let address_ch1 = AudioObjectPropertyAddress {
                selector: PROP_VOLUME_SCALAR,
                scope: SCOPE_OUTPUT,
                element: 1,
            };
            let status = unsafe {
                AudioObjectGetPropertyData(
                    device,
                    &address_ch1,
                    0,
                    std::ptr::null(),
                    &mut size,
                    &mut volume as *mut f32 as *mut c_void,
                )
            };
            if status != 0 {
                return Err(format!("Failed to get volume: {}", status));
            }
        }
        Ok(volume)
    }

    pub fn set_system_volume(level: f32) -> Result<(), String> {
        let device = default_output_device()?;
        let address = AudioObjectPropertyAddress {
            selector: PROP_VOLUME_SCALAR,
            scope: SCOPE_OUTPUT,
            element: ELEMENT_MAIN,
        };
        let status = unsafe {
            AudioObjectSetPropertyData(
                device,
                &address,
                0,
                std::ptr::null(),
                std::mem::size_of::<f32>() as u32,
                &level as *const f32 as *const c_void,
            )
        };
        if status != 0 {
            // Fall back to setting channels 1 and 2 individually
            for ch in 1..=2 {
                let address_ch = AudioObjectPropertyAddress {
                    selector: PROP_VOLUME_SCALAR,
                    scope: SCOPE_OUTPUT,
                    element: ch,
                };
                let s = unsafe {
                    AudioObjectSetPropertyData(
                        device,
                        &address_ch,
                        0,
                        std::ptr::null(),
                        std::mem::size_of::<f32>() as u32,
                        &level as *const f32 as *const c_void,
                    )
                };
                if s != 0 {
                    return Err(format!("Failed to set volume on channel {}: {}", ch, s));
                }
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    pub fn get_system_volume() -> Result<f32, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create device enumerator: {}", e))?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .map_err(|e| format!("Failed to get default audio endpoint: {}", e))?;
            let volume: IAudioEndpointVolume = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| format!("Failed to activate audio endpoint: {}", e))?;
            volume
                .GetMasterVolumeLevelScalar()
                .map_err(|e| format!("Failed to get volume: {}", e))
        }
    }

    pub fn set_system_volume(level: f32) -> Result<(), String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create device enumerator: {}", e))?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .map_err(|e| format!("Failed to get default audio endpoint: {}", e))?;
            let volume: IAudioEndpointVolume = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| format!("Failed to activate audio endpoint: {}", e))?;
            volume
                .SetMasterVolumeLevelScalar(level, std::ptr::null())
                .map_err(|e| format!("Failed to set volume: {}", e))
        }
    }
}

// Linux: no system volume API — feature is a no-op
#[cfg(target_os = "linux")]
mod platform {
    pub fn get_system_volume() -> Result<f32, String> {
        Err("Volume control not supported on Linux yet".into())
    }

    pub fn set_system_volume(_level: f32) -> Result<(), String> {
        Err("Volume control not supported on Linux yet".into())
    }
}

pub use platform::{get_system_volume, set_system_volume};

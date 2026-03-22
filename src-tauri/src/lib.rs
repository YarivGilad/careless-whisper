pub mod audio;
pub mod commands;
pub mod config;
pub mod hotkey;
pub mod models;
pub mod output;
pub mod transcribe;
pub mod tray;

use commands::*;
use config::settings::Settings;
use output::paste::FocusTarget;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub whisper_ctx: Mutex<Option<whisper_rs::WhisperContext>>,
    pub recording: Mutex<Option<audio::capture::RecordingHandle>>,
    pub target_focus: Mutex<Option<FocusTarget>>,
    pub original_volume: Mutex<Option<f32>>,
}

/// macOS: Checks if the app has Accessibility permission.
/// Only opens the System Settings prompt if the permission is NOT already granted.
/// This avoids the repeated prompt that macOS shows when calling
/// AXIsProcessTrustedWithOptions with kAXTrustedCheckOptionPrompt=true on every launch.
#[cfg(target_os = "macos")]
fn request_accessibility_if_needed() {
    use std::os::raw::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(cf: *mut c_void);
        static kCFBooleanTrue: *const c_void;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kAXTrustedCheckOptionPrompt: *const c_void;
    }

    unsafe {
        // First: check WITHOUT prompting
        let already_trusted = AXIsProcessTrusted() != 0;
        log::info!(
            "[permissions] Accessibility: already_trusted = {}",
            already_trusted
        );

        if already_trusted {
            // Permission is cached and valid — no need to prompt
            return;
        }

        // Not trusted — show the prompt once so the user can grant it
        log::info!("[permissions] Accessibility not granted, showing system prompt");
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const c_void,
        );
        let _trusted = AXIsProcessTrustedWithOptions(options);
        CFRelease(options as *mut c_void);
    }
}

/// macOS: Check microphone authorization status via swift subprocess.
/// The AVFoundation Objective-C FFI from Rust has symbol resolution issues with
/// AVMediaTypeAudio, so we shell out to swift which handles it natively.
/// Returns: 0 = not determined, 1 = denied, 2 = restricted, 3 = authorized
#[cfg(target_os = "macos")]
pub fn check_microphone_permission() -> i32 {
    let output = std::process::Command::new("swift")
        .args([
            "-e",
            "import AVFoundation; print(AVCaptureDevice.authorizationStatus(for: .audio).rawValue)",
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let status = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<i32>()
                .unwrap_or(-1);
            let label = match status {
                0 => "not_determined",
                1 => "denied",
                2 => "restricted",
                3 => "authorized",
                _ => "unknown",
            };
            log::info!("[permissions] Microphone: status = {} ({})", status, label);
            status
        }
        Ok(out) => {
            log::warn!(
                "[permissions] swift mic check failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            -1
        }
        Err(e) => {
            log::warn!("[permissions] failed to run swift: {}", e);
            -1
        }
    }
}

/// macOS: Request microphone permission via swift subprocess.
/// Triggers the system permission dialog if status is "not determined".
/// Blocks for up to 30 seconds waiting for user response.
#[cfg(target_os = "macos")]
fn request_microphone_permission() {
    log::info!("[permissions] Requesting microphone access via AVCaptureDevice");
    std::thread::spawn(|| {
        let output = std::process::Command::new("swift")
            .args([
                "-e",
                concat!(
                    "import AVFoundation; import Foundation; ",
                    "let sem = DispatchSemaphore(value: 0); ",
                    "AVCaptureDevice.requestAccess(for: .audio) { granted in ",
                    "  print(granted ? 3 : 1); sem.signal() ",
                    "}; ",
                    "_ = sem.wait(timeout: .now() + 30)"
                ),
            ])
            .output();

        match output {
            Ok(out) => {
                let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
                log::info!("[permissions] Microphone request result: {}", result);
            }
            Err(e) => {
                log::warn!("[permissions] Failed to request mic permission: {}", e);
            }
        }
    });
}

/// Linux: Writes a PID file and creates a named pipe (FIFO) that listens for
/// toggle commands. When anything is written to the pipe, recording is toggled.
/// This is used as a fallback on Wayland where X11 global key grabs don't work.
/// A GNOME/KDE custom keybinding can run:
///   echo toggle > ~/.local/share/careless-whisper/careless-whisper.sock
#[cfg(target_os = "linux")]
fn setup_fifo_listener(app_handle: tauri::AppHandle) {
    use std::io::Write;

    let data_dir = dirs::data_dir()
        .unwrap_or_default()
        .join("careless-whisper");

    // Write PID file (still useful for kill / status checks)
    let pid_path = data_dir.join("careless-whisper.pid");
    if let Ok(mut f) = std::fs::File::create(&pid_path) {
        let _ = writeln!(f, "{}", std::process::id());
    }

    // Create a named pipe (FIFO) for receiving toggle commands
    let fifo_path = data_dir.join("careless-whisper.sock");

    // Remove stale FIFO
    let _ = std::fs::remove_file(&fifo_path);

    // Create the FIFO
    let fifo_c = std::ffi::CString::new(fifo_path.to_str().unwrap()).unwrap();
    let ret = unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o644) };
    if ret != 0 {
        log::error!(
            "Failed to create FIFO at {}: {}",
            fifo_path.display(),
            std::io::Error::last_os_error()
        );
        return;
    }

    log::info!("FIFO listener at {}", fifo_path.display());

    // Spawn a thread that blocks on reading from the FIFO.
    // Each time something is written (and the writer closes), we toggle.
    std::thread::spawn(move || {
        use std::io::Read;

        loop {
            // Opening a FIFO for reading blocks until a writer opens it.
            let mut file = match std::fs::File::open(&fifo_path) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Failed to open FIFO: {}", e);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            };

            // Read whatever was written (we don't care about content)
            let mut buf = [0u8; 128];
            let _ = file.read(&mut buf);

            log::info!("FIFO toggle received");
            let state = app_handle.state::<AppState>();
            let is_recording = state.recording.lock().unwrap().is_some();

            if is_recording {
                let _ = app_handle.emit("hotkey-stop", ());
            } else {
                let target = crate::output::paste::get_frontmost_target();
                log::info!("FIFO captured target_focus = {:?}", target);
                *state.target_focus.lock().unwrap() = target;
                let _ = app_handle.emit("hotkey-start", ());
            }
        }
    });
}

fn log_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join("careless-whisper")
        .join("careless-whisper.log")
}

fn init_logging() {
    use simplelog::*;
    use std::fs;

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Truncate if over 1 MB
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 1_000_000 {
            let _ = fs::write(&path, b"");
        }
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![
        TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Stderr, ColorChoice::Auto),
    ];

    if let Ok(f) = file {
        loggers.push(WriteLogger::new(LevelFilter::Info, Config::default(), f));
    }

    let _ = CombinedLogger::init(loggers);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    let settings = Settings::load();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState {
            settings: Mutex::new(settings),
            whisper_ctx: Mutex::new(None),
            recording: Mutex::new(None),
            target_focus: Mutex::new(None),
            original_volume: Mutex::new(None),
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                // Log bundle ID for debugging permission identity
                if let Ok(output) = std::process::Command::new("defaults")
                    .args(["read", "/proc/curproc/../Info", "CFBundleIdentifier"])
                    .output()
                {
                    log::info!(
                        "[permissions] Bundle ID from defaults: {}",
                        String::from_utf8_lossy(&output.stdout).trim()
                    );
                }
                log::info!(
                    "[permissions] PID = {}, executable = {:?}",
                    std::process::id(),
                    std::env::current_exe().ok()
                );

                // Check + prompt for accessibility only if not already granted
                request_accessibility_if_needed();

                // Check microphone permission; request if not yet determined
                let mic_status = check_microphone_permission();
                if mic_status == 0 {
                    // Not determined — trigger the system prompt
                    request_microphone_permission();
                }
            }

            tray::setup_tray(&app.handle())?;

            // Register global hotkey — if it fails (e.g. already registered by
            // another app), log the error and continue so the app still starts.
            // The user can change the hotkey from the Settings window.
            if let Err(e) = hotkey::manager::register_hotkey(app) {
                log::error!("Failed to register hotkey: {}. Change it in Settings.", e);
                let _ = app.handle().emit("backend-error", serde_json::json!({
                    "message": "Could not register hotkey. You can change it in Settings."
                }));
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            // Linux: set up FIFO listener as a fallback for Wayland where
            // X11 global key grabs don't work. A desktop custom keybinding
            // can write to the FIFO to toggle recording.
            #[cfg(target_os = "linux")]
            setup_fifo_listener(app.handle().clone());

            // First launch: show settings if no model downloaded yet
            let models_dir = dirs::data_dir()
                .unwrap_or_default()
                .join("careless-whisper")
                .join("models");
            let has_model = std::fs::read_dir(&models_dir)
                .ok()
                .and_then(|mut d| d.next())
                .is_some();
            if !has_model {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_settings,
            update_settings,
            list_models,
            download_model,
            delete_model,
            set_active_model,
            check_accessibility,
            request_accessibility,
            check_microphone,
            request_microphone,
            get_launch_at_login,
            set_launch_at_login,
            get_recent_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// System tray setup
// App lives in the macOS menu bar — no dock icon (LSUIElement = true)

use tauri::{
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime, WindowEvent,
};

// Whisper language codes shown in the tray submenu. Keep in sync with the
// Language dropdown in src/components/Settings.tsx.
const LANGUAGES: &[(&str, &str)] = &[
    ("auto", "Auto-detect"),
    ("en", "English"),
    ("he", "Hebrew"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("ja", "Japanese"),
    ("zh", "Chinese"),
    ("pt", "Portuguese"),
    ("ru", "Russian"),
    ("ko", "Korean"),
    ("ar", "Arabic"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("hi", "Hindi"),
    ("tr", "Turkish"),
    ("pl", "Polish"),
    ("uk", "Ukrainian"),
];

fn build_tray_menu<R: Runtime>(app: &AppHandle<R>, current_lang: &str) -> tauri::Result<Menu<R>> {
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;

    let lang_items: Vec<CheckMenuItem<R>> = LANGUAGES
        .iter()
        .map(|(code, label)| {
            CheckMenuItem::with_id(
                app,
                format!("lang_{}", code),
                *label,
                true,
                *code == current_lang,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;

    let lang_refs: Vec<&dyn IsMenuItem<R>> = lang_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<R>)
        .collect();
    let language_submenu = Submenu::with_id_and_items(app, "language", "Language", true, &lang_refs)?;

    Menu::with_items(app, &[&settings, &language_submenu, &sep, &quit])
}

fn change_language<R: Runtime>(app: &AppHandle<R>, code: &str) {
    let state = app.state::<crate::AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        if s.language == code {
            return;
        }
        s.language = code.to_string();
        if let Err(e) = s.save() {
            log::error!("[tray] failed to persist language change: {}", e);
            return;
        }
    }
    log::info!("[tray] language changed to {}", code);

    match build_tray_menu(app, code) {
        Ok(new_menu) => {
            if let Some(tray) = app.tray_by_id("main") {
                if let Err(e) = tray.set_menu(Some(new_menu)) {
                    log::warn!("[tray] failed to refresh menu after language change: {}", e);
                }
            }
        }
        Err(e) => log::warn!("[tray] failed to rebuild menu: {}", e),
    }

    let _ = app.emit(
        "settings-updated",
        serde_json::json!({ "language": code }),
    );
}

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let current_lang = {
        let state = app.state::<crate::AppState>();
        let s = state.settings.lock().unwrap();
        s.language.clone()
    };
    let menu = build_tray_menu(app, &current_lang)?;

    let tray_icon = match tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
    {
        Ok(icon) => icon,
        Err(error) => {
            log::warn!("[tray] failed to load bundled tray icon: {error}");
            app.default_window_icon()
                .cloned()
                .ok_or_else(|| tauri::Error::AssetNotFound("No tray icon available".into()))?
        }
    };

    if let Some(window) = app.get_webview_window("settings") {
        // Linux/Wayland (KWin) doesn't deliver native titlebar button clicks to
        // the WebKitGTK surface, so the buttons are dead. Drop the native
        // decorations and use the custom in-webview titlebar (see TitleBar.tsx).
        // macOS/Windows keep their working native decorations.
        #[cfg(target_os = "linux")]
        let _ = window.set_decorations(false);

        let win = window.clone();
        window.on_window_event(move |event| {
            // Closing the window only hides it — the app lives in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = win.hide();
            }
        });
    }

    TrayIconBuilder::with_id("main")
        .icon(tray_icon)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if let Some(code) = id.strip_prefix("lang_") {
                change_language(app, code);
                return;
            }
            match id {
                "settings" => {
                    if let Some(window) = app.get_webview_window("settings") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    // On Linux, Tauri's graceful shutdown drops the wry/tao
                    // Context (built on non-thread-safe `Rc`). If an in-flight
                    // async IPC task still holds an AppHandle/Webview clone,
                    // its drop can land on a tokio worker thread and race the
                    // main thread, aborting with "tcache double free". Exit the
                    // process directly to skip Rust drop glue. Settings are
                    // persisted synchronously on every change, so nothing needs
                    // flushing at exit.
                    #[cfg(target_os = "linux")]
                    std::process::exit(0);
                    #[cfg(not(target_os = "linux"))]
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

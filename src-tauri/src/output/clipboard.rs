use arboard::Clipboard;

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    // Retry up to 3 times — on Windows the clipboard can be transiently locked
    // by other applications (browsers, password managers, clipboard managers).
    let mut last_err = String::new();
    for attempt in 0..3 {
        match try_copy(text) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempt < 2 {
                    log::warn!(
                        "[clipboard] attempt {} failed: {}, retrying...",
                        attempt + 1,
                        last_err
                    );
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }
    Err(format!("Clipboard failed after 3 attempts: {}", last_err))
}

fn try_copy(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub fn read_clipboard() -> Option<String> {
    Clipboard::new().ok().and_then(|mut cb| cb.get_text().ok())
}

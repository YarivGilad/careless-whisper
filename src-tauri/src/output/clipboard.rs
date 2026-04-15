use arboard::Clipboard;

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub fn read_clipboard() -> Option<String> {
    Clipboard::new().ok().and_then(|mut cb| cb.get_text().ok())
}

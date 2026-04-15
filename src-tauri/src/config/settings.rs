use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    PushToTalk,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    TopCenter,
    BottomCenter,
    TopLeft,
    TopRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub hotkey: String,
    pub recording_mode: RecordingMode,
    pub active_model: String,
    pub language: String,
    pub auto_paste: bool,
    pub max_recording_seconds: u32,
    pub launch_at_login: bool,
    pub overlay_position: OverlayPosition,
    #[serde(default = "default_true")]
    pub lower_volume_while_recording: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CmdOrCtrl+Shift+Space".to_string(),
            recording_mode: RecordingMode::Toggle,
            active_model: "base".to_string(),
            language: "auto".to_string(),
            auto_paste: true,
            max_recording_seconds: 120,
            launch_at_login: false,
            overlay_position: OverlayPosition::TopCenter,
            lower_volume_while_recording: true,
        }
    }
}

fn config_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("careless-whisper")
        .join("config.json")
}

impl Settings {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default_values() {
        let s = Settings::default();
        assert_eq!(s.hotkey, "CmdOrCtrl+Shift+Space");
        assert_eq!(s.recording_mode, RecordingMode::Toggle);
        assert_eq!(s.active_model, "base");
        assert_eq!(s.language, "auto");
        assert!(s.auto_paste);
        assert_eq!(s.max_recording_seconds, 120);
        assert!(!s.launch_at_login);
        assert_eq!(s.overlay_position, OverlayPosition::TopCenter);
        assert!(s.lower_volume_while_recording);
    }

    #[test]
    fn test_settings_roundtrip_serialization() {
        let original = Settings::default();
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_settings_missing_field_uses_default() {
        let json = r#"{
            "hotkey": "CmdOrCtrl+Shift+Space",
            "recording_mode": "toggle",
            "active_model": "base",
            "language": "auto",
            "auto_paste": true,
            "max_recording_seconds": 120,
            "launch_at_login": false,
            "overlay_position": "top_center"
        }"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert!(settings.lower_volume_while_recording);
    }

    #[test]
    fn test_settings_unknown_fields_ignored() {
        let json = r#"{
            "hotkey": "CmdOrCtrl+Shift+Space",
            "recording_mode": "toggle",
            "active_model": "base",
            "language": "auto",
            "auto_paste": true,
            "max_recording_seconds": 120,
            "launch_at_login": false,
            "overlay_position": "top_center",
            "lower_volume_while_recording": true,
            "unknown_future_field": 42,
            "another_unknown": "hello"
        }"#;
        let result: Result<Settings, _> = serde_json::from_str(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_settings_corrupt_json() {
        let json = "{ this is not valid json }}}";
        let result: Result<Settings, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_recording_mode_serde() {
        let push: RecordingMode = serde_json::from_str(r#""push_to_talk""#).unwrap();
        assert_eq!(push, RecordingMode::PushToTalk);

        let toggle: RecordingMode = serde_json::from_str(r#""toggle""#).unwrap();
        assert_eq!(toggle, RecordingMode::Toggle);
    }

    #[test]
    fn test_overlay_position_serde() {
        let variants = [
            ("\"top_center\"", OverlayPosition::TopCenter),
            ("\"bottom_center\"", OverlayPosition::BottomCenter),
            ("\"top_left\"", OverlayPosition::TopLeft),
            ("\"top_right\"", OverlayPosition::TopRight),
        ];
        for (json_str, expected) in &variants {
            let deserialized: OverlayPosition = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, *expected);

            let serialized = serde_json::to_string(expected).unwrap();
            let round_tripped: OverlayPosition = serde_json::from_str(&serialized).unwrap();
            assert_eq!(round_tripped, *expected);
        }
    }

    #[test]
    fn test_settings_serialization_push_to_talk() {
        let settings = Settings {
            recording_mode: RecordingMode::PushToTalk,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).expect("should serialize");
        assert!(json.contains("push_to_talk"));
        let deserialized: Settings = serde_json::from_str(&json).expect("should deserialize");
        assert!(matches!(
            deserialized.recording_mode,
            RecordingMode::PushToTalk
        ));
    }

    #[test]
    fn test_settings_custom_values() {
        let settings = Settings {
            hotkey: "Ctrl+Alt+T".to_string(),
            recording_mode: RecordingMode::PushToTalk,
            active_model: "large-v3".to_string(),
            language: "en".to_string(),
            auto_paste: false,
            max_recording_seconds: 60,
            launch_at_login: true,
            overlay_position: OverlayPosition::BottomCenter,
            lower_volume_while_recording: false,
        };
        assert_eq!(settings.hotkey, "Ctrl+Alt+T");
        assert!(matches!(settings.recording_mode, RecordingMode::PushToTalk));
        assert_eq!(settings.active_model, "large-v3");
        assert_eq!(settings.language, "en");
        assert!(!settings.auto_paste);
        assert_eq!(settings.max_recording_seconds, 60);
        assert!(settings.launch_at_login);
        assert!(matches!(
            settings.overlay_position,
            OverlayPosition::BottomCenter
        ));
        assert!(!settings.lower_volume_while_recording);
    }
}

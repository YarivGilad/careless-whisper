use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub disk_size_mb: u32,
    pub ram_mb: u32,
    pub is_downloaded: bool,
}

/// (name, disk_mb, ram_mb, sha256)
/// Hashes from https://huggingface.co/ggerganov/whisper.cpp — LFS pointer metadata
const MODELS: &[(&str, u32, u32, &str)] = &[
    (
        "tiny",
        75,
        390,
        "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
    ),
    (
        "base",
        142,
        500,
        "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    ),
    (
        "small",
        466,
        1024,
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    ),
    (
        "medium",
        1500,
        2600,
        "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
    ),
    (
        "large-v3",
        3000,
        5120,
        "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
    ),
];

pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join("careless-whisper")
        .join("models")
}

pub fn model_path(name: &str) -> PathBuf {
    models_dir().join(format!("ggml-{}.bin", name))
}

pub fn list_models() -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|(name, disk_mb, ram_mb, _sha256)| ModelInfo {
            name: name.to_string(),
            disk_size_mb: *disk_mb,
            ram_mb: *ram_mb,
            is_downloaded: model_path(name).exists(),
        })
        .collect()
}

pub(crate) fn expected_sha256(name: &str) -> Option<&'static str> {
    MODELS
        .iter()
        .find(|(n, _, _, _)| *n == name)
        .map(|(_, _, _, hash)| *hash)
}

pub async fn download_model(app: AppHandle, name: String) -> Result<(), String> {
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        name
    );

    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let part_path = dir.join(format!("ggml-{}.bin.part", name));
    let final_path = model_path(&name);

    let response = reqwest::get(&url).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            (downloaded * 100 / total) as u32
        } else {
            0
        };
        let _ = app.emit(
            "download-progress",
            serde_json::json!({ "model": name, "percent": percent }),
        );
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    // Verify SHA256 before making the file available
    if let Some(expected) = expected_sha256(&name) {
        let computed = sha256_file(&part_path).map_err(|e| e.to_string())?;
        if computed != expected {
            let _ = std::fs::remove_file(&part_path);
            return Err(format!(
                "Model '{}' verification failed — the file may have been updated upstream. \
                 Please open a GitHub issue at https://github.com/YarivGilad/careless-whisper/issues \
                 so we can update the checksum. The incomplete file has been deleted.",
                name
            ));
        }
        log::info!("[download] SHA256 verified for {}", name);
    }

    std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;

    Ok(())
}

/// Pre-load validation: checks the model file exists, is non-empty, and matches its expected SHA256.
pub fn validate_model_file(name: &str) -> Result<(), String> {
    let path = model_path(name);

    if !path.exists() {
        return Err(format!("Model '{}' is not downloaded", name));
    }

    let metadata = std::fs::metadata(&path)
        .map_err(|e| format!("Cannot read model '{}' at {}: {}", name, path.display(), e))?;

    if metadata.len() == 0 {
        return Err(format!(
            "Model '{}' file is empty (0 bytes) at {}. Try deleting and re-downloading it.",
            name,
            path.display()
        ));
    }

    if let Some(expected) = expected_sha256(name) {
        let computed = sha256_file(&path)
            .map_err(|e| format!("Failed to read model '{}' for verification: {}", name, e))?;
        if computed != expected {
            return Err(format!(
                "Model '{}' is corrupted (SHA256 mismatch). File size: {} bytes at {}. \
                 Please delete and re-download it from Settings → Model Manager.",
                name,
                metadata.len(),
                path.display()
            ));
        }
    }

    Ok(())
}

pub fn delete_model(name: &str) -> Result<(), String> {
    let path = model_path(name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_path_construction() {
        assert_eq!(model_path("base"), models_dir().join("ggml-base.bin"));
        assert_eq!(
            model_path("large-v3"),
            models_dir().join("ggml-large-v3.bin")
        );
        assert_eq!(model_path("tiny"), models_dir().join("ggml-tiny.bin"));
    }

    #[test]
    fn test_models_dir_uses_careless_whisper() {
        let dir = models_dir();
        assert!(dir.to_string_lossy().contains("careless-whisper"));
        assert_eq!(dir.file_name().unwrap(), "models");
    }

    #[test]
    fn test_validate_model_name_valid() {
        let valid = ["tiny", "base", "small", "medium", "large-v3"];
        for name in &valid {
            assert!(
                MODELS.iter().any(|(n, _, _, _)| n == name),
                "{} should be in MODELS",
                name
            );
        }
    }

    #[test]
    fn test_validate_model_name_rejects_traversal() {
        assert!(
            !MODELS.iter().any(|(n, _, _, _)| *n == "../evil"),
            "../evil should not be in MODELS"
        );
    }

    #[test]
    fn test_validate_model_name_rejects_empty() {
        assert!(
            !MODELS.iter().any(|(n, _, _, _)| n.is_empty()),
            "empty string should not be in MODELS"
        );
    }

    #[test]
    fn test_validate_model_name_rejects_arbitrary() {
        assert!(!MODELS.iter().any(|(n, _, _, _)| *n == "large-v2"));
        assert!(!MODELS.iter().any(|(n, _, _, _)| *n == "TINY"));
    }

    #[test]
    fn test_list_models_count() {
        let models = list_models();
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_list_models_returns_all_models() {
        let models = list_models();
        assert_eq!(models.len(), 5);
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"tiny"));
        assert!(names.contains(&"base"));
        assert!(names.contains(&"small"));
        assert!(names.contains(&"medium"));
        assert!(names.contains(&"large-v3"));
    }

    #[test]
    fn test_list_models_has_correct_sizes() {
        let models = list_models();
        let tiny = models.iter().find(|m| m.name == "tiny").unwrap();
        assert_eq!(tiny.disk_size_mb, 75);
        assert_eq!(tiny.ram_mb, 390);

        let large = models.iter().find(|m| m.name == "large-v3").unwrap();
        assert_eq!(large.disk_size_mb, 3000);
        assert_eq!(large.ram_mb, 5120);
    }

    #[test]
    fn test_expected_sha256_for_all_models() {
        let model_names = ["tiny", "base", "small", "medium", "large-v3"];
        for name in model_names {
            let hash = expected_sha256(name);
            assert!(hash.is_some(), "should have SHA256 for {}", name);
            let hash = hash.unwrap();
            assert_eq!(hash.len(), 64, "SHA256 should be 64 hex chars");
            assert!(
                hash.chars().all(|c| c.is_ascii_hexdigit()),
                "should be valid hex"
            );
        }
    }

    #[test]
    fn test_expected_sha256_known() {
        let hash = expected_sha256("base");
        assert!(hash.is_some());
        assert_eq!(
            hash.unwrap(),
            "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"
        );
    }

    #[test]
    fn test_expected_sha256_unknown_model() {
        assert!(expected_sha256("unknown").is_none());
        assert!(expected_sha256("").is_none());
        assert!(expected_sha256("tiny-en").is_none());
    }

    #[test]
    fn test_validate_model_file_nonexistent() {
        let result = validate_model_file("nonexistent-model");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not downloaded"));
    }

    #[test]
    fn test_model_download_url_format() {
        let url = format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
            "base"
        );
        assert_eq!(
            url,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
        );
    }

    #[test]
    fn test_sha256_known_value() {
        use sha2::{Digest, Sha256};
        let data = b"hello";
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = format!("{:x}", hasher.finalize());
        assert_eq!(
            result,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha256_file_correct() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.bin");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();
        drop(f);

        let hash = sha256_file(&file_path).unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}

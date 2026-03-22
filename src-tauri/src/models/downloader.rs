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
    ("tiny",     75,   390,  "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21"),
    ("base",     142,  500,  "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"),
    ("small",    466,  1024, "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"),
    ("medium",   1500, 2600, "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208"),
    ("large-v3", 3000, 5120, "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2"),
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

fn expected_sha256(name: &str) -> Option<&'static str> {
    MODELS.iter()
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
                "Model '{}' integrity check failed: hash mismatch. \
                 Expected {}, got {}. The file has been deleted.",
                name, expected, computed
            ));
        }
        log::info!("[download] SHA256 verified for {}", name);
    }

    std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn delete_model(name: &str) -> Result<(), String> {
    let path = model_path(name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
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

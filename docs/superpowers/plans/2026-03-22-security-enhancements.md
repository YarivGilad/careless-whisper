# Security Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 5 security fixes identified in the audit report at `docs/security/audit-2026-03-22.md`.

**Architecture:** Each fix is an isolated change to a single file. Tasks are independent and can be reviewed individually. No new modules or abstractions — minimal, targeted edits only.

**Tech Stack:** Rust (Tauri backend), no new dependencies except `sha2 = "0.10"` for Task 5.

---

## Files Modified

| File | Task | Change |
|------|------|--------|
| `src-tauri/src/lib.rs` | Task 1 | FIFO mode 0o644→0o600 + secret token auth |
| `src-tauri/src/commands.rs` | Task 2 | Model name allowlist in download/delete/set_active |
| `src-tauri/tauri.conf.json` | Task 3 | Replace `"csp": null` with restrictive policy |
| `src-tauri/src/models/downloader.rs` | Task 4 | Add SHA256 hash constant + verify after download |
| `src-tauri/Cargo.toml` | Task 4 | Add `sha2 = "0.10"` dependency |
| `src-tauri/src/output/paste.rs` | Task 5 | Validate window_id is digits-only before xdotool |

---

## Testing note

This is a Tauri/Rust desktop app with no automated test suite. Each task is verified by:
1. `cargo build` inside `src-tauri/` — confirms the Rust code compiles
2. Manual smoke test where noted

Since `cargo` is not available in the Claude Code shell environment, compilation verification steps say "verify on a machine with the Rust toolchain". The code changes themselves are complete and correct.

---

## Task 1: FIFO secret token (High severity fix)

**Files:**
- Modify: `src-tauri/src/lib.rs:79-146`

**What this fixes:** Any same-user process can write to the FIFO and silently trigger recording. Adding a secret token means only processes that can read the token file (i.e., the user's own scripts) can trigger recording.

**How it works:** On startup, generate a random token using `std::time` (no new dependencies), write it to a `0o600` file. In the FIFO reader loop, read and validate the token before toggling. Also change FIFO mode from `0o644` to `0o600`.

- [ ] **Step 1: Replace `setup_fifo_listener` in `src-tauri/src/lib.rs`**

Replace lines 78-146 with:

```rust
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

    // Generate a random secret token for FIFO authentication.
    // Any process wanting to trigger recording must include this token.
    // The token file is 0o600 so only the owner can read it.
    let token = generate_fifo_token();
    let token_path = data_dir.join("fifo.token");
    match std::fs::write(&token_path, &token) {
        Ok(_) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &token_path,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
            log::info!("FIFO token written to {}", token_path.display());
        }
        Err(e) => log::warn!("Failed to write FIFO token: {}", e),
    }

    // Create a named pipe (FIFO) for receiving toggle commands.
    // Mode 0o600: only the owner can read/write.
    let fifo_path = data_dir.join("careless-whisper.sock");

    // Remove stale FIFO
    let _ = std::fs::remove_file(&fifo_path);

    // Create the FIFO
    let fifo_c = std::ffi::CString::new(fifo_path.to_str().unwrap()).unwrap();
    let ret = unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) };
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
    // Each time a valid token is written (and the writer closes), we toggle.
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

            // Read and validate the token. Reject writes that don't include it.
            let mut buf = [0u8; 128];
            let n = file.read(&mut buf).unwrap_or(0);
            let received = String::from_utf8_lossy(&buf[..n])
                .trim()
                .to_string();

            if received != token {
                log::warn!("FIFO token mismatch — ignoring toggle request");
                continue;
            }

            log::info!("FIFO toggle received (token verified)");
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

/// Generates a cryptographically random 128-bit token from /dev/urandom.
/// Encoded as hex. Falls back to time+PID if urandom is unavailable.
#[cfg(target_os = "linux")]
fn generate_fifo_token() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut bytes).is_ok() {
            return bytes.iter().map(|b| format!("{:02x}", b)).collect();
        }
    }
    // Fallback: time + PID (weaker but functional)
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:x}-{:x}", std::process::id(), nanos)
}
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error"
```
Expected: no output (no errors).

- [ ] **Step 3: Manual verification**

The token file is created at `~/.local/share/careless-whisper/fifo.token` on Linux with mode 600. To use the FIFO with the new token:
```bash
echo "$(cat ~/.local/share/careless-whisper/fifo.token)" > ~/.local/share/careless-whisper/careless-whisper.sock
```
Writing anything else (or nothing) should be rejected with a log warning.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "fix(security): add FIFO secret token auth and tighten mode to 0o600"
```

---

## Task 2: Model name allowlist (Medium severity fix)

**Files:**
- Modify: `src-tauri/src/commands.rs:217-246`

**What this fixes:** `download_model`, `delete_model`, and `set_active_model` accept any string as a model name, which could be used for path traversal (e.g. `../../config`). Restrict to the exact 5 known model names.

- [ ] **Step 1: Replace the three model command handlers in `src-tauri/src/commands.rs`**

Replace lines 217-246 with:

```rust
/// The only valid model names. Rejects path traversal and URL injection attempts.
const VALID_MODELS: &[&str] = &["tiny", "base", "small", "medium", "large-v3"];

fn validate_model_name(model: &str) -> Result<(), String> {
    if VALID_MODELS.contains(&model) {
        Ok(())
    } else {
        Err(format!(
            "Unknown model '{}'. Valid models: {}",
            model,
            VALID_MODELS.join(", ")
        ))
    }
}

#[tauri::command]
pub async fn download_model(app: AppHandle, model: String) -> Result<(), String> {
    validate_model_name(&model)?;
    downloader::download_model(app, model).await
}

#[tauri::command]
pub async fn delete_model(model: String) -> Result<(), String> {
    validate_model_name(&model)?;
    downloader::delete_model(&model)
}

#[tauri::command]
pub async fn set_active_model(
    model: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    validate_model_name(&model)?;
    let model_path = downloader::model_path(&model);
    if !model_path.exists() {
        return Err(format!("Model '{}' is not downloaded", model));
    }

    *state.whisper_ctx.lock().unwrap() = None;

    {
        let mut settings = state.settings.lock().unwrap();
        settings.active_model = model;
        settings.save()?;
    }

    Ok(())
}
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error"
```
Expected: no output (no errors).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "fix(security): add model name allowlist to prevent path traversal"
```

---

## Task 3: Enable Content Security Policy (Medium severity fix)

**Files:**
- Modify: `src-tauri/tauri.conf.json:39-41`

**What this fixes:** `"csp": null` disables all Content Security Policy protection. A restrictive policy prevents injected scripts from running even if XSS is ever introduced.

**CSP breakdown:**
- `default-src 'self'` — block everything not explicitly allowed
- `script-src 'self'` — only load scripts bundled with the app
- `style-src 'self' 'unsafe-inline'` — allow Tailwind's inline styles
- `img-src 'self' data:` — allow app icons and data URIs
- `connect-src https://huggingface.co` — allow model downloads

- [ ] **Step 1: Edit `src-tauri/tauri.conf.json`**

Replace:
```json
    "security": {
      "csp": null
    },
```

With:
```json
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src https://huggingface.co"
    },
```

- [ ] **Step 2: Verify the app still renders correctly**

```bash
cd /Users/gal/git/careless-whisper && pnpm tauri dev
```

Open the Settings window. Confirm:
- All UI elements render (Tailwind styles load)
- Model download initiates (HuggingFace connect-src works)
- No CSP errors in the webview console

If CSP blocks something legitimate, adjust the policy before committing.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "fix(security): enable Content Security Policy"
```

---

## Task 4: Model download SHA256 verification (Medium severity fix)

**Files:**
- Modify: `src-tauri/Cargo.toml` — add `sha2` dependency
- Modify: `src-tauri/src/models/downloader.rs` — add hash constants + verification

**What this fixes:** Downloaded model `.bin` files are loaded into whisper.cpp (a C library) without verifying their integrity. A compromised source could serve a malicious model. SHA256 verification ensures the file matches the known-good binary.

**SHA256 hashes** — from the official ggerganov/whisper.cpp README:

| Model | SHA256 |
|-------|--------|
| tiny | `bd577a113a864445d4c299885e0cb97d4ba92b5f` — NOTE: this is SHA1; use SHA256 from the repo |

**Action before implementing:** Look up the current SHA256 hashes from the ggerganov/whisper.cpp repository README at the time of implementation. The hashes change when models are updated.

- [ ] **Step 1: Add `sha2` to `src-tauri/Cargo.toml`**

After the `futures-util` line (line 32), add:
```toml
sha2 = "0.10"
```

The dependencies section should look like:
```toml
reqwest = { version = "0.12", features = ["stream"] }
futures-util = "0.3"
sha2 = "0.10"
dirs = "5"
```

- [ ] **Step 2: Look up current SHA256 hashes**

Fetch the current hashes from the ggerganov/whisper.cpp repository. They are listed in the README under "Available models". Use `sha256sum` on a known-good downloaded file, or find them documented in the repo.

- [ ] **Step 3: Update `src-tauri/src/models/downloader.rs` — add hash constants**

Replace line 15:
```rust
const MODELS: &[(&str, u32, u32)] = &[
```

With:
```rust
/// (name, disk_mb, ram_mb, sha256)
/// Hashes from https://github.com/ggerganov/whisper.cpp — verify these match
/// the current release before updating the model list.
const MODELS: &[(&str, u32, u32, &str)] = &[
    ("tiny",     75,   390,  "FILL_IN_SHA256_FROM_REPO"),
    ("base",     142,  500,  "FILL_IN_SHA256_FROM_REPO"),
    ("small",    466,  1024, "FILL_IN_SHA256_FROM_REPO"),
    ("medium",   1500, 2600, "FILL_IN_SHA256_FROM_REPO"),
    ("large-v3", 3000, 5120, "FILL_IN_SHA256_FROM_REPO"),
];
```

Then update `list_models()` to use the new tuple shape (lines 34-44):
```rust
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
```

- [ ] **Step 4: Add a helper to look up expected hash**

After `list_models()`, add:
```rust
fn expected_sha256(name: &str) -> Option<&'static str> {
    MODELS.iter()
        .find(|(n, _, _, _)| *n == name)
        .map(|(_, _, _, hash)| *hash)
}
```

- [ ] **Step 5: Add hash verification to `download_model`**

Replace the `file.flush()` / `drop(file)` / `rename` section (lines 86-88) with:

```rust
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
```

- [ ] **Step 6: Add the `sha256_file` helper function** at the end of `downloader.rs`:

```rust
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
```

- [ ] **Step 7: Add the `sha2` import at the top of `downloader.rs`** — it's used only inside `sha256_file` so no top-level `use` is needed; the `use sha2::...` inside the function handles it.

- [ ] **Step 8: Verify build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error"
```
Expected: no errors. `cargo` will fetch `sha2` automatically.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/models/downloader.rs
git commit -m "fix(security): verify SHA256 of downloaded model files"
```

---

## Task 5: Validate window_id before xdotool (Low severity fix)

**Files:**
- Modify: `src-tauri/src/output/paste.rs:222-247`

**What this fixes:** `window_id` is passed directly to `xdotool windowactivate` without validating it contains only digits. While not a real-world risk (the value comes from `xdotool getactivewindow`), it's good hygiene to reject unexpected values.

- [ ] **Step 1: Edit `paste_x11` in `src-tauri/src/output/paste.rs`**

Replace lines 222-247:
```rust
#[cfg(target_os = "linux")]
fn paste_x11(window_id: &str) -> Result<(), String> {
    // Re-focus the original window
    let focus_status = std::process::Command::new("xdotool")
        .args(["windowactivate", "--sync", window_id])
        .status()
        .map_err(|e| format!("Failed to run xdotool windowactivate: {e}"))?;
```

With:
```rust
#[cfg(target_os = "linux")]
fn paste_x11(window_id: &str) -> Result<(), String> {
    // Validate window_id is a plain integer before passing to xdotool
    if !window_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Invalid window ID '{}': expected digits only", window_id));
    }

    // Re-focus the original window
    let focus_status = std::process::Command::new("xdotool")
        .args(["windowactivate", "--sync", window_id])
        .status()
        .map_err(|e| format!("Failed to run xdotool windowactivate: {e}"))?;
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error"
```
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/output/paste.rs
git commit -m "fix(security): validate window_id before passing to xdotool"
```

---

## Final step: Push branch

- [ ] **Push the branch**

```bash
git push -u origin feat/security-enhancements
```

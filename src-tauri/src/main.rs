#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rand::distributions::Alphanumeric;
use rand::Rng;
use secrecy::{ExposeSecret, Secret};
use tauri::{Emitter, Manager};
use uuid::Uuid;
use zip::unstable::write::FileOptionsExt;
use zip::write::{FileOptions, ZipWriter};
use zip::{AesMode, CompressionMethod};

struct AppState {
    cancel_flag: Arc<AtomicBool>,
}

#[derive(serde::Deserialize, serde::Serialize)]
enum EncryptionMethod {
    Aes256,
    CryptoZip,
    SevenZip,
}

#[tauri::command]
fn generate_password() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

#[tauri::command]
async fn encrypt_files(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    file_paths: Vec<String>,
    output_path: String,
    password: Secret<String>,
    encryption_method: EncryptionMethod,
) -> Result<String, String> {
    state.cancel_flag.store(false, Ordering::SeqCst);

    match encryption_method {
        EncryptionMethod::SevenZip => {
            let temp_dir_name = format!("eazip-{}", Uuid::new_v4());
            let temp_dir_path = std::env::temp_dir().join(&temp_dir_name);
            fs::create_dir(&temp_dir_path).map_err(|e| e.to_string())?;

            app_handle.emit("encryption_progress", 25).unwrap(); // Stage 1: Setup

            let total_size: u64 = file_paths.iter().map(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum();
            let mut bytes_copied: u64 = 0;

            for file_path_str in &file_paths {
                if state.cancel_flag.load(Ordering::SeqCst) {
                    let _ = fs::remove_dir_all(&temp_dir_path);
                    return Err("Encryption cancelled by user.".to_string());
                }
                let from = Path::new(file_path_str);
                let to = temp_dir_path.join(from.file_name().unwrap());
                fs::copy(from, &to).map_err(|e| e.to_string())?;
                
                bytes_copied += fs::metadata(&to).map(|m| m.len()).unwrap_or(0);
                // Progress from 25% to 75% during copy
                let progress = 25 + (bytes_copied as f64 / total_size as f64 * 50.0) as u8;
                app_handle.emit("encryption_progress", progress).unwrap();
            }

            app_handle.emit("encryption_progress", 75).unwrap(); // Stage 2: Copying complete

            sevenz_rust2::compress_to_path_encrypted(
                &temp_dir_path,
                &output_path,
                password.expose_secret().as_str().into(),
            )
            .map_err(|e| e.to_string())?;

            app_handle.emit("encryption_progress", 100).unwrap(); // Stage 3: Compression complete

            fs::remove_dir_all(&temp_dir_path).map_err(|e| e.to_string())?;

            Ok(format!(
                "Files encrypted successfully to: {}",
                output_path
            ))
        }
        _ => {
            let output_path_buf = Path::new(&output_path);
            let file = File::create(&output_path_buf)
                .map_err(|e| format!("Failed to create output file: {}", e))?;
            let mut zip = ZipWriter::new(file);

            match encryption_method {
                EncryptionMethod::Aes256 => {
                    let options: FileOptions<'_, ()> = FileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .with_aes_encryption(AesMode::Aes256, password.expose_secret());
                    for (_i, file_path_str) in file_paths.iter().enumerate() {
                        if state.cancel_flag.load(Ordering::SeqCst) {
                            let _ = std::fs::remove_file(&output_path_buf);
                            return Err("Encryption cancelled by user.".to_string());
                        }
                        let original_path = Path::new(file_path_str);
                        let file_name = original_path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .ok_or_else(|| format!("Invalid file name for: {:?}", original_path))?;
                        zip.start_file(file_name, options.clone())
                            .map_err(|e| format!("Failed to start file in zip: {}", e))?;
                        let mut f = File::open(&original_path)
                            .map_err(|e| format!("Failed to open file: {}", e))?;
                        let file_size = f
                            .metadata()
                            .map_err(|e| format!("Failed to get file metadata: {}", e))?
                            .len();
                        let mut buffer = [0; 4096];
                        let mut bytes_read_total = 0;
                        loop {
                            if state.cancel_flag.load(Ordering::SeqCst) {
                                let _ = std::fs::remove_file(&output_path_buf);
                                return Err("Encryption cancelled by user.".to_string());
                            }
                            let bytes_read = f
                                .read(&mut buffer)
                                .map_err(|e| format!("Failed to read file: {}", e))?;
                            if bytes_read == 0 {
                                break;
                            }
                            zip.write_all(&buffer[..bytes_read])
                                .map_err(|e| format!("Failed to write to zip: {}", e))?;
                            bytes_read_total += bytes_read as u64;
                            let file_progress =
                                (bytes_read_total as f64 / file_size as f64 * 100.0) as u8;
                            app_handle
                                .emit("encryption_progress", file_progress)
                                .map_err(|e| format!("Failed to emit progress event: {}", e))?;
                        }
                    }
                }
                EncryptionMethod::CryptoZip => {
                    let options: FileOptions<'_, ()> = FileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .with_deprecated_encryption(password.expose_secret().as_bytes());
                    for (_i, file_path_str) in file_paths.iter().enumerate() {
                        if state.cancel_flag.load(Ordering::SeqCst) {
                            let _ = std::fs::remove_file(&output_path_buf);
                            return Err("Encryption cancelled by user.".to_string());
                        }
                        let original_path = Path::new(file_path_str);
                        let file_name = original_path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .ok_or_else(|| format!("Invalid file name for: {:?}", original_path))?;
                        zip.start_file(file_name, options.clone())
                            .map_err(|e| format!("Failed to start file in zip: {}", e))?;
                        let mut f = File::open(&original_path)
                            .map_err(|e| format!("Failed to open file: {}", e))?;
                        let file_size = f
                            .metadata()
                            .map_err(|e| format!("Failed to get file metadata: {}", e))?
                            .len();
                        let mut buffer = [0; 4096];
                        let mut bytes_read_total = 0;
                        loop {
                            if state.cancel_flag.load(Ordering::SeqCst) {
                                let _ = std::fs::remove_file(&output_path_buf);
                                return Err("Encryption cancelled by user.".to_string());
                            }
                            let bytes_read = f
                                .read(&mut buffer)
                                .map_err(|e| format!("Failed to read file: {}", e))?;
                            if bytes_read == 0 {
                                break;
                            }
                            zip.write_all(&buffer[..bytes_read])
                                .map_err(|e| format!("Failed to write to zip: {}", e))?;
                            bytes_read_total += bytes_read as u64;
                            let file_progress =
                                (bytes_read_total as f64 / file_size as f64 * 100.0) as u8;
                            app_handle
                                .emit("encryption_progress", file_progress)
                                .map_err(|e| format!("Failed to emit progress event: {}", e))?;
                        }
                    }
                }
                EncryptionMethod::SevenZip => unreachable!(), // Already handled
            };

            zip.finish()
                .map_err(|e| format!("Failed to finish zip: {}", e))?;

            Ok(format!(
                "Files encrypted successfully to: {}",
                output_path_buf.display()
            ))
        }
    }
}

#[tauri::command]
fn cancel_encryption(state: tauri::State<'_, AppState>) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

fn main() {
    let app_state = AppState {
        cancel_flag: Arc::new(AtomicBool::new(false)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            generate_password,
            encrypt_files,
            cancel_encryption
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

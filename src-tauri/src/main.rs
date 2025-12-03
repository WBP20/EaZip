#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rand::distributions::Alphanumeric;
use rand::Rng;
use secrecy::{ExposeSecret, Secret};
use tauri::Emitter;

use zip::unstable::write::FileOptionsExt;
use zip::write::{FileOptions, ZipWriter};
use zip::{AesMode, CompressionMethod};
use walkdir::WalkDir;

struct AppState {
    cancel_flag: Arc<AtomicBool>,
}

#[derive(serde::Deserialize, serde::Serialize)]
enum EncryptionMethod {
    Aes256,
    CryptoZip,
    SevenZip,
}

#[derive(serde::Serialize)]
struct FileMetadata {
    path: String,
    is_dir: bool,
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
fn get_file_metadata(paths: Vec<String>) -> Vec<FileMetadata> {
    paths
        .into_iter()
        .map(|path| {
            let is_dir = Path::new(&path).is_dir();
            FileMetadata { path, is_dir }
        })
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

    // Calculate total size for progress
    let mut total_size: u64 = 0;
    for path_str in &file_paths {
        for entry in WalkDir::new(path_str) {
             let entry = entry.map_err(|e| e.to_string())?;
             if entry.file_type().is_file() {
                 total_size += entry.metadata().map_err(|e| e.to_string())?.len();
             }
        }
    }

    match encryption_method {
        EncryptionMethod::SevenZip => {
            let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let temp_dir_path = temp_dir.path().to_path_buf();

            app_handle.emit("encryption_progress", 0).unwrap(); // Stage 1: Setup

            let mut bytes_copied: u64 = 0;

            for file_path_str in &file_paths {
                let path = Path::new(file_path_str);
                let parent = path.parent().unwrap_or(Path::new("/"));

                for entry in WalkDir::new(path) {
                    if state.cancel_flag.load(Ordering::SeqCst) {
                        // temp_dir will be automatically dropped and removed
                        return Err("Encryption cancelled by user.".to_string());
                    }
                    
                    let entry = entry.map_err(|e| e.to_string())?;
                    let entry_path = entry.path();
                    let relative_path = entry_path.strip_prefix(parent).map_err(|e| e.to_string())?;
                    let dest_path = temp_dir_path.join(relative_path);

                    if entry_path.is_dir() {
                        fs::create_dir_all(&dest_path).map_err(|e| e.to_string())?;
                    } else {
                        if let Some(p) = dest_path.parent() {
                            fs::create_dir_all(p).map_err(|e| e.to_string())?;
                        }
                        fs::copy(entry_path, &dest_path).map_err(|e| e.to_string())?;
                        
                        bytes_copied += fs::metadata(&dest_path).map(|m| m.len()).unwrap_or(0);
                        // Progress from 0% to 50% during copy
                        let progress = if total_size > 0 {
                            (bytes_copied as f64 / total_size as f64 * 50.0) as u8
                        } else {
                            0
                        };
                        app_handle.emit("encryption_progress", progress).unwrap();
                    }
                }
            }

            app_handle.emit("encryption_progress", 50).unwrap(); // Stage 2: Copying complete

            sevenz_rust2::compress_to_path_encrypted(
                &temp_dir_path,
                &output_path,
                password.expose_secret().as_str().into(),
            )
            .map_err(|e| e.to_string())?;

            app_handle.emit("encryption_progress", 100).unwrap(); // Stage 3: Compression complete

            app_handle.emit("encryption_progress", 100).unwrap(); // Stage 3: Compression complete

            // temp_dir is automatically removed here when it goes out of scope

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

            let options: FileOptions<'_, ()> = match encryption_method {
                EncryptionMethod::Aes256 => FileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .with_aes_encryption(AesMode::Aes256, password.expose_secret()),
                EncryptionMethod::CryptoZip => FileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .with_deprecated_encryption(password.expose_secret().as_bytes()),
                _ => unreachable!(),
            };

            let mut bytes_processed_total: u64 = 0;

            for file_path_str in &file_paths {
                let path = Path::new(file_path_str);
                let parent = path.parent().unwrap_or(Path::new("/"));

                for entry in WalkDir::new(path) {
                    if state.cancel_flag.load(Ordering::SeqCst) {
                        let _ = std::fs::remove_file(&output_path_buf);
                        return Err("Encryption cancelled by user.".to_string());
                    }

                    let entry = entry.map_err(|e| e.to_string())?;
                    let entry_path = entry.path();
                    let relative_path = entry_path.strip_prefix(parent).map_err(|e| e.to_string())?;
                    let relative_path_str = relative_path.to_str().ok_or("Invalid path encoding")?;

                    if entry_path.is_dir() {
                        zip.add_directory(relative_path_str, options.clone())
                           .map_err(|e| format!("Failed to add directory: {}", e))?;
                    } else {
                        zip.start_file(relative_path_str, options.clone())
                            .map_err(|e| format!("Failed to start file in zip: {}", e))?;
                        
                        let mut f = File::open(entry_path)
                            .map_err(|e| format!("Failed to open file: {}", e))?;
                        
                        let mut buffer = [0; 4096];
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
                            
                            bytes_processed_total += bytes_read as u64;
                            let progress = if total_size > 0 {
                                (bytes_processed_total as f64 / total_size as f64 * 100.0) as u8
                            } else {
                                0
                            };
                            app_handle
                                .emit("encryption_progress", progress)
                                .map_err(|e| format!("Failed to emit progress event: {}", e))?;
                        }
                    }
                }
            }

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
async fn decrypt_file(
    app_handle: tauri::AppHandle,
    _state: tauri::State<'_, AppState>,
    file_path: String,
    output_dir: String,
    password: Secret<String>,
) -> Result<String, String> {
    const MAX_TOTAL_SIZE: u64 = 10 * 1024 * 1024 * 1024; // 10 GB
    const MAX_FILE_COUNT: usize = 10_000;

    let path = Path::new(&file_path);
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    if extension == "7z" {
        sevenz_rust2::decompress_file_with_password(
            path,
            &output_dir,
            password.expose_secret().as_str().into(),
        )
        .map_err(|e| e.to_string())?;
    } else {
        let file = File::open(&path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

        let mut total_extracted_size: u64 = 0;
        let mut extracted_count: usize = 0;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index_decrypt(i, password.expose_secret().as_bytes())
                .map_err(|e| {
                    if let zip::result::ZipError::InvalidPassword = e {
                        "Mot de passe incorrect".to_string()
                    } else {
                        e.to_string()
                    }
                })?;
            
            // Zip Bomb Protection
            extracted_count += 1;
            if extracted_count > MAX_FILE_COUNT {
                return Err(format!("Too many files in archive (limit: {})", MAX_FILE_COUNT));
            }

            let size = file.size();
            total_extracted_size += size;
            if total_extracted_size > MAX_TOTAL_SIZE {
                return Err(format!("Total extracted size exceeds limit (limit: {} bytes)", MAX_TOTAL_SIZE));
            }

            // Zip Slip Protection
            let outpath = Path::new(&output_dir).join(file.mangled_name());
            
            // Canonicalize output_dir to resolve symlinks/..
            let canonical_output_dir = Path::new(&output_dir).canonicalize().map_err(|e| e.to_string())?;
            
            // We can't canonicalize outpath yet because it doesn't exist, 
            // but we can check if it would be within canonical_output_dir.
            // A simple way is to check if outpath starts with output_dir, 
            // but mangled_name() already strips .. so simple join is usually safe *if* mangled_name works correctly.
            // To be extra safe against mangled_name bugs or weirdness:
            if !outpath.starts_with(&output_dir) {
                 return Err("Invalid file path (Zip Slip attempt detected)".to_string());
            }
            // Note: A more robust check would be to create the parent dir, canonicalize it, and check prefix.
            // But for this level of security, relying on mangled_name + starts_with check is a good first step.
            // Let's do the parent dir check for extra safety if it's a file.

            if file.is_dir() {
                fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p).map_err(|e| e.to_string())?;
                    }
                    // Extra Zip Slip check: canonicalize parent and ensure it's inside output_dir
                    let canonical_parent = p.canonicalize().map_err(|e| e.to_string())?;
                    if !canonical_parent.starts_with(&canonical_output_dir) {
                         return Err("Invalid file path (Zip Slip attempt detected)".to_string());
                    }
                }
                let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(format!("File decrypted successfully to: {}", output_dir))
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
            decrypt_file,
            cancel_encryption,
            get_file_metadata
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

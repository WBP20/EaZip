#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
#[serde(rename_all = "camelCase")]
struct FileMetadata {
    path: String,
    name: String,
    is_dir: bool,
    is_symlink: bool,
    size: u64,
    error: Option<String>,
    debug_info: Option<String>,
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
        .map(|path_str| {
            let path = Path::new(&path_str);
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&path_str)
                .to_string();

            // Use symlink_metadata to detect symlinks
            let (is_dir, is_symlink, size, error) = match fs::symlink_metadata(&path) {
                Ok(meta) => {
                    let is_symlink = meta.file_type().is_symlink();
                    let mut is_dir = meta.is_dir();
                    let mut size = meta.len();
                    let mut error = None;

                    // If it's a symlink, resolve it to check if it points to a dir
                    if is_symlink {
                        match fs::metadata(&path) {
                            Ok(resolved_meta) => {
                                is_dir = resolved_meta.is_dir();
                                size = resolved_meta.len();
                            }
                            Err(e) => {
                                error = Some(format!("Symlink broken: {}", e));
                            }
                        }
                    }
                    (is_dir, is_symlink, size, error)
                }
                Err(e) => (false, false, 0, Some(e.to_string())),
            };
            
            // Debug: If it's supposed to be a dir but isn't, get raw mode
            let error = if !is_dir && error.is_none() {
                 if let Ok(m) = fs::metadata(&path) {
                     #[cfg(unix)]
                     {
                         use std::os::unix::fs::MetadataExt;
                         Some(format!("Not a dir. Mode: {:o}, Size: {}", m.mode(), m.len()))
                     }
                     #[cfg(not(unix))]
                     {
                         Some(format!("Not a dir. Size: {}", m.len()))
                     }
                 } else {
                     error
                 }
            } else {
                error
            };
            
            let debug_info = if let Ok(m) = fs::metadata(&path) {
                 let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
                 #[cfg(unix)]
                 {
                     use std::os::unix::fs::MetadataExt;
                     Some(format!("Canonical: {}, Mode: {:o}, IsDir: {}, IsFile: {}", canonical.display(), m.mode(), m.is_dir(), m.is_file()))
                 }
                 #[cfg(not(unix))]
                 {
                     Some(format!("Canonical: {}, IsDir: {}, IsFile: {}", canonical.display(), m.is_dir(), m.is_file()))
                 }
            } else {
                 None
            };

            FileMetadata {
                path: path_str,
                name,
                is_dir,
                is_symlink,
                size,
                error,
                debug_info,
            }
        })
        .collect()
}

struct CollectedEntry {
    abs_path: std::path::PathBuf,
    rel_path: std::path::PathBuf,
    is_dir: bool,
    size: u64,
}

fn collect_entries(
    file_paths: &[String],
    canonical_output_path: &Path,
) -> Result<(Vec<CollectedEntry>, u64), String> {
    let mut entries = Vec::new();
    let mut total_size = 0u64;

    for file_path_str in file_paths {
        let root = Path::new(file_path_str);
        let parent = root.parent().unwrap_or(Path::new("/"));

        for entry in WalkDir::new(root) {
            let entry = entry.map_err(|e| e.to_string())?;
            let entry_path = entry.path();

            // Optimization: Only check full path if file name matches output file name
            // This prevents the infinite recursion loop without checking every single file
            if let Some(name) = canonical_output_path.file_name() {
                if entry.file_name() == name {
                    if let Ok(p) = entry_path.canonicalize() {
                        if p == canonical_output_path {
                            continue;
                        }
                    }
                }
            }

            let rel = entry_path
                .strip_prefix(parent)
                .map_err(|e| e.to_string())?
                .to_path_buf();

            let is_dir = entry.file_type().is_dir();
            let size = if is_dir {
                0
            } else {
                entry.metadata().map_err(|e| e.to_string())?.len()
            };

            if !is_dir {
                total_size = total_size.saturating_add(size);
            }

            entries.push(CollectedEntry {
                abs_path: entry_path.to_path_buf(),
                rel_path: rel,
                is_dir,
                size,
            });
        }
    }

    Ok((entries, total_size))
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
    let cancel_flag = state.cancel_flag.clone(); // Clone Arc for thread
    let password = password.expose_secret().clone(); // Clone password string

    tauri::async_runtime::spawn_blocking(move || {
        cancel_flag.store(false, Ordering::SeqCst);
        
        app_handle.emit("encryption_status", "Analyse des fichiers...").unwrap();

        // Canonicalize output path to prevent recursion
        let canonical_output_path = Path::new(&output_path).canonicalize().unwrap_or_else(|_| Path::new(&output_path).to_path_buf());

        // Single pass collection
        let (entries, total_size) = collect_entries(&file_paths, &canonical_output_path)?;

        match encryption_method {
            EncryptionMethod::SevenZip => {
                let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
                let temp_dir_path = temp_dir.path().to_path_buf();

                app_handle.emit("encryption_status", "Préparation de la copie...").unwrap();
                app_handle.emit("encryption_progress", 0).unwrap(); // Stage 1: Setup

                let mut bytes_copied: u64 = 0;
                let mut last_update_time = Instant::now();
                let mut last_progress_percent: u8 = 0;

                for entry in &entries {
                    if cancel_flag.load(Ordering::SeqCst) {
                        return Err("Encryption cancelled by user.".to_string());
                    }

                    let dest_path = temp_dir_path.join(&entry.rel_path);

                    if entry.is_dir {
                        fs::create_dir_all(&dest_path).map_err(|e| e.to_string())?;
                    } else {
                        if let Some(p) = dest_path.parent() {
                            fs::create_dir_all(p).map_err(|e| e.to_string())?;
                        }
                        fs::copy(&entry.abs_path, &dest_path).map_err(|e| e.to_string())?;
                        
                        bytes_copied += entry.size;
                        // Progress from 0% to 50% during copy
                        let progress = if total_size > 0 {
                            (bytes_copied as f64 / total_size as f64 * 50.0) as u8
                        } else {
                            0
                        };
                        
                        let now = Instant::now();
                        if progress > last_progress_percent || now.duration_since(last_update_time) >= Duration::from_millis(100) {
                             app_handle.emit("encryption_progress", progress).unwrap();
                             app_handle.emit("encryption_status", format!("Copie: {}", entry.abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("..."))).unwrap();
                             last_update_time = now;
                             last_progress_percent = progress;
                        }
                    }
                }

                app_handle.emit("encryption_progress", 50).unwrap(); // Stage 2: Copying complete
                app_handle.emit("encryption_progress", 50).unwrap(); // Stage 2: Copying complete
                app_handle.emit("encryption_status", "Compression de l'archive (cette étape peut être longue)...").unwrap();

                let running = Arc::new(AtomicBool::new(true));
                let running_clone = running.clone();
                let app_for_thread = app_handle.clone();

                // Fake progress thread for compression phase (50% -> 95%)
                std::thread::spawn(move || {
                    let mut progress: u8 = 50;
                    let max_progress: u8 = 95;
                    
                    while running_clone.load(Ordering::SeqCst) && progress < max_progress {
                        let _ = app_for_thread.emit("encryption_progress", progress);
                        progress += 1;
                        // Slow progress: 45% over ~22 seconds (500ms * 45)
                        std::thread::sleep(Duration::from_millis(500));
                    }
                });

                let res = sevenz_rust2::compress_to_path_encrypted(
                    &temp_dir_path,
                    &output_path,
                    password.as_str().into(),
                );

                running.store(false, Ordering::SeqCst);
                res.map_err(|e| e.to_string())?;

                app_handle.emit("encryption_progress", 100).unwrap(); // Stage 3: Compression complete
                app_handle.emit("encryption_status", "Terminé !").unwrap();

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
                
                app_handle.emit("encryption_status", "Chiffrement en cours...").unwrap();

                let options: FileOptions<'_, ()> = match encryption_method {
                    EncryptionMethod::Aes256 => FileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .with_aes_encryption(AesMode::Aes256, &password),
                    EncryptionMethod::CryptoZip => FileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .with_deprecated_encryption(password.as_bytes()),
                    _ => unreachable!(),
                };

                let mut bytes_processed_total: u64 = 0;
                let mut last_update_time = Instant::now();
                let mut last_progress_percent: u8 = 0;

                for entry in &entries {
                    if cancel_flag.load(Ordering::SeqCst) {
                        let _ = std::fs::remove_file(&output_path_buf);
                        return Err("Encryption cancelled by user.".to_string());
                    }

                    let rel_str = entry.rel_path.to_str().ok_or("Invalid path encoding")?;

                    if entry.is_dir {
                        zip.add_directory(rel_str, options.clone())
                           .map_err(|e| format!("Failed to add directory: {}", e))?;
                    } else {
                        zip.start_file(rel_str, options.clone())
                            .map_err(|e| format!("Failed to start file in zip: {}", e))?;
                        
                        let mut f = File::open(&entry.abs_path)
                            .map_err(|e| format!("Failed to open file: {}", e))?;
                        
                        let mut buffer = vec![0; 1024 * 1024]; // 1MB buffer
                        loop {
                            if cancel_flag.load(Ordering::SeqCst) {
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
                            
                            let now = Instant::now();
                            if progress > last_progress_percent || now.duration_since(last_update_time) >= Duration::from_millis(100) {
                                app_handle
                                    .emit("encryption_progress", progress)
                                    .map_err(|e| format!("Failed to emit progress event: {}", e))?;
                                app_handle.emit("encryption_status", format!("Chiffrement: {}", entry.abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("..."))).unwrap();
                                last_update_time = now;
                                last_progress_percent = progress;
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
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn decrypt_file(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    file_path: String,
    output_dir: String,
    password: Secret<String>,
) -> Result<String, String> {
    const MAX_TOTAL_SIZE: u64 = 10 * 1024 * 1024 * 1024; // 10 GB
    const MAX_FILE_COUNT: usize = 10_000;

    let cancel_flag = state.cancel_flag.clone();
    let password = password.expose_secret().clone();

    tauri::async_runtime::spawn_blocking(move || {
        cancel_flag.store(false, Ordering::SeqCst);
        
        let path = Path::new(&file_path);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

        if extension == "7z" {
            app_handle.emit("encryption_status", "Déchiffrement 7z en cours...").unwrap();
            
            let running = Arc::new(AtomicBool::new(true));
            let running_clone = running.clone();
            let app_for_thread = app_handle.clone();
            
            // Fake progress thread
            std::thread::spawn(move || {
                let mut progress: u8 = 0;
                let max_progress: u8 = 95;
                
                while running_clone.load(Ordering::SeqCst) && progress < max_progress {
                    let _ = app_for_thread.emit("encryption_progress", progress);
                    progress += 1;
                    // Slow progress: 95% over ~47 seconds (500ms * 95)
                    // Adjust sleep to make it faster or slower depending on expected size
                    std::thread::sleep(Duration::from_millis(500));
                }
            });

            let res = sevenz_rust2::decompress_file_with_password(
                path,
                &output_dir,
                password.as_str().into(),
            );
            
            running.store(false, Ordering::SeqCst);
            res.map_err(|e| e.to_string())?;
        } else {
            app_handle.emit("encryption_status", "Ouverture de l'archive...").unwrap();
            let file = File::open(&path).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

            app_handle.emit("encryption_status", "Calcul de la taille totale...").unwrap();
            // Calculate total size for progress
            let mut total_size: u64 = 0;
            let len = archive.len();
            for i in 0..len {
                if cancel_flag.load(Ordering::SeqCst) {
                    return Err("Decryption cancelled by user.".to_string());
                }
                if i % 50 == 0 {
                    app_handle.emit("encryption_status", format!("Analyse du contenu... ({}/{})", i, len)).unwrap();
                }

                // We must use by_index_decrypt even for size calculation if the file is encrypted
                let file = archive.by_index_decrypt(i, password.as_bytes()).map_err(|e| e.to_string())?;
                total_size += file.size();
            }

            let mut total_extracted_size: u64 = 0;
            let mut extracted_count: usize = 0;
            let mut last_update_time = Instant::now();
            let mut last_progress_percent: u8 = 0;

            app_handle.emit("encryption_status", "Déchiffrement en cours...").unwrap();

            for i in 0..archive.len() {
                if cancel_flag.load(Ordering::SeqCst) {
                    return Err("Decryption cancelled by user.".to_string());
                }

                let mut file = archive
                    .by_index_decrypt(i, password.as_bytes())
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
                // We check total_extracted_size dynamically as we write, but checking here is good too
                if total_extracted_size + size > MAX_TOTAL_SIZE {
                     return Err(format!("Total extracted size exceeds limit (limit: {} bytes)", MAX_TOTAL_SIZE));
                }

                // Zip Slip Protection
                let outpath = Path::new(&output_dir).join(file.mangled_name());
                let canonical_output_dir = Path::new(&output_dir).canonicalize().map_err(|e| e.to_string())?;
                
                if !outpath.starts_with(&output_dir) {
                     return Err("Invalid file path (Zip Slip attempt detected)".to_string());
                }

                if file.is_dir() {
                    fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
                } else {
                    if let Some(p) = outpath.parent() {
                        if !p.exists() {
                            fs::create_dir_all(p).map_err(|e| e.to_string())?;
                        }
                        let canonical_parent = p.canonicalize().map_err(|e| e.to_string())?;
                        if !canonical_parent.starts_with(&canonical_output_dir) {
                             return Err("Invalid file path (Zip Slip attempt detected)".to_string());
                        }
                    }
                    
                    let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
                    
                    // Manual copy with progress
                    let mut buffer = vec![0; 1024 * 1024]; // 1MB buffer
                    loop {
                        if cancel_flag.load(Ordering::SeqCst) {
                            return Err("Decryption cancelled by user.".to_string());
                        }
                        let bytes_read = file.read(&mut buffer).map_err(|e| e.to_string())?;
                        if bytes_read == 0 {
                            break;
                        }
                        outfile.write_all(&buffer[..bytes_read]).map_err(|e| e.to_string())?;
                        
                        total_extracted_size += bytes_read as u64;
                        
                        let progress = if total_size > 0 {
                            (total_extracted_size as f64 / total_size as f64 * 100.0) as u8
                        } else {
                            0
                        };

                        let now = Instant::now();
                        if progress > last_progress_percent || now.duration_since(last_update_time) >= Duration::from_millis(100) {
                            app_handle.emit("encryption_progress", progress).unwrap();
                            // Optional: emit filename status if desired, but might be too fast
                            // app_handle.emit("encryption_status", format!("Extraction: {}", file.name())).unwrap();
                            last_update_time = now;
                            last_progress_percent = progress;
                        }
                    }
                }
            }
        }

        app_handle.emit("encryption_progress", 100).unwrap();
        app_handle.emit("encryption_status", "Déchiffrement terminé !").unwrap();

        Ok(format!("File decrypted successfully to: {}", output_dir))
    }).await.map_err(|e| e.to_string())?
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

pub mod model;
pub mod pipeline;

use model::{Progress, Settings, StackReport};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

/// Shared application state.
///
/// `running` prevents two stack jobs at once. `cancel` is the token the
/// pipeline polls to stop early: the UI flips it from another thread, and the
/// worker abandons at the next checkpoint.
#[derive(Default)]
struct AppState {
    running: Mutex<bool>,
    cancel: Arc<AtomicBool>,
}

// Tauri event carrying progress updates to the frontend.
const PROGRESS_EVENT: &str = "hyperion://progress";

/// Scans a folder: counts RAW files, detects the object, estimates crop size.
/// Returns a summary the UI shows before launching the full run.
#[tauri::command]
fn scan_folder(path: String) -> Result<serde_json::Value, String> {
    let dir = PathBuf::from(&path);
    let files = pipeline::decode::list_raw_files(&dir).map_err(|e| e.to_string())?;

    // Cache state: an existing manifest for this source folder means the
    // demosaiced TIFFs are already available and import will be instant.
    let cache_dir = dir.join(".hyperion_cache");
    let cached = pipeline::cache::Manifest::load(&cache_dir)
        .filter(|m| m.source_dir == path)
        .map(|m| m.frames.len())
        .unwrap_or(0);

    let preview = if let Some(first) = files.first() {
        // Prefer the cached TIFF (fast read); otherwise decode the RAW.
        let cached_tiff = pipeline::cache::cache_path_for(&cache_dir, first, 0);
        let source = if cached_tiff.exists() { &cached_tiff } else { first };

        match pipeline::decode::load_rgb16_any(source) {
            Ok(d) => {
                let det = pipeline::detect::detect_planet(&d.rgb16, d.width, d.height, None);
                serde_json::json!({
                    "width": d.width,
                    "height": d.height,
                    "model": d.model,
                    "planetDetected": det.is_some(),
                    "planetRadius": det.as_ref().map(|x| x.radius),
                })
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        }
    } else {
        serde_json::json!({ "error": "no RAW file" })
    };

    Ok(serde_json::json!({
        "count": files.len(),
        "path": path,
        "cached": cached,
        "preview": preview,
    }))
}

/// Runs the full pipeline (import, analysis, alignment, stacking, export).
/// Progress is pushed on the `hyperion://progress` event.
#[tauri::command]
async fn run_stack(
    app: AppHandle,
    state: State<'_, AppState>,
    source_dir: String,
    cache_dir: String,
    settings: Settings,
) -> Result<StackReport, String> {
    {
        let mut running = state.running.lock().unwrap();
        if *running {
            return Err("a job is already running".into());
        }
        *running = true;
    }

    // Fresh token for this run: a cancel from a previous run must not leak.
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();

    let app_for_progress = app.clone();
    let progress = move |p: Progress| {
        let _ = app_for_progress.emit(PROGRESS_EVENT, p);
    };

    let src = PathBuf::from(&source_dir);
    let cache = PathBuf::from(&cache_dir);

    let result = tauri::async_runtime::spawn_blocking(move || {
        pipeline::run_pipeline(&src, &cache, &settings, &progress, &cancel)
    })
    .await
    .map_err(|e| e.to_string());

    {
        let mut running = state.running.lock().unwrap();
        *running = false;
    }

    match result {
        Ok(Ok(report)) => Ok(report),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e),
    }
}

/// Requests cancellation of the running job. The pipeline stops at its next
/// checkpoint and reports a `Cancelled` error rather than a failure.
#[tauri::command]
fn cancel_stack(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Reports whether a job is currently running.
#[tauri::command]
fn is_running(state: State<'_, AppState>) -> bool {
    *state.running.lock().unwrap()
}

/// Renders a PNG preview of the stack and returns it as a base64 data URL.
///
/// `max_size` bounds the preview's longest side (proportional downscale) to
/// keep the response light. `crop` optionally extracts a region at full
/// resolution (to judge real 1:1 sharpness).
#[tauri::command]
fn preview_image(
    path: String,
    max_size: Option<usize>,
    crop: Option<serde_json::Value>,
) -> Result<String, String> {
    pipeline::preview::render_preview(
        std::path::Path::new(&path),
        max_size.unwrap_or(1400),
        crop.as_ref(),
    )
    .map_err(|e| e.to_string())
}

/// Copies the final TIFF to a user-chosen location ("Save as" button).
///
/// Refuses to overwrite an existing file: a bad destination must never
/// destroy a previous version of the stack. The frontend then offers another
/// name.
#[tauri::command]
fn save_copy(from: String, to: String) -> Result<(), String> {
    let dest = std::path::Path::new(&to);
    if dest.exists() {
        return Err(format!("file already exists: {to}"));
    }
    let src = std::path::Path::new(&from);
    if !src.exists() {
        return Err(format!("source stack not found: {from}"));
    }
    std::fs::copy(src, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes the cache of a source folder (the `.hyperion_cache` directory).
/// Returns the number of bytes freed.
#[tauri::command]
fn clear_cache(source_dir: String) -> Result<u64, String> {
    let cache_dir = PathBuf::from(&source_dir).join(".hyperion_cache");
    if !cache_dir.exists() {
        return Ok(0);
    }
    let freed = pipeline::cache::dir_size(&cache_dir);
    std::fs::remove_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    Ok(freed)
}

/// Returns the cache size (bytes) for a source folder, plus how many stale
/// entries exist (cache files whose source RAW is gone).
#[tauri::command]
fn cache_info(source_dir: String) -> Result<serde_json::Value, String> {
    let cache_dir = PathBuf::from(&source_dir).join(".hyperion_cache");
    if !cache_dir.exists() {
        return Ok(serde_json::json!({ "size": 0, "frames": 0, "stale": 0 }));
    }
    let size = pipeline::cache::dir_size(&cache_dir);
    let manifest = pipeline::cache::Manifest::load(&cache_dir);
    let (frames, stale) = match manifest {
        Some(m) => {
            let stale = m
                .frames
                .iter()
                .filter(|f| !std::path::Path::new(&f.cache_path).exists())
                .count();
            (m.frames.len(), stale)
        }
        None => (0, 0),
    };
    Ok(serde_json::json!({ "size": size, "frames": frames, "stale": stale }))
}

/// Purges stale cache entries (manifest rows whose TIFF no longer exists).
/// Returns the number of entries removed.
#[tauri::command]
fn prune_cache(source_dir: String) -> Result<usize, String> {
    let cache_dir = PathBuf::from(&source_dir).join(".hyperion_cache");
    let mut manifest = match pipeline::cache::Manifest::load(&cache_dir) {
        Some(m) => m,
        None => return Ok(0),
    };
    let before = manifest.frames.len();
    manifest.frames.retain(|f| std::path::Path::new(&f.cache_path).exists());
    let removed = before - manifest.frames.len();
    if removed > 0 {
        manifest.save(&cache_dir).map_err(|e| e.to_string())?;
    }
    Ok(removed)
}

/// Returns the free space (bytes) on the filesystem holding `path`.
///
/// Best effort: `None` when the platform call fails, so the UI can simply skip
/// the disk-space warning.
#[tauri::command]
fn available_space(path: String) -> Option<u64> {
    pipeline::cache::available_space(std::path::Path::new(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            run_stack,
            cancel_stack,
            is_running,
            preview_image,
            save_copy,
            clear_cache,
            cache_info,
            prune_cache,
            available_space
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

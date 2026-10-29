pub mod model;
pub mod pipeline;

use model::{Progress, Settings, StackReport};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

/// Etat partage : empeche deux traitements simultanes et permet un suivi.
#[derive(Default)]
struct AppState {
    running: Mutex<bool>,
}

// Evenement Tauri emis vers le frontend pour la progression.
const PROGRESS_EVENT: &str = "hyperion://progress";

/// Analyse un dossier : compte les RAW, detecte la planete, estime la taille du crop.
/// Renvoie un resume affichable dans l'UI avant de lancer le traitement complet.
#[tauri::command]
fn scan_folder(path: String) -> Result<serde_json::Value, String> {
    let dir = PathBuf::from(&path);
    let files = pipeline::decode::list_raw_files(&dir).map_err(|e| e.to_string())?;

    // Etat du cache : un manifeste existant pour ce dossier source signifie que
    // les TIFF dematriçes sont deja disponibles et que l'import sera instantane.
    let cache_dir = dir.join(".hyperion_cache");
    let cached = pipeline::cache::Manifest::load(&cache_dir)
        .filter(|m| m.source_dir == path)
        .map(|m| m.frames.len())
        .unwrap_or(0);

    let preview = if let Some(first) = files.first() {
        // Priorite au TIFF de cache (lecture rapide) ; sinon on decode le RAW.
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
        serde_json::json!({ "error": "aucun fichier RAW" })
    };

    Ok(serde_json::json!({
        "count": files.len(),
        "path": path,
        "cached": cached,
        "preview": preview,
    }))
}

/// Lance le pipeline complet (import, analyse, alignement, empilement, export).
/// La progression est poussee sur l'evenement `hyperion://progress`.
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
            return Err("un traitement est deja en cours".into());
        }
        *running = true;
    }

    let app_for_progress = app.clone();
    let progress = move |p: Progress| {
        let _ = app_for_progress.emit(PROGRESS_EVENT, p);
    };

    let src = PathBuf::from(&source_dir);
    let cache = PathBuf::from(&cache_dir);

    let result = tauri::async_runtime::spawn_blocking(move || {
        pipeline::run_pipeline(&src, &cache, &settings, &progress)
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

/// Genere un apercu PNG du stack et le renvoie en data URL base64.
///
/// `max_size` borne la dimension de l'apercu (reduction proportionnelle) pour
/// garder la reponse legere. `crop` optionnel permet d'extraire une zone en
/// pleine resolution (pour juger la nettete reelle a 1:1).
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

/// Enregistre une image TIFF (stack final) a l'emplacement choisi par
/// l'utilisateur. Sert au bouton « Enregistrer sous ».
///
/// On refuse d'ecraser un fichier existant : une mauvaise destination ne doit
/// jamais detruire une version precedente du stack. Le frontend propose alors
/// un autre nom.
#[tauri::command]
fn save_copy(from: String, to: String) -> Result<(), String> {
    let dest = std::path::Path::new(&to);
    if dest.exists() {
        return Err(format!("le fichier existe deja : {}", to));
    }
    let src = std::path::Path::new(&from);
    if !src.exists() {
        return Err(format!("le stack source est introuvable : {}", from));
    }
    std::fs::copy(src, dest).map_err(|e| e.to_string())?;
    Ok(())
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
            preview_image,
            save_copy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
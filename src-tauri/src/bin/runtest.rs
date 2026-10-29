//! Validation manuelle : execute le pipeline complet (import, detection,
//! qualite, alignement, empilement, export) sur un dossier, avec progression
//! affichee dans le terminal.
//!
//! Usage : cargo run --bin runtest -- <dossier_source> [cache_dir] [keep_fraction]
//!
//! Si le dossier ne contient pas de RAW bruts mais des TIFF 16-bit deja
//! dematriçes, le pipeline les utilise directement (lecture, pas de decodage).

use hyperion_studio_lib::model::{Phase, Progress, Settings};
use hyperion_studio_lib::pipeline;
use std::sync::Mutex;

/// Analyse une chaine d'orientation du type "180", "180h", "90v", "hv".
fn parse_orientation(s: &str) -> hyperion_studio_lib::model::Orientation {
    let lower = s.to_ascii_lowercase();
    let rotate = if lower.contains("270") {
        270
    } else if lower.contains("180") {
        180
    } else if lower.contains("90") {
        90
    } else {
        0
    };
    hyperion_studio_lib::model::Orientation {
        rotate,
        flip_h: lower.contains('h'),
        flip_v: lower.contains('v'),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(src) = args.get(1) else {
        eprintln!("usage: runtest <dossier_source> [cache_dir] [keep_fraction] [orientation]");
        std::process::exit(2);
    };
    let src_path = std::path::PathBuf::from(src);
    let cache = args
        .get(2)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| src_path.join(".hyperion_cache"));
    let keep: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.25);
    // 4e argument optionnel : orientation, ex. "180", "180h", "90v", "hv".
    let orientation = match args.get(4) {
        Some(s) => parse_orientation(s),
        None => hyperion_studio_lib::model::Orientation::default(),
    };
    // 5e argument optionnel : "enhance" active le post-traitement par defaut.
    // 6e : force des ondelettes (ex. "1.2"). 7e : force CLAHE (ex. "0.2").
    // 8e : clarte (ex. "0.15").
    let mut enhance = hyperion_studio_lib::model::EnhanceSettings::default();
    if args.get(5).map(|s| s == "enhance").unwrap_or(false) {
        enhance.enabled = true;
        if let Some(v) = args.get(6).and_then(|s| s.parse::<f32>().ok()) {
            enhance.wavelet_amount = v;
        }
        if let Some(v) = args.get(7).and_then(|s| s.parse::<f32>().ok()) {
            enhance.clahe_amount = v;
        }
        if let Some(v) = args.get(8).and_then(|s| s.parse::<f32>().ok()) {
            enhance.clarity = v;
        }
        if let Some(v) = args.get(9).and_then(|s| s.parse::<f32>().ok()) {
            enhance.gamma = v;
        }
    }
    // 10e argument : "ap" active l'alignement local (desactive par defaut,
    // car l'implementation actuelle degrade l'image).
    let ap_enabled = args.get(10).map(|s| s == "ap").unwrap_or(false);
    // 11e argument : "noalign" desactive tout alignement (diagnostic).
    let do_align = args.get(11).map(|s| s != "noalign").unwrap_or(true);
    // 12e argument : taille de bloc (0 = empilement global).
    let block_size = args
        .get(12)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);

    let settings = Settings {
        keep_fraction: keep,
        orientation,
        enhance,
        ap_enabled,
        block_size,
        align: do_align,
        ..Default::default()
    };

    let last = Mutex::new(String::new());
    let progress = move |p: Progress| {
        let msg = match p {
            Progress::Phase { phase, message } => {
                let tag = match phase {
                    Phase::Idle => "idle",
                    Phase::Importing => "IMPORT",
                    Phase::Analyzing => "ANALYSE",
                    Phase::Stacking => "STACK",
                    Phase::Enhancing => "ENHANCE",
                    Phase::Done => "DONE",
                    Phase::Failed => "FAILED",
                };
                format!("[{tag}] {message}")
            }
            Progress::Import { done, total } => format!("  import {done}/{total}"),
            Progress::Analyze { done, total } => format!("  analyse {done}/{total}"),
            Progress::Stack { done, total } => format!("  stack {done}/{total}"),
            Progress::Finished { report } => format!(
                "TERMINE: {} frames gardees / {} | qualite moy {:.3} | crop {}x{} | {}",
                report.kept_frames,
                report.total_frames,
                report.mean_quality,
                report.crop.width,
                report.crop.height,
                report.output_path
            ),
            Progress::Error { message } => format!("ERREUR: {message}"),
        };
        // N'affiche que les changements de ligne (evite le spam par frame).
        let mut l = last.lock().unwrap();
        let short = msg.trim_start_matches(' ').split('/').next().unwrap_or("").to_string();
        if short != *l || msg.starts_with("TERM") || msg.starts_with("[") || msg.starts_with("ERR") {
            println!("{msg}");
            *l = short;
        }
    };

    let t0 = std::time::Instant::now();
    match pipeline::run_pipeline(&src_path, &cache, &settings, &progress) {
        Ok(r) => {
            println!("\nSUCCES en {:?}", t0.elapsed());
            println!("  sortie : {}", r.output_path);
            println!("  frames : {} gardees / {}", r.kept_frames, r.total_frames);
        }
        Err(e) => {
            eprintln!("\nECHEC: {e:#}");
            std::process::exit(1);
        }
    }
}

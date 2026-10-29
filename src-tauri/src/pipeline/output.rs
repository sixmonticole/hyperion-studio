//! Ecriture du stack final (TIFF 16-bit) et du rapport JSON de tracabilite.

use crate::model::StackReport;
use crate::pipeline::decode::write_rgb16_tiff;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Chemin de sortie du stack pour un dossier source donne :
/// `<source>/hyperion_out/<nom>_stack.tif`.
pub fn output_path_for(source_dir: &Path) -> PathBuf {
    let stem = source_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("stack");
    source_dir
        .join("hyperion_out")
        .join(format!("{stem}_stack.tif"))
}

/// Chemin du stack **brut** (avant post-traitement) :
/// `<source>/hyperion_out/<nom>_stack_brut.tif`.
///
/// Le fichier n'existe que si le post-traitement est active : il sert a
/// comparer l'image finale avec l'empilement pur, depuis l'interface.
pub fn raw_output_path_for(source_dir: &Path) -> PathBuf {
    let stem = source_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("stack");
    source_dir
        .join("hyperion_out")
        .join(format!("{stem}_stack_brut.tif"))
}

/// Ecrit un TIFF 16-bit a un emplacement donne, en creant le dossier parent.
pub fn write_tiff(path: &Path, rgb16: &[u16], width: usize, height: usize) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creation du dossier de sortie {}", parent.display()))?;
    }
    write_rgb16_tiff(path, rgb16, width, height)
}

/// Ecrit le stack final et le rapport associe (a cote du TIFF).
pub fn write_output(
    tiff_path: &Path,
    rgb16: &[u16],
    width: usize,
    height: usize,
    report: &StackReport,
) -> Result<()> {
    if let Some(parent) = tiff_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creation du dossier de sortie {}", parent.display()))?;
    }

    write_rgb16_tiff(tiff_path, rgb16, width, height)?;

    let report_path = tiff_path.with_extension("json");
    let data = serde_json::to_string_pretty(report)?;
    std::fs::write(&report_path, data)
        .with_context(|| format!("ecriture du rapport {}", report_path.display()))?;

    Ok(())
}

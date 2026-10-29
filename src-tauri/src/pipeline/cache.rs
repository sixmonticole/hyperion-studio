//! Gestion du dossier de cache TIFF 16-bit.
//!
//! Un RAW dematriçe est un TIFF 16-bit RGB non compresse ecrit une seule fois.
//! Un manifeste JSON (`manifest.json`) memorise la correspondance RAW -> TIFF
//! et permet de reprendre une session sans redematriçàge.

use crate::model::FrameInfo;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub source_dir: String,
    pub version: u32,
    pub frames: Vec<FrameInfo>,
}

impl Manifest {
    /// Nom du fichier manifeste dans le dossier de cache.
    pub const FILE: &'static str = "manifest.json";

    /// Chemin du manifeste pour un dossier de cache donne.
    pub fn path_in(cache_dir: &Path) -> PathBuf {
        cache_dir.join(Self::FILE)
    }

    /// Charge un manifeste existant, s'il est valide.
    pub fn load(cache_dir: &Path) -> Option<Self> {
        let path = Self::path_in(cache_dir);
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Ecrit le manifeste sur disque.
    pub fn save(&self, cache_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(cache_dir)
            .with_context(|| format!("creation du cache {}", cache_dir.display()))?;
        let path = Self::path_in(cache_dir);
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, data)
            .with_context(|| format!("ecriture du manifeste {}", path.display()))?;
        Ok(())
    }

    /// Indique si le TIFF de cache existe deja et n'est pas vide.
    #[allow(dead_code)]
    pub fn is_cached(cache_path: &str) -> bool {
        std::fs::metadata(cache_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
    }
}

/// Derive un chemin de TIFF de cache a partir d'un RAW source et d'un index.
pub fn cache_path_for(cache_dir: &Path, raw_path: &Path, index: usize) -> PathBuf {
    let stem = raw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("frame");
    cache_dir.join(format!("{index:05}_{stem}.tif"))
}

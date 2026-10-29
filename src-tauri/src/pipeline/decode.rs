//! Decodage des RAW (ARW Sony & autres) via `rawler`, puis ecriture d'un TIFF
//! 16-bit RGB dans le cache disque.
//!
//! Dematriçage : on s'appuie sur le dematriçeur **PPG** (Patterned Pixel
//! Grouping) de `rawler`, nettement plus fin que le bilineaire (meilleure
//! restitution des details fins, pas de quadrillage visible en zoom).
//!
//! Pipeline applique, dans l'ordre :
//! 1. Rescale (niveaux noir/blanc) ;
//! 2. Demosaic PPG vers RGB lineaire ;
//! 3. Crop sur la zone active du capteur ;
//! 4. Balance des blancs automatique (gray-world) : la Lune et les planetes sont
//!    des sources grises, on neutralise donc la temperature de couleur du
//!    boitier au lieu de l'appliquer telle quelle.

use anyhow::{Context, Result, bail};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Resultat du decodage d'un RAW.
pub struct DecodedRaw {
    /// Pixels RGB entrelaces en 16-bit (longueur = w * h * 3).
    pub rgb16: Vec<u16>,
    pub width: usize,
    pub height: usize,
    /// Modele de boitier tel qu'encode dans le fichier.
    pub model: String,
}

/// Decode un RAW et renvoie l'image RVB 16-bit dematriçee.
pub fn decode_raw_to_rgb16(path: &Path) -> Result<DecodedRaw> {
    let raw = rawler::decode_file(path)
        .with_context(|| format!("rawler: echec de decodage de {}", path.display()))?;

    let model = raw.model.clone();

    // Etape 1-3 : rescale, dematriçage PPG, crop sur la zone active.
    // On n'applique ni la balance des blancs du boitier ni la courbe sRGB :
    // l'image reste lineaire, ce qui est le bon point de depart pour un stack
    // planetaire (le gamma et la balance sont geres plus tard, explicitement).
    let develop = RawDevelop::new_with(&[
        ProcessingStep::Rescale,
        ProcessingStep::Demosaic,
        ProcessingStep::CropActiveArea,
    ]);

    let intermediate = develop
        .develop_intermediate(&raw)
        .map_err(|e| anyhow::anyhow!("developpement RAW impossible pour {}: {e}", path.display()))?;

    let (mut data, width, height) = match intermediate {
        Intermediate::ThreeColor(rgb) => {
            let dim = rgb.dim();
            let flat = rgb.flatten();
            (flat, dim.w as usize, dim.h as usize)
        }
        Intermediate::Monochrome(px) => {
            // Capteur monochrome : on duplique la luminance sur les 3 canaux.
            let dim = px.dim();
            let mut rgb = Vec::with_capacity(px.data.len() * 3);
            for &v in &px.data {
                rgb.push(v);
                rgb.push(v);
                rgb.push(v);
            }
            (rgb, dim.w as usize, dim.h as usize)
        }
        Intermediate::FourColor(_) => bail!(
            "capteur a quatre couleurs non supporte ({})",
            path.display()
        ),
    };

    // Etape 4 : balance des blancs automatique.
    neutralize_gray_world(&mut data, width, height);

    // Conversion f32 lineaire [0,1] -> u16.
    let rgb16: Vec<u16> = data
        .iter()
        .map(|&v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
        .collect();

    Ok(DecodedRaw {
        rgb16,
        width,
        height,
        model,
    })
}

/// Ecrit une image RGB 16-bit entrelacee dans un fichier TIFF non compresse.
///
/// L'ecriture est **atomique** : on ecrit d'abord dans un fichier temporaire du
/// meme dossier, puis on le renomme. Un plantage ou une coupure en cours
/// d'ecriture laisse ainsi un `.tmp` inoffensif, jamais un TIFF tronque qui
/// serait pris pour un cache valide au prochain run.
pub fn write_rgb16_tiff(path: &Path, rgb16: &[u16], width: usize, height: usize) -> Result<()> {
    let tmp_path = tmp_path_for(path);
    {
        let file =
            File::create(&tmp_path).with_context(|| format!("creation du TIFF {}", tmp_path.display()))?;
        let mut writer = BufWriter::new(file);

        let mut encoder = tiff::encoder::TiffEncoder::new(&mut writer)
            .with_context(|| format!("init encodeur TIFF {}", tmp_path.display()))?;
        encoder
            .write_image::<tiff::encoder::colortype::RGB16>(width as u32, height as u32, rgb16)
            .with_context(|| format!("ecriture TIFF {}", tmp_path.display()))?;

        // Force la vidange du tampon avant le renommage.
        writer
            .flush()
            .with_context(|| format!("vidange du TIFF {}", tmp_path.display()))?;
    }

    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "renommage atomique {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

/// Chemin temporaire associe a un TIFF de cache (meme dossier, suffixe `.tmp`).
fn tmp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("frame.tif")
        .to_string();
    name.push_str(".tmp");
    path.with_file_name(name)
}

/// Lit une image RGB 16-bit depuis un TIFF du cache.
pub fn read_rgb16_tiff(path: &Path) -> Result<(Vec<u16>, usize, usize)> {
    let file =
        File::open(path).with_context(|| format!("ouverture du TIFF {}", path.display()))?;
    let mut decoder = tiff::decoder::Decoder::new(file)
        .with_context(|| format!("init decodeur TIFF {}", path.display()))?;
    let (width, height) = decoder.dimensions()?;
    let image = decoder.read_image()?;
    match image {
        tiff::decoder::DecodingResult::U16(v) => Ok((v, width as usize, height as usize)),
        _ => bail!("TIFF {} n'est pas en 16-bit", path.display()),
    }
}

/// Charge une image RGB 16-bit depuis un fichier, qu'il soit TIFF (deja
/// dematriçe) ou RAW (decode a la volee). Renvoie aussi le modele si connu.
pub fn load_rgb16_any(path: &Path) -> Result<LoadedImage> {
    if is_tiff(path) {
        let (rgb16, width, height) = read_rgb16_tiff(path)?;
        Ok(LoadedImage {
            rgb16,
            width,
            height,
            model: None,
        })
    } else {
        let d = decode_raw_to_rgb16(path)?;
        Ok(LoadedImage {
            rgb16: d.rgb16,
            width: d.width,
            height: d.height,
            model: Some(d.model),
        })
    }
}

/// Image chargee, avec le modele de boitier si l'info est disponible.
pub struct LoadedImage {
    pub rgb16: Vec<u16>,
    pub width: usize,
    pub height: usize,
    pub model: Option<String>,
}

/// Indique si un fichier est un TIFF (deja dematriçe, donc pas de RAW a decoder).
pub fn is_tiff(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "tif" | "tiff"))
        .unwrap_or(false)
}

/// Egalise les moyennes des canaux R, V, B (hypothese gray-world).
///
/// Sur la Lune et les planetes, la scene est globalement grise : si une couleur
/// domine, c'est un biais de temperature du boitier. On calcule la moyenne de
/// chaque canal et on ramene tout vers une cible commune, en conservant la
/// dynamique. Les pixels tres sombres (fond de ciel) sont ignores pour ne pas
/// fausser la mesure.
fn neutralize_gray_world(rgb: &mut [f32], _width: usize, _height: usize) {
    let n_px = rgb.len() / 3;
    if n_px == 0 {
        return;
    }

    let mut lum_sum = 0.0f64;
    for px in rgb.chunks_exact(3) {
        lum_sum += 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
    }
    let lum_mean = lum_sum / n_px as f64;
    let lum_cut = lum_mean * 0.5;

    let mut sum = [0.0f64; 3];
    let mut count = 0.0f64;
    for px in rgb.chunks_exact(3) {
        let l = 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
        if (l as f64) < lum_cut {
            continue;
        }
        sum[0] += px[0] as f64;
        sum[1] += px[1] as f64;
        sum[2] += px[2] as f64;
        count += 1.0;
    }
    if count <= 0.0 {
        return;
    }

    let mean = [sum[0] / count, sum[1] / count, sum[2] / count];
    let target = (mean[0] + mean[1] + mean[2]) / 3.0;
    if target <= 0.0 {
        return;
    }

    let clamp_gain = |g: f64| g.clamp(0.5, 2.0);
    let gain = [
        clamp_gain(target / mean[0].max(1e-6)),
        clamp_gain(target / mean[1].max(1e-6)),
        clamp_gain(target / mean[2].max(1e-6)),
    ];

    for px in rgb.chunks_exact_mut(3) {
        px[0] = (px[0] as f64 * gain[0]) as f32;
        px[1] = (px[1] as f64 * gain[1]) as f32;
        px[2] = (px[2] as f64 * gain[2]) as f32;
    }
}

/// Renvoie la liste triee des fichiers RAW d'un dossier (extensions courantes).
///
/// Les sous-dossiers de sortie du pipeline (`hyperion_out`) et de cache
/// (`.hyperion_cache`) sont ignores, pour qu'une relecture ne les prenne pas
/// pour de nouvelles frames.
pub fn list_raw_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("lecture du dossier {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains("_stack"))
                    .unwrap_or(false)
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| {
                        matches!(
                            e.to_ascii_lowercase().as_str(),
                            "arw" | "tif" | "tiff" | "dng" | "cr2" | "cr3" | "nef" | "raf" | "orf"
                        )
                    })
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    Ok(files)
}

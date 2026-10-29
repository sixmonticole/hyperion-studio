//! Orchestrateur du pipeline : import -> detection -> qualite -> alignement -> stack -> export.

pub mod align;
pub mod ap;
pub mod similarity;
pub mod cache;
pub mod decode;
pub mod detect;
pub mod enhance;
pub mod output;
pub mod preview;
pub mod quality;
pub mod stack;

#[cfg(test)]
mod tests;

use crate::model::{CropRect, FrameInfo, Phase, Progress, Settings, SkippedFrame, StackReport};
use anyhow::{Result, bail};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Callback de progression (envoye vers l'UI Tauri).
pub type ProgressFn = dyn Fn(Progress) + Send + Sync;

/// Point d'entree : traite un dossier complet et renvoie le rapport final.
///
/// `progress` est appele depuis plusieurs threads ; il doit etre thread-safe
/// (en pratique, il emet un evenement Tauri qui l'est).
pub fn run_pipeline(
    source_dir: &Path,
    cache_dir: &Path,
    settings: &Settings,
    progress: &ProgressFn,
) -> Result<StackReport> {
    // --- 1. Import : liste les RAW et les dematriçe dans le cache. ---
    progress(Progress::Phase {
        phase: Phase::Importing,
        message: "Recherche des fichiers RAW...".into(),
    });

    let raw_files = decode::list_raw_files(source_dir)?;
    if raw_files.is_empty() {
        bail!("aucun fichier RAW trouve dans {}", source_dir.display());
    }
    let total = raw_files.len();
    std::fs::create_dir_all(cache_dir)?;

    // Manifeste precedent (meme dossier source) : charge pour info/tracabilite.

    // Chaque frame est importee independamment. Une frame illisible est
    // ecartee et signalee, mais **ne fait pas echouer la session** : sur une
    // rafale de 200 poses, perdre tout le traitement pour un fichier corrompu
    // serait absurde.
    let done = Mutex::new(0usize);
    let skipped: Mutex<Vec<SkippedFrame>> = Mutex::new(Vec::new());

    let imported: Vec<Option<FrameInfo>> = raw_files
        .par_iter()
        .enumerate()
        .map(|(index, raw_path)| -> Option<FrameInfo> {
            let name = raw_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();

            // Cas particulier : le fichier source est deja un TIFF 16-bit
            // (dossier prealablement converti, ex. sortie de PIPP/Siril).
            // On l'utilise directement, sans le recopier dans le cache.
            let result = if decode::is_tiff(raw_path) {
                // On verifie qu'il est bien lisible avant de l'integrer : un
                // TIFF tronque casserait l'analyse en aval.
                match decode::read_rgb16_tiff(raw_path) {
                    Ok(_) => Ok(FrameInfo::new(
                        raw_path.to_string_lossy().to_string(),
                        raw_path.to_string_lossy().to_string(),
                        index,
                    )),
                    Err(e) => Err(e.to_string()),
                }
            } else {
                let cache_path = cache::cache_path_for(cache_dir, raw_path, index);
                let cache_str = cache_path.to_string_lossy().to_string();

                // Reutilise le cache s'il existe deja et n'est pas vide. Un
                // fichier de taille nulle est le vestige d'une ecriture
                // interrompue : on le re-decode plutot que de le croire valide.
                let cache_ok = std::fs::metadata(&cache_path)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false);

                if cache_ok {
                    Ok(FrameInfo::new(
                        raw_path.to_string_lossy().to_string(),
                        cache_str,
                        index,
                    ))
                } else {
                    match decode::decode_raw_to_rgb16(raw_path).and_then(|decoded| {
                        decode::write_rgb16_tiff(
                            &cache_path,
                            &decoded.rgb16,
                            decoded.width,
                            decoded.height,
                        )
                    }) {
                        Ok(()) => Ok(FrameInfo::new(
                            raw_path.to_string_lossy().to_string(),
                            cache_str,
                            index,
                        )),
                        Err(e) => Err(format!("{e:#}")),
                    }
                }
            };

            let mut d = done.lock().unwrap();
            *d += 1;
            progress(Progress::Import { done: *d, total });

            match result {
                Ok(frame) => Some(frame),
                Err(reason) => {
                    progress(Progress::Warning {
                        name: name.clone(),
                        reason: reason.clone(),
                    });
                    skipped.lock().unwrap().push(SkippedFrame { name, reason });
                    None
                }
            }
        })
        .collect();

    let skipped: Vec<SkippedFrame> = skipped.into_inner().unwrap();
    let mut frames: Vec<FrameInfo> = imported.into_iter().flatten().collect();
    frames.sort_by_key(|f| f.index);

    if frames.is_empty() {
        bail!(
            "aucune frame exploitable sur {} fichier(s) : toutes illisibles",
            total
        );
    }
    if frames.len() < 3 {
        bail!(
            "seulement {} frame(s) exploitable(s) sur {} : trop peu pour empiler",
            frames.len(),
            total
        );
    }

    // Manifeste : trace la correspondance RAW -> TIFF et permet de reprendre.
    let manifest = cache::Manifest {
        source_dir: source_dir.to_string_lossy().to_string(),
        version: 1,
        frames: frames.clone(),
    };
    manifest.save(cache_dir)?;

    // --- 2. Detection de la planete sur une frame de reference. ---
    progress(Progress::Phase {
        phase: Phase::Analyzing,
        message: "Detection de la planete...".into(),
    });

    let ref_tiff = PathBuf::from(&frames[0].cache_path);
    let (_ref_rgb, full_w, full_h) = decode::read_rgb16_tiff(&ref_tiff)?;

    // Boite englobante de l'objet pour dimensionner le crop.
    //
    // **Empilement par blocs** (defaut) : chaque bloc est aligne independamment.
    // La fenetre doit contenir l'objet dans **tous les blocs**, mais pas au long
    // de la derive continue frame-a-frame (qui atteint des centaines de pixels) :
    // on echantillonne une frame par bloc et on prend leur union.
    //
    // **Empilement global** : toutes les frames sont alignees sur une reference
    // unique ; on prend l'union sur toute la session, sinon l'objet sortirait du
    // cadre sur les frames les plus derivees.
    let detection = if settings.block_size > 0 {
        let bs = settings.block_size.max(1);
        // Une frame de reference par bloc (la premiere de chaque bloc).
        let by_block: Vec<FrameInfo> = frames
            .iter()
            .step_by(bs)
            .cloned()
            .collect();
        let sample = if by_block.is_empty() { &frames[..] } else { &by_block[..] };
        union_detection(sample, full_w, full_h)?
            .or_else(|| detect::detect_planet(&_ref_rgb, full_w, full_h, None))
            .ok_or_else(|| anyhow::anyhow!("objet non detecte sur les blocs"))?
    } else {
        union_detection(&frames, full_w, full_h)?
            .ok_or_else(|| anyhow::anyhow!("objet non detecte sur la session"))?
    };

    // Marge de rotation de champ : un point a distance R du centre se deplace de
    // R * angle ; on previsionne une rotation d'environ 1 degre.
    let rotation_margin = if settings.derotate {
        let diag = ((detection.bbox.2 - detection.bbox.0).max(detection.bbox.3 - detection.bbox.1))
            as f64;
        diag * 0.5 * 0.02 // ~1 degre ~= 0.0175 rad, arrondi a 2%.
    } else {
        0.0
    };
    let crop: CropRect =
        detect::crop_window(&detection, full_w, full_h, settings.crop_size, rotation_margin);

    // --- 3. Crop + score qualite de chaque frame, en parallele. ---
    let (crop_w, crop_h) = (crop.width, crop.height);
    let n_frames = frames.len();
    let done = Mutex::new(0usize);
    let extra_skipped: Mutex<Vec<SkippedFrame>> = Mutex::new(Vec::new());
    let scored: Vec<(usize, f64)> = frames
        .par_iter()
        .filter_map(|f| -> Option<(usize, f64)> {
            let name = f
                .source_path
                .rsplit('/')
                .next()
                .unwrap_or(&f.source_path)
                .to_string();

            let score = match decode::read_rgb16_tiff(Path::new(&f.cache_path)) {
                Ok((rgb, w, h)) => {
                    let cropped = crop_rgb16(&rgb, w, h, &crop);
                    Some(quality::sharpness_score(&cropped, crop_w, crop_h))
                }
                Err(e) => {
                    // Le TIFF de cache est devenu illisible : on ecarte la
                    // frame plutot que d'interrompre toute la session.
                    progress(Progress::Warning {
                        name: name.clone(),
                        reason: format!("{e:#}"),
                    });
                    extra_skipped
                        .lock()
                        .unwrap()
                        .push(SkippedFrame { name, reason: format!("{e:#}") });
                    None
                }
            };

            let mut d = done.lock().unwrap();
            *d += 1;
            progress(Progress::Analyze { done: *d, total: n_frames });
            score.map(|s| (f.index, s))
        })
        .collect();

    // Les frames ecartees a l'analyse sortent du lot.
    let extra_skipped = extra_skipped.into_inner().unwrap();
    if !extra_skipped.is_empty() {
        let bad: std::collections::HashSet<usize> = frames
            .iter()
            .filter(|f| {
                extra_skipped
                    .iter()
                    .any(|s| s.name == f.source_path.rsplit('/').next().unwrap_or(""))
            })
            .map(|f| f.index)
            .collect();
        frames.retain(|f| !bad.contains(&f.index));
    }
    let mut skipped = skipped;
    skipped.extend(extra_skipped);

    if frames.is_empty() {
        bail!("aucune frame exploitable apres analyse : toutes illisibles");
    }
    let total = frames.len();

    // Normalisation des scores + selection des meilleures frames.
    let raw_scores: Vec<f64> = scored.iter().map(|(_, s)| *s).collect();
    let norm = quality::normalize_scores(&raw_scores);
    let score_by_index: std::collections::HashMap<usize, f64> = scored
        .iter()
        .map(|(i, _)| *i)
        .zip(norm.iter().cloned())
        .collect();

    for f in frames.iter_mut() {
        f.quality = score_by_index.get(&f.index).cloned();
    }

    // Seuil : on garde les `keep_fraction` meilleures par score normalise.
    let mut ordered = norm.clone();
    ordered.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let keep_count = ((total as f64 * settings.keep_fraction).ceil() as usize).clamp(1, total);
    let threshold = ordered[keep_count - 1];

    for f in frames.iter_mut() {
        f.keep = f.quality.map(|q| q >= threshold).unwrap_or(false);
    }

    let kept: Vec<&FrameInfo> = frames.iter().filter(|f| f.keep).collect();
    let kept_count = kept.len();

    // --- 4. Alignement + empilement. ---
    progress(Progress::Phase {
        phase: Phase::Stacking,
        message: format!("Empilement de {kept_count} frames..."),
    });

    // Frame de reference pour l'alignement : la meilleure par score.
    let best = frames
        .iter()
        .filter(|f| f.keep)
        .max_by(|a, b| {
            a.quality
                .unwrap_or(0.0)
                .partial_cmp(&b.quality.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("au moins une frame gardee");

    let (best_rgb, _, _) = decode::read_rgb16_tiff(Path::new(&best.cache_path))?;
    let best_crop = crop_rgb16(&best_rgb, full_w, full_h, &crop);
    let pixels = crop_w * crop_h;

    let ref_lum = align::luminance(&best_crop, pixels);

    // Alignement par similarite (translation + rotation + echelle) : corrige la
    // rotation de champ, incontournable en trepied fixe sur une longue session.
    let sim_aligner = if settings.align && settings.derotate {
        Some(similarity::SimilarityAligner::new(&ref_lum, crop_w, crop_h, 300))
    } else {
        None
    };

    // Alignement global par translation, en repli (ou si la derotation est off).
    let aligner = if settings.align && sim_aligner.is_none() {
        Some(align::Aligner::new(&ref_lum, crop_w, crop_h))
    } else {
        None
    };

    // Aligneur a points locaux (AP), construit sur la reference.
    let step = if settings.ap_size > 0 {
        settings.ap_size
    } else {
        64
    };
    let ap_aligner = if settings.align && settings.ap_enabled {
        Some(ap::ApAligner::new(&ref_lum, crop_w, crop_h, step))
    } else {
        None
    };

    if let Some(s) = &sim_aligner {
        progress(Progress::Phase {
            phase: Phase::Stacking,
            message: format!(
                "Empilement avec derotation ({} points de controle)...",
                s.point_count()
            ),
        });
    }

    let result = if settings.block_size > 0 {
        // --- Empilement par blocs ---
        //
        // Sur une longue session en trepied fixe, la Lune tourne et son
        // eclairage change : empiler des frames tres eloignees dans le temps
        // moyenne des images qui ne se superposent plus, et le resultat est
        // flou. On empile donc par **blocs de frames consecutives** (intra-bloc
        // la rotation est negligeable), puis on aligne et moyenne les blocs.
        stack_blockwise(
            &kept,
            &crop,
            pixels,
            settings,
            progress,
            kept_count,
            settings.block_size,
        )?
    } else if sim_aligner.is_some() {
        // Repli : alignement par similarite sur l'ensemble (experimental).
        stack_global(
            &kept, &crop, pixels, settings, progress, kept_count, None, sim_aligner.as_ref(),
            None,
        )?
    } else {
        // Repli : empilement global aligne (sessions courtes uniquement).
        stack_global(
            &kept, &crop, pixels, settings, progress, kept_count, aligner.as_ref(),
            sim_aligner.as_ref(), ap_aligner.as_ref(),
        )?
    };

    // Orientation finale (rotation / miroir) : l'EXIF ne peut pas connaitre le
    // retournement introduit par le telescope, c'est donc un reglage explicite.
    let (result, out_w, out_h) =
        align::apply_orientation(&result, crop_w, crop_h, settings.orientation);

    // --- 4. Ecriture du stack brut (avant post-traitement). ---
    // On l'ecrit uniquement si le post-traitement est active : sinon le fichier
    // serait strictement identique a l'image finale.
    let raw_path = if settings.enhance.enabled {
        let p = output::raw_output_path_for(source_dir);
        output::write_tiff(&p, &result, out_w, out_h)?;
        Some(p)
    } else {
        None
    };

    // Post-traitement final (ondelettes, niveaux, CLAHE, clarte).
    let result = if settings.enhance.enabled {
        progress(Progress::Phase {
            phase: Phase::Enhancing,
            message: "Renforcement de la nettete et du contraste...".into(),
        });
        enhance::enhance(&result, out_w, out_h, &settings.enhance)
    } else {
        result
    };

    // --- 5. Export. ---
    progress(Progress::Phase {
        phase: Phase::Done,
        message: "Ecriture du fichier final...".into(),
    });

    let mean_quality = frames
        .iter()
        .filter(|f| f.keep)
        .filter_map(|f| f.quality)
        .sum::<f64>()
        / kept_count.max(1) as f64;

    let out_path = output::output_path_for(source_dir);

    let report = StackReport {
        source_dir: source_dir.to_string_lossy().to_string(),
        output_path: out_path.to_string_lossy().to_string(),
        raw_output_path: raw_path.map(|p| p.to_string_lossy().to_string()),
        total_frames: total,
        kept_frames: kept_count,
        mean_quality,
        crop,
        settings: settings.clone(),
        skipped: skipped.clone(),
    };

    output::write_output(&out_path, &result, out_w, out_h, &report)?;

    progress(Progress::Finished {
        report: Box::new(report.clone()),
    });

    Ok(report)
}

/// Empile une liste de frames en un seul stack aligne (translation), en
/// parallelisant sur les frames et en serialisant l'accumulation.
///
/// Renvoie le stack RGB 16-bit dans la fenetre de crop.
fn stack_frame_set(
    frames: &[&FrameInfo],
    crop: &CropRect,
    pixels: usize,
    settings: &Settings,
    aligner: Option<&align::Aligner>,
    ap_aligner: Option<&ap::ApAligner>,
) -> Result<Vec<u16>> {
    let (crop_w, crop_h) = (crop.width, crop.height);
    let mut acc = stack::Accumulator::new(crop_w, crop_h);
    let acc_lock = Mutex::new(&mut acc);

    frames.par_iter().try_for_each(|f| -> Result<()> {
        let (rgb, w, h) = decode::read_rgb16_tiff(Path::new(&f.cache_path))?;
        let cropped = crop_rgb16(&rgb, w, h, crop);

        // Alignement (translation) puis AP si demande.
        let (aligned, valid) = if let Some(al) = aligner {
            let lum = align::luminance(&cropped, pixels);
            let shift = al.estimate(&lum);
            let (a, m) = align::shift_rgb16_masked(&cropped, crop_w, crop_h, shift);
            if let Some(apx) = ap_aligner {
                let lum2 = align::luminance(&a, pixels);
                match apx.align_frame(&lum2, &a) {
                    Some(a2) => (a2, m),
                    None => (a, m),
                }
            } else {
                (a, m)
            }
        } else {
            (cropped, vec![1u8; pixels])
        };

        let weight = if settings.weighted {
            f.quality.unwrap_or(0.5) as f32
        } else {
            1.0
        };
        let scale = if settings.normalize {
            let m = stack::mean_luminance(&aligned);
            if m > 1.0 { 8000.0 / m } else { 1.0 }
        } else {
            1.0
        };

        let mut guard = acc_lock.lock().unwrap();
        guard.add(&aligned, Some(&valid), weight, scale);
        Ok(())
    })?;

    drop(acc_lock);
    Ok(acc.finish())
}

/// Empile la session **par blocs de frames consecutives**, puis moyenne les
/// blocs entre eux.
///
/// C'est la strategie recommandee : intra-bloc, la rotation de champ est
/// negligeable, donc chaque bloc est net ; les blocs etant peu nombreux, leur
/// alignement mutuel est robuste malgre la grande derive globale.
#[allow(clippy::too_many_arguments)]
fn stack_blockwise(
    kept: &[&FrameInfo],
    crop: &CropRect,
    pixels: usize,
    settings: &Settings,
    progress: &ProgressFn,
    kept_count: usize,
    block_size: usize,
) -> Result<Vec<u16>> {
    let (crop_w, crop_h) = (crop.width, crop.height);

    // Ordre temporel : les blocs doivent etre des **groupes consecutifs**.
    let mut order: Vec<&FrameInfo> = kept.to_vec();
    order.sort_by_key(|f| f.index);

    let n_blocks = order.len().div_ceil(block_size).max(1);
    progress(Progress::Phase {
        phase: Phase::Stacking,
        message: format!(
            "Empilement par blocs : {n_blocks} blocs de {block_size} frames..."
        ),
    });

    // Empile chaque bloc (alignement interne sur sa meilleure frame).
    let total_for_progress = kept_count.max(1);
    let done_blocks = Mutex::new(0usize);
    let mut blocks: Vec<Vec<u16>> = Vec::with_capacity(n_blocks);

    for chunk in order.chunks(block_size) {
        // Reference du bloc : sa meilleure frame par score.
        let best = chunk
            .iter()
            .max_by(|a, b| {
                a.quality
                    .unwrap_or(0.0)
                    .partial_cmp(&b.quality.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or(chunk[0]);
        let (brgb, bw, bh) = decode::read_rgb16_tiff(Path::new(&best.cache_path))?;
        let bcrop = crop_rgb16(&brgb, bw, bh, crop);
        let ref_lum = align::luminance(&bcrop, pixels);
        let block_aligner = if settings.align {
            Some(align::Aligner::new(&ref_lum, crop_w, crop_h))
        } else {
            None
        };

        blocks.push(stack_frame_set(
            chunk,
            crop,
            pixels,
            settings,
            block_aligner.as_ref(),
            None,
        )?);

        let mut d = done_blocks.lock().unwrap();
        *d += chunk.len();
        progress(Progress::Stack {
            done: (*d).min(total_for_progress),
            total: total_for_progress,
        });
    }

    // Fusion des blocs : on les aligne mutuellement (sur le premier) et on les
    // moyenne. Peu de blocs => alignement robuste.
    let ref_lum = align::luminance(&blocks[0], pixels);
    let merger = if settings.align {
        Some(align::Aligner::new(&ref_lum, crop_w, crop_h))
    } else {
        None
    };

    let mut acc = stack::Accumulator::new(crop_w, crop_h);
    for (bi, blk) in blocks.iter().enumerate() {
        let (aligned, mask) = if bi == 0 {
            (blk.clone(), vec![1u8; pixels])
        } else if let Some(m) = &merger {
            let lum = align::luminance(blk, pixels);
            let shift = m.estimate(&lum);
            align::shift_rgb16_masked(blk, crop_w, crop_h, shift)
        } else {
            (blk.clone(), vec![1u8; pixels])
        };
        let scale = if settings.normalize {
            let ml = stack::mean_luminance(&aligned);
            if ml > 1.0 { 8000.0 / ml } else { 1.0 }
        } else {
            1.0
        };
        acc.add(&aligned, Some(&mask), 1.0, scale);
    }

    Ok(acc.finish())
}

/// Empilement global (repli) : aligne chaque frame directement sur la reference.
#[allow(clippy::too_many_arguments)]
fn stack_global(
    kept: &[&FrameInfo],
    crop: &CropRect,
    pixels: usize,
    settings: &Settings,
    progress: &ProgressFn,
    kept_count: usize,
    aligner: Option<&align::Aligner>,
    sim_aligner: Option<&similarity::SimilarityAligner>,
    ap_aligner: Option<&ap::ApAligner>,
) -> Result<Vec<u16>> {
    let (crop_w, crop_h) = (crop.width, crop.height);
    let mut acc = stack::Accumulator::new(crop_w, crop_h);
    let done = Mutex::new(0usize);
    let acc_lock = Mutex::new(&mut acc);

    kept.par_iter().try_for_each(|f| -> Result<()> {
        let (rgb, w, h) = decode::read_rgb16_tiff(Path::new(&f.cache_path))?;
        let cropped = crop_rgb16(&rgb, w, h, crop);
        let lum = align::luminance(&cropped, pixels);

        let (globally_aligned, valid): (Vec<u16>, Vec<u8>) = if let Some(sa) = sim_aligner {
            match sa.estimate(&lum) {
                Some(t) => similarity::warp_similarity_masked(&cropped, crop_w, crop_h, &t),
                None => (cropped, vec![1u8; pixels]),
            }
        } else if let Some(al) = aligner {
            let shift = al.estimate(&lum);
            align::shift_rgb16_masked(&cropped, crop_w, crop_h, shift)
        } else {
            (cropped, vec![1u8; pixels])
        };

        let (aligned, valid) = if let Some(apx) = ap_aligner {
            let lum2 = align::luminance(&globally_aligned, pixels);
            match apx.align_frame(&lum2, &globally_aligned) {
                Some(a) => (a, valid),
                None => (globally_aligned, valid),
            }
        } else {
            (globally_aligned, valid)
        };

        let weight = if settings.weighted {
            f.quality.unwrap_or(0.5) as f32
        } else {
            1.0
        };
        let scale = if settings.normalize {
            let m = stack::mean_luminance(&aligned);
            if m > 1.0 { 8000.0 / m } else { 1.0 }
        } else {
            1.0
        };
        let mut guard = acc_lock.lock().unwrap();
        guard.add(&aligned, Some(&valid), weight, scale);

        let mut d = done.lock().unwrap();
        *d += 1;
        progress(Progress::Stack { done: *d, total: kept_count.max(1) });
        Ok(())
    })?;

    drop(acc_lock);
    Ok(acc.finish())
}

/// Extrait une sous-fenetre RGB 16-bit (crop).
pub fn crop_rgb16(rgb16: &[u16], width: usize, height: usize, crop: &CropRect) -> Vec<u16> {
    let mut out = vec![0u16; crop.width * crop.height * 3];
    for y in 0..crop.height {
        let sy = crop.y + y;
        if sy >= height {
            break;
        }
        for x in 0..crop.width {
            let sx = crop.x + x;
            if sx >= width {
                break;
            }
            let si = (sy * width + sx) * 3;
            let di = (y * crop.width + x) * 3;
            out[di] = rgb16[si];
            out[di + 1] = rgb16[si + 1];
            out[di + 2] = rgb16[si + 2];
        }
    }
    out
}

/// Mesure l'etendue de l'objet sur plusieurs frames echantillonnees et renvoie
/// la boite englobante union, centree.
///
/// C'est ce qui dimensionne correctement la fenetre de crop en trepied fixe :
/// la fenetre doit contenir l'objet dans **toutes** les frames. Mesurer le
/// deplacement entre deux frames isolees (comme avant) surestimait la marge et
/// pouvait faire prendre l'image entiere.
fn union_detection(
    frames: &[FrameInfo],
    full_w: usize,
    full_h: usize,
) -> Result<Option<detect::Detection>> {
    if frames.is_empty() {
        return Ok(None);
    }

    // Echantillonne densement : la derive de la Lune n'est pas monotone (elle
    // oscille de plusieurs centaines de pixels), donc un echantillonnage clairseme
    // (9 points) ratait l'etendue reelle et le crop ne contenait pas l'objet dans
    // toutes les frames. On echantillonne au pas de ~n/24, borne a 60 points.
    let n = frames.len();
    let samples = n.min(60);
    let mut minx = usize::MAX;
    let mut miny = usize::MAX;
    let mut maxx = 0usize;
    let mut maxy = 0usize;
    let mut found = false;

    for k in 0..samples {
        let idx = if samples == 1 { 0 } else { k * (n - 1) / (samples - 1) };
        let (rgb, _, _) = decode::read_rgb16_tiff(Path::new(&frames[idx].cache_path))?;
        if let Some(d) = detect::detect_planet(&rgb, full_w, full_h, None) {
            minx = minx.min(d.bbox.0);
            miny = miny.min(d.bbox.1);
            maxx = maxx.max(d.bbox.2);
            maxy = maxy.max(d.bbox.3);
            found = true;
        }
    }

    if !found {
        return Ok(None);
    }

    let radius = ((maxx - minx).max(maxy - miny)) as f64 / 2.0;
    Ok(Some(detect::Detection {
        cx: (minx + maxx) as f64 / 2.0,
        cy: (miny + maxy) as f64 / 2.0,
        radius,
        bbox: (minx, miny, maxx, maxy),
    }))
}

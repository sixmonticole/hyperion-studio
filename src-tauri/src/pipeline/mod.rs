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

use crate::model::{
    CropRect, FrameInfo, Phase, Progress, Settings, SkipReason, SkippedFrame, StackReport, Stage,
};
use anyhow::{Result, bail};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Callback de progression (envoye vers l'UI Tauri).
pub type ProgressFn = dyn Fn(Progress) + Send + Sync;

/// Sentinel returned when the user cancels. The UI matches on this string to
/// distinguish a cancellation from a real failure.
pub const CANCELLED: &str = "__cancelled__";

/// Per-region selection context, shared by the stacking functions.
///
/// Holds the tile grid and, for each frame, the mask of tiles it is admitted
/// into. When `active` is false the whole feature is bypassed and stacking
/// behaves exactly as before (every kept frame contributes everywhere), so the
/// global path remains available as a reference.
struct TileSelect {
    tiles_x: usize,
    /// Tile side in pixels.
    tile: usize,
    /// Per frame index, the tile gate (1 = the frame contributes to that tile).
    gates: std::collections::HashMap<usize, Vec<u8>>,
    active: bool,
}

impl TileSelect {
    /// Builds the gate for one frame, or `None` when the frame should
    /// contribute everywhere (feature off, or frame not gated).
    fn gate_for(&self, frame_index: usize) -> Option<stack::TileGate<'_>> {
        if !self.active {
            return None;
        }
        self.gates.get(&frame_index).map(|g| stack::TileGate {
            gate: g,
            tiles_x: self.tiles_x,
            tile: self.tile,
        })
    }
}

/// Returns `Err` if cancellation was requested. Called at loop checkpoints so
/// a long run stops promptly rather than finishing useless work.
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("{}", CANCELLED);
    }
    Ok(())
}

/// Point d'entree : traite un dossier complet et renvoie le rapport final.
///
/// `progress` est appele depuis plusieurs threads ; il doit etre thread-safe
/// (en pratique, il emet un evenement Tauri qui l'est).
pub fn run_pipeline(
    source_dir: &Path,
    cache_dir: &Path,
    settings: &Settings,
    progress: &ProgressFn,
    cancel: &AtomicBool,
) -> Result<StackReport> {
    check_cancel(cancel)?;

    // --- 1. Import : liste les RAW et les dematriçe dans le cache. ---
    progress(Progress::Phase {
        phase: Phase::Importing,
        stage: Stage::ScanningFiles,
    });

    let raw_files = decode::list_raw_files(source_dir)?;
    if raw_files.is_empty() {
        bail!("no RAW file found in {}", source_dir.display());
    }
    let total = raw_files.len();
    std::fs::create_dir_all(cache_dir)?;

    // Previous manifest for the same source folder: used to detect sources
    // that changed since they were cached (same name, different bytes).
    let previous = cache::Manifest::load(cache_dir).filter(|m| m.source_dir == source_dir.to_string_lossy());

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
            // Cancellation is checked per frame: the UI stays responsive and a
            // cancel during a 200-frame import stops within one decode.
            if cancel.load(Ordering::Relaxed) {
                return None;
            }

            let name = raw_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();

            // Already a 16-bit TIFF (a previously converted folder, e.g. from
            // PIPP/Siril): used directly, without copying into the cache.
            let result = if decode::is_tiff(raw_path) {
                match decode::read_rgb16_tiff(raw_path) {
                    Ok(_) => Ok(FrameInfo::new(
                        raw_path.to_string_lossy().to_string(),
                        raw_path.to_string_lossy().to_string(),
                        index,
                    )),
                    Err(_) => Err(SkipReason::Unreadable),
                }
            } else {
                let cache_path = cache::cache_path_for(cache_dir, raw_path, index);
                let cache_str = cache_path.to_string_lossy().to_string();
                let source_str = raw_path.to_string_lossy().to_string();

                // A cached TIFF is reused only if it exists, is not empty, and
                // its source has not changed since. Same name + different size
                // means a different file, so the stale TIFF is rebuilt.
                let changed = previous
                    .as_ref()
                    .and_then(|m| m.find_by_source(&source_str))
                    .map(|old| {
                        let old_len = std::fs::metadata(&old.cache_path)
                            .map(|m| m.len())
                            .unwrap_or(0);
                        let src_len = std::fs::metadata(raw_path).map(|m| m.len()).unwrap_or(0);
                        old_len != src_len
                    })
                    .unwrap_or(false);

                if cache::is_usable(&cache_path) && !changed {
                    Ok(FrameInfo::new(source_str, cache_str, index))
                } else {
                    match decode::decode_raw_to_rgb16(raw_path).and_then(|decoded| {
                        decode::write_rgb16_tiff(
                            &cache_path,
                            &decoded.rgb16,
                            decoded.width,
                            decoded.height,
                        )
                    }) {
                        Ok(()) => Ok(FrameInfo::new(source_str, cache_str, index)),
                        Err(_) => Err(SkipReason::Unreadable),
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

    // A cancel during import discards the partial result.
    check_cancel(cancel)?;

    let skipped: Vec<SkippedFrame> = skipped.into_inner().unwrap();
    let mut frames: Vec<FrameInfo> = imported.into_iter().flatten().collect();
    frames.sort_by_key(|f| f.index);

    if frames.is_empty() {
        bail!("no usable frame out of {total} file(s)");
    }
    if frames.len() < 3 {
        bail!("only {} usable frame(s) out of {total}: too few to stack", frames.len());
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
        stage: Stage::DetectingObject,
    });

    let ref_tiff = PathBuf::from(&frames[0].cache_path);
    let (_ref_rgb, full_w, full_h) = decode::read_rgb16_tiff(&ref_tiff)?;

    // Bounding box of the object, used to size the crop.
    //
    // **Blockwise stacking** (default): each block is aligned independently.
    // The window must contain the object in **every block**, but not across
    // the continuous frame-to-frame drift (which reaches hundreds of pixels):
    // so we sample one frame per block and take their union.
    //
    // **Global stacking**: all frames are aligned on a single reference; we
    // take the union over the whole session, otherwise the object would leave
    // the frame on the most drifted ones.
    let detection = if settings.block_size > 0 {
        let bs = settings.block_size.max(1);
        let by_block: Vec<FrameInfo> = frames.iter().step_by(bs).cloned().collect();
        let sample = if by_block.is_empty() { &frames[..] } else { &by_block[..] };
        union_detection(sample, full_w, full_h)?
            .or_else(|| detect::detect_planet(&_ref_rgb, full_w, full_h, None))
            .ok_or_else(|| anyhow::anyhow!("object not detected on the blocks"))?
    } else {
        union_detection(&frames, full_w, full_h)?
            .ok_or_else(|| anyhow::anyhow!("object not detected on the session"))?
    };

    // Field-rotation margin: a point at distance R from the centre moves by
    // R * angle; we allow for about 1 degree of rotation.
    let rotation_margin = if settings.derotate {
        let diag = ((detection.bbox.2 - detection.bbox.0).max(detection.bbox.3 - detection.bbox.1))
            as f64;
        diag * 0.5 * 0.02 // ~1 degree ~= 0.0175 rad, rounded to 2%.
    } else {
        0.0
    };
    let crop: CropRect =
        detect::crop_window(&detection, full_w, full_h, settings.crop_size, rotation_margin);

    // --- 3. Crop + quality score for each frame, in parallel. ---
    progress(Progress::Phase {
        phase: Phase::Analyzing,
        stage: Stage::ScoringFrames,
    });
    let (crop_w, crop_h) = (crop.width, crop.height);
    let n_frames = frames.len();
    let tile = if settings.tile_select {
        settings.tile_size.max(8)
    } else {
        0
    };
    let done = Mutex::new(0usize);
    let extra_skipped: Mutex<Vec<SkippedFrame>> = Mutex::new(Vec::new());
    let scored: Vec<(usize, quality::FrameScore)> = frames
        .par_iter()
        .filter_map(|f| -> Option<(usize, quality::FrameScore)> {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let name = f
                .source_path
                .rsplit('/')
                .next()
                .unwrap_or(&f.source_path)
                .to_string();

            let score = match decode::read_rgb16_tiff(Path::new(&f.cache_path)) {
                Ok((rgb, w, h)) => {
                    let cropped = crop_rgb16(&rgb, w, h, &crop);
                    Some(quality::score_frame(&cropped, crop_w, crop_h, tile))
                }
                Err(_) => {
                    // The cached TIFF became unreadable: skip the frame rather
                    // than abort the whole session.
                    progress(Progress::Warning {
                        name: name.clone(),
                        reason: SkipReason::MissingCache,
                    });
                    extra_skipped.lock().unwrap().push(SkippedFrame {
                        name,
                        reason: SkipReason::MissingCache,
                    });
                    None
                }
            };

            let mut d = done.lock().unwrap();
            *d += 1;
            progress(Progress::Analyze { done: *d, total: n_frames });
            score.map(|s| (f.index, s))
        })
        .collect();

    check_cancel(cancel)?;

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
    let raw_scores: Vec<f64> = scored.iter().map(|(_, s)| s.global).collect();
    let norm = quality::normalize_scores(&raw_scores);
    let score_by_index: std::collections::HashMap<usize, f64> = scored
        .iter()
        .map(|(i, _)| *i)
        .zip(norm.iter().cloned())
        .collect();

    // On conserve aussi les scores par tuile, qui servent a la selection par
    // zone. Le nombre de tuiles est commun a toutes les frames.
    let tiles_x = scored.first().map(|(_, s)| s.tiles_x).unwrap_or(1);
    let tiles_y = scored.first().map(|(_, s)| s.tiles_y).unwrap_or(1);
    let n_tiles = tiles_x * tiles_y;
    let mut tile_by_index: std::collections::HashMap<usize, Vec<f32>> =
        std::collections::HashMap::with_capacity(scored.len());
    for (i, s) in &scored {
        tile_by_index.insert(*i, s.tiles.clone());
    }

    for f in frames.iter_mut() {
        f.quality = score_by_index.get(&f.index).cloned();
        f.tiles_x = tiles_x;
        f.tile_scores = tile_by_index.get(&f.index).cloned().unwrap_or_default();
    }

    // Seuil global : on garde les `keep_fraction` meilleures par score normalise.
    let mut ordered = norm.clone();
    ordered.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let keep_count = ((total as f64 * settings.keep_fraction).ceil() as usize).clamp(1, total);
    let threshold = ordered[keep_count - 1];

    for f in frames.iter_mut() {
        f.keep = f.quality.map(|q| q >= threshold).unwrap_or(false);
    }

    // --- Selection par zone ------------------------------------------------
    //
    // Pour chaque tuile, on classe les frames par leur score **sur cette
    // tuile** et on retient les meilleures. Une frame peut donc etre retenue
    // au terminateur et ecartee au limbe, et inversement. C'est ce qui permet
    // de tirer chaque region de l'image des frames nettes a cet endroit.
    //
    // On stocke le resultat sous forme de gate par frame : pour chaque frame,
    // le masque des tuiles ou elle est admise.
    let mut frame_gates: std::collections::HashMap<usize, Vec<u8>> =
        std::collections::HashMap::new();
    let tile_select_active = settings.tile_select && n_tiles > 1 && !scored.is_empty();

    if tile_select_active {
        // keep_count par tuile, borne par le nombre de frames disponibles.
        let per_tile_keep = keep_count.min(scored.len()).max(1);
        // Collecte des scores par tuile.
        let mut per_tile: Vec<Vec<(usize, f32)>> = vec![Vec::with_capacity(scored.len()); n_tiles];
        for (idx, s) in &scored {
            for t in 0..n_tiles.min(s.tiles.len()) {
                per_tile[t].push((*idx, s.tiles[t]));
            }
        }

        for t in 0..n_tiles {
            let col = &mut per_tile[t];
            col.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            // Les tuiles sans signal utilisable (score nul) n'excluent
            // personne : on prefere empiler toutes les frames plutot que de
            // laisser un trou.
            let usable = col.iter().filter(|(_, v)| *v > 0.0).count();
            if usable < per_tile_keep {
                // Pas assez de signal sur cette tuile : toutes les frames
                // admises (elles seront ponderees normalement).
                continue;
            }
            for (rank, (idx, score)) in col.iter().enumerate() {
                if *score <= 0.0 {
                    break;
                }
                if rank < per_tile_keep {
                    frame_gates.entry(*idx).or_insert_with(|| vec![0u8; n_tiles])[t] = 1;
                }
            }
        }
    }

    let kept: Vec<&FrameInfo> = frames.iter().filter(|f| f.keep).collect();
    let kept_count = kept.len();

    // --- 4. Alignment + stacking. ---
    progress(Progress::Phase {
        phase: Phase::Stacking,
        stage: Stage::Aligning,
    });

    // Reference frame for alignment: the best by score.
    let best = frames
        .iter()
        .filter(|f| f.keep)
        .max_by(|a, b| {
            a.quality
                .unwrap_or(0.0)
                .partial_cmp(&b.quality.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("at least one kept frame");

    let (best_rgb, _, _) = decode::read_rgb16_tiff(Path::new(&best.cache_path))?;
    let best_crop = crop_rgb16(&best_rgb, full_w, full_h, &crop);
    let pixels = crop_w * crop_h;

    let ref_lum = align::luminance(&best_crop, pixels);

    // Similarity alignment (translation + rotation + scale): corrects field
    // rotation, unavoidable on a fixed tripod over a long session.
    let sim_aligner = if settings.align && settings.derotate {
        Some(similarity::SimilarityAligner::new(&ref_lum, crop_w, crop_h, 300))
    } else {
        None
    };

    // Global translation alignment, as a fallback (or when derotation is off).
    let aligner = if settings.align && sim_aligner.is_none() {
        Some(align::Aligner::new(&ref_lum, crop_w, crop_h))
    } else {
        None
    };

    // Local alignment points (AP), built on the reference.
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

    // Per-region selection context: the tile grid and the per-frame gate.
    // Empty when per-region selection is off (or when there is a single tile),
    // in which case stacking behaves exactly as before.
    let tile_ctx = TileSelect {
        tiles_x,
        tile: if tile_select_active { tile } else { 0 },
        gates: frame_gates,
        active: tile_select_active,
    };

    progress(Progress::Phase {
        phase: Phase::Stacking,
        stage: Stage::Stacking,
    });

    let result = if settings.block_size > 0 {
        // --- Blockwise stacking ---
        //
        // Over a long session on a fixed tripod, the Moon rotates and its
        // lighting changes: averaging frames far apart in time blends images
        // that no longer overlap, and the result is blurred. So we stack
        // **blocks of consecutive frames** (inside a block the rotation is
        // negligible), then align and average the blocks.
        stack_blockwise(
            &kept,
            &crop,
            pixels,
            settings,
            &tile_ctx,
            progress,
            kept_count,
            settings.block_size,
            cancel,
        )?
    } else if sim_aligner.is_some() {
        // Fallback: similarity alignment over the whole set (experimental).
        stack_global(
            &kept, &crop, pixels, settings, &tile_ctx, progress, kept_count, None,
            sim_aligner.as_ref(), None, cancel,
        )?
    } else {
        // Fallback: globally aligned stacking (short sessions only).
        stack_global(
            &kept, &crop, pixels, settings, &tile_ctx, progress, kept_count, aligner.as_ref(),
            sim_aligner.as_ref(), ap_aligner.as_ref(), cancel,
        )?
    };

    // Final orientation (rotation / mirror): EXIF cannot know the flip
    // introduced by the telescope, so it is an explicit setting.
    let (result, out_w, out_h) =
        align::apply_orientation(&result, crop_w, crop_h, settings.orientation);

    check_cancel(cancel)?;

    // --- 5. Write the raw stack (before post-processing). ---
    // Written only when enhancement is on: otherwise the file would be
    // strictly identical to the final image.
    let raw_path = if settings.enhance.enabled {
        let p = output::raw_output_path_for(source_dir);
        output::write_tiff(&p, &result, out_w, out_h)?;
        Some(p)
    } else {
        None
    };

    // Final post-processing (wavelets, levels, CLAHE, clarity).
    let result = if settings.enhance.enabled {
        progress(Progress::Phase {
            phase: Phase::Enhancing,
            stage: Stage::Enhancing,
        });
        enhance::enhance(&result, out_w, out_h, &settings.enhance)
    } else {
        result
    };

    check_cancel(cancel)?;

    // --- 6. Export. ---
    progress(Progress::Phase {
        phase: Phase::Done,
        stage: Stage::WritingOutput,
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
        raw_output_path: raw_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        total_frames: total,
        kept_frames: kept_count,
        mean_quality,
        crop,
        settings: settings.clone(),
        skipped: skipped.clone(),
    };

    output::write_output(&out_path, &result, out_w, out_h, &report)?;

    // The raw stack gets its own JSON report: it is a real deliverable, not a
    // throwaway preview, and it should be traceable on its own.
    if let Some(rp) = &raw_path {
        output::write_report(rp, &report)?;
    }

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
    tile_ctx: &TileSelect,
    aligner: Option<&align::Aligner>,
    ap_aligner: Option<&ap::ApAligner>,
) -> Result<Vec<u16>> {
    let (crop_w, crop_h) = (crop.width, crop.height);
    let mut acc = stack::Accumulator::new(crop_w, crop_h);
    let acc_lock = Mutex::new(&mut acc);

    frames.par_iter().try_for_each(|f| -> Result<()> {
        let (rgb, w, h) = decode::read_rgb16_tiff(Path::new(&f.cache_path))?;
        let cropped = crop_rgb16(&rgb, w, h, crop);

        // Alignment (translation), then AP when requested.
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

        let gate = tile_ctx.gate_for(f.index);
        let mut guard = acc_lock.lock().unwrap();
        guard.add_gated(&aligned, Some(&valid), weight, scale, gate);
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
    tile_ctx: &TileSelect,
    progress: &ProgressFn,
    kept_count: usize,
    block_size: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u16>> {
    let (crop_w, crop_h) = (crop.width, crop.height);

    // Temporal order: blocks must be **consecutive** groups.
    let mut order: Vec<&FrameInfo> = kept.to_vec();
    order.sort_by_key(|f| f.index);

    let n_blocks = order.len().div_ceil(block_size).max(1);
    progress(Progress::Phase {
        phase: Phase::Stacking,
        stage: Stage::Stacking,
    });

    // Stack each block (internal alignment on its best frame).
    let total_for_progress = kept_count.max(1);
    let done_blocks = Mutex::new(0usize);
    let mut blocks: Vec<Vec<u16>> = Vec::with_capacity(n_blocks);

    for chunk in order.chunks(block_size) {
        check_cancel(cancel)?;
        // Block reference: its best frame by score.
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
            tile_ctx,
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
    tile_ctx: &TileSelect,
    progress: &ProgressFn,
    kept_count: usize,
    aligner: Option<&align::Aligner>,
    sim_aligner: Option<&similarity::SimilarityAligner>,
    ap_aligner: Option<&ap::ApAligner>,
    cancel: &AtomicBool,
) -> Result<Vec<u16>> {
    let (crop_w, crop_h) = (crop.width, crop.height);
    let mut acc = stack::Accumulator::new(crop_w, crop_h);
    let done = Mutex::new(0usize);
    let acc_lock = Mutex::new(&mut acc);

    kept.par_iter().try_for_each(|f| -> Result<()> {
        // Cancellation aborts the loop promptly; the partial accumulator is
        // discarded by the caller.
        if cancel.load(Ordering::Relaxed) {
            bail!("{}", CANCELLED);
        }
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
        let gate = tile_ctx.gate_for(f.index);
        let mut guard = acc_lock.lock().unwrap();
        guard.add_gated(&aligned, Some(&valid), weight, scale, gate);

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

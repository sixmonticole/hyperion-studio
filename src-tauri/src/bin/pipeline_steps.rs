//! Exporte une image PNG a chaque etape du pipeline, pour validation visuelle.
//!
//! Etapes produites (dans <out_dir>) :
//!  1_brute.png            une frame brute (sans traitement)
//!  2_moyenne_simple.png   moyenne de toutes les frames sans alignement
//!  3_alignee.png          une frame apres alignement (shift sub-pixel)
//!  4_stack_brut.png       le stack aligne, sans post-traitement
//!  5_stack_normalise.png  le stack apres niveaux/gamma
//!  6_stack_ondelettes.png le stack apres ondelettes (nettete)
//!  7_final.png            le stack apres post-traitement complet
//!
//! Usage : pipeline_steps <dossier_frames> <out_dir> [n] [keep_fraction]

use hyperion_studio_lib::model::{CropRect, EnhanceSettings};
use hyperion_studio_lib::pipeline::{align, decode, detect, enhance, quality, stack};
use image::{ImageBuffer, Rgb};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/t20".into());
    let out_dir = a.get(2).cloned().unwrap_or_else(|| "/tmp/etapes".into());
    let n_lim: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(20);
    let keep: f64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    std::fs::create_dir_all(&out_dir).expect("mkdir");

    let mut files = decode::list_raw_files(Path::new(&dir)).expect("list");
    files.sort();
    files.truncate(n_lim.min(files.len()));
    println!("{} frames en entree", files.len());

    // Detection + crop sur la premiere frame.
    let (first, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read");
    let det = detect::detect_planet(&first, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);

    let first_crop = crop_into(&first, w, h, &crop);
    save_png(&first_crop, crop.width, crop.height, &format!("{out_dir}/1_brute.png"));

    // Score de qualite et selection.
    let mut scored: Vec<(usize, f64)> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (rgb, _, _) = decode::read_rgb16_tiff(f).expect("read");
            let c = crop_into(&rgb, w, h, &crop);
            (i, quality::sharpness_score(&c, crop.width, crop.height))
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let keep_n = ((files.len() as f64 * keep).ceil() as usize).max(1);
    let mut kept: Vec<usize> = scored.iter().take(keep_n).map(|(i, _)| *i).collect();
    kept.sort();
    println!("{} frames gardees sur {}", kept.len(), files.len());

    let pixels = crop.width * crop.height;

    // Moyenne simple (sans alignement) : garde les frames telles quelles.
    let mut acc_simple = stack::Accumulator::new(crop.width, crop.height);
    for &i in &kept {
        let (rgb, _, _) = decode::read_rgb16_tiff(&files[i]).expect("read");
        let c = crop_into(&rgb, w, h, &crop);
        let mask = vec![1u8; pixels];
        acc_simple.add(&c, Some(&mask), 1.0, 1.0);
    }
    let simple = acc_simple.finish();
    save_png(&simple, crop.width, crop.height, &format!("{out_dir}/2_moyenne_simple.png"));

    // Alignement : reference = meilleure frame.
    let best_idx = kept[0];
    let (brgb, _, _) = decode::read_rgb16_tiff(&files[best_idx]).expect("read");
    let best_crop = crop_into(&brgb, w, h, &crop);
    let ref_lum = align::luminance(&best_crop, pixels);
    let aligner = align::Aligner::new(&ref_lum, crop.width, crop.height);

    let mut acc = stack::Accumulator::new(crop.width, crop.height);
    let mut emitted_aligned = false;
    for &i in &kept {
        let (rgb, _, _) = decode::read_rgb16_tiff(&files[i]).expect("read");
        let c = crop_into(&rgb, w, h, &crop);
        let lum = align::luminance(&c, pixels);
        let shift = aligner.estimate(&lum);
        let (al, mask) = align::shift_rgb16_masked(&c, crop.width, crop.height, shift);
        if !emitted_aligned && i != best_idx {
            save_png(&al, crop.width, crop.height, &format!("{out_dir}/3_alignee.png"));
            emitted_aligned = true;
        }
        let scale = {
            let m = stack::mean_luminance(&al);
            if m > 1.0 { 8000.0 / m } else { 1.0 }
        };
        acc.add(&al, Some(&mask), 1.0, scale);
    }
    let stacked = acc.finish();
    save_png(&stacked, crop.width, crop.height, &format!("{out_dir}/4_stack_brut.png"));

    // Post-traitement par etapes pour voir l'effet de chacune.
    // 5 : niveaux + gamma seulement.
    let mut s_levels = EnhanceSettings::default();
    s_levels.enabled = true;
    s_levels.wavelet_amount = 0.0;
    s_levels.clahe_amount = 0.0;
    s_levels.clarity = 0.0;
    let lv = enhance::enhance(&stacked, crop.width, crop.height, &s_levels);
    save_png(&lv, crop.width, crop.height, &format!("{out_dir}/5_stack_normalise.png"));

    // 6 : ondelettes seules (nettete).
    let mut s_wav = EnhanceSettings::default();
    s_wav.enabled = true;
    s_wav.wavelet_amount = 1.0;
    s_wav.clahe_amount = 0.0;
    s_wav.clarity = 0.0;
    let wv = enhance::enhance(&stacked, crop.width, crop.height, &s_wav);
    save_png(&wv, crop.width, crop.height, &format!("{out_dir}/6_stack_ondelettes.png"));

    // 7 : tout.
    let mut s_all = EnhanceSettings::default();
    s_all.enabled = true;
    s_all.wavelet_amount = 1.0;
    s_all.clahe_amount = 0.15;
    s_all.clarity = 0.12;
    let full = enhance::enhance(&stacked, crop.width, crop.height, &s_all);
    save_png(&full, crop.width, crop.height, &format!("{out_dir}/7_final.png"));

    println!("images ecrites dans {out_dir}");
}

fn crop_into(rgb: &[u16], w: usize, h: usize, crop: &CropRect) -> Vec<u16> {
    let mut out = vec![0u16; crop.width * crop.height * 3];
    for y in 0..crop.height {
        let sy = crop.y + y;
        if sy >= h {
            break;
        }
        for x in 0..crop.width {
            let sx = crop.x + x;
            if sx >= w {
                break;
            }
            let si = (sy * w + sx) * 3;
            let di = (y * crop.width + x) * 3;
            out[di..di + 3].copy_from_slice(&rgb[si..si + 3]);
        }
    }
    out
}

/// Ecrit un PNG (echelle reduite a 2000 px de large au plus) avec normalisation
/// du niveau blanc au 99,5e percentile, pour que l'apercu soit lisible.
fn save_png(rgb: &[u16], w: usize, h: usize, path: &str) {
    let mut hist = vec![0u32; 65536];
    for px in rgb.chunks_exact(3) {
        hist[px[1] as usize] += 1;
    }
    let total: u32 = hist.iter().sum();
    let target = (total as f64 * 0.995) as u32;
    let mut accp = 0u32;
    let mut p995 = 65535usize;
    for (i, &c) in hist.iter().enumerate() {
        accp += c;
        if accp >= target {
            p995 = i.max(1);
            break;
        }
    }
    let scale = 65535.0 / p995 as f32;

    let mut buf = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(w as u32, h as u32);
    for (i, px) in rgb.chunks_exact(3).enumerate() {
        let x = (i % w) as u32;
        let y = (i / w) as u32;
        let c = |v: u16| ((v as f32 * scale).clamp(0.0, 65535.0) / 257.0) as u8;
        buf.put_pixel(x, y, Rgb([c(px[0]), c(px[1]), c(px[2])]));
    }
    let (tw, th) = if w > 2000 {
        (2000u32, (h as f64 * 2000.0 / w as f64) as u32)
    } else {
        (w as u32, h as u32)
    };
    image::imageops::resize(&buf, tw, th, image::imageops::FilterType::Lanczos3)
        .save(path)
        .expect("png");
    println!("  -> {path}");
}

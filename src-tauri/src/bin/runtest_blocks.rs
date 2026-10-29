//! Empile une session par **blocs consecutifs** et fusionne les blocs.
//!
//! Justification : sur une longue session en trepied fixe, la Lune tourne (et
//! son eclairage change), si bien qu'empiler des frames tres eloignees dans le
//! temps moyenne des images qui ne se superposent plus. En revanche, a
//! l'interieur d'un bloc court (quelques dizaines de frames), la rotation est
//! negligeable et l'empilement est net.
//!
//! Strategie :
//!  1. Decouper la session en blocs de `block` frames consecutives.
//!  2. Empiler chaque bloc (net).
//!  3. Aligner les **blocs** entre eux (peu nombreux, donc robuste) et les
//!     moyenner pour reduire encore le bruit.
//!
//! Usage : runtest_blocks <dossier> <cache> [bloc] [keep_par_bloc]

use hyperion_studio_lib::model::CropRect;
use hyperion_studio_lib::pipeline::{align, decode, detect, quality, stack};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = a.get(1).cloned().unwrap_or_else(|| "/tmp/lune200".into());
    let cache = a.get(2).cloned().unwrap_or_else(|| "/tmp/cache200".into());
    let block: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
    let keep_per_block: f64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let source_dir = Path::new(&src);
    let cache_dir = Path::new(&cache);

    std::fs::create_dir_all(cache_dir).expect("cache");
    let mut files = decode::list_raw_files(source_dir).expect("list");
    if files.is_empty() {
        eprintln!("aucun fichier dans {src}");
        std::process::exit(1);
    }
    files.sort();
    println!("{} frames, blocs de {block}", files.len());

    let first = decode::load_rgb16_any(&files[0]).expect("read");
    let (w, h) = (first.width, first.height);
    let det = detect::detect_planet(&first.rgb16, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    let px = crop.width * crop.height;
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);

    let n_blocks = (files.len() + block - 1) / block;
    let mut block_stacks: Vec<Vec<u16>> = Vec::new();

    for b in 0..n_blocks {
        let start = b * block;
        let end = ((b + 1) * block).min(files.len());
        let slice = &files[start..end];

        // Charger + cropper le bloc.
        let mut cropped_frames: Vec<Vec<u16>> = Vec::new();
        for f in slice {
            let img = decode::load_rgb16_any(f).expect("read");
            cropped_frames.push(crop_into(&img.rgb16, img.width, img.height, &crop));
        }

        // Score de nettete pour garder les meilleures du bloc.
        let mut scored: Vec<(usize, f64)> = cropped_frames
            .iter()
            .enumerate()
            .map(|(i, c)| (i, quality::sharpness_score(c, crop.width, crop.height)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let kn = ((slice.len() as f64 * keep_per_block).ceil() as usize).max(1);
        let mut keep_idx: Vec<usize> = scored.iter().take(kn).map(|(i, _)| *i).collect();
        keep_idx.sort();

        // Reference : la meilleure du bloc.
        let ref_i = keep_idx[0];
        let ref_lum = align::luminance(&cropped_frames[ref_i], px);
        let aligner = align::Aligner::new(&ref_lum, crop.width, crop.height);

        let mut acc = stack::Accumulator::new(crop.width, crop.height);
        for &i in &keep_idx {
            let lum = align::luminance(&cropped_frames[i], px);
            let shift = aligner.estimate(&lum);
            let (al, mask) =
                align::shift_rgb16_masked(&cropped_frames[i], crop.width, crop.height, shift);
            let scale = {
                let m = stack::mean_luminance(&al);
                if m > 1.0 { 8000.0 / m } else { 1.0 }
            };
            acc.add(&al, Some(&mask), 1.0, scale);
        }
        block_stacks.push(acc.finish());
        println!("  bloc {}/{} (frames {}-{}) empile", b + 1, n_blocks, start + 1, end);
    }

    // Aligner et moyenner les blocs entre eux.
    println!("alignement des {} blocs...", block_stacks.len());
    let ref_lum = align::luminance(&block_stacks[0], px);
    let aligner = align::Aligner::new(&ref_lum, crop.width, crop.height);
    let mut acc = stack::Accumulator::new(crop.width, crop.height);
    for (bi, bs) in block_stacks.iter().enumerate() {
        let lum = align::luminance(bs, px);
        let shift = if bi == 0 { (0.0, 0.0) } else { aligner.estimate(&lum) };
        let (al, mask) = align::shift_rgb16_masked(bs, crop.width, crop.height, shift);
        acc.add(&al, Some(&mask), 1.0, 1.0);
    }
    let result = acc.finish();

    // Ecriture.
    let out_dir = source_dir.join("hyperion_out");
    std::fs::create_dir_all(&out_dir).expect("out");
    let tif = out_dir.join("blocs_stack.tif");
    decode::write_rgb16_tiff(&tif, &result, crop.width, crop.height).expect("write");
    println!(
        "TERMINE: {} blocs x {} frames -> {}",
        block_stacks.len(),
        block,
        tif.display()
    );
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

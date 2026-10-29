//! Diagnostique la similarite entre la reference et chaque frame : rotation
//! (degres), echelle et translation, plus le nombre de points apparies.
//!
//! Usage : diag_similarity <dossier_cache> [n] [keep_fraction]

use hyperion_studio_lib::model::CropRect;
use hyperion_studio_lib::pipeline::{align, decode, detect, similarity::SimilarityAligner};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache200".into());
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(12);

    let files = decode::list_raw_files(Path::new(&dir)).expect("list");
    println!("{} fichiers", files.len());

    let (ref_rgb, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read");
    let det = detect::detect_planet(&ref_rgb, w, h, None).expect("detect");

    // Crop genereux pour contenir la rotation (marge importante).
    let mut crop = detect::crop_window(&det, w, h, 0, 0.0);
    // Elargit le crop de 15% pour laisser de la marge a la rotation.
    let grow = (crop.width as f64 * 0.15) as usize;
    crop = widen(crop, grow, w, h);
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);

    let ref_crop = crop_into(&ref_rgb, w, h, &crop);
    let ref_lum = align::luminance(&ref_crop, crop.width * crop.height);

    let t0 = std::time::Instant::now();
    let aligner = SimilarityAligner::new(&ref_lum, crop.width, crop.height, 400);
    println!("{} points de controle, construit en {:?}", aligner.point_count(), t0.elapsed());

    println!("\nframe |   rotation |  echelle |    tx    |    ty");
    for (i, f) in files.iter().take(n).enumerate() {
        let (rgb, _, _) = decode::read_rgb16_tiff(f).expect("read");
        let c = crop_into(&rgb, w, h, &crop);
        let lum = align::luminance(&c, crop.width * crop.height);
        let t = std::time::Instant::now();
        match aligner.estimate(&lum) {
            Some(s) => println!(
                "  {i:3} | {:8.3}°  | {:7.4}  | {:8.2} | {:8.2}   ({:?})",
                s.angle_deg(), s.scale, s.tx, s.ty, t.elapsed()
            ),
            None => println!("  {i:3} | echec d'estimation"),
        }
    }
}

fn widen(c: CropRect, grow: usize, w: usize, h: usize) -> CropRect {
    let x = c.x.saturating_sub(grow);
    let y = c.y.saturating_sub(grow);
    let x2 = (c.x + c.width + grow).min(w);
    let y2 = (c.y + c.height + grow).min(h);
    CropRect {
        x,
        y,
        width: x2 - x,
        height: y2 - y,
    }
}

fn crop_into(rgb: &[u16], w: usize, h: usize, crop: &CropRect) -> Vec<u16> {
    let mut out = vec![0u16; crop.width * crop.height * 3];
    for y in 0..crop.height {
        let sy = crop.y + y;
        if sy >= h { break; }
        for x in 0..crop.width {
            let sx = crop.x + x;
            if sx >= w { break; }
            let si = (sy * w + sx) * 3;
            let di = (y * crop.width + x) * 3;
            out[di..di + 3].copy_from_slice(&rgb[si..si + 3]);
        }
    }
    out
}

//! Diagnostique l'alignement : pour un dossier de TIFF, affiche le score de
//! nettete et le deplacement estime de chaque frame (les premieres).
//!
//! Usage : diag_align <dossier_cache> [n]

use hyperion_studio_lib::pipeline::{align, decode, detect};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache200".into());
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);

    let files = decode::list_raw_files(Path::new(&dir)).expect("list");
    println!("{} fichiers dans {dir}", files.len());

    // Reference = premiere frame (pour ce diagnostic).
    let (ref_rgb, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read ref");
    println!("reference: {w}x{h}");

    // Detection pour le crop.
    let det = detect::detect_planet(&ref_rgb, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop: x={} y={} {}x{}", crop.x, crop.y, crop.width, crop.height);

    let ref_crop = crop_into(&ref_rgb, w, h, &crop);
    let ref_lum = align::luminance(&ref_crop, crop.width * crop.height);
    let aligner = align::Aligner::new(&ref_lum, crop.width, crop.height);

    println!("\nframe | deplacement estime (dx, dy)");
    for (i, f) in files.iter().take(n).enumerate() {
        let (rgb, _, _) = decode::read_rgb16_tiff(f).expect("read");
        let c = crop_into(&rgb, w, h, &crop);
        let lum = align::luminance(&c, crop.width * crop.height);
        let (dx, dy) = aligner.estimate(&lum);
        println!("  {i:3} | dx={dx:8.3}  dy={dy:8.3}");
    }
}

fn crop_into(rgb: &[u16], w: usize, h: usize, crop: &hyperion_studio_lib::model::CropRect) -> Vec<u16> {
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

//! Verifie la fiabilite de l'alignement sur une longue serie : mesure le shift
//! de chaque frame par rapport a la reference, puis par rapport a la frame
//! precedente (chaînage). Compare les deux et signale les frames pour lesquelles
//! l'alignement direct echoue (pic de correlation ambigu).
//!
//! Usage : diag_chain <dossier_cache> [n]

use hyperion_studio_lib::model::CropRect;
use hyperion_studio_lib::pipeline::{align, decode, detect};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache200".into());
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(200);

    let mut files = decode::list_raw_files(std::path::Path::new(&dir)).expect("list");
    files.sort();
    // Le cache contient des fichiers "NNNNN_DSC...". On trie pour garder l'ordre.
    println!("{} fichiers", files.len());

    let (r0, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read");
    let det = detect::detect_planet(&r0, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);
    let px = crop.width * crop.height;

    // Reference = premiere frame.
    let c0 = crop_into(&r0, w, h, &crop);
    let l0 = align::luminance(&c0, px);
    let direct = align::Aligner::new(&l0, crop.width, crop.height);

    // Aligneur pour le chainage : reutilise (on recree par frame, cout acceptable
    // pour ce diagnostic).
    let mut prev_lum = l0.clone();
    let mut cum = (0.0f64, 0.0f64);

    println!("\nframe |  direct dx,dy      |   chainage cumule   | ecart");
    let mut bad = 0;
    for (i, f) in files.iter().take(n).enumerate() {
        let (rgb, _, _) = decode::read_rgb16_tiff(f).expect("read");
        let c = crop_into(&rgb, w, h, &crop);
        let lum = align::luminance(&c, px);

        let (dx, dy) = direct.estimate(&lum);

        // Chaine : shift entre frame precedente et celle-ci.
        let step = align::Aligner::new(&prev_lum, crop.width, crop.height);
        let (sx, sy) = step.estimate(&lum);
        cum.0 += sx;
        cum.1 += sy;

        // Un alignement direct "correct" doit etre coherent avec le chainage.
        let ecart = ((dx - cum.0).powi(2) + (dy - cum.1).powi(2)).sqrt();
        let flag = if ecart > 5.0 { bad += 1; "  <-- INCOHERENT" } else { "" };
        if i % 10 == 0 || ecart > 5.0 {
            println!(
                "{i:5} | {dx:8.1},{dy:8.1} | {c:8.1},{d:8.1} | {ecart:7.1}{flag}",
                c = cum.0,
                d = cum.1
            );
        }
        prev_lum = lum;
    }
    println!("\n{bad} frames incoherentes sur {n}");
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

//! Verifie la qualite de l'alignement entre deux frames : applique le shift
//! estime a la frame B et calcule l'erreur moyenne avec A (recalee).
//! Compare aussi avec une recherche exhaustive du meilleur shift entier, pour
//! savoir si l'estimation par correlation de phase est juste.
//!
//! Usage : diag_pair <dossier_cache> [i] [j]

use hyperion_studio_lib::model::CropRect;
use hyperion_studio_lib::pipeline::{align, decode, detect};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache_t20".into());
    let i: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let j: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);

    let mut files = decode::list_raw_files(std::path::Path::new(&dir)).expect("list");
    files.sort();
    if files.len() <= i.max(j) {
        eprintln!("pas assez de fichiers");
        std::process::exit(1);
    }

    let (ra, w, h) = decode::read_rgb16_tiff(&files[i]).expect("read A");
    let (rb, _, _) = decode::read_rgb16_tiff(&files[j]).expect("read B");
    let det = detect::detect_planet(&ra, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);

    let ca = crop_into(&ra, w, h, &crop);
    let cb = crop_into(&rb, w, h, &crop);
    let la = align::luminance(&ca, crop.width * crop.height);
    let lb = align::luminance(&cb, crop.width * crop.height);

    // Estimation par correlation de phase.
    let al = align::Aligner::new(&la, crop.width, crop.height);
    let (dx, dy) = al.estimate(&lb);
    println!("shift estime par FFT : dx={dx:.3} dy={dy:.3}");

    // Erreur residuelle apres shift FFT.
    let shifted = align::shift_rgb16(&cb, crop.width, crop.height, (dx, dy));
    let e_fft = mean_abs_diff(&ca, &shifted);
    println!("erreur moyenne apres shift FFT : {e_fft:.1}");

    // Recherche exhaustive (entier) du meilleur shift, sur une grille limitee.
    let mut best = (0i64, 0i64, f64::MAX);
    for sy in -40i64..=40 {
        for sx in -40i64..=40 {
            // Echantillonne grossierement (1 pixel sur 8) pour la vitesse.
            let e = sampled_diff(&ca, &cb, crop.width, crop.height, sx, sy, 8);
            if e < best.2 {
                best = (sx, sy, e);
            }
        }
    }
    println!(
        "meilleur shift entier (recherche) : dx={} dy={} erreur={:.1}",
        best.0, best.1, best.2
    );

    // Affinement autour du meilleur entier.
    let shifted_best = align::shift_rgb16(&cb, crop.width, crop.height, (best.0 as f64, best.1 as f64));
    let e_best = mean_abs_diff(&ca, &shifted_best);
    println!("erreur moyenne apres shift entier optimal : {e_best:.1}");
}

fn sampled_diff(
    a: &[u16],
    b: &[u16],
    w: usize,
    h: usize,
    sx: i64,
    sy: i64,
    step: usize,
) -> f64 {
    let mut sum = 0.0;
    let mut n = 0.0;
    let mut y = 200;
    while y < h - 200 {
        let mut x = 200;
        while x < w - 200 {
            let bx = x as i64 + sx;
            let by = y as i64 + sy;
            if bx < 0 || by < 0 || bx >= w as i64 || by >= h as i64 {
                x += step;
                continue;
            }
            let ia = (y * w + x) * 3;
            let ib = (by as usize * w + bx as usize) * 3;
            sum += (a[ia] as f64 - b[ib] as f64).abs();
            n += 1.0;
            x += step;
        }
        y += step;
    }
    if n > 0.0 {
        sum / n
    } else {
        f64::MAX
    }
}

fn mean_abs_diff(a: &[u16], b: &[u16]) -> f64 {
    let mut sum = 0.0f64;
    let mut n = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        // Ignore les pixels noirs (hors cadre apres shift).
        if *x == 0 || *y == 0 {
            continue;
        }
        sum += (*x as f64 - *y as f64).abs();
        n += 1.0;
    }
    if n > 0.0 {
        sum / n
    } else {
        0.0
    }
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

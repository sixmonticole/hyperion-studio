//! Teste l'hypothese d'AutoStakkert : sur plusieurs frames, une meme zone n'est
//! pas nette partout. Mesure le score de nettete d'une petite zone (carreau)
//! sur chaque frame, et verifie s'il y a une forte variance (donc des instants
//! ou la zone est nette, d'autres non).
//!
//! Usage : diag_local <dossier_cache> [n] [tile]

use hyperion_studio_lib::model::CropRect;
use hyperion_studio_lib::pipeline::{decode, detect, quality};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache_t20".into());
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let tile: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(200);

    let mut files = decode::list_raw_files(std::path::Path::new(&dir)).expect("list");
    files.sort();
    let (ra, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read");
    let det = detect::detect_planet(&ra, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);

    // On choisit 4 carreaux bien separes (quartiers) dans le crop.
    let qw = crop.width / 2;
    let qh = crop.height / 2;
    let tiles = [
        ("HG", 0, 0),
        ("HD", qw - tile, 0),
        ("BG", 0, qh - tile),
        ("BD", qw - tile, qh - tile),
    ];

    println!("\nscore de nettete par carreau ({tile}px) sur {n} frames :");
    println!("frame  |    HG    |    HD    |    BG    |    BD");
    let mut series: Vec<Vec<f64>> = vec![Vec::new(); 4];
    for (k, f) in files.iter().take(n).enumerate() {
        let (rgb, _, _) = decode::read_rgb16_tiff(f).expect("read");
        let c = crop_into(&rgb, w, h, &crop);
        let mut row = Vec::new();
        for (ti, (_, tx, ty)) in tiles.iter().enumerate() {
            let t = sub_tile(&c, crop.width, *tx, *ty, tile);
            let s = quality::sharpness_score(&t, tile, tile);
            series[ti].push(s);
            row.push(s);
        }
        println!(
            "{k:5}  | {:8.1} | {:8.1} | {:8.1} | {:8.1}",
            row[0], row[1], row[2], row[3]
        );
    }

    println!("\nvariation relative par carreau (max-min)/moyenne :");
    for (ti, (name, _, _)) in tiles.iter().enumerate() {
        let s = &series[ti];
        let mean = s.iter().sum::<f64>() / s.len() as f64;
        let min = s.iter().cloned().fold(f64::MAX, f64::min);
        let max = s.iter().cloned().fold(f64::MIN, f64::max);
        let rel = (max - min) / mean.max(1e-9);
        println!(
            "  {name}: moy={mean:.1} min={min:.1} max={max:.1}  variation={:.0}%",
            rel * 100.0
        );
    }
}

fn sub_tile(src: &[u16], w: usize, x0: usize, y0: usize, size: usize) -> Vec<u16> {
    let mut out = vec![0u16; size * size * 3];
    for y in 0..size {
        for x in 0..size {
            let si = ((y0 + y) * w + (x0 + x)) * 3;
            let di = (y * size + x) * 3;
            out[di..di + 3].copy_from_slice(&src[si..si + 3]);
        }
    }
    out
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

//! Benchmark du seul cout d'alignement+empilement a une taille de crop donnee.
//!
//! Usage : cargo run --release --bin bench -- <image.tif> <crop_px> [iters]

use hyperion_studio_lib::pipeline::{align, decode};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("usage: bench <image.tif> <crop_px> [iters]");
        std::process::exit(2);
    };
    let crop: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(512);
    let iters: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);

    let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(input)).expect("lecture");
    let pixels = crop * crop;

    // Sous-image centree de taille crop x crop.
    let x0 = (w - crop) / 2;
    let y0 = (h - crop) / 2;
    let mut sub = vec![0u16; pixels * 3];
    for y in 0..crop {
        for x in 0..crop {
            let si = ((y0 + y) * w + x0 + x) * 3;
            let di = (y * crop + x) * 3;
            sub[di..di + 3].copy_from_slice(&rgb[si..si + 3]);
        }
    }

    let lum = align::luminance(&sub, pixels);
    let t0 = std::time::Instant::now();
    let aligner = align::Aligner::new(&lum, crop, crop);
    let t_init = t0.elapsed();

    let t1 = std::time::Instant::now();
    for _ in 0..iters {
        let _ = aligner.estimate(&lum);
    }
    let t_est = t1.elapsed() / iters as u32;

    let t2 = std::time::Instant::now();
    for _ in 0..iters {
        let _ = align::shift_rgb16(&sub, crop, crop, (1.0, -1.0));
    }
    let t_shift = t2.elapsed() / iters as u32;

    println!("crop {crop}x{crop} ({pixels} px) :");
    println!("  init aligneur      : {t_init:?}");
    println!("  estimation / frame : {t_est:?}");
    println!("  shift / frame      : {t_shift:?}");
}

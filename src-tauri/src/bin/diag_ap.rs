//! Diagnostic: does the AP aligner recover a range of known shifts?
//!
//! Usage: cargo run --features devtools --bin diag_ap

use hyperion_studio_lib::pipeline::{align, ap};

fn main() {
    let (w, h) = (256usize, 256usize);

    // Smooth multi-scale structure plus modest noise (realistic lunar detail).
    let mut img = vec![0u16; w * h * 3];
    let mut rng: u32 = 987654;
    let mut rand = || {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        (rng >> 16) & 0x3FF
    };
    for y in 0..h {
        for x in 0..w {
            let fx = x as f64 / w as f64;
            let fy = y as f64 / h as f64;
            let s = (fx * 18.0).sin() * (fy * 14.0).cos()
                + 0.5 * (fx * 47.0).sin() * (fy * 39.0).cos();
            let v = (30000.0 + 20000.0 * s + rand() as f64).clamp(0.0, 65535.0);
            let i = (y * w + x) * 3;
            img[i] = v as u16;
            img[i + 1] = v as u16;
            img[i + 2] = v as u16;
        }
    }

    let ref_lum = align::luminance(&img, w * h);
    let apx = ap::ApAligner::new(&ref_lum, w, h, 48);
    let ga = align::Aligner::new(&ref_lum, w, h);
    println!("anchors = {}", apx.anchor_count());

    for (sx, sy) in [(6, 4), (6, -4), (3, 0), (0, 3), (10, 7), (-5, 8)] {
        // frame(x,y) = img(x - sx, y - sy): to restore, shift by (-sx, -sy).
        let mut frame = vec![0u16; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let px = (x as i32 - sx).clamp(0, w as i32 - 1) as usize;
                let py = (y as i32 - sy).clamp(0, h as i32 - 1) as usize;
                let si = (py * w + px) * 3;
                let di = (y * w + x) * 3;
                frame[di..di + 3].copy_from_slice(&img[si..si + 3]);
            }
        }
        let frame_lum = align::luminance(&frame, w * h);
        let coarse = ga.estimate(&frame_lum);
        let ap_shift = apx.mean_shift(&frame_lum, coarse);
        println!(
            "true=({:+},{:+})  global=({:+.2},{:+.2})  ap={:?}",
            -sx,
            -sy,
            coarse.0,
            coarse.1,
            ap_shift.map(|(a, b)| (format!("{a:+.2}"), format!("{b:+.2}")))
        );
    }
}

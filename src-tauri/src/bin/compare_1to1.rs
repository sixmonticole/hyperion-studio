//! Side-by-side 1:1 comparison of two frames, to judge sharpness honestly.
//!
//! Extracts the SAME region at full resolution from two images (RAW or TIFF),
//! normalises them on a shared white point, and writes a single PNG with the
//! two crops side by side and a thin divider. A 1:1 view is the only way to
//! tell real sharpness from noise: a reduced preview hides both.
//!
//! Usage: compare_1to1 <A> <B> <out.png> [--size N] [--cx F] [--cy F]
//!   --size  region side in pixels (default 600)
//!   --cx/--cy  centre of the region as a fraction of the image (default 0.5)

use hyperion_studio_lib::pipeline::decode;
use image::{ImageBuffer, Rgb};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: compare_1to1 <A> <B> <out.png> [--size N] [--cx F] [--cy F]");
        std::process::exit(1);
    }
    let a_path = &args[1];
    let b_path = &args[2];
    let out_path = &args[3];

    let mut size = 600usize;
    let mut cx = 0.5f64;
    let mut cy = 0.5f64;
    let mut i = 4;
    while i < args.len() {
        match args[i].as_str() {
            "--size" => {
                i += 1;
                size = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(600);
            }
            "--cx" => {
                i += 1;
                cx = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.5);
            }
            "--cy" => {
                i += 1;
                cy = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.5);
            }
            _ => {}
        }
        i += 1;
    }

    let (ra, wa, ha) = decode::load_rgb16_any(Path::new(a_path)).map(|d| (d.rgb16, d.width, d.height)).expect("read A");
    let (rb, wb, hb) = decode::load_rgb16_any(Path::new(b_path)).map(|d| (d.rgb16, d.width, d.height)).expect("read B");
    println!("A {}x{}   B {}x{}", wa, ha, wb, hb);

    // Shared white point: the 99.5th percentile of the brighter frame, so the
    // comparison is not skewed by a different stretch on each side.
    let p = percentile_lum(&ra, 0.995).max(percentile_lum(&rb, 0.995)).max(1.0);
    println!("shared white point (p99.5) = {p:.0}");

    let crop = |rgb: &[u16], w: usize, h: usize| -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        let ox = ((w as f64 * cx) as isize - size as isize / 2).max(0) as usize;
        let oy = ((h as f64 * cy) as isize - size as isize / 2).max(0) as usize;
        let mut buf = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(size as u32, size as u32);
        let scale = 65535.0 / p;
        for y in 0..size {
            for x in 0..size {
                let sx = (ox + x).min(w - 1);
                let sy = (oy + y).min(h - 1);
                let si = (sy * w + sx) * 3;
                let px = |v: u16| ((v as f32 * scale).clamp(0.0, 65535.0) / 257.0) as u8;
                buf.put_pixel(
                    x as u32,
                    y as u32,
                    Rgb([px(rgb[si]), px(rgb[si + 1]), px(rgb[si + 2])]),
                );
            }
        }
        buf
    };

    let ca = crop(&ra, wa, ha);
    let cb = crop(&rb, wb, hb);

    // Compose: A | divider | B.
    let gap = 4u32;
    let total_w = size as u32 * 2 + gap;
    let mut out = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(total_w, size as u32);
    for y in 0..size as u32 {
        for x in 0..size as u32 {
            out.put_pixel(x, y, *ca.get_pixel(x, y));
            out.put_pixel(x + size as u32 + gap, y, *cb.get_pixel(x, y));
        }
        for x in size as u32..size as u32 + gap {
            out.put_pixel(x, y, Rgb([200, 60, 60]));
        }
    }

    out.save(out_path).expect("write png");
    println!("written {out_path}");
}

fn percentile_lum(rgb: &[u16], p: f64) -> f32 {
    let mut hist = vec![0u32; 65536];
    for px in rgb.chunks_exact(3) {
        let l = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as usize;
        hist[l.min(65535)] += 1;
    }
    let total: u64 = hist.iter().map(|&c| c as u64).sum();
    let target = (total as f64 * p) as u64;
    let mut acc = 0u64;
    for (i, &c) in hist.iter().enumerate() {
        acc += c as u64;
        if acc >= target {
            return i.max(1) as f32;
        }
    }
    65535.0
}

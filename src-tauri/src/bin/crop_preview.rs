//! Genere un recadrage (gros plan) d'un TIFF 16-bit, normalise en PNG 8-bit.
//!
//! Usage : crop_preview <in.tif> <out.png> <x> <y> <largeur> [gain]

use hyperion_studio_lib::pipeline::decode;
use image::{ImageBuffer, Rgb};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 6 {
        eprintln!("usage: crop_preview <in.tif> <out.png> <x> <y> <w> [gain]");
        std::process::exit(2);
    }
    let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(&a[1])).expect("lecture");
    let x0: usize = a[3].parse().unwrap();
    let y0: usize = a[4].parse().unwrap();
    let size: usize = a[5].parse().unwrap();
    let gain: f32 = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let mut vals: Vec<u16> = Vec::with_capacity(size * size);
    for y in y0..(y0 + size).min(h) {
        for x in x0..(x0 + size).min(w) {
            let i = (y * w + x) * 3;
            vals.push(rgb[i + 1]);
        }
    }
    let mut sorted = vals.clone();
    sorted.sort_unstable();
    let p99 = sorted[(sorted.len() as f64 * 0.995) as usize].max(1) as f32;
    let scale = gain * (65535.0 / p99);

    let mut buf = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(size as u32, size as u32);
    for y in 0..size {
        for x in 0..size {
            let sx = x0 + x;
            let sy = y0 + y;
            let (r, g, b) = if sx < w && sy < h {
                let i = (sy * w + sx) * 3;
                (rgb[i], rgb[i + 1], rgb[i + 2])
            } else {
                (0, 0, 0)
            };
            let c = |v: u16| ((v as f32 * scale).clamp(0.0, 65535.0) / 257.0) as u8;
            buf.put_pixel(x as u32, y as u32, Rgb([c(r), c(g), c(b)]));
        }
    }
    buf.save(&a[2]).expect("ecriture PNG");
    eprintln!("crop {size}x{size} en ({x0},{y0}) p99.5={p99} -> {}", a[2]);
}

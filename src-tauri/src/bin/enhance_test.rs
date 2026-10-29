//! Teste le post-traitement seul sur une image, sans stack.
//!
//! Usage : enhance_test <in.tif> <out.png> [wavelet] [clahe] [clarity] [gamma] [white]

use hyperion_studio_lib::model::EnhanceSettings;
use hyperion_studio_lib::pipeline::{decode, enhance};
use image::{ImageBuffer, Rgb};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: enhance_test <in.tif> <out.png> [wavelet] [clahe] [clarity] [gamma] [white]");
        std::process::exit(2);
    }
    let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(&a[1])).expect("lecture");

    let mut s = EnhanceSettings::default();
    s.enabled = true;
    if let Some(v) = a.get(3).and_then(|x| x.parse().ok()) { s.wavelet_amount = v; }
    if let Some(v) = a.get(4).and_then(|x| x.parse().ok()) { s.clahe_amount = v; }
    if let Some(v) = a.get(5).and_then(|x| x.parse().ok()) { s.clarity = v; }
    if let Some(v) = a.get(6).and_then(|x| x.parse().ok()) { s.gamma = v; }
    if let Some(v) = a.get(7).and_then(|x| x.parse().ok()) { s.white_point = v; }

    eprintln!("application du post-traitement {w}x{h}...");
    let t0 = std::time::Instant::now();
    let out16 = enhance::enhance(&rgb, w, h, &s);
    eprintln!("fait en {:?}", t0.elapsed());

    // Apercu normalise.
    let mut hist = vec![0u32; 65536];
    for px in out16.chunks_exact(3) { hist[px[1] as usize] += 1; }
    let total: u32 = hist.iter().sum();
    let target = (total as f64 * 0.995) as u32;
    let mut acc = 0u32; let mut p995 = 65535usize;
    for (i, &c) in hist.iter().enumerate() { acc += c; if acc >= target { p995 = i.max(1); break; } }
    let scale = 65535.0 / p995 as f32;

    let mut buf = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(w as u32, h as u32);
    for (i, px) in out16.chunks_exact(3).enumerate() {
        let x = (i % w) as u32; let y = (i / w) as u32;
        let c = |v: u16| ((v as f32 * scale).clamp(0.0, 65535.0) / 257.0) as u8;
        buf.put_pixel(x, y, Rgb([c(px[0]), c(px[1]), c(px[2])]));
    }
    let (tw, th) = if w > 1600 { (1600u32, (h as f64 * 1600.0 / w as f64) as u32) } else { (w as u32, h as u32) };
    image::imageops::resize(&buf, tw, th, image::imageops::FilterType::Triangle).save(&a[2]).expect("png");
    eprintln!("-> {}", a[2]);
}

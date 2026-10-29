//! Genere un apercu PNG 8-bit normalise depuis un TIFF 16-bit, pour verifier
//! visuellement le resultat du decodage / empilement.
//!
//! Usage : cargo run --bin preview -- <entree.tif> <sortie.png> [gain]

use hyperion_studio_lib::pipeline::decode;
use image::{ImageBuffer, Rgb};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("usage: preview <entree.tif> <sortie.png> [gain]");
        std::process::exit(2);
    };
    let out = args.get(2).cloned().unwrap_or_else(|| "preview.png".into());
    let gain: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let (rgb16, w, h) = match decode::read_rgb16_tiff(std::path::Path::new(input)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ECHEC lecture: {e:#}");
            std::process::exit(1);
        }
    };

    // Normalisation : on ramene l'echelle sur 99.5e percentile pour ne pas
    // ecreter les details. Approximation par echantillonnage.
    let mut hist = [0u32; 65536];
    for px in rgb16.chunks_exact(3) {
        let g = px[1] as usize;
        hist[g] += 1;
    }
    let total: u32 = hist.iter().sum();
    let target = (total as f64 * 0.995) as u32;
    let mut acc = 0u32;
    let mut p995 = 65535usize;
    for (i, &c) in hist.iter().enumerate() {
        acc += c;
        if acc >= target {
            p995 = i.max(1);
            break;
        }
    }
    let scale = gain * (65535.0 / p995 as f32);

    let mut buf = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(w as u32, h as u32);
    for (i, px) in rgb16.chunks_exact(3).enumerate() {
        let x = (i % w) as u32;
        let y = (i / w) as u32;
        let c = |v: u16| ((v as f32 * scale).clamp(0.0, 65535.0) / 257.0) as u8;
        buf.put_pixel(x, y, Rgb([c(px[0]), c(px[1]), c(px[2])]));
    }

    // Redimensionne pour un apercu leger si l'image est grande.
    let (tw, th) = if w > 1600 {
        (1600u32, (h as f64 * 1600.0 / w as f64) as u32)
    } else {
        (w as u32, h as u32)
    };
    let resized = image::imageops::resize(
        &buf,
        tw,
        th,
        image::imageops::FilterType::Triangle,
    );
    resized.save(&out).expect("ecriture PNG");
    eprintln!("apercu {tw}x{th} (p99.5={p995}, scale={scale:.2}) -> {out}");
}

//! Rendu d'apercus PNG pour l'interface.
//!
//! Deux usages :
//! - **apercu general** : l'image entiere, reduite, pour afficher le resultat ;
//! - **apercu 1:1** : une zone extraite en pleine resolution, pour juger la
//!   nettete reelle (un apercu reduit masque le bruit et le piqué).
//!
//! Le resultat est renvoye en data URL (`data:image/png;base64,...`) directement
//! affichable dans une balise `<img>`.

use crate::pipeline::decode;
use base64::Engine;
use image::{ImageBuffer, Rgb};
use std::path::Path;

/// Rend un apercu d'un TIFF 16-bit en data URL PNG.
///
/// `max_size` : dimension maximale (cote le plus long) de l'apercu. La reduction
/// se fait avec un filtre Lanczos pour un rendu fidele.
///
/// `crop` : zone optionnelle a extraire **en pleine resolution** avant mise a
/// l'echelle, au format `{ "x": .., "y": .., "size": .. }` (carre). Sert a
/// l'inspection 1:1 d'un detail (un cratere).
pub fn render_preview(
    path: &Path,
    max_size: usize,
    crop: Option<&serde_json::Value>,
) -> anyhow::Result<String> {
    let (rgb, w, h) = decode::read_rgb16_tiff(path)?;

    // Extraction optionnelle d'une zone carree.
    let (src, sw, sh) = if let Some(c) = crop {
        let x = c.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let y = c.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let size = c.get("size").and_then(|v| v.as_u64()).unwrap_or(600) as usize;
        let size = size.min(w.saturating_sub(x)).min(h.saturating_sub(y)).max(1);
        let mut out = vec![0u16; size * size * 3];
        for yy in 0..size {
            for xx in 0..size {
                let si = ((y + yy) * w + (x + xx)) * 3;
                let di = (yy * size + xx) * 3;
                out[di..di + 3].copy_from_slice(&rgb[si..si + 3]);
            }
        }
        (out, size, size)
    } else {
        (rgb, w, h)
    };

    // Normalisation du niveau blanc : le stack brut est tres sombre, on l'etire
    // sur le 99,5e percentile pour un apercu lisible (comme les outils d'apercu
    // classiques). Ceci n'affecte PAS le TIFF enregistre.
    let mut hist = vec![0u32; 65536];
    for px in src.chunks_exact(3) {
        hist[px[1] as usize] += 1;
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
    let scale = 65535.0 / p995 as f32;

    // Conversion en 8-bit.
    let mut buf = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(sw as u32, sh as u32);
    for (i, px) in src.chunks_exact(3).enumerate() {
        let x = (i % sw) as u32;
        let y = (i / sw) as u32;
        let c = |v: u16| ((v as f32 * scale).clamp(0.0, 65535.0) / 257.0) as u8;
        buf.put_pixel(x, y, Rgb([c(px[0]), c(px[1]), c(px[2])]));
    }

    // Reduction proportionnelle si demande.
    let out = if sw.max(sh) > max_size {
        let ratio = max_size as f64 / sw.max(sh) as f64;
        let tw = ((sw as f64 * ratio).round() as u32).max(1);
        let th = ((sh as f64 * ratio).round() as u32).max(1);
        image::imageops::resize(&buf, tw, th, image::imageops::FilterType::Lanczos3)
    } else {
        buf
    };

    // Encodage PNG en memoire.
    let mut png = Vec::new();
    out.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(format!("data:image/png;base64,{b64}"))
}

//! Profil d'une session : pour chaque frame, mesures de nettete, exposition et
//! deplacement global (par rapport a la premiere frame).
//!
//! Sert a diagnostiquer pourquoi un empilement de nombreuses frames est flou :
//! derive de mise au point, variation d'exposition, ou turbulence.
//!
//! Usage : profile_session <dossier_cache> [n]

use hyperion_studio_lib::pipeline::{align, decode, detect, quality};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache200".into());
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    let files = decode::list_raw_files(std::path::Path::new(&dir)).expect("list");
    let total = if n == 0 { files.len() } else { n.min(files.len()) };
    println!("{total} frames analysees sur {}", files.len());

    // Reference = premiere frame pour mesurer la derive.
    let (ref_rgb, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read");
    let det = detect::detect_planet(&ref_rgb, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);

    let ref_crop = crop_into(&ref_rgb, w, h, &crop);
    let ref_lum = align::luminance(&ref_crop, crop.width * crop.height);
    let aligner = align::Aligner::new(&ref_lum, crop.width, crop.height);

    println!("\nframe |   nettete |   moy.  |  p99.9  |   dx    |   dy");
    for (i, f) in files.iter().take(total).enumerate() {
        let (rgb, _, _) = decode::read_rgb16_tiff(f).expect("read");
        let c = crop_into(&rgb, w, h, &crop);
        let score = quality::sharpness_score(&c, crop.width, crop.height);

        // Moyenne de luminance (exposition) et percentile haut (niveau des blancs).
        let lum = align::luminance(&c, crop.width * crop.height);
        let mean: f64 = lum.iter().map(|&v| v as f64).sum::<f64>() / lum.len() as f64;
        let mut v: Vec<f32> = lum.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p999 = v[(v.len() as f64 * 0.999) as usize];

        let (dx, dy) = aligner.estimate(&lum);
        println!(
            "{i:5} | {score:9.1} | {mean:7.1} | {p999:7.0} | {dx:7.2} | {dy:7.2}"
        );
    }
}

fn crop_into(
    rgb: &[u16],
    w: usize,
    h: usize,
    crop: &hyperion_studio_lib::model::CropRect,
) -> Vec<u16> {
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

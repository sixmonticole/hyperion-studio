//! Validation manuelle : lance la detection de planete et affiche la fenetre
//! de crop proposee, sur une image deja dematriçee (TIFF 16-bit).
//!
//! Usage : cargo run --bin detect -- <image.tif> [crop_force]

use hyperion_studio_lib::pipeline::{decode, detect};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("usage: detect <image.tif> [crop_force]");
        std::process::exit(2);
    };
    let forced: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    let t0 = std::time::Instant::now();
    let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(input)).expect("lecture TIFF");
    eprintln!("image {w}x{h} lue en {:?}", t0.elapsed());

    let t1 = std::time::Instant::now();
    match detect::detect_planet(&rgb, w, h, None) {
        Some(d) => {
            eprintln!(
                "planete: centre=({:.1}, {:.1}) rayon={:.1} bbox=({}, {}, {}, {}) en {:?}",
                d.cx, d.cy, d.radius, d.bbox.0, d.bbox.1, d.bbox.2, d.bbox.3,
                t1.elapsed()
            );
            let crop = detect::crop_window(&d, w, h, forced, 0.0);
            eprintln!(
                "crop propose: x={} y={} {}x{} ({} px)",
                crop.x,
                crop.y,
                crop.width,
                crop.height,
                crop.width * crop.height
            );
        }
        None => {
            eprintln!("AUCUNE planete detectee");
            std::process::exit(1);
        }
    }
}

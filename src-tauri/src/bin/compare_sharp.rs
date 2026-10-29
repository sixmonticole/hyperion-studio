//! Compare la nettete de deux images (par ex. une frame seule et un stack), en
//! mesurant la variance du Laplacien sur une meme region relative (fraction de
//! l'image), ce qui rend la comparaison independante du crop et de l'echelle.
//!
//! Usage : compare_sharp <imageA.tif> <imageB.tif>

use hyperion_studio_lib::pipeline::{decode, quality};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: compare_sharp <A.tif> <B.tif>");
        std::process::exit(2);
    }

    for path in [&a[1], &a[2]] {
        let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(path)).expect("lecture");
        let score = quality::sharpness_score(&rgb, w, h);
        // Mesure sur le carre central (40% de la plus petite dimension).
        let side = ((w.min(h) as f64) * 0.4) as usize;
        let x0 = (w - side) / 2;
        let y0 = (h - side) / 2;
        let mut sub = vec![0u16; side * side * 3];
        for y in 0..side {
            let sy = y0 + y;
            for x in 0..side {
                let sx = x0 + x;
                let si = (sy * w + sx) * 3;
                let di = (y * side + x) * 3;
                sub[di..di + 3].copy_from_slice(&rgb[si..si + 3]);
            }
        }
        let centre = quality::sharpness_score(&sub, side, side);
        println!(
            "{} : {w}x{h}  nettete globale={score:.1}  centre({side}px)={centre:.1}",
            path
        );
    }
}

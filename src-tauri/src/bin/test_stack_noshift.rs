//! Test de diagnostic : empile les frames d'un dossier SANS alignement
//! geometrique (shift nul), pour isoler l'effet du reechantillonnage sub-pixel.
//!
//! Sans shift, les frames ne se superposent pas : on doit voir du bruit reduit
//! mais une image "multiple". Si cette image "multiple" est PLUS NETTE (au sens
//! des structures resolues) que le stack aligne, c'est le reechantillonnage qui
//! detruit le detail.
//!
//! Usage : test_stack_noshift <dossier_cache_tiff> <out.tif>

use hyperion_studio_lib::model::CropRect;
use hyperion_studio_lib::pipeline::{decode, detect, stack};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache_t20b".into());
    let out = a.get(2).cloned().unwrap_or_else(|| "/tmp/noshift.tif".into());

    let files = decode::list_raw_files(Path::new(&dir)).expect("list");
    if files.is_empty() {
        eprintln!("aucun fichier dans {dir}");
        std::process::exit(1);
    }

    let (ref_rgb, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read");
    let det = detect::detect_planet(&ref_rgb, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop {}x{} en ({},{})", crop.width, crop.height, crop.x, crop.y);

    let mut acc = stack::Accumulator::new(crop.width, crop.height);
    for f in &files {
        let (rgb, fw, fh) = decode::read_rgb16_tiff(f).expect("read");
        let c = crop_into(&rgb, fw, fh, &crop);
        // Shift NUL : aucun reechantillonnage, on empile tel quel.
        let mask = vec![1u8; crop.width * crop.height];
        acc.add(&c, Some(&mask), 1.0, 1.0);
    }
    let res = acc.finish();
    decode::write_rgb16_tiff(Path::new(&out), &res, crop.width, crop.height).expect("write");
    println!("{} frames empilees sans shift -> {out}", files.len());
}

fn crop_into(rgb: &[u16], w: usize, h: usize, crop: &CropRect) -> Vec<u16> {
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

//! Verifie la presence de rotation de champ entre la premiere et la derniere
//! frame : detecte l'objet, puis estime le deplacement local en plusieurs zones
//! (haut, bas, gauche, droite). Si les deplacements locaux divergent, il y a
//! rotation.
//!
//! Usage : diag_rotation <dossier>

use hyperion_studio_lib::pipeline::{align, decode, detect};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| "/tmp/cache200".into());
    let files = decode::list_raw_files(Path::new(&dir)).expect("list");
    if files.len() < 2 {
        println!("pas assez de fichiers");
        return;
    }

    let (ref_rgb, w, h) = decode::read_rgb16_tiff(&files[0]).expect("read");
    let (last_rgb, _, _) = decode::read_rgb16_tiff(files.last().unwrap()).expect("read");

    let det = detect::detect_planet(&ref_rgb, w, h, None).expect("detect");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    println!("crop {}x{}", crop.width, crop.height);

    // Sous-zones : on estime le deplacement local dans 4 quadrants.
    let qw = crop.width / 2;
    let qh = crop.height / 2;
    let zones = [
        ("haut-gauche", 0, 0),
        ("haut-droit", qw, 0),
        ("bas-gauche", 0, qh),
        ("bas-droit", qw, qh),
    ];

    println!("\nDeplacement local par quadrant (px) :");
    for (name, zx, zy) in zones {
        let sub_ref = sub_image(&ref_rgb, w, h, &crop, zx, zy, qw, qh);
        let sub_last = sub_image(&last_rgb, w, h, &crop, zx, zy, qw, qh);

        let lum_ref = align::luminance(&sub_ref, qw * qh);
        let lum_last = align::luminance(&sub_last, qw * qh);
        let al = align::Aligner::new(&lum_ref, qw, qh);
        let (dx, dy) = al.estimate(&lum_last);
        println!("  {name:12} : dx={dx:7.2}  dy={dy:7.2}");
    }
    println!("\nSi les 4 quadrants donnent des deplacements differents, il y a de la");
    println!("rotation de champ (ou distorsion) que l'alignement global ne corrige pas.");
}

fn sub_image(
    rgb: &[u16],
    w: usize,
    h: usize,
    crop: &hyperion_studio_lib::model::CropRect,
    zx: usize,
    zy: usize,
    sw: usize,
    sh: usize,
) -> Vec<u16> {
    let mut out = vec![0u16; sw * sh * 3];
    for y in 0..sh {
        let sy = crop.y + zy + y;
        if sy >= h { break; }
        for x in 0..sw {
            let sx = crop.x + zx + x;
            if sx >= w { break; }
            let si = (sy * w + sx) * 3;
            let di = (y * sw + x) * 3;
            out[di..di + 3].copy_from_slice(&rgb[si..si + 3]);
        }
    }
    out
}

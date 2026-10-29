//! Tests unitaires des briques du pipeline (sans fichiers RAW reels).
//!
//! On genere des images RGB 16-bit synthetiques : une "planete" (disque brillant)
//! sur fond sombre, avec eventuellement un deplacement connu, puis on verifie
//! que detection, crop, alignement, qualite et empilement se comportent bien.

use crate::model::Settings;
use crate::pipeline::{align, detect, quality, stack};

/// Cree une image RGB 16-bit : disque brillant de rayon `r` centre en (cx, cy).
fn synth_frame(w: usize, h: usize, cx: f64, cy: f64, r: f64, noise: u16) -> Vec<u16> {
    let mut img = vec![0u16; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
            let v = if d <= r {
                // Degrade radial pour creer des gradients (mesurables en nettete).
                (60000.0 * (1.0 - 0.5 * d / r)).round() as u16
            } else {
                noise
            };
            let idx = (y * w + x) * 3;
            img[idx] = v;
            img[idx + 1] = v;
            img[idx + 2] = v;
        }
    }
    img
}

#[test]
fn detection_trouve_le_centre() {
    let (w, h) = (400, 300);
    let img = synth_frame(w, h, 180.0, 120.0, 40.0, 200);
    let det = detect::detect_planet(&img, w, h, None).expect("planete detectee");
    assert!((det.cx - 180.0).abs() < 6.0, "cx={}", det.cx);
    assert!((det.cy - 120.0).abs() < 6.0, "cy={}", det.cy);
    assert!(det.radius > 30.0 && det.radius < 60.0, "r={}", det.radius);
}

#[test]
fn crop_englobe_la_planete() {
    let (w, h) = (400, 300);
    let img = synth_frame(w, h, 200.0, 150.0, 50.0, 100);
    let det = detect::detect_planet(&img, w, h, None).unwrap();
    let crop = detect::crop_window(&det, w, h, 0, 10.0);
    // La fenetre doit contenir le centre detecte.
    assert!(crop.x as f64 <= det.cx && det.cx <= (crop.x + crop.width) as f64);
    assert!(crop.y as f64 <= det.cy && det.cy <= (crop.y + crop.height) as f64);
    assert!(crop.width >= 100 && crop.height >= 100);
}

/// Cas qui bugait : objet decale dans le coin et partiellement eclaire (phase).
/// La fenetre doit contenir l'objet entier, pas seulement son coeur lumineux.
#[test]
fn crop_contient_objet_decale_et_partiel() {
    let (w, h) = (400, 300);
    // Disque centre pres du bord superieur gauche.
    let img = gibbeuse(&w, &h, 90.0, 70.0, 45.0, 4000);
    let det = detect::detect_planet(&img, w, h, None).expect("objet detecte");

    // L'objet occupe environ [45,135] x [25,115] ; sa boite est connue.
    let (minx, miny, maxx, maxy) = det.bbox;

    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    assert!(
        crop.x <= minx && maxx <= crop.x + crop.width,
        "objet coupe horizontalement : crop x={} w={} vs objet [{},{}]",
        crop.x,
        crop.width,
        minx,
        maxx
    );
    assert!(
        crop.y <= miny && maxy <= crop.y + crop.height,
        "objet coupe verticalement : crop y={} h={} vs objet [{},{}]",
        crop.y,
        crop.height,
        miny,
        maxy
    );
}

/// Objet qui remplit tout le champ : on ne doit rien couper, donc garder l'image
/// entiere plutot que de forcer un carre plus petit.
#[test]
fn crop_objet_plein_champ_garde_tout() {
    let (w, h) = (300, 250);
    // Disque qui couvre tout le cadre.
    let img = gibbeuse(&w, &h, 150.0, 125.0, 200.0, 5000);
    let det = detect::detect_planet(&img, w, h, None).expect("objet detecte");
    let crop = detect::crop_window(&det, w, h, 0, 0.0);
    let (minx, miny, maxx, maxy) = det.bbox;
    assert!(crop.x <= minx && maxx <= crop.x + crop.width);
    assert!(crop.y <= miny && maxy <= crop.y + crop.height);
}

#[test]
fn alignement_retrouve_le_deplacement() {
    let (w, h) = (256, 256);
    let reference = synth_frame(w, h, 128.0, 128.0, 40.0, 150);
    // Frame decalee de (+7, -5) : la planete a bouge vers la droite et le haut.
    let shifted = synth_frame(w, h, 135.0, 123.0, 40.0, 150);

    let ref_lum = align::luminance(&reference, w * h);
    let shifted_lum = align::luminance(&shifted, w * h);
    let aligner = align::Aligner::new(&ref_lum, w, h);
    // La correction renvoyee doit etre l'oppose du deplacement.
    let (dx, dy) = aligner.estimate(&shifted_lum);

    assert!((dx + 7.0).abs() < 1.5, "dx={dx}");
    assert!((dy - 5.0).abs() < 1.5, "dy={dy}");

    // Re-aligner avec la correction doit ramener la frame sur la reference.
    let realigned = align::shift_rgb16(&shifted, w, h, (dx, dy));
    let re_lum = align::luminance(&realigned, w * h);
    let (dx2, dy2) = aligner.estimate(&re_lum);
    assert!(dx2.abs() < 1.5 && dy2.abs() < 1.5, "residu dx={dx2} dy={dy2}");
}

#[test]
fn alignement_decalage_nul() {
    let (w, h) = (128, 128);
    let frame = synth_frame(w, h, 64.0, 64.0, 24.0, 180);
    let lum = align::luminance(&frame, w * h);
    let aligner = align::Aligner::new(&lum, w, h);
    let (dx, dy) = aligner.estimate(&lum);
    assert!(dx.abs() < 1.0 && dy.abs() < 1.0, "dx={dx} dy={dy}");
}

#[test]
fn shift_rgb16_translate_subpixel() {
    let (w, h) = (32, 32);
    // Image a gradient lineaire en x : un decalage sub-pixel doit interpoler.
    let mut frame = vec![0u16; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let v = (x * 1000) as u16;
            let i = (y * w + x) * 3;
            frame[i] = v;
            frame[i + 1] = v;
            frame[i + 2] = v;
        }
    }

    // Decalage entier : out(x) = src(x - 2) doit etre quasi exact (Catmull-Rom
    // est interpolant aux positions entieres).
    let out = align::shift_rgb16(&frame, w, h, (2.0, 0.0));
    let (si, di) = ((16 * w + 16) * 3, (16 * w + 18) * 3);
    assert!(
        (out[di] as i32 - frame[si] as i32).abs() <= 1,
        "shift entier : {} vs {}",
        out[di],
        frame[si]
    );

    // Decalage demi-pixel : out(16) = src(15.5), donc entre src(15) et src(16).
    let half = align::shift_rgb16(&frame, w, h, (0.5, 0.0));
    let di2 = (16 * w + 16) * 3;
    let lo = frame[(16 * w + 15) * 3] as i32;
    let hi = frame[(16 * w + 16) * 3] as i32;
    let got = half[di2] as i32;
    assert!(
        got >= lo && got <= hi,
        "shift 0.5 px hors de [{lo}, {hi}] : {got}"
    );

    // Le masque doit marquer invalides les pixels dont la source sort du cadre.
    let (_shifted, mask) = align::shift_rgb16_masked(&frame, w, h, (5.0, 0.0));
    assert_eq!(mask.len(), w * h);
    assert_eq!(mask[16 * w + 2], 0, "pixel hors cadre doit etre invalide");
    assert_eq!(mask[16 * w + 16], 1, "pixel dans le cadre doit etre valide");
}

#[test]
fn nettete_distingue_flou_et_net() {
    let (w, h) = (128, 128);
    let net = synth_frame(w, h, 64.0, 64.0, 30.0, 100);
    // Version "floue" : meme image lissee par moyenne 5x5.
    let flou = box_blur(&net, w, h, 5);

    let s_net = quality::sharpness_score(&net, w, h);
    let s_flou = quality::sharpness_score(&flou, w, h);
    assert!(s_net > s_flou, "net={s_net} flou={s_flou}");
}

#[test]
fn normalisation_scores_dans_zero_un() {
    let scores = vec![10.0, 20.0, 30.0, 40.0];
    let norm = quality::normalize_scores(&scores);
    assert_eq!(norm.len(), 4);
    assert!(norm.iter().all(|&v| (0.0..=1.0).contains(&v)));
    assert!((norm[0] - 0.0).abs() < 1e-9);
    assert!((norm[3] - 1.0).abs() < 1e-9);
}

#[test]
fn empilement_reduit_le_bruit() {
    // 20 frames identiques bruitees differemment ; le stack doit etre plus
    // lisse (ecart-type plus faible) que chaque frame d'origine.
    let (w, h) = (64, 64);
    let mut acc = stack::Accumulator::new(w, h);
    let mut seed = 12345u64;
    let mut frames = Vec::new();

    for _ in 0..20 {
        let mut f = synth_frame(w, h, 32.0, 32.0, 12.0, 30000);
        for v in f.iter_mut() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let n = ((seed >> 33) % 4000) as i32 - 2000;
            *v = (*v as i32 + n).clamp(0, 65535) as u16;
        }
        frames.push(f.clone());
        acc.add(&f, None, 1.0, 1.0);
    }

    let result = acc.finish();
    assert_eq!(acc.count, 20);
    assert_eq!(result.len(), w * h * 3);

    // Luminance : ecart-type du stack < ecart-type moyen des frames bruitees.
    let std_frame = std_dev_lum(&frames[0]);
    let std_stack = std_dev_lum(&result);
    assert!(std_stack < std_frame, "std_stack={std_stack} std_frame={std_frame}");
}

#[test]
fn defauts_reglages_sont_automatiques() {
    let s = Settings::default();
    assert_eq!(s.crop_size, 0, "crop auto par defaut");
    assert!(s.align && s.normalize && s.weighted);
    assert!((s.keep_fraction - 0.25).abs() < 1e-9);
}

// --- utilitaires de test ---

/// Cree une image avec un disque dont seule une partie est eclairee (phase).
/// La zone eclairee va du bord droit du disque vers la gauche (croissant), ce
/// qui decale le centroide lumineux : c'est le cas qui faisait couper la Lune.
fn gibbeuse(w: &usize, h: &usize, cx: f64, cy: f64, r: f64, lit: u16) -> Vec<u16> {
    let mut img = vec![0u16; w * h * 3];
    for y in 0..*h {
        for x in 0..*w {
            let d = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
            if d <= r {
                // Partie non eclairee : tres sombre mais non nulle (limbe visible).
                let v = if (x as f64) > cx { lit } else { 200u16 };
                let idx = (y * w + x) * 3;
                img[idx] = v;
                img[idx + 1] = v;
                img[idx + 2] = v;
            }
        }
    }
    img
}

/// Applique une orientation et verifie que les dimensions et un pixel pivot
/// se comportent comme attendu.
#[test]
fn orientation_rotation_et_miroir() {
    use crate::model::Orientation;
    // Image 3x2 : pixel (0,0) marcate 100, (2,0) marque 200.
    let w = 3;
    let h = 2;
    let mut img = vec![0u16; w * h * 3];
    for i in 0..(w * h) {
        img[i * 3] = (i as u16) * 10; // 0,10,20,30,40,50 en R
    }

    // Rotation 180 : le premier pixel R devient 50, dimensions inchangees.
    let (r180, w2, h2) = align::apply_orientation(&img, w, h, Orientation {
        rotate: 180,
        flip_h: false,
        flip_v: false,
    });
    assert_eq!((w2, h2), (3, 2));
    assert_eq!(r180[0], 50);
    assert_eq!(r180[(w * h - 1) * 3], 0);

    // Rotation 90 : dimensions echangees (2, 3).
    let (_, w3, h3) = align::apply_orientation(&img, w, h, Orientation {
        rotate: 90,
        flip_h: false,
        flip_v: false,
    });
    assert_eq!((w3, h3), (2, 3));

    // Miroir horizontal : le pixel (0,0) prend la valeur de (2,0) = 20.
    let (fh, w4, h4) = align::apply_orientation(&img, w, h, Orientation {
        rotate: 0,
        flip_h: true,
        flip_v: false,
    });
    assert_eq!((w4, h4), (3, 2));
    assert_eq!(fh[0], 20);
}

/// Un miroir applique deux fois doit redonner l'image d'origine.
#[test]
fn orientation_involutive() {
    use crate::model::Orientation;
    let w = 5;
    let h = 4;
    let img: Vec<u16> = (0..(w * h * 3)).map(|i| (i % 65535) as u16).collect();

    let (once, _, _) = align::apply_orientation(&img, w, h, Orientation {
        rotate: 0,
        flip_h: true,
        flip_v: true,
    });
    let (twice, w2, h2) = align::apply_orientation(&once, w, h, Orientation {
        rotate: 0,
        flip_h: true,
        flip_v: true,
    });
    assert_eq!((w2, h2), (w, h));
    assert_eq!(twice, img);
}

fn box_blur(src: &[u16], w: usize, h: usize, k: usize) -> Vec<u16> {
    let mut out = src.to_vec();
    let r = (k / 2) as i64;
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            let mut n = 0u32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let sx = x as i64 + dx;
                    let sy = y as i64 + dy;
                    if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                        continue;
                    }
                    let idx = (sy as usize * w + sx as usize) * 3;
                    sum[0] += src[idx] as u32;
                    sum[1] += src[idx + 1] as u32;
                    sum[2] += src[idx + 2] as u32;
                    n += 1;
                }
            }
            let oi = (y * w + x) * 3;
            out[oi] = (sum[0] / n) as u16;
            out[oi + 1] = (sum[1] / n) as u16;
            out[oi + 2] = (sum[2] / n) as u16;
        }
    }
    out
}

fn std_dev_lum(img: &[u16]) -> f64 {
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let n = (img.len() / 3) as f64;
    for px in img.chunks_exact(3) {
        let l = 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
        sum += l;
        sum_sq += l * l;
    }
    let mean = sum / n;
    (sum_sq / n - mean * mean).max(0.0).sqrt()
}

// --- Robustesse : ecriture atomique et lecture de fichiers corrompus ---------

/// L'ecriture d'un TIFF doit etre atomique : aucun `.tmp` ne doit subsister
/// apres un succes, et le fichier final doit etre relisible.
#[test]
fn ecriture_tiff_atomique_et_relisible() {
    let dir = std::env::temp_dir().join(format!("hyperion_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("frame.tif");

    let (w, h) = (32, 24);
    let img = synth_frame(w, h, 16.0, 12.0, 8.0, 100);
    crate::pipeline::decode::write_rgb16_tiff(&path, &img, w, h).expect("ecriture");

    assert!(path.exists(), "le TIFF final doit exister");

    // Aucun fichier temporaire ne doit rester.
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "aucun .tmp ne doit subsister");

    // Le fichier doit etre relisible et coherent.
    let (back, bw, bh) = crate::pipeline::decode::read_rgb16_tiff(&path).expect("relecture");
    assert_eq!((bw, bh), (w, h));
    assert_eq!(back.len(), img.len());
    assert_eq!(back, img, "les pixels doivent etre identiques");

    std::fs::remove_dir_all(&dir).ok();
}

/// Un fichier de cache tronque doit etre detecte comme illisible plutot que
/// de produire des pixels absurdes silencieusement.
#[test]
fn tiff_tronque_est_illisible() {
    let dir = std::env::temp_dir().join(format!("hyperion_bad_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("corrompu.tif");

    // Un fichier qui n'est pas un TIFF du tout.
    std::fs::write(&path, b"ceci n'est pas un TIFF").unwrap();
    assert!(
        crate::pipeline::decode::read_rgb16_tiff(&path).is_err(),
        "un faux TIFF doit etre rejete"
    );

    // Un fichier vide : le pipeline le considere comme non cache.
    let empty = dir.join("vide.tif");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(std::fs::metadata(&empty).unwrap().len(), 0);

    std::fs::remove_dir_all(&dir).ok();
}

// --- Enhance: highlight protection (the "Tycho crushed to white" regression) --

/// A bright region close to the white point must stay detailed: two distinct
/// input levels must remain distinct after enhancement. A hard clamp would
/// flatten them into the same value, which is exactly what used to happen.
#[test]
fn enhancement_preserves_highlight_detail() {
    use crate::model::EnhanceSettings;

    let (w, h) = (64, 64);
    // A gradient near the top of the range: the two brightest rows differ.
    let mut img = vec![0u16; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            // Values from ~0.30 to ~0.99 of full scale.
            let v = (0.30 + 0.69 * (y as f32 / h as f32) * 65535.0) as u16;
            let idx = (y * w + x) * 3;
            img[idx] = v;
            img[idx + 1] = v;
            img[idx + 2] = v;
        }
    }

    let s = EnhanceSettings {
        enabled: true,
        wavelet_amount: 0.0,
        wavelet_levels: 1,
        black_point: 0.0,
        white_point: 0.0,
        gamma: 1.0,
        clahe_amount: 0.0,
        clahe_tiles: 8,
        clarity: 0.0,
    };
    let out = crate::pipeline::enhance::enhance(&img, w, h, &s);

    // Sample two input levels near the top and check they stay ordered and
    // distinct. If the stretch saturated, they would collapse to the same u16.
    let idx_a = ((h - 2) * w) * 3;
    let idx_b = ((h - 3) * w) * 3;
    let a = out[idx_a] as i32;
    let b = out[idx_b] as i32;
    assert!(a > b, "higher input must stay higher: a={a} b={b}");
    assert!(
        a - b >= 2,
        "highlight detail must survive the stretch: a={a} b={b}"
    );
    assert!(a < 65535, "the brightest value must not saturate");
}

// --- Per-region selection (C) -----------------------------------------------

/// With a tile grid, a frame that is sharp in one half and blurred in the
/// other must score high in the sharp tiles and low in the blurred ones.
#[test]
fn tile_scores_locate_sharpness() {
    use crate::pipeline::quality;

    let (w, h) = (128, 64);
    let tile = 32;
    // Left half: fine checkerboard (high local detail). Right half: flat.
    let mut img = vec![0u16; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let v: u16 = if x < w / 2 {
                if (x + y) % 2 == 0 { 60000 } else { 1000 }
            } else {
                30000
            };
            let i = (y * w + x) * 3;
            img[i] = v;
            img[i + 1] = v;
            img[i + 2] = v;
        }
    }

    let s = quality::score_frame(&img, w, h, tile);
    assert_eq!(s.tiles_x, 4, "128 px / 32 = 4 tiles across");
    assert_eq!(s.tiles_y, 2, "64 px / 32 = 2 tiles down");
    assert_eq!(s.tiles.len(), 8);

    // Tiles 0 and 1 (top/bottom of the left half) must beat tiles 2 and 3.
    let left_top = s.tiles[0];
    let left_bottom = s.tiles[4]; // row 1, col 0
    let right_top = s.tiles[2];
    let right_bottom = s.tiles[6];
    assert!(
        left_top > right_top && left_bottom > right_bottom,
        "detailed tiles must outscore flat ones: {left_top} {left_bottom} vs {right_top} {right_bottom}"
    );
}

/// The per-tile gate must let a frame contribute to some tiles and not others.
/// This is the core of per-region selection: no pixel of a non-admitted tile is
/// averaged in.
#[test]
fn tile_gate_limits_contribution() {
    use crate::pipeline::stack::{Accumulator, TileGate};

    let (w, h) = (8, 8);
    // Tile grid of 2x2 tiles of 4x4.
    let tiles_x = 2;
    let tile = 4;

    let mut acc = Accumulator::new(w, h);
    // A uniform frame at 1000.
    let frame = vec![1000u16; w * h * 3];
    // Admit only tile 0 (top-left). Others must stay empty.
    let gate = vec![1u8, 0, 0, 0];
    acc.add_gated(
        &frame,
        None,
        1.0,
        1.0,
        Some(TileGate {
            gate: &gate,
            tiles_x,
            tile,
        }),
    );
    let out = acc.finish();

    // Top-left pixel (0,0) is filled.
    assert_eq!(out[0], 1000);
    // Top-right pixel (7,0) belongs to tile 1 (not admitted): stays black.
    assert_eq!(out[7 * 3], 0, "non-admitted tile must remain empty");
    // Bottom-left pixel (0,7) belongs to tile 2 (not admitted).
    assert_eq!(out[(7 * w) * 3], 0, "non-admitted tile must remain empty");
}

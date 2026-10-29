//! Detection de l'objet (planete, Lune) sur une frame de reference et calcul de
//! la fenetre de crop.
//!
//! En planetaire, la cible est la source la plus brillante et la plus etendue.
//! On utilise deux seuils distincts :
//! - un seuil haut (moyenne + k * ecart-type) pour localiser le coeur lumineux et
//!   le centroide ;
//! - un seuil bas pour mesurer l'etendue reelle de l'objet, terminateur et limbe
//!   sombres compris. Sans ce second seuil, une Lune gibbeuse voit sa partie
//!   sombre exclue et la fenetre de crop la coupe.

use crate::model::CropRect;

/// Resultat de la detection sur une frame.
pub struct Detection {
    /// Centre du coeur lumineux (x, y) en coordonnees image.
    pub cx: f64,
    pub cy: f64,
    /// Rayon approche (demi-etendue du masque de seuil haut).
    pub radius: f64,
    /// Etendue reelle de l'objet (minx, miny, maxx, maxy), seuil bas inclus.
    pub bbox: (usize, usize, usize, usize),
}

/// Detecte l'objet dans une image RGB 16-bit.
///
/// `rgb16` est entrelacee (w * h * 3). `region` restreint la recherche a une
/// sous-fenetre (x, y, w, h) ; utile pour accelerer sur une grande image.
pub fn detect_planet(
    rgb16: &[u16],
    width: usize,
    height: usize,
    region: Option<(usize, usize, usize, usize)>,
) -> Option<Detection> {
    let (rx, ry, rw, rh) = region.unwrap_or((0, 0, width, height));
    let rx = rx.min(width);
    let ry = ry.min(height);
    let rw = rw.min(width - rx);
    let rh = rh.min(height - ry);
    if rw == 0 || rh == 0 {
        return None;
    }

    // Luminance sur la sous-fenetre, sous-echantillonnee pour la vitesse.
    let step = (rw.min(rh) / 256).max(1);
    // Nombre reel de colonnes echantillonnees (indispensable pour recalculer
    // les coordonnees depuis l'index lineaire).
    let cols = (rw + step - 1) / step;
    let rows = (rh + step - 1) / step;
    let mut lum = Vec::with_capacity(cols * rows);
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;

    for gy in 0..rows {
        let y = ry + gy * step;
        if y >= ry + rh {
            break;
        }
        for gx in 0..cols {
            let x = rx + gx * step;
            if x >= rx + rw {
                break;
            }
            let idx = (y * width + x) * 3;
            let l = 0.299 * rgb16[idx] as f64
                + 0.587 * rgb16[idx + 1] as f64
                + 0.114 * rgb16[idx + 2] as f64;
            lum.push(l);
            sum += l;
            sum_sq += l * l;
        }
    }
    if lum.is_empty() {
        return None;
    }

    let n = lum.len() as f64;
    let mean = sum / n;
    let var = (sum_sq / n - mean * mean).max(0.0);
    let std = var.sqrt();
    let threshold_high = mean + 2.5 * std;
    // Seuil bas : capte le corps de l'objet y compris ses zones sombres, tout en
    // restant au-dessus du fond de ciel. Le facteur 0.15 * std est volontairement
    // prudent (le limbe lunaire tombe tres bas en luminance).
    let threshold_low = mean + 0.15 * std;

    if !threshold_high.is_finite() || threshold_high <= 0.0 {
        return None;
    }

    // Centroide pondere sur les pixels du seuil haut.
    let mut w_sum = 0.0f64;
    let mut x_sum = 0.0f64;
    let mut y_sum = 0.0f64;
    // Etendue mesuree au seuil bas (limbe) : elle sert seulement a E L A R G I R
    // legerement le coeur, jamais a definir la boite seule.
    let (mut minx, mut miny, mut maxx, mut maxy) = (usize::MAX, usize::MAX, 0usize, 0usize);
    // Etendue du coeur (seuil haut), pour le rayon ET pour la boite englobante.
    let (mut hminx, mut hminy, mut hmaxx, mut hmaxy) = (usize::MAX, usize::MAX, 0usize, 0usize);

    for (i, &l) in lum.iter().enumerate() {
        if l < threshold_low {
            continue;
        }
        let gx = i % cols;
        let gy = i / cols;
        let px = rx + gx * step;
        let py = ry + gy * step;

        // Etendue reelle (seuil bas).
        minx = minx.min(px);
        miny = miny.min(py);
        maxx = maxx.max(px);
        maxy = maxy.max(py);

        // Coeur lumineux (seuil haut) pour le centroide.
        if l >= threshold_high {
            let wgt = l - mean;
            w_sum += wgt;
            x_sum += wgt * px as f64;
            y_sum += wgt * py as f64;
            hminx = hminx.min(px);
            hminy = hminy.min(py);
            hmaxx = hmaxx.max(px);
            hmaxy = hmaxy.max(py);
        }
    }

    if minx == usize::MAX {
        return None;
    }

    // Si le coeur n'a rien donne (objet uniforme), retomber sur l'etendue basse.
    let (cx, cy) = if w_sum > 0.0 {
        (x_sum / w_sum, y_sum / w_sum)
    } else {
        (
            (minx + maxx) as f64 / 2.0,
            (miny + maxy) as f64 / 2.0,
        )
    };

    let radius = if hminx != usize::MAX {
        (((hmaxx - hminx).max(hmaxy - hminy)) as f64 / 2.0).max(1.0)
    } else {
        (((maxx - minx).max(maxy - miny)) as f64 / 2.0).max(1.0)
    };

    // **Boite englobante** : on part du coeur lumineux (seuil haut), qui est
    // fiable, et on l'elargit d'une petite fraction (`radius * 0.10`) pour
    // englober le limbe sombre sans envahir le fond de ciel.
    //
    // Definir la boite sur le seuil bas etait la cause d'un cadrage trop large :
    // sur une image ou la Lune n'occupe qu'une partie du champ, le seuil bas
    // capte une vaste couronne de fond, et la fenetre de crop finissait par
    // remplir toute l'image (d'ou l'absence de marge noire).
    let bbox = if hminx != usize::MAX {
        let grow = (radius * 0.10).round() as usize;
        (
            hminx.saturating_sub(grow),
            hminy.saturating_sub(grow),
            (hmaxx + grow).min(width.saturating_sub(1)),
            (hmaxy + grow).min(height.saturating_sub(1)),
        )
    } else {
        (minx, miny, maxx, maxy)
    };

    Some(Detection {
        cx,
        cy,
        radius,
        bbox,
    })
}

/// Construit une fenetre de crop carree qui contient **toujours** l'objet entier.
///
/// Regle : la fenetre est centree sur la boite englobante de l'objet, et sa
/// taille est au moins celle de cette boite (augmentee d'une marge et du
/// deplacement estime entre frames). Elle est ensuite rognee dans les limites de
/// l'image, mais sans jamais amputer l'objet : si l'objet touche un bord, on
/// agrandit la fenetre de l'autre cote quand c'est possible.
///
/// `motion` est l'amplitude de deplacement estimee entre frames (en px).
pub fn crop_window(
    detection: &Detection,
    width: usize,
    height: usize,
    forced_size: usize,
    motion: f64,
) -> CropRect {
    let (minx, miny, maxx, maxy) = detection.bbox;
    let obj_w = maxx.saturating_sub(minx) as f64;
    let obj_h = maxy.saturating_sub(miny) as f64;

    // Centre sur la boite englobante (et non sur le centroide : sur une Lune
    // gibbeuse, le centroide du coeur lumineux n'est pas au centre du disque).
    let obj_cx = (minx + maxx) as f64 / 2.0;
    let obj_cy = (miny + maxy) as f64 / 2.0;

    // Taille requise : l'objet + une marge. La marge est **proportionnelle**
    // (8 % du diametre) pour laisser une couronne de noir autour de l'objet :
    // une marge fixe de quelques pixels collait la Lune au bord du cadre.
    let margin = (obj_w.max(obj_h) * 0.08).max(12.0) + motion;
    let needed = obj_w.max(obj_h) + 2.0 * margin;

    let mut size = if forced_size > 0 {
        forced_size as f64
    } else {
        needed
    };

    // Filet de securite : ne jamais descendre sous la taille de l'objet.
    size = size.max(obj_w + 2.0 * margin).max(obj_h + 2.0 * margin);

    // La fenetre ne peut pas depasser l'image (carree, donc min des deux cotes).
    let max_size = width.min(height) as f64;
    size = size.min(max_size).max(16.0);
    let size = size.round() as usize;

    // Placement : centre sur l'objet, puis recadre dans l'image. Si l'objet
    // touche un bord, on pousse la fenetre vers l'interieur sans la reduire,
    // pour garder l'objet entier autant que la taille d'image le permet.
    let half = size as i64 / 2;
    let cx = obj_cx.round() as i64;
    let cy = obj_cy.round() as i64;

    let max_x = (width as i64 - size as i64).max(0);
    let max_y = (height as i64 - size as i64).max(0);

    let x = (cx - half).clamp(0, max_x) as usize;
    let y = (cy - half).clamp(0, max_y) as usize;

    // Verification : si malgre tout l'objet deborde (image trop petite pour la
    // taille demandee), on elargit la fenetre a l'image entiere pour ne rien
    // couper. C'est le comportement attendu pour la Lune qui remplit le champ.
    let obj_within = minx >= x && maxx <= x + size && miny >= y && maxy <= y + size;
    if !obj_within {
        let need = (obj_w.max(obj_h) + 2.0 * margin).min(max_size);
        let size2 = need.round() as usize;
        if size2 != size && size2 >= (width.min(height)) {
            // L'objet occupe tout le champ : on crop au maximum possible.
            return CropRect {
                x: 0,
                y: 0,
                width,
                height,
            };
        }
    }

    CropRect {
        x,
        y,
        width: size.min(width),
        height: size.min(height),
    }
}

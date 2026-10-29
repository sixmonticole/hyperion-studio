//! Score de nettete de chaque frame, pour ne garder que les meilleures.
//!
//! On combine deux indicateurs classiques en planetaire :
//! - la variance du Laplacien (sensibilite au flou)
//! - la mesure de Tenengrad (energie des gradients, via Sobel)
//!
//! Les deux sont calcules sur la luminance de la fenetre de crop uniquement, ce
//! qui evite que le fond de ciel bruite ne domine le score.

/// Resultat du scoring d'une frame : un score global, et un score par tuile.
///
/// Le score par tuile est ce qui permet la **selection par zone** : chaque
/// region de l'image est tiree des frames qui sont nettes *a cet endroit*, au
/// lieu de subir le flou d'une frame jugee globalement bonne mais molle au
/// terminateur, ou l'inverse.
pub struct FrameScore {
    /// Score global (moyenne des tuiles), conserve pour la compatibilite et
    /// pour l'affichage.
    pub global: f64,
    /// Score par tuile, dans l'ordre ligne par ligne (index = ty * tiles_x + tx).
    pub tiles: Vec<f32>,
    /// Nombre de tuiles en largeur (pour reconstruire l'index).
    pub tiles_x: usize,
    /// Nombre de tuiles en hauteur.
    pub tiles_y: usize,
}

/// Calcule la nettete globale et par tuile d'une frame croppee.
///
/// `tile` est la taille d'une tuile carree en pixels (0 ou 1 = pas de
/// subdivision, auquel cas `tiles` contient un seul element).
pub fn score_frame(
    cropped_rgb16: &[u16],
    width: usize,
    height: usize,
    tile: usize,
) -> FrameScore {
    if width < 3 || height < 3 {
        return FrameScore {
            global: 0.0,
            tiles: vec![0.0],
            tiles_x: 1,
            tiles_y: 1,
        };
    }

    // Luminance plane de la fenetre.
    let mut lum = vec![0.0f32; width * height];
    for (i, px) in lum.iter_mut().enumerate() {
        let idx = i * 3;
        *px = 0.299 * cropped_rgb16[idx] as f32
            + 0.587 * cropped_rgb16[idx + 1] as f32
            + 0.114 * cropped_rgb16[idx + 2] as f32;
    }

    // Tile grid. `tile <= 1` means "no subdivision": a single tile spanning the
    // whole image (this is what `sharpness_score` relies on).
    let no_subdiv = tile <= 1;
    let tiles_x = if no_subdiv { 1 } else { width.div_ceil(tile).max(1) };
    let tiles_y = if no_subdiv { 1 } else { height.div_ceil(tile).max(1) };
    let n_tiles = tiles_x * tiles_y;

    // Accumulateurs par tuile : (somme lap, somme lap^2, somme gradient, n).
    let mut acc = vec![(0.0f64, 0.0f64, 0.0f64, 0.0f64); n_tiles];

    let at = |x: usize, y: usize| -> f32 { lum[y * width + x] };

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let lap = -4.0 * at(x, y)
                + at(x - 1, y)
                + at(x + 1, y)
                + at(x, y - 1)
                + at(x, y + 1);

            let gx = -at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1)
                + at(x + 1, y - 1)
                + 2.0 * at(x + 1, y)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);

            let tx = if no_subdiv { 0 } else { (x / tile).min(tiles_x - 1) };
            let ty = if no_subdiv { 0 } else { (y / tile).min(tiles_y - 1) };
            let a = &mut acc[ty * tiles_x + tx];
            a.0 += lap as f64;
            a.1 += (lap * lap) as f64;
            a.2 += (gx * gx + gy * gy) as f64;
            a.3 += 1.0;
        }
    }

    // Score par tuile, avec la meme combinaison que le score global.
    let mut tiles = vec![0.0f32; n_tiles];
    let (mut gsum, mut gcount) = (0.0f64, 0.0f64);
    for (i, &(lap_sum, lap_sq, teng, n)) in acc.iter().enumerate() {
        if n <= 0.0 {
            // Tuile trop petite pour un noyau 3x3 : score neutre.
            tiles[i] = 0.0;
            continue;
        }
        let mean = lap_sum / n;
        let lap_var = (lap_sq / n - mean * mean).max(0.0);
        let s = (lap_var.sqrt() + (teng / n).sqrt()).sqrt();
        tiles[i] = s as f32;
        gsum += s;
        gcount += 1.0;
    }

    let global = if gcount > 0.0 { gsum / gcount } else { 0.0 };

    FrameScore {
        global,
        tiles,
        tiles_x,
        tiles_y,
    }
}

/// Calcule le score de nettete d'une frame croppee (luminance uniquement).
///
/// Enveloppe de commodite sans subdivision, conservee pour les tests et les
/// usages qui n'ont pas besoin de la grille.
pub fn sharpness_score(cropped_rgb16: &[u16], width: usize, height: usize) -> f64 {
    score_frame(cropped_rgb16, width, height, 0).global
}

/// Normalise les scores bruts en [0, 1] par min-max.
pub fn normalize_scores(scores: &[f64]) -> Vec<f64> {
    let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if !range.is_finite() || range <= 0.0 {
        return vec![1.0; scores.len()];
    }
    scores.iter().map(|&s| (s - min) / range).collect()
}
